use fs2::FileExt;
use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use sha2::{Sha256, Digest};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Read;
use tar::Archive;
use colored::*;
use semver::Version;

use crate::types::{InstalledExtension, Manifest};
use crate::resolver::{ResolvedEnv, expand_env};
use std::collections::HashMap;

// ── .xsilignore support ───────────────────────────────────────────────────────

/// A single parsed rule from `.xsilignore`.
#[allow(dead_code)]
struct IgnoreRule {
    /// Original cleaned pattern string (for display/debug).
    raw: String,
    /// If true the rule un-ignores previously ignored paths.
    negated: bool,
    /// Pattern only matches directories (trailing `/` in source).
    dir_only: bool,
    /// Pattern is anchored to the package root (leading `/` in source).
    anchored: bool,
    /// Compiled glob pattern used for matching.
    pattern: glob::Pattern,
}

/// Collection of ignore rules loaded from `.xsilignore`.
///
/// Rules are evaluated in declaration order; the **last** matching rule wins
/// (identical to git/npm semantics).
pub struct IgnoreRules {
    rules: Vec<IgnoreRule>,
}

impl IgnoreRules {
    /// Built-in entries that are **always** excluded, regardless of user rules.
    const BUILTIN: &'static [&'static str] = &[
        ".git",
        ".xsilignore",
        ".DS_Store",
        "Thumbs.db",
    ];

    /// Load rules from `<dir>/.xsilignore`.  If the file is absent, an empty
    /// rule set (only built-ins apply) is returned without error.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(".xsilignore");
        let source = fs::read_to_string(&path).unwrap_or_default();
        let mut rules: Vec<IgnoreRule> = Vec::new();

        for line in source.lines() {
            let line = line.trim();
            // Skip blank lines and comments.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let negated = line.starts_with('!');
            let raw = if negated { &line[1..] } else { line };

            // Trailing `/` means directory-only match.
            let dir_only = raw.ends_with('/');
            let raw = raw.trim_end_matches('/');

            // Leading `/` means anchored to the root of the package.
            let anchored = raw.starts_with('/');
            let raw = raw.trim_start_matches('/');

            if raw.is_empty() {
                continue;
            }

            // Build a glob pattern.  For non-anchored patterns without a `/
            // in the middle (e.g. "*.log") we prepend "**/` so the pattern
            // matches at any depth.
            let glob_str = if !anchored && !raw.contains('/') {
                format!("**/{}", raw)
            } else {
                raw.to_string()
            };

            if let Ok(pattern) = glob::Pattern::new(&glob_str) {
                rules.push(IgnoreRule {
                    raw: glob_str,
                    negated,
                    dir_only,
                    anchored,
                    pattern,
                });
            }
            // Silently skip malformed patterns (consistent with git behaviour).
        }

        IgnoreRules { rules }
    }

    /// Return `true` if `rel_path` (relative to the package root, Unix separators)
    /// should be excluded from the archive.
    ///
    /// `is_dir` must be `true` when the path refers to a directory so that
    /// directory-only rules (`trailing /`) are applied correctly.
    pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool {
        // Built-in rules: match any path component against each builtin name.
        let basename = rel_path.rsplit('/').next().unwrap_or(rel_path);
        for builtin in Self::BUILTIN {
            if basename == *builtin {
                return true;
            }
        }

        let opts = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };

        let mut ignored = false;

        for rule in &self.rules {
            if rule.dir_only && !is_dir {
                continue;
            }
            if rule.pattern.matches_with(rel_path, opts) {
                ignored = !rule.negated;
            }
        }

        ignored
    }
}

pub struct ExtensionManager {
    root_dir: PathBuf,
    extensions_dir: PathBuf,
    config_file: PathBuf,
}

