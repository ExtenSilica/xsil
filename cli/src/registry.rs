use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::collections::HashMap;
use colored::*;

use crate::types::{RegistryPackage, ResolveArtifactsResponse, UserProfile};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct Config {
    registry: Option<String>,
    token: Option<String>,
}

fn config_path() -> PathBuf {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(crate::constants::CONFIG_RELATIVE_PATH)
}

fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&content) {
                return cfg;
            }
        }
    }
    Config::default()
}

fn save_config(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}

// ── Client ────────────────────────────────────────────────────────────────────

pub struct RegistryClient {
    base_url: String,
    client: Client,
}

impl RegistryClient {
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .no_gzip()
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url: base_url.to_string(),
            client,
        }
    }

    /// Build a client from the stored config, falling back to the default registry URL.
    pub fn from_config() -> Self {
        let cfg = load_config();
        let url = cfg.registry
            .unwrap_or_else(|| crate::constants::DEFAULT_REGISTRY.to_string());
        Self::new(&url)
    }

    fn load_token(&self) -> Option<String> {
        load_config().token
    }

    fn auth_header(&self) -> Option<String> {
        self.load_token().map(|t| format!("Bearer {}", t))
    }

    fn dependency_key(name: &str, version: &str, platform: &str, sha256: &str) -> String {
        let sha = sha256
            .trim()
            .trim_start_matches("sha256:")
            .trim_start_matches("sha256-")
            .to_ascii_lowercase();
        format!("{}::{}::{}::{}", name.trim(), version.trim(), platform.trim(), sha)
    }

    // ── Auth endpoints ────────────────────────────────────────────────────────

    /// Interactive login: prompts for email + password, calls POST /auth/login,
    /// and stores the returned token in the config file.
    pub fn login(&self) -> Result<()> {
        print!("Email: ");
        std::io::stdout().flush()?;
        let mut email = String::new();
        std::io::stdin().read_line(&mut email)?;
        let email = email.trim().to_string();

        print!("Password: ");
        std::io::stdout().flush()?;
        // Read password without echoing it to the terminal.
        let password = rpassword::read_password().context("Failed to read password")?;

        let body = serde_json::json!({ "email": email, "password": password });
        let resp = self
            .client
            .post(format!("{}/auth/login", self.base_url))
            .json(&body)
            .send()
            .context("Failed to reach the registry")?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().context("Invalid response from registry")?;

        if !status.is_success() {
            bail!(
                "Login failed: {}",
                json.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error")
            );
        }

        let token = json
            .get("token")
            .and_then(|v| v.as_str())
            .context("No token in login response")?
            .to_string();

        let username = json
            .pointer("/user/username")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)");

        let mut cfg = load_config();
        cfg.token = Some(token);
        save_config(&cfg)?;

        println!("{} Logged in as {}.", "✔".green(), username.bold());
        Ok(())
    }

    /// Call POST /auth/logout to invalidate the current token, then clear it locally.
    pub fn logout(&self) -> Result<()> {
        let token = self
            .load_token()
            .context("Not logged in. Nothing to do.")?;

        let resp = self
            .client
            .post(format!("{}/auth/logout", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .context("Failed to reach the registry")?;

        if !resp.status().is_success() {
            // Token may already be invalid; still clear it locally.
            eprintln!("{} Registry returned {}; clearing token anyway.", "!".yellow(), resp.status());
        }

        let mut cfg = load_config();
        cfg.token = None;
        save_config(&cfg)?;

        println!("{} Logged out.", "✔".green());
        Ok(())
    }

    /// Call GET /auth/me and return the authenticated user's profile.
    pub fn whoami(&self) -> Result<UserProfile> {
        let auth = self
            .auth_header()
            .context("Not logged in. Run `xsil login` first.")?;

        let resp = self
            .client
            .get(format!("{}/auth/me", self.base_url))
            .header("Authorization", auth)
            .send()
            .context("Failed to reach the registry")?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().context("Invalid response")?;

        if !status.is_success() {
            bail!(
                "{}",
                json.get("error").and_then(|v| v.as_str()).unwrap_or("Not authenticated")
            );
        }

        let user: UserProfile = serde_json::from_value(
            json.get("user").cloned().unwrap_or(json),
        )
        .context("Failed to parse user profile")?;

        Ok(user)
    }

    // ── Package publish ───────────────────────────────────────────────────────

    /// Upload a `.xsil` archive to the registry as a new package version.
    pub fn publish(
        &self,
        slug: &str,
        version: &str,
        changelog: &str,
        isa: &str,
        targets_json: &str,
        toolchain: &str,
        keywords_csv: &str,
        checksum_payload: &str,
        checksum_archive: &str,
        size: u64,
        xsil_bytes: Vec<u8>,
    ) -> Result<serde_json::Value> {
        let auth = self
            .auth_header()
            .context("Not logged in. Run `xsil login` first.")?;

        let file_part = reqwest::blocking::multipart::Part::bytes(xsil_bytes)
            .file_name(format!("{}-{}.xsil", slug, version))
            .mime_str("application/octet-stream")?;

        let form = reqwest::blocking::multipart::Form::new()
            .part("file", file_part)
            .text("version", version.to_string())
            .text("changelog", changelog.to_string())
            .text("isa", isa.to_string())
            .text("targets", targets_json.to_string())
            .text("toolchain", toolchain.to_string())
            .text("keywords", keywords_csv.to_string())
            .text("checksumPayload", checksum_payload.to_string())
            .text("checksumArchive", checksum_archive.to_string())
            .text("size", size.to_string());

        let resp = self
            .client
            .post(format!("{}/packages/{}/versions", self.base_url, slug))
            .header("Authorization", auth)
            .multipart(form)
            .send()
            .context("Failed to reach the registry")?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().context("Invalid response from registry")?;

        if !status.is_success() {
            bail!(
                "Publish failed ({}): {}",
                status,
                json.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error")
            );
        }

        Ok(json)
    }

    // ── Package search / fetch ────────────────────────────────────────────────

    /// Search the registry. Empty query returns all packages.
    pub fn search_packages(&self, query: &str) -> Result<Vec<RegistryPackage>> {
        let url = if query.trim().is_empty() {
            format!("{}/packages", self.base_url)
        } else {
            format!(
                "{}/packages?q={}",
                self.base_url,
                urlencoding::encode(query)
            )
        };
        let resp = self
            .client
            .get(&url)
            .send()
            .context("Failed to connect to registry")?;
        if !resp.status().is_success() {
            bail!("Search failed: {}", resp.status());
        }
        let list: Vec<RegistryPackage> =
            resp.json().context("Failed to parse search results")?;
        Ok(list)
    }


    /// Fetch metadata for a single package by slug.
    pub fn get_package(&self, slug: &str) -> Result<RegistryPackage> {
        let url = format!("{}/packages/{}", self.base_url, slug);
        let resp = self
            .client
            .get(&url)
            .send()
            .context("Failed to connect to registry")?;
        if resp.status().as_u16() == 404 {
            bail!("Package '{}' not found in the registry.", slug);
        }
        if !resp.status().is_success() {
            bail!("Registry error: {}", resp.status());
        }
        let pkg: RegistryPackage = resp.json().context("Failed to parse package metadata")?;
        Ok(pkg)
    }


    /// Call `PATCH /packages/:slug/versions/:version` to yank or restore a version.
    ///
    /// Scoped slugs (`@scope/pkg`) are passed verbatim; the backend route
    /// handles `/@:scope/:name/versions/:version` transparently.
    pub fn yank_version(
        &self,
        slug: &str,
        version: &str,
        yanked: bool,
        reason: Option<&str>,
    ) -> Result<serde_json::Value> {
        let token = self
            .load_token()
            .context("Not logged in. Run `xsil login` first.")?;

        let url = format!("{}/packages/{}/versions/{}", self.base_url, slug, version);

        let mut body = serde_json::json!({ "yanked": yanked });
        if let Some(r) = reason {
            body["reason"] = serde_json::Value::String(r.to_string());
        }

        let resp = self
            .client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .context("Failed to reach the registry")?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().context("Invalid response from registry")?;

        if !status.is_success() {
            bail!(
                "{}",
                json.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown registry error")
            );
        }

        Ok(json)
    }

    /// Download a file from an arbitrary URL, attaching the Bearer token if available.
    pub fn download_from_url(&self, url: &str) -> Result<Vec<u8>> {
        let mut req = self.client.get(url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let mut resp = req.send().context("Failed to download file")?;
        if !resp.status().is_success() {
            bail!("Download failed: {}", resp.status());
        }
        let mut buffer = Vec::new();
        resp.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    /// Resolve resolved-mode dependency URLs through the backend so auth/policy is enforced.
    /// Returns a map keyed by "<name>::<version>::<platform>::<sha256>".
    pub fn resolve_artifacts(
        &self,
        dependencies: &serde_json::Value,
    ) -> Result<HashMap<String, String>> {
        let auth = self
            .auth_header()
            .context("Not logged in. Run `xsil login` first to resolve dependency artifacts.")?;

        let body = serde_json::json!({ "dependencies": dependencies });
        let resp = self
            .client
            .post(format!("{}/api/artifacts/resolve", self.base_url))
            .header("Authorization", auth)
            .json(&body)
            .send()
            .context("Failed to reach artifact resolver endpoint")?;

        let status = resp.status();
        let json: serde_json::Value = resp
            .json()
            .context("Invalid response from artifact resolver")?;
        if !status.is_success() {
            bail!(
                "Artifact resolve failed ({}): {}",
                status,
                json.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
            );
        }

        let parsed: ResolveArtifactsResponse = serde_json::from_value(json)
            .context("Failed to parse resolved artifacts response")?;
        if !parsed.missing.is_empty() {
            let sample = parsed
                .missing
                .iter()
                .take(3)
                .map(|m| format!("{}@{} [{}]", m.name, m.version, m.platform))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "Missing dependency artifacts in ExtenSilica registry: {}{}",
                sample,
                if parsed.missing.len() > 3 { "..." } else { "" }
            );
        }

        let mut out = HashMap::new();
        for r in parsed.resolved {
            let key = Self::dependency_key(&r.name, &r.version, &r.platform, &r.sha256);
            out.insert(key, r.url);
        }
        Ok(out)
    }
}

