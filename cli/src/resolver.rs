use anyhow::{bail, Context, Result};
use colored::*;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::Manifest;
use crate::registry::RegistryClient;

#[derive(Debug, Clone)]
pub struct ResolvedEnv {
    pub vars: HashMap<String, String>,
    pub path_prefixes: Vec<PathBuf>,
}

fn detect_platform() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("linux", "x86_64") => "linux-x86_64".to_string(),
        ("linux", "aarch64") => "linux-aarch64".to_string(),
        ("macos", "aarch64") => "macos-aarch64".to_string(),
        ("macos", "x86_64") => "macos-x86_64".to_string(),
        _ => format!("{}-{}", os, arch),
    }
}

fn strip_sha256_prefix(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("sha256:") {
        rest.trim()
    } else if let Some(rest) = t.strip_prefix("sha256-") {
        rest.trim()
    } else {
        t
    }
}

fn dependency_key(name: &str, version: &str, platform: &str, sha256: &str) -> String {
    format!(
        "{}::{}::{}::{}",
        name.trim(),
        version.trim(),
        platform.trim(),
        strip_sha256_prefix(sha256).to_ascii_lowercase()
    )
}

fn sanitize_env_key(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn cache_root() -> PathBuf {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".extensilica").join("cache").join("tools")
}

fn ensure_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("create dir {}", p.display()))
}

fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("download {}", url))?;
    if !resp.status().is_success() {
        bail!("Download failed: {} ({})", url, resp.status());
    }
    let mut r = resp;
    let mut buf: Vec<u8> = Vec::new();
    r.copy_to(&mut buf).context("read download body")?;
    Ok(buf)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn unpack_tool_archive(bytes: &[u8], dest: &Path, url: &str) -> Result<()> {
    ensure_dir(dest)?;
    let lower = url.to_ascii_lowercase();

    // Prefer content sniffing so authenticated/proxied URLs without file extensions still work.
    let is_gzip = bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b;
    let is_zstd = bytes.len() >= 4 && bytes[0] == 0x28 && bytes[1] == 0xB5 && bytes[2] == 0x2F && bytes[3] == 0xFD;

    if is_gzip || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let gz = flate2::read::GzDecoder::new(bytes);
        let mut ar = tar::Archive::new(gz);
        ar.unpack(dest).context("unpack .tar.gz")?;
        return Ok(());
    }
    if is_zstd || lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        let dec = zstd::stream::read::Decoder::new(bytes).context("init zstd decoder")?;
        let mut ar = tar::Archive::new(dec);
        ar.unpack(dest).context("unpack .tar.zst")?;
        return Ok(());
    }
    if lower.ends_with(".tar") {
        let mut ar = tar::Archive::new(bytes);
        ar.unpack(dest).context("unpack .tar")?;
        return Ok(());
    }
    // Last chance: attempt plain tar even without extension.
    if let Ok(mut entries) = tar::Archive::new(bytes).entries() {
        if entries.next().transpose().is_ok() {
            let mut ar = tar::Archive::new(bytes);
            ar.unpack(dest).context("unpack tar stream")?;
            return Ok(());
        }
    }
    bail!("Unsupported tool archive format for URL: {}", url);
}

fn pick_toolchain_root_key(tool_roots: &HashMap<String, PathBuf>) -> Option<String> {
    // Heuristic: prefer a tool explicitly named "toolchain", otherwise pick the first that contains "toolchain",
    // then "llvm", then "gcc". This keeps early manifests simple while spec evolves.
    let keys: Vec<String> = tool_roots.keys().cloned().collect();
    if keys.iter().any(|k| k == "toolchain") {
        return Some("toolchain".to_string());
    }
    for needle in ["toolchain", "riscv-gnu-toolchain", "llvm", "gcc"] {
        if let Some(k) = keys.iter().find(|k| k.contains(needle)) {
            return Some(k.clone());
        }
    }
    keys.into_iter().next()
}

