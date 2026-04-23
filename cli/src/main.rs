//! ExtenSilica CLI — publish, install, run, and test `.xsil` packages.

use clap::{Parser, Subcommand};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::PathBuf;
use anyhow::{bail, Context, Result};
use simplelog::*;
use std::fs::File;
use std::io::Read;
use semver::Version;

mod types;
mod registry;
mod manager;
mod init;
mod constants;

use registry::RegistryClient;
use manager::ExtensionManager;
use types::{Manifest, RegistryPackage, RegistryVersion};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "xsil",
    bin_name = "xsil",
    about = "ExtenSilica CLI — the package manager for .xsil packages",
    long_about = "Publish, install, run, and test .xsil packages from the ExtenSilica registry.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Validate and pack without uploading or executing anything.
    #[arg(long, global = true)]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate with the registry (stores an API token locally)
    Login,

    /// Invalidate the current API token and clear local credentials
    Logout,

    /// Print the currently authenticated user
    Whoami,

    /// Publish a package to the registry
    ///
    /// Accepts an unpacked directory (with manifest.json) or a pre-built .xsil file.
    Publish {
        /// Path to an unpacked package directory or a .xsil archive
        path: String,
        /// Override message included in the version entry
        #[arg(long, default_value = "")]
        changelog: String,
    },

    /// Create a new local package directory (manifest, sim/, tests/, docs/, toolchain stub)
    Init {
        /// Unscoped package slug: lowercase letters, digits, hyphens (also the directory name)
        name: String,
        /// Parent directory for `<name>/` (default: current working directory)
        #[arg(long, value_name = "DIR")]
        parent: Option<PathBuf>,
        /// Remove an existing `<parent>/<name>` directory before creating files
        #[arg(long)]
        force: bool,
        /// Value for `manifest.author` (default: `git config user.name`, else `your-username`)
        #[arg(long, value_name = "NAME")]
        author: Option<String>,
    },

    /// Download and install a package under ~/.extensilica/extensions/
    Install {
        /// Registry slug, slug@version, .xsil file path, or unpacked directory
        package: String,
        #[arg(long, help = "Reinstall even if this version is already present")]
        force: bool,
        #[arg(long, help = "Install yanked versions (not recommended)")]
        override_security: bool,
    },

    /// Fetch, verify, and execute the package entry point
    Run {
        /// Registry slug, slug@version, .xsil file path, or unpacked directory
        package: String,
    },

    /// Fetch, verify, and run the package test suite
    Test {
        /// Registry slug, slug@version, .xsil file path, or unpacked directory
        package: String,
    },

    /// Display registry metadata and local install status for a package
    Info {
        /// Registry slug or local .xsil path
        package: String,
    },

    /// Search the registry by name, description, or keyword
    Search {
        /// Search query
        query: String,
        #[arg(long, default_value = "10", help = "Maximum results to display")]
        limit: usize,
    },

    /// Yank a published version so it is excluded from default installs
    ///
    /// Yanked versions remain accessible with `xsil install <pkg>@<ver> --override-security`
    /// but are hidden from version resolution and marked in `xsil info` output.
    ///
    /// Examples:
    ///   xsil yank rvx-demo@1.0.0
    ///   xsil yank rvx-demo@1.0.0 --reason "Critical bug in sim/run.sh"
    ///   xsil yank @risc-v-labs/riscv-aes@1.0.0
    ///   xsil yank rvx-demo@1.0.0 --restore
    Yank {
        /// Package identifier including version: slug@x.y.z or @scope/pkg@x.y.z
        package_version: String,

        /// Short explanation shown in registry UI and `xsil info` output
        #[arg(long, short = 'r', value_name = "REASON")]
        reason: Option<String>,

        /// Restore (un-yank) the version instead of yanking it
        #[arg(long)]
        restore: bool,
    },
}