impl ExtensionManager {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            extensions_dir: root_dir.join("extensions"),
            config_file: root_dir.join("installed.json"),
            root_dir,
        }
    }

    // ── Locking ───────────────────────────────────────────────────────────────

    pub fn acquire_lock(&self) -> Result<std::fs::File> {
        let lock_path = self.root_dir.join("tmp").join("install.lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .context("Failed to open lock file")?;
        file.try_lock_exclusive()
            .map_err(|_| anyhow::anyhow!("Another installation is in progress"))?;
        Ok(file)
    }

    // ── Install ───────────────────────────────────────────────────────────────

    /// Install a `.xsil` tarball under `~/.extensilica/extensions/<name>/<version>/`.
    /// Validates the payload hash from `manifest.json` before moving to the final location.
    pub fn install_extension(
        &self,
        name: &str,
        version: &str,
        tarball_data: &[u8],
        force: bool,
    ) -> Result<()> {
        let install_path = self.extensions_dir.join(name).join(version);
        if install_path.exists() {
            if !force {
                bail!(
                    "{} v{} is already installed. Use --force to reinstall.",
                    name, version
                );
            }
            fs::remove_dir_all(&install_path)?;
        }

        // Extract to a temp dir first so we can validate before committing.
        let temp_dir = self
            .root_dir
            .join("tmp")
            .join(format!("install-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir)?;

        let tar = GzDecoder::new(tarball_data);
        let mut archive = Archive::new(tar);
        archive
            .unpack(&temp_dir)
            .context("Failed to unpack archive")?;

        let manifest_path = temp_dir.join("manifest.json");
        if !manifest_path.exists() {
            fs::remove_dir_all(&temp_dir).ok();
            bail!("Invalid package: manifest.json missing from archive root.");
        }

        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: Manifest = serde_json::from_str(&manifest_content)
            .context("manifest.json is not valid JSON.")?;

        // Semver validation.
        if Version::parse(&manifest.version).is_err() {
            fs::remove_dir_all(&temp_dir).ok();
            bail!("manifest version '{}' is not valid semver.", manifest.version);
        }

        // Payload hash validation.
        let expected_hash = manifest.effective_payload_hash();
        if !expected_hash.is_empty() {
            let calculated = self.compute_payload_hash(&temp_dir)?;
            if calculated != expected_hash {
                fs::remove_dir_all(&temp_dir).ok();
                bail!(
                    "Payload integrity check failed.\n  expected: {}\n  actual:   {}",
                    expected_hash,
                    calculated
                );
            }
            println!("{} Payload integrity verified.", "✔".green());
        }

        // Move to final install location.
        fs::create_dir_all(&install_path)?;
        for entry in fs::read_dir(&temp_dir)? {
            let entry = entry?;
            fs::rename(entry.path(), install_path.join(entry.file_name()))?;
        }
        fs::remove_dir_all(&temp_dir).ok();

        self.register_installed(name, version, &install_path)?;
        Ok(())
    }

    // ── Unpack + validate (for run/test) ──────────────────────────────────────

    /// Extract a `.xsil` tarball to a temporary directory and validate the payload hash.
    /// Returns `(temp_dir, manifest)`. Caller must remove temp_dir when done.
    pub fn extract_and_validate_xsil(
        &self,
        tarball_data: &[u8],
    ) -> Result<(PathBuf, Manifest)> {
        let temp_dir = self
            .root_dir
            .join("tmp")
            .join(format!("run-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir)?;

        let tar = GzDecoder::new(tarball_data);
        let mut archive = Archive::new(tar);
        archive
            .unpack(&temp_dir)
            .context("Failed to unpack archive")?;

        let manifest_path = temp_dir.join("manifest.json");
        if !manifest_path.exists() {
            fs::remove_dir_all(&temp_dir).ok();
            bail!("Package missing manifest.json");
        }

        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: Manifest =
            serde_json::from_str(&manifest_content).context("Invalid manifest.json")?;

        let expected_hash = manifest.effective_payload_hash();
        if !expected_hash.is_empty() {
            let calculated = self.compute_payload_hash(&temp_dir)?;
            if calculated != expected_hash {
                fs::remove_dir_all(&temp_dir).ok();
                bail!(
                    "Payload integrity failed: expected {} got {}",
                    expected_hash,
                    calculated
                );
            }
            println!("{} Payload integrity verified.", "✔".green());
        }

        Ok((temp_dir, manifest))
    }

    /// Validate an unpacked directory in-place (for `xsil run <dir>`).
    pub fn validate_local_package_directory(&self, dir: &Path) -> Result<(PathBuf, Manifest)> {
        let root = dir.canonicalize().context("Package directory not found")?;
        let manifest_path = root.join("manifest.json");
        if !manifest_path.is_file() {
            bail!("{} missing", manifest_path.display());
        }
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: Manifest =
            serde_json::from_str(&manifest_content).context("Invalid manifest.json")?;

        let expected_hash = manifest.effective_payload_hash();
        if !expected_hash.is_empty() {
            let calculated = self.compute_payload_hash(&root)?;
            if calculated != expected_hash {
                bail!(
                    "Payload integrity failed for {}: expected {} got {}. \
                     Regenerate manifest checksums or run from a built .xsil.",
                    root.display(),
                    expected_hash,
                    calculated
                );
            }
            println!("{} Payload integrity verified (local).", "✔".green());
        }

        Ok((root, manifest))
    }

    // ── Pack for publish ──────────────────────────────────────────────────────

    /// Pack a directory into a `.xsil` gzip-tar archive, honouring `.xsilignore`.
    ///
    /// Rules applied (in order):
    ///   1. Built-in exclusions: `.git`, `.xsilignore`, `.DS_Store`, `Thumbs.db`
    ///   2. User patterns from `.xsilignore` (gitignore semantics; last rule wins)
    ///
    /// Returns the raw archive bytes; does NOT modify the manifest.
    pub fn pack_directory(&self, dir: &Path) -> Result<Vec<u8>> {
        let manifest_path = dir.join("manifest.json");
        if !manifest_path.exists() {
            bail!("manifest.json not found in {}", dir.display());
        }

        let ignore = IgnoreRules::load(dir);
        let root = dir.canonicalize()?;

        // Collect all non-ignored files, sorted for reproducible archives.
        let mut files: Vec<PathBuf> = Vec::new();
        self.collect_files_filtered(&root, &root, &ignore, &mut files)?;
        files.sort();

        let mut buf = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);

            for abs_path in &files {
                let rel = abs_path
                    .strip_prefix(&root)
                    .context("file outside package root")?;
                tar.append_path_with_name(abs_path, rel)?;
            }

            tar.finish()?;
        }

        let skipped = {
            let mut all: Vec<PathBuf> = Vec::new();
            self.collect_files(&root, &mut all)?;
            all.len().saturating_sub(files.len())
        };
        if skipped > 0 {
            println!("  {} {} file(s) excluded by .xsilignore", "↓".dimmed(), skipped);
        }

        Ok(buf)
    }

    /// Compute the SHA-256 payload hash (all non-manifest, non-ignored files,
    /// sorted by relative path).  Respects `.xsilignore` when operating on a
    /// source directory so the hash matches what `pack_directory` would produce.
    pub fn compute_payload_hash(&self, dir: &Path) -> Result<String> {
        let ignore = IgnoreRules::load(dir);
        let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());

        let mut files: Vec<PathBuf> = Vec::new();
        self.collect_files_filtered(&root, &root, &ignore, &mut files)?;
        files.sort();

        let mut hasher = Sha256::new();
        for path in &files {
            // Skip the root-level manifest from the payload hash.
            if path.file_name().unwrap_or_default() == "manifest.json"
                && path.parent().map(|p| p.canonicalize().ok()) == Some(root.canonicalize().ok())
            {
                continue;
            }
            let mut file = fs::File::open(path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            hasher.update(&buffer);
        }

        Ok(hex::encode(hasher.finalize()))
    }

    /// Compute the SHA-256 of raw archive bytes.
    pub fn compute_archive_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Compute the total byte size of all non-manifest, non-ignored files.
    #[allow(dead_code)]
    pub fn compute_payload_size(&self, dir: &Path) -> Result<u64> {
        let ignore = IgnoreRules::load(dir);
        let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());

        let mut files: Vec<PathBuf> = Vec::new();
        self.collect_files_filtered(&root, &root, &ignore, &mut files)?;

        let mut size = 0u64;
        for path in &files {
            if path.file_name().unwrap_or_default() == "manifest.json"
                && path.parent().map(|p| p.canonicalize().ok()) == Some(root.canonicalize().ok())
            {
                continue;
            }
            size += fs::metadata(path)?.len();
        }
        Ok(size)
    }

    /// Recursive file collector that applies `IgnoreRules`.
    /// `root` is the package root (for computing relative paths).
    fn collect_files_filtered(
        &self,
        root: &Path,
        dir: &Path,
        ignore: &IgnoreRules,
        files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let is_dir = path.is_dir();

            // Compute Unix-style relative path for pattern matching.
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            if ignore.is_ignored(&rel, is_dir) {
                continue;
            }

            if is_dir {
                self.collect_files_filtered(root, &path, ignore, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }

    /// Unfiltered recursive file collector (used internally to report skipped counts).
    fn collect_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    self.collect_files(&path, files)?;
                } else {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Run a shell command with the package root as the working directory.
    #[allow(dead_code)]
    pub fn run_shell_in_package(&self, package_root: &Path, command: &str) -> Result<()> {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(package_root)
            .status()
            .context("Failed to run command")?;
        if !status.success() {
            bail!("Command exited with status: {:?}", status.code());
        }
        Ok(())
    }

    /// Run a shell command with an env overlay and PATH prefix list.
    pub fn run_shell_in_package_resolved(
        &self,
        package_root: &Path,
        command: &str,
        resolved: &ResolvedEnv,
        extra_env: &HashMap<String, String>,
    ) -> Result<()> {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(package_root);

        // Compute PATH with prefixes.
        let mut path_parts: Vec<String> = Vec::new();
        for p in &resolved.path_prefixes {
            path_parts.push(p.to_string_lossy().to_string());
        }
        if let Ok(cur) = std::env::var("PATH") {
            path_parts.push(cur);
        }
        let merged_path = path_parts
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(":");

        cmd.env("PATH", merged_path);
        for (k, v) in &resolved.vars {
            cmd.env(k, v);
        }

        // Apply execution.env (expanded).
        for (k, v) in extra_env {
            cmd.env(k, expand_env(v, &resolved.vars));
        }

        let status = cmd.status().context("Failed to run command")?;
        if !status.success() {
            bail!("Command exited with status: {:?}", status.code());
        }
        Ok(())
    }

    // ── Installed package registry ────────────────────────────────────────────

    pub fn get_installed_extension_path(&self, name: &str) -> Result<PathBuf> {
        let installed = self.load_installed_map()?;
        let ext = installed
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("'{}' is not installed.", name))?;
        Ok(PathBuf::from(&ext.path))
    }

    #[allow(dead_code)]
    pub fn list_installed(&self) -> Result<Vec<InstalledExtension>> {
        Ok(self.load_installed_map()?.into_values().collect())
    }

    #[allow(dead_code)]
    pub fn remove_extension(&self, name: &str) -> Result<()> {
        let mut map = self.load_installed_map()?;
        let path = map
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("'{}' is not installed.", name))
            .map(|ext| PathBuf::from(&ext.path))?;
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        map.remove(name);
        self.save_installed_map(&map)?;
        Ok(())
    }

    pub fn read_manifest(&self, path: &Path) -> Result<Manifest> {
        let content = fs::read_to_string(path.join("manifest.json"))
            .context("manifest.json not found")?;
        serde_json::from_str(&content).context("Invalid manifest.json")
    }

    fn register_installed(&self, name: &str, version: &str, path: &Path) -> Result<()> {
        let mut map = self.load_installed_map()?;
        map.insert(
            name.to_string(),
            InstalledExtension {
                name: name.to_string(),
                version: version.to_string(),
                installed_at: chrono::Local::now().to_rfc3339(),
                path: path.to_string_lossy().to_string(),
            },
        );
        self.save_installed_map(&map)
    }

    fn load_installed_map(&self) -> Result<HashMap<String, InstalledExtension>> {
        if !self.config_file.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&self.config_file)?;
        Ok(serde_json::from_str(&content).unwrap_or_default())
    }

    fn save_installed_map(&self, map: &HashMap<String, InstalledExtension>) -> Result<()> {
        fs::write(&self.config_file, serde_json::to_string_pretty(map)?)?;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_pkg(tmp: &Path, xsilignore: Option<&str>, extra_files: &[(&str, &str)]) {
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::create_dir_all(tmp.join("sim/bin")).unwrap();
        fs::create_dir_all(tmp.join(".git")).unwrap();

        fs::write(tmp.join("manifest.json"),
            r#"{"name":"t","version":"1.0.0","author":"x","description":"x","isa":"rv64gc","targets":{}}"#
        ).unwrap();
        fs::write(tmp.join("README.md"), "readme").unwrap();
        fs::write(tmp.join("src/hello.c"), "int main(){}").unwrap();
        fs::write(tmp.join("sim/bin/hello.elf"), b"ELF").unwrap();
        fs::write(tmp.join(".git/config"), "[core]").unwrap();
        fs::write(tmp.join(".DS_Store"), "mac junk").unwrap();
        fs::write(tmp.join("debug.log"), "log").unwrap();

        if let Some(content) = xsilignore {
            fs::write(tmp.join(".xsilignore"), content).unwrap();
        }
        for (rel, content) in extra_files {
            if let Some(parent) = Path::new(rel).parent() {
                fs::create_dir_all(tmp.join(parent)).unwrap();
            }
            fs::write(tmp.join(rel), content).unwrap();
        }
    }

    fn archive_members(archive_bytes: &[u8]) -> Vec<String> {
        let gz = GzDecoder::new(archive_bytes);
        let mut ar = Archive::new(gz);
        ar.entries().unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn builtin_exclusions_always_apply() {
        let tmp = tempfile::tempdir().unwrap();
        make_pkg(tmp.path(), None, &[]);
        let mgr = ExtensionManager::new(tmp.path().join("xsil-root"));
        let bytes = mgr.pack_directory(tmp.path()).unwrap();
        let members = archive_members(&bytes);
        assert!(!members.iter().any(|m| m.contains(".git")),    ".git must be excluded");
        assert!(!members.iter().any(|m| m.contains(".DS_Store")),".DS_Store must be excluded");
        assert!(!members.iter().any(|m| m.contains(".xsilignore")),".xsilignore itself must be excluded");
        assert!(members.iter().any(|m| m.contains("manifest.json")), "manifest.json must be included");
        assert!(members.iter().any(|m| m.contains("README.md")),     "README.md must be included");
    }

    #[test]
    fn xsilignore_excludes_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        make_pkg(tmp.path(), Some("sim/bin/\n*.log\n"), &[]);
        let mgr = ExtensionManager::new(tmp.path().join("xsil-root"));
        let bytes = mgr.pack_directory(tmp.path()).unwrap();
        let members = archive_members(&bytes);
        assert!(!members.iter().any(|m| m.contains("sim/bin")), "sim/bin/ must be excluded");
        assert!(!members.iter().any(|m| m.ends_with(".log")),   "*.log must be excluded");
        assert!(members.iter().any(|m| m.contains("src/hello.c")), "src/hello.c must be included");
    }

    #[test]
    fn negation_unignores_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_pkg(tmp.path(), Some("*.log\n!debug.log\n"), &[]);
        let mgr = ExtensionManager::new(tmp.path().join("xsil-root"));
        let bytes = mgr.pack_directory(tmp.path()).unwrap();
        let members = archive_members(&bytes);
        // debug.log is un-ignored by the ! rule
        assert!(members.iter().any(|m| m.contains("debug.log")), "debug.log must be un-ignored");
    }

    #[test]
    fn is_ignored_builtin() {
        let rules = IgnoreRules { rules: vec![] };
        assert!(rules.is_ignored(".git", true));
        assert!(rules.is_ignored(".DS_Store", false));
        assert!(rules.is_ignored(".xsilignore", false));
        assert!(!rules.is_ignored("README.md", false));
    }

    #[test]
    fn is_ignored_glob_pattern() {
        let rules = IgnoreRules::load(Path::new("/nonexistent-dir-for-test"));
        // Empty rules (file not found) — only builtins apply.
        assert!(!rules.is_ignored("src/hello.c", false));
    }

    /// Golden payload hash for `examples/rvx-demo` — keep in sync with store-backend `xsilPublishValidate`.
    #[test]
    fn rvx_demo_payload_hash_golden() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/rvx-demo");
        let root = root.canonicalize().expect("rvx-demo path");
        let mgr = ExtensionManager::new(std::env::temp_dir());
        let h = mgr.compute_payload_hash(&root).expect("hash");
        assert_eq!(
            h,
            "d84630cf41ca43dd9e06f151f6b2ed59ed54159c244b7f22dc953d59cccc5856"
        );
    }

    /// Documents the exact relative path order used for `compute_payload_hash` (PathBuf sort).
    #[test]
    fn rvx_demo_payload_hash_order_snapshot() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/rvx-demo");
        let root = root.canonicalize().expect("rvx-demo path");
        let mgr = ExtensionManager::new(std::env::temp_dir());
        let ignore = IgnoreRules::load(&root);
        let mut files: Vec<PathBuf> = Vec::new();
        mgr
            .collect_files_filtered(&root, &root, &ignore, &mut files)
            .unwrap();
        files.sort();
        let hashed_order: Vec<String> = files
            .iter()
            .filter(|p| {
                !(p.file_name().unwrap_or_default() == "manifest.json"
                    && p.parent().map(|x| x.canonicalize().ok()) == Some(root.canonicalize().ok()))
            })
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            hashed_order.join("\n"),
            "README.md\ndocs/overview.md\nsim/run.sh\nsim/spike.yaml\nsrc/hello.c\ntests/expected.txt\ntests/run.sh\ntoolchain/README.md"
        );
    }
}