/// Resolve v0.2 dependencies/tools into a cache and return environment variables for execution.
///
/// - Downloads artifacts if missing
/// - Verifies sha256
/// - Extracts into ~/.extensilica/cache/tools/<name>/<version>/<platform>/<sha256>/
/// - Exposes XSIL_<TOOL>_ROOT env vars
pub fn resolve_execution_env(
    manifest: &Manifest,
    package_root: &Path,
    registry: Option<&RegistryClient>,
) -> Result<ResolvedEnv> {
    let platform = detect_platform();
    let mut vars: HashMap<String, String> = HashMap::new();
    let mut path_prefixes: Vec<PathBuf> = Vec::new();

    // Bundled toolchain (spec v2 style): set XSIL_TOOLCHAIN_ROOT to toolchain.root inside package if present.
    if let Some(ref tc) = manifest.toolchain {
        if let Some(root) = tc.get("root").and_then(|v| v.as_str()) {
            let p = package_root.join(root);
            if p.exists() {
                vars.insert("XSIL_TOOLCHAIN_ROOT".to_string(), p.to_string_lossy().to_string());
                path_prefixes.push(p.join("bin"));
            }
        }
    }

    let mode = manifest
        .resolution
        .as_ref()
        .and_then(|r| r.get("mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    if mode == "host-dependent" || mode.is_empty() {
        // No auto-resolution. Still allow execution.env expansion to use bundled paths.
        return Ok(ResolvedEnv { vars, path_prefixes });
    }

    if mode != "resolved" && mode != "bundled" {
        bail!("Unsupported resolution.mode '{}'", mode);
    }

    // Bundled means "no downloads", but still allow dependencies.tools to exist (ignored).
    if mode == "bundled" {
        return Ok(ResolvedEnv { vars, path_prefixes });
    }

    let deps = manifest.dependencies.as_ref().context("resolution.mode=resolved requires manifest.dependencies")?;
    let tools = deps
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let client = Client::builder().no_gzip().build().unwrap_or_else(|_| Client::new());
    let resolved_urls: HashMap<String, String> = if let (Some(reg), Some(deps)) = (registry, manifest.dependencies.as_ref()) {
        reg.resolve_artifacts(deps)?
    } else {
        HashMap::new()
    };
    let mut tool_roots: HashMap<String, PathBuf> = HashMap::new();

    println!("{} Resolving dependencies...", "➤".blue());

    for t in tools {
        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
        let version = t.get("version").and_then(|v| v.as_str()).unwrap_or("").trim();
        if name.is_empty() || version.is_empty() {
            bail!("Invalid tool dependency (name/version required)");
        }
        if version.eq_ignore_ascii_case("latest") {
            bail!("Tool dependency {} uses forbidden version 'latest'", name);
        }

        let platforms = t
            .get("platforms")
            .and_then(|v| v.as_object())
            .context("tool.platforms is required")?;

        let art = platforms
            .get(&platform)
            .and_then(|v| v.as_object())
            .with_context(|| format!("tool {}@{} has no artifact for platform {}", name, version, platform))?;

        let declared_url = art
            .get("url")
            .and_then(|v| v.as_str())
            .context("tool artifact url is required")?
            .trim()
            .to_string();
        let sha = art
            .get("sha256")
            .and_then(|v| v.as_str())
            .context("tool artifact sha256 is required")?;
        let sha = strip_sha256_prefix(sha).to_string();
        if sha.len() < 32 {
            bail!("tool {}@{} has invalid sha256", name, version);
        }
        let key = dependency_key(name, version, &platform, &sha);
        let url = resolved_urls
            .get(&key)
            .cloned()
            .unwrap_or_else(|| declared_url.clone());

        let dest = cache_root()
            .join(name)
            .join(version)
            .join(&platform)
            .join(&sha);

        if dest.exists() {
            println!("{} {}@{} found in cache", "✓".green(), name.bold(), version.cyan());
        } else {
            println!("{} downloading {}@{}", "↓".blue(), name.bold(), version.cyan());
            let bytes = if let Some(reg) = registry {
                reg.download_from_url(&url)?
            } else {
                download_bytes(&client, &url)?
            };
            let got = sha256_hex(&bytes);
            if got != sha {
                bail!(
                    "sha256 mismatch for {}@{}\n  expected: {}\n  actual:   {}",
                    name,
                    version,
                    sha,
                    got
                );
            }
            // Extract into a temp directory then rename.
            let tmp = dest
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!(".tmp-{}", uuid::Uuid::new_v4()));
            if tmp.exists() {
                fs::remove_dir_all(&tmp).ok();
            }
            unpack_tool_archive(&bytes, &tmp, &url)?;
            ensure_dir(dest.parent().unwrap_or_else(|| Path::new(".")))?;
            fs::rename(&tmp, &dest).context("commit tool to cache")?;
            println!("{} sha256 verified", "✓".green());
        }

        // Root is the extracted directory. If it contains a single top-level folder, use it.
        let mut root = dest.clone();
        if let Ok(entries) = fs::read_dir(&dest) {
            let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|x| x.path())).collect();
            names.sort();
            if names.len() == 1 && names[0].is_dir() {
                root = names[0].clone();
            }
        }

        tool_roots.insert(name.to_string(), root.clone());
        let env_key = format!("XSIL_{}_ROOT", sanitize_env_key(name));
        vars.insert(env_key, root.to_string_lossy().to_string());
        // Common convention: tool root has bin/
        path_prefixes.push(root.join("bin"));
    }

    if !vars.contains_key("XSIL_TOOLCHAIN_ROOT") {
        if let Some(k) = pick_toolchain_root_key(&tool_roots) {
            if let Some(p) = tool_roots.get(&k) {
                vars.insert("XSIL_TOOLCHAIN_ROOT".to_string(), p.to_string_lossy().to_string());
            }
        }
    }

    Ok(ResolvedEnv { vars, path_prefixes })
}

/// Expand `$VAR` / `${VAR}` occurrences in `value` using `vars`, and pass-through to process env as fallback.
pub fn expand_env(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{}}}", k), v);
        out = out.replace(&format!("${}", k), v);
    }
    // Also expand $PATH from current environment.
    if let Ok(p) = std::env::var("PATH") {
        out = out.replace("${PATH}", &p);
        out = out.replace("$PATH", &p);
    }
    out
}