// ── Entry points ──────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {:#}", "error".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let (_config_file, _extensions_dir, root_dir) = setup_paths()?;

    let log_file = root_dir.join("logs").join("cli.log");
    let _ = WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        File::options()
            .create(true)
            .append(true)
            .open(log_file)
            .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap()),
    );

    let registry = RegistryClient::from_config();
    let manager = ExtensionManager::new(root_dir.clone());

    log::info!("CLI command: {:?}", cli.command);

    match &cli.command {
        // ── Auth commands ─────────────────────────────────────────────────────
        Commands::Login => {
            registry.login()?;
        }

        Commands::Logout => {
            registry.logout()?;
        }

        Commands::Whoami => {
            let user = registry.whoami()?;
            println!("  Username : {}", user.username.bold());
            println!("  Email    : {}", user.email);
            if let Some(bio) = &user.bio {
                if !bio.is_empty() {
                    println!("  Bio      : {}", bio);
                }
            }
            if let Some(created) = &user.created_at {
                println!("  Member since : {}", created);
            }
        }

        // ── Publish ───────────────────────────────────────────────────────────
        Commands::Publish { path, changelog } => {
            cmd_publish(&registry, &manager, path, changelog, cli.dry_run)?;
        }

        Commands::Init {
            name,
            parent,
            force,
            author,
        } => {
            if cli.dry_run {
                println!(
                    "{} Would create package skeleton at {}/{}",
                    "✔".green(),
                    parent
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| ".".to_string()),
                    name
                );
            } else {
                let created = init::cmd_init(&manager, name, parent.as_deref(), *force, author.as_deref())?;
                println!(
                    "{} Created package skeleton at {}",
                    "✔".green(),
                    created.display().to_string().bold()
                );
                println!("  {}", "Next:".dimmed());
                println!("    cd {}", created.display());
                println!("    xsil run .");
                println!("    xsil test .");
                println!("    xsil publish . --dry-run");
            }
        }

        // ── Install ───────────────────────────────────────────────────────────
        Commands::Install {
            package,
            force,
            override_security,
        } => {
            let _lock = manager.acquire_lock()?;
            cmd_install(&registry, &manager, package, *force, *override_security, cli.dry_run)?;
        }

        // ── Run ───────────────────────────────────────────────────────────────
        Commands::Run { package } => {
            let (work_dir, manifest, cleanup) = resolve_and_load(&registry, &manager, package)?;
            let entry = manifest
                .entry
                .as_ref()
                .context("manifest.json has no `entry` field (required for xsil run)")?;
            if cli.dry_run {
                println!("{} Dry run: would execute entry: {}", "✔".green(), entry);
            } else {
                println!("{} Running: {}", "➤".blue(), entry);
                manager.run_shell_in_package(&work_dir, entry)?;
                println!("{} Done.", "✔".green());
            }
            if cleanup {
                fs::remove_dir_all(&work_dir).ok();
            }
        }

        // ── Test ──────────────────────────────────────────────────────────────
        Commands::Test { package } => {
            let (work_dir, manifest, cleanup) = resolve_and_load(&registry, &manager, package)?;
            let test_cmd = if let Some(ref te) = manifest.test_entry {
                te.clone()
            } else if work_dir.join("tests/run.sh").is_file() {
                "tests/run.sh".to_string()
            } else {
                if cleanup {
                    fs::remove_dir_all(&work_dir).ok();
                }
                bail!("No test entry: set `testEntry` in manifest.json or add tests/run.sh");
            };
            if cli.dry_run {
                println!("{} Dry run: would run tests: {}", "✔".green(), test_cmd);
            } else {
                println!("{} Running tests: {}", "➤".blue(), test_cmd);
                manager.run_shell_in_package(&work_dir, &test_cmd)?;
                println!("{} Tests passed.", "✔".green());
            }
            if cleanup {
                fs::remove_dir_all(&work_dir).ok();
            }
        }

        // ── Info ──────────────────────────────────────────────────────────────
        Commands::Info { package } => {
            cmd_info(&registry, &manager, package)?;
        }

        // ── Search ────────────────────────────────────────────────────────────
        Commands::Search { query, limit } => {
            cmd_search(&registry, query, *limit)?;
        }

        // ── Yank ──────────────────────────────────────────────────────────────
        Commands::Yank { package_version, reason, restore } => {
            cmd_yank(&registry, package_version, reason.as_deref(), *restore)?;
        }
    }

    Ok(())
}

// ── Command implementations ───────────────────────────────────────────────────

