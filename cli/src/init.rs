//! `xsil init` — scaffold a new local `.xsil` package directory.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::manager::ExtensionManager;
use crate::types::{Manifest, ManifestChecksums};

/// Names blocked for the same reasons as the registry (subset; see store-backend name policy).
const RESERVED_SLUGS: &[&str] = &[
    "xsil", "extensilica", "registry", "store", "admin", "root", "test", "demo", "example",
];

fn default_author() -> String {
    std::process::Command::new("git")
        .args(["config", "--get", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "your-username".to_string())
}

/// Unscoped slug: 2–64 chars, `a-z0-9` with inner hyphens, no leading/trailing hyphen.
fn validate_init_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        bail!("Package name cannot be empty.");
    }
    if slug.contains('@') || slug.contains('/') || slug.contains('\\') {
        bail!(
            "Use an unscoped slug (letters, digits, hyphens only). \
             For scoped packages (`@org/pkg`), create the tree with `xsil init my-pkg` and edit `manifest.json`."
        );
    }
    let b = slug.as_bytes();
    if b.len() < 2 || b.len() > 64 {
        bail!("Package name must be between 2 and 64 characters.");
    }
    if b[0] == b'-' || b[b.len() - 1] == b'-' {
        bail!("Package name must not start or end with a hyphen.");
    }
    for &ch in b {
        if !matches!(ch, b'a'..=b'z' | b'0'..=b'9' | b'-') {
            bail!(
                "Package name may only contain lowercase letters, digits, and hyphens (got invalid byte {}).",
                ch
            );
        }
    }
    if RESERVED_SLUGS.contains(&slug) {
        bail!("\"{}\" is reserved; choose another name.", slug);
    }
    Ok(())
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let m = fs::metadata(path)?.permissions().mode();
    fs::set_permissions(path, fs::Permissions::from_mode(m | 0o111))
        .with_context(|| format!("chmod +x {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

/// Create `parent/<slug>/` with a minimal runnable layout, then write `manifest.json` with a fresh payload checksum.
pub fn cmd_init(
    manager: &ExtensionManager,
    slug: &str,
    parent: Option<&Path>,
    force: bool,
    author: Option<&str>,
) -> Result<PathBuf> {
    validate_init_slug(slug)?;

    let base = parent
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let root = base.join(slug);

    if root.exists() {
        if !force {
            bail!(
                "{} already exists. Pass --force to remove it and create a fresh skeleton.",
                root.display()
            );
        }
        fs::remove_dir_all(&root).with_context(|| format!("remove {}", root.display()))?;
    }

    fs::create_dir_all(root.join("sim")).context("create sim/")?;
    fs::create_dir_all(root.join("tests")).context("create tests/")?;
    fs::create_dir_all(root.join("docs")).context("create docs/")?;
    fs::create_dir_all(root.join("toolchain")).context("create toolchain/")?;

    let author_str = author
        .map(String::from)
        .unwrap_or_else(default_author);

    write(
        &root.join("README.md"),
        &format!(
            r#"# {slug}

Skeleton package created with `xsil init`. Replace this README and wire up `sim/run.sh`.

## Quick commands

```bash
xsil run .
xsil test .
xsil publish . --dry-run
```
"#
        ),
    )?;

    write(
        &root.join("docs/overview.md"),
        &format!(
            r#"# {slug}

Describe your RISC-V extension, simulation setup, and how to reproduce results.
"#
        ),
    )?;

    write(
        &root.join("toolchain/README.md"),
        r#"# Toolchain

This skeleton does not bundle a compiler. Install a RISC-V cross-compiler (for example `riscv64-unknown-elf-gcc`) or document how obtain one, then point `manifest.json` `toolchain` at your layout.

If you keep binaries out of Git, list paths under `toolchain/bin/` in `.xsilignore` like the reference `examples/rvx-demo` package.
"#,
    )?;

    write(
        &root.join(".xsilignore"),
        r#"# Build artefacts (extend as needed)
sim/bin/

# Optional: exclude bundled toolchain binaries from the archive
toolchain/bin/
toolchain/lib/
toolchain/include/

# Editor / OS noise
.vscode/
.idea/
.DS_Store
Thumbs.db
*.tmp
*.log
"#,
    )?;

    let sim_run = format!(
        r#"#!/bin/sh
# Entry for `xsil run {slug}` — replace with your build + simulation flow.
set -e
printf '%s: Skeleton OK (no ELF built yet)\n' "{slug}"
printf 'ISA: RV64GC\n'
printf 'Edit sim/run.sh, tests/run.sh, and manifest.json as you iterate.\n'
"#,
        slug = slug
    );
    let sim_path = root.join("sim/run.sh");
    write(&sim_path, &sim_run)?;
    mark_executable(&sim_path)?;

    let tests_run = format!(
        r#"#!/bin/sh
# `xsil test {slug}` — tighten these checks as your package grows.
set -e
out=$(sh sim/run.sh 2>&1)
printf '%s\n' "$out" | grep -qF "{slug}: Skeleton OK"
printf '%s\n' "$out" | grep -qF "RV64GC"
printf 'All checks passed.\n'
"#,
        slug = slug
    );
    let tests_path = root.join("tests/run.sh");
    write(&tests_path, &tests_run)?;
    mark_executable(&tests_path)?;

    write(
        &root.join("sim/spike.yaml"),
        r#"isa: rv64gc
priv: m
mem: 128m
"#,
    )?;

    let hash = manager
        .compute_payload_hash(&root)
        .context("compute payload hash for new package")?;
    let size = manager
        .compute_payload_size(&root)
        .context("compute payload size for new package")?;

    let manifest = Manifest {
        name: slug.to_string(),
        version: "0.1.0".to_string(),
        description: format!("TODO: short description for {}", slug),
        author: author_str,
        isa: Some("RV64GC".to_string()),
        entry: Some("sh sim/run.sh".to_string()),
        test_entry: Some("sh tests/run.sh".to_string()),
        toolchain: Some(serde_json::json!({
            "root": "toolchain",
            "triple": "riscv64-unknown-elf",
            "external": true
        })),
        targets: Some(serde_json::json!({
            "spike": {
                "isa": "rv64gc",
                "priv": "m",
                "mem": "128m",
                "config": "sim/spike.yaml"
            }
        })),
        keywords: Some(vec![
            "riscv".to_string(),
            "xsil".to_string(),
            slug.to_string(),
        ]),
        license: Some("MIT".to_string()),
        repository: None,
        homepage: None,
        payload_hash: String::new(),
        checksums: Some(ManifestChecksums {
            payload: format!("sha256:{}", hash),
            archive: String::new(),
        }),
        payload_size: size,
    };

    let json = serde_json::to_string_pretty(&manifest).context("serialize manifest.json")?;
    write(&root.join("manifest.json"), &json)?;

    Ok(root)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::cmd_init;
    use crate::manager::ExtensionManager;

    #[test]
    fn scaffold_passes_local_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let root = cmd_init(&mgr, "init-test-pkg", Some(parent.as_path()), false, Some("tester"))
            .expect("cmd_init");

        assert!(root.join("manifest.json").is_file());
        let _ = mgr
            .validate_local_package_directory(&root)
            .expect("validate_local_package_directory");
    }
}
