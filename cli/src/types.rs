use serde::{Deserialize, Serialize};

// ── Manifest (in-package manifest.json) ──────────────────────────────────────

/// Contents of `manifest.json` at the root of a `.xsil` archive.
/// All fields that existed only for signing/licensing are removed.
#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub author: String,

    pub isa: Option<String>,

    /// Primary execution entry point, relative to package root.
    pub entry: Option<String>,

    /// Optional test entry point; default is `tests/run.sh` if present.
    #[serde(rename = "testEntry")]
    pub test_entry: Option<String>,

    /// v0.2 execution block (preferred): { entry, testEntry, env, ... }.
    pub execution: Option<serde_json::Value>,

    /// v0.2 dependencies block (preferred): { tools: [...], ... }.
    pub dependencies: Option<serde_json::Value>,

    /// v0.2 resolution block (preferred): { mode: "bundled"|"resolved"|"host-dependent", ... }.
    pub resolution: Option<serde_json::Value>,

    /// Toolchain descriptor — kept as a raw JSON value so any manifest shape is accepted.
    pub toolchain: Option<serde_json::Value>,

    /// Execution targets — kept as a raw JSON value (spike, qemu, fpga, etc.).
    pub targets: Option<serde_json::Value>,

    /// Search keywords for the registry.
    pub keywords: Option<Vec<String>>,

    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,

    /// SHA-256 of all non-manifest files (sorted path order). Used for integrity
    /// validation at install/run time. Accepts bare hex or "sha256:<hex>" prefix.
    #[serde(rename = "payloadHash", default)]
    pub payload_hash: String,

    /// Alternative field name from spec v2 checksums object. Preferred over payloadHash when present.
    pub checksums: Option<ManifestChecksums>,

    #[serde(rename = "payloadSize", default)]
    pub payload_size: u64,
}

impl Manifest {
    /// Returns the expected payload hash regardless of which field it was stored in.
    /// Prefers `checksums.payload` (v2) over `payloadHash` (v1).
    pub fn effective_payload_hash(&self) -> &str {
        if let Some(ref c) = self.checksums {
            if !c.payload.is_empty() {
                return c.payload.trim_start_matches("sha256:");
            }
        }
        self.payload_hash.trim_start_matches("sha256-").trim_start_matches("sha256:")
    }

    /// Entry command for `xsil run`, preferring v0.2 `execution.entry` over legacy `entry`.
    pub fn effective_entry(&self) -> Option<String> {
        if let Some(ref exec) = self.execution {
            if let Some(v) = exec.get("entry").and_then(|x| x.as_str()) {
                let t = v.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        self.entry
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Test entry command for `xsil test`, preferring v0.2 `execution.testEntry` over legacy `testEntry`.
    pub fn effective_test_entry(&self) -> Option<String> {
        if let Some(ref exec) = self.execution {
            if let Some(v) = exec.get("testEntry").and_then(|x| x.as_str()) {
                let t = v.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        self.test_entry
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ManifestChecksums {
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub archive: String,
}

// ── Registry API types ────────────────────────────────────────────────────────

/// Org scope echoed on `GET /packages/:slug` when the package belongs to an organization.
#[derive(Debug, Deserialize, Clone)]
pub struct RegistryOrgSummary {
    pub slug: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

/// Package metadata returned by `GET /packages/:slug`.
#[derive(Debug, Deserialize)]
pub struct RegistryPackage {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub description: String,
    #[serde(rename = "shortDescription")]
    pub short_description: Option<String>,
    pub author: String,
    pub keywords: Option<Vec<String>>,
    pub license: Option<String>,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: Option<String>,
    #[serde(rename = "homepageUrl")]
    pub homepage_url: Option<String>,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<String>,
    #[serde(rename = "totalDownloads", default)]
    pub total_downloads: u64,
    #[serde(rename = "weeklyDownloads", default)]
    pub weekly_downloads: u64,
    #[serde(default)]
    pub org: Option<RegistryOrgSummary>,
    pub versions: Vec<RegistryVersion>,
}

/// One version entry inside a `RegistryPackage`.
#[derive(Debug, Deserialize, Clone)]
pub struct RegistryVersion {
    pub version: String,
    #[serde(rename = "xsilUrl")]
    pub xsil_url: String,
    /// SHA-256 of the full archive (for transit verification).
    pub checksum: Option<String>,
    /// SHA-256 of non-manifest files (matches Manifest.payloadHash).
    #[serde(rename = "checksumPayload")]
    pub checksum_payload: Option<String>,
    pub isa: Option<String>,
    pub toolchain: Option<String>,
    pub targets: Option<String>,
    pub size: Option<u64>,
    #[serde(rename = "downloadCount", default)]
    pub download_count: u64,
    #[serde(rename = "isYanked", default)]
    pub is_yanked: bool,
    #[serde(rename = "yankReason")]
    pub yank_reason: Option<String>,
    pub changelog: Option<String>,
    #[serde(rename = "publishedAt")]
    pub published_at: Option<String>,
    /// JSON string of manifest `execution` (registry echo).
    pub execution: Option<String>,
    /// JSON string of manifest `dependencies` (registry echo).
    pub dependencies: Option<String>,
    /// Manifest `resolution.mode` value stored at publish time (`bundled`, `resolved`, …).
    #[serde(rename = "resolutionMode")]
    pub resolution_mode: Option<String>,
}

// ── Auth API types ────────────────────────────────────────────────────────────

/// User profile returned by `GET /auth/me`.
#[derive(Debug, Deserialize)]
pub struct UserProfile {
    pub id: u32,
    pub username: String,
    pub email: String,
    pub bio: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

// ── Local install state ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub name: String,
    pub version: String,
    pub installed_at: String,
    pub path: String,
}