fn cmd_publish(
    registry: &RegistryClient,
    manager: &ExtensionManager,
    path: &str,
    changelog: &str,
    dry_run: bool,
) -> Result<()> {
    let input = PathBuf::from(path);

    // Determine if input is a directory or an existing .xsil file.
    let (xsil_bytes, manifest) = if input.is_dir() {
        // Validate manifest.
        let manifest_path = input.join("manifest.json");
        if !manifest_path.exists() {
            bail!("manifest.json not found in {}", input.display());
        }
        let content = fs::read_to_string(&manifest_path)?;
        let manifest: Manifest = serde_json::from_str(&content)
            .context("manifest.json is not valid JSON")?;

        // Validate required fields.
        validate_publish_manifest(&manifest)?;

        println!("{} Packing {}...", "➤".blue(), input.display());
        let bytes = manager.pack_directory(&input)?;
        (bytes, manifest)
    } else if input.extension().map_or(false, |e| e == "xsil") {
        let bytes = fs::read(&input).context("Failed to read .xsil file")?;
        // Extract manifest from the archive.
        let manifest = extract_manifest_from_bytes(&bytes)?;
        validate_publish_manifest(&manifest)?;
        (bytes, manifest)
    } else {
        bail!("Expected a directory or a .xsil file, got: {}", path);
    };

    let slug = &manifest.name;
    let version = &manifest.version;
    let isa = manifest.isa.as_deref().unwrap_or("");
    let targets_json = manifest
        .targets
        .as_ref()
        .map(|t| t.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let toolchain = manifest
        .toolchain
        .as_ref()
        .map(|t| t.to_string())
        .unwrap_or_default();
    let keywords_csv = manifest
        .keywords
        .as_deref()
        .map(|kw| kw.join(","))
        .unwrap_or_default();

    // Compute checksums.
    let checksum_payload = manager.compute_payload_hash(PathBuf::from(path).as_path())
        .unwrap_or_else(|_| {
            // If input was a .xsil file we can't easily recompute from bytes here;
            // use the manifest value.
            manifest.effective_payload_hash().to_string()
        });
    let checksum_archive = manager.compute_archive_checksum(&xsil_bytes);
    let size = xsil_bytes.len() as u64;

    println!(
        "{} {} v{} ({} bytes)",
        "✔".green(),
        slug.bold(),
        version.cyan(),
        size
    );
    println!("  checksumPayload : sha256:{}", checksum_payload);
    println!("  checksumArchive : sha256:{}", checksum_archive);

    if dry_run {
        println!("{} Dry run — no upload performed.", "✔".green());
        return Ok(());
    }

    println!("{} Uploading to registry...", "➤".blue());
    let result = registry.publish(
        slug,
        version,
        changelog,
        isa,
        &targets_json,
        &toolchain,
        &keywords_csv,
        &format!("sha256:{}", checksum_payload),
        &format!("sha256:{}", checksum_archive),
        size,
        xsil_bytes,
    )?;

    let url = result
        .get("xsilUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("(registry)");

    println!(
        "{} Published: {} v{}\n  {}",
        "✔".green(),
        slug.bold(),
        version.cyan(),
        url
    );

    Ok(())
}

fn cmd_install(
    registry: &RegistryClient,
    manager: &ExtensionManager,
    package: &str,
    force: bool,
    override_security: bool,
    dry_run: bool,
) -> Result<()> {
    let path = PathBuf::from(package);

    // Local .xsil file.
    if path.is_file() && path.extension().map_or(false, |e| e == "xsil") {
        println!("{} Installing from file {}...", "➤".blue(), package);
        let bytes = fs::read(&path).context("Failed to read .xsil file")?;
        let manifest = extract_manifest_from_bytes(&bytes)?;
        if dry_run {
            println!(
                "{} Dry run: would install {} v{}",
                "✔".green(),
                manifest.name.bold(),
                manifest.version
            );
            return Ok(());
        }
        manager.install_extension(&manifest.name, &manifest.version, &bytes, force)?;
        println!(
            "{} Installed {} v{}",
            "✔".green(),
            manifest.name.bold(),
            manifest.version.cyan()
        );
        return Ok(());
    }

    // Resolve slug (with optional @version).
    let (slug, requested_version) = parse_package_arg(package);

    println!("{} Resolving {}...", "➤".blue(), slug.bold());
    let pkg = registry.get_package(&slug)?;

    let version = resolve_version(&pkg, requested_version.as_deref(), override_security)?;

    if version.is_yanked && !override_security {
        bail!(
            "Version {} of {} is yanked. Use --override-security to force.",
            version.version,
            slug
        );
    }
    if version.is_yanked {
        println!(
            "{} WARNING: installing yanked version {} (--override-security).",
            "!".red(),
            version.version
        );
    }

    // Downgrade check.
    if let Ok(installed_path) = manager.get_installed_extension_path(&slug) {
        if let Ok(m) = manager.read_manifest(&installed_path) {
            if let (Ok(installed_ver), Ok(target_ver)) = (
                Version::parse(&m.version),
                Version::parse(&version.version),
            ) {
                if target_ver < installed_ver && !force {
                    bail!(
                        "Would install older version ({} < {}). Use --force.",
                        target_ver, installed_ver
                    );
                }
            }
        }
    }

    if dry_run {
        println!(
            "{} Dry run: would install {} v{}",
            "✔".green(),
            slug.bold(),
            version.version
        );
        return Ok(());
    }

    let pb = progress_spinner("Downloading...");
    let bytes = registry.download_from_url(&version.xsil_url)?;
    pb.finish_with_message("Download complete");

    manager.install_extension(&slug, &version.version, &bytes, force)?;
    println!(
        "{} Installed {} v{}",
        "✔".green(),
        slug.bold(),
        version.version.cyan()
    );
    Ok(())
}

fn cmd_info(
    registry: &RegistryClient,
    manager: &ExtensionManager,
    package: &str,
) -> Result<()> {
    // Parse optional @version suffix (e.g. "rvx-demo@1.2.0" or "rvx-demo@latest").
    let (slug, requested_version) = parse_package_arg(package);

    println!("{} Fetching info for {}...", "➤".blue(), slug.bold());
    let pkg = registry.get_package(&slug)?;

    println!("  Name        : {}", pkg.name.bold());
    println!("  Slug        : {}", pkg.slug);
    println!("  Author      : {}", pkg.author);
    println!("  Description : {}", pkg.description);

    if let Some(ref kw) = pkg.keywords {
        if !kw.is_empty() {
            println!("  Keywords    : {}", kw.join(", "));
        }
    }
    if let Some(ref license) = pkg.license {
        println!("  License     : {}", license);
    }
    if let Some(ref repo) = pkg.repository_url {
        println!("  Repository  : {}", repo);
    }
    println!("  Downloads   : {}", pkg.total_downloads);
    println!("  Versions    : {}", pkg.versions.len());

    if let Some(ref latest) = pkg.latest_version {
        println!("  Latest      : {}", latest.cyan());
    } else if let Some(v) = pkg.versions.first() {
        println!("  Latest      : {}", v.version.cyan());
    }

    // If a specific version was requested, show its details.
    if let Some(ref ver_str) = requested_version {
        match resolve_version(&pkg, Some(ver_str.as_str()), false) {
            Ok(v) => {
                println!();
                println!("  ── Version {} ──", v.version.cyan().bold());
                let isa = v.isa.as_deref().unwrap_or("—");
                let dl = v.download_count;
                println!("  ISA         : {}", isa);
                println!("  Downloads   : {}", dl);
                println!("  Published   : {}", v.published_at.as_deref().unwrap_or("—"));
                if let Some(ref cs) = v.checksum {
                    println!("  Checksum    : {}", &cs[..cs.len().min(20)]);
                }
                let cl = v.changelog.as_deref().unwrap_or("");
                if let Some(first_line) = cl.lines().next().map(str::trim).filter(|s| !s.is_empty()) {
                    println!("  Changelog   : {}", first_line);
                }
                if v.is_yanked {
                    println!("  {} This version is yanked.", "⚠".yellow());
                    if let Some(ref reason) = v.yank_reason {
                        println!("  Reason      : {}", reason);
                    }
                }
                println!();
                println!("  Install     : xsil install {}@{}", pkg.slug, v.version);
            }
            Err(e) => {
                eprintln!("{} {}", "⚠".yellow(), e);
            }
        }
    }

    // Show non-yanked version list when no specific version was requested.
    if requested_version.is_none() {
        let active: Vec<&RegistryVersion> = pkg.versions.iter().filter(|v| !v.is_yanked).collect();
        if !active.is_empty() {
            println!("  Available   :");
            for v in &active {
                let dl = v.download_count;
                let isa = v.isa.as_deref().unwrap_or("?");
                let is_latest = pkg.latest_version.as_deref() == Some(v.version.as_str());
                let tag = if is_latest { " (latest)".green().to_string() } else { String::new() };
                println!("    {} ({}  — {} dl){}", v.version.cyan(), isa, dl, tag);
            }
        }
    }

    // Local install status.
    if let Ok(installed_path) = manager.get_installed_extension_path(&slug) {
        if let Ok(m) = manager.read_manifest(&installed_path) {
            println!("  Installed   : {} at {}", m.version.green(), installed_path.display());
        }
    }

    Ok(())
}

fn cmd_search(registry: &RegistryClient, query: &str, limit: usize) -> Result<()> {
    println!("{} Searching for \"{}\"...", "➤".blue(), query);
    let results = registry.search_packages(query)?;

    if results.is_empty() {
        println!("No packages found.");
        return Ok(());
    }

    let shown = results.iter().take(limit);
    for pkg in shown {
        let latest = pkg.latest_version.as_deref().unwrap_or("?");
        println!(
            "  {} {} — {}",
            pkg.slug.bold(),
            latest.cyan(),
            pkg.description
        );
    }

    let total = results.len();
    if total > limit {
        println!("  … and {} more. Use --limit to show more.", total - limit);
    }

    Ok(())
}

// ── Package resolution helpers ────────────────────────────────────────────────

/// Resolve and load a package workspace for run/test commands.
/// Returns (work_dir, manifest, needs_cleanup).
fn resolve_and_load(
    registry: &RegistryClient,
    manager: &ExtensionManager,
    package: &str,
) -> Result<(PathBuf, Manifest, bool)> {
    let path = PathBuf::from(package);

    // Unpacked local directory.
    if path.is_dir() {
        let (dir, manifest) = manager.validate_local_package_directory(&path)?;
        return Ok((dir, manifest, false));
    }

    // Local .xsil file.
    if path.is_file() && path.extension().map_or(false, |e| e == "xsil") {
        let bytes = fs::read(&path).context("Failed to read .xsil file")?;
        let (dir, manifest) = manager.extract_and_validate_xsil(&bytes)?;
        return Ok((dir, manifest, true));
    }

    // Registry slug.
    let (slug, requested_version) = parse_package_arg(package);

    println!("{} Resolving {}...", "➤".blue(), slug.bold());
    let pkg = registry.get_package(&slug)?;
    let version = resolve_version(&pkg, requested_version.as_deref(), false)?;

    if version.is_yanked {
        bail!(
            "Version {} is yanked. Use --override-security if you must.",
            version.version
        );
    }

    let pb = progress_spinner("Downloading...");
    let bytes = registry.download_from_url(&version.xsil_url)?;
    pb.finish_with_message("Download complete");

    println!("{} Validating integrity...", "➤".blue());
    let (dir, manifest) = manager.extract_and_validate_xsil(&bytes)?;
    Ok((dir, manifest, true))
}

/// Parse "slug" or "slug@version" into (slug, Option<version>).
///
/// Handles scoped packages correctly:
///   - `pkg`              → ("pkg",        None)
///   - `pkg@1.0.0`        → ("pkg",        Some("1.0.0"))
///   - `@scope/pkg`       → ("@scope/pkg", None)
///   - `@scope/pkg@1.0.0` → ("@scope/pkg", Some("1.0.0"))
fn parse_package_arg(package: &str) -> (String, Option<String>) {
    if let Some(rest) = package.strip_prefix('@') {
        // Scoped package — find the version separator *after* the slash
        // e.g. "scope/pkg@1.0.0" → slug="@scope/pkg", ver="1.0.0"
        if let Some(at_pos) = rest.find('@') {
            let slug = format!("@{}", &rest[..at_pos]);
            let ver  = rest[at_pos + 1..].to_string();
            (slug, Some(ver))
        } else {
            (package.to_string(), None)
        }
    } else {
        // Unscoped package
        match package.split_once('@') {
            Some((name, ver)) => (name.to_string(), Some(ver.to_string())),
            None => (package.to_string(), None),
        }
    }
}

/// Pick the correct version from registry metadata.
///
/// Version resolution rules:
/// - `None` or `"latest"` → highest non-yanked semver (backend already returns versions
///   in semver-descending order, so the first non-yanked entry is the latest)
/// - Any other string     → exact version match
fn resolve_version<'a>(
    pkg: &'a RegistryPackage,
    requested: Option<&str>,
    allow_yanked: bool,
) -> Result<&'a RegistryVersion> {
    match requested {
        None | Some("latest") => {
            // Prefer the latestVersion field the registry provides as an explicit tag.
            if let Some(ref tagged) = pkg.latest_version {
                if let Some(v) = pkg.versions.iter().find(|ver| ver.version == *tagged) {
                    return Ok(v);
                }
            }
            // Fall back: first non-yanked in the (semver-sorted) list.
            pkg.versions
                .iter()
                .find(|ver| !ver.is_yanked)
                .or_else(|| if allow_yanked { pkg.versions.first() } else { None })
                .with_context(|| format!("No installable versions found for {}", pkg.slug))
        }
        Some(v) => {
            pkg.versions
                .iter()
                .find(|ver| ver.version == v)
                .with_context(|| format!(
                    "Version '{}' not found for '{}'. Run `xsil info {}` to see available versions.",
                    v, pkg.slug, pkg.slug
                ))
        }
    }
}

/// Extract `manifest.json` from raw `.xsil` bytes without unpacking to disk.
fn extract_manifest_from_bytes(data: &[u8]) -> Result<Manifest> {
    let tar = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(tar);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.file_name().map_or(false, |n| n == "manifest.json")
            && path.components().count() == 1
        {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return serde_json::from_str(&content).context("Invalid manifest.json in archive");
        }
    }
    bail!("manifest.json not found in archive root");
}

/// Validate the fields required before publishing.
fn validate_publish_manifest(m: &Manifest) -> Result<()> {
    if m.name.is_empty() {
        bail!("manifest.name is required");
    }
    if m.version.is_empty() {
        bail!("manifest.version is required");
    }
    if Version::parse(&m.version).is_err() {
        bail!("manifest.version '{}' is not valid semver (e.g. 1.0.0)", m.version);
    }
    if m.description.is_empty() {
        bail!("manifest.description is required");
    }
    if m.author.is_empty() {
        bail!("manifest.author is required");
    }
    if m.entry.is_none() {
        bail!("manifest.entry is required");
    }
    Ok(())
}

fn cmd_yank(
    registry: &RegistryClient,
    package_version: &str,
    reason: Option<&str>,
    restore: bool,
) -> Result<()> {
    // Parse "pkg@x.y.z" or "@scope/pkg@x.y.z".
    let (slug, version_opt) = parse_package_arg(package_version);

    let version = version_opt.with_context(|| {
        format!(
            "Version is required: use <package>@<version> (e.g. {}@1.0.0)",
            slug
        )
    })?;

    if slug.is_empty() {
        bail!("Package name is required (e.g. rvx-demo@1.0.0)");
    }
    if Version::parse(&version).is_err() {
        bail!("'{}' is not a valid semver version (e.g. 1.0.0)", version);
    }

    if restore {
        println!("{} Restoring {}@{}...", "➤".blue(), slug.bold(), version.cyan());
    } else {
        println!("{} Yanking {}@{}...", "➤".blue(), slug.bold(), version.cyan());
        if let Some(r) = reason {
            println!("  Reason  : {}", r);
        }
    }

    let result = registry.yank_version(&slug, &version, !restore, reason)?;

    let is_yanked = result
        .get("isYanked")
        .and_then(|v| v.as_bool())
        .unwrap_or(!restore);

    let latest = result
        .get("latestVersion")
        .and_then(|v| v.as_str());

    if is_yanked {
        println!("{} Yanked {}@{}", "✔".green(), slug.bold(), version.cyan());
        if reason.is_some() {
            println!("  Reason  : {}", reason.unwrap_or(""));
        }
    } else {
        println!("{} Restored {}@{}", "✔".green(), slug.bold(), version.cyan());
    }

    match latest {
        Some(v) if !v.is_empty() => println!("  Latest  : {}", v.green()),
        _ => println!(
            "  {} All versions of {} are yanked — no installable version available.",
            "⚠".yellow(),
            slug.bold()
        ),
    }

    Ok(())
}

fn progress_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(msg.to_string());
    pb
}

// ── Path setup ────────────────────────────────────────────────────────────────

fn setup_paths() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let home = directories::UserDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Could not find user home directory"))?;
    let root = home.home_dir().join(".extensilica");
    let extensions = root.join("extensions");
    let config_file = root.join("config.json");
    let logs = root.join("logs");
    let tmp = root.join("tmp");

    for dir in &[&extensions, &logs, &tmp] {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
    }
    if !config_file.exists() {
        fs::write(&config_file, "{}")?;
    }

    Ok((config_file, extensions, root))
}
