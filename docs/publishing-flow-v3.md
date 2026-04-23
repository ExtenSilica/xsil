# Publishing Flow v3 — XSIL Package Registry

**Status:** Canonical definition  
**Date:** March 2026  
**Branch:** `pivot/npmjs-like-xsil-registry`  
**Related:** [auth-v3.md](./auth-v3.md), [data-model-v3.md](./data-model-v3.md), [spec/xsil.md](../spec/xsil.md)

---

## Overview

Publishing a `.xsil` package to the ExtenSilica registry is a five-step process:

```
1. Authenticate  →  2. Prepare package  →  3. xsil publish
     →  4. Backend validates + stores  →  5. Version visible on website
```

The CLI owns steps 1–3 (local), the backend owns steps 4–5 (server-side). The two communicate over a single authenticated HTTP multipart request.

---

## Step 1 — Authenticate

Before publishing, the user must hold a valid API token.

```bash
xsil login
```

The CLI prompts for `email` and `password`, calls `POST /auth/login`, and writes the returned token to `~/.extensilica/config.toml`:

```toml
registry = "https://api.extensilica.com"
token    = "a3f8c2d1..."
```

Subsequent `xsil publish` commands read the token from this file and send it as:

```
Authorization: Bearer <token>
```

If no token is stored, `xsil publish` exits with an error before making any network request:

```
error: not logged in. Run `xsil login` first.
```

---

## Step 2 — Prepare a Valid Package

A publishable package is either:

- An **unpacked directory** with `manifest.json` at the root (the CLI packs it into `.xsil` on publish), or
- A pre-built **`.xsil` archive** (the CLI inspects and publishes as-is).

### Required directory structure

```
my-extension/
├── manifest.json        ← required: identity + execution model
├── README.md            ← required: shown on package page
├── docs/                ← required (may contain placeholder files)
├── tests/               ← required (must include a runnable test entry)
├── sim/                 ← required (must include entry script)
└── toolchain/           ← required (self-contained compiler tree)
```

Optional:

```
├── bitstream/           ← only needed if targets.fpga is present
└── assets/              ← auxiliary files
```

### Minimal `manifest.json`

```json
{
  "name": "rvv-demo",
  "version": "1.0.0",
  "description": "RISC-V Vector extension demo with Spike simulation.",
  "author": "alice",
  "isa": "RV64GCV",
  "entry": "sim/run.sh",
  "toolchain": { "root": "toolchain", "triple": "riscv64-unknown-elf" },
  "targets": { "spike": {} }
}
```

`checksums` are computed by the CLI at publish time — you do not need to set them manually.

---

## Step 3 — `xsil publish`

```bash
xsil publish ./my-extension/
# or
xsil publish ./my-extension-1.0.0.xsil
```

The CLI executes the following local steps before any network request:

```
CLI (local)
 │
 ├─ 1. Read manifest.json
 │       Abort if missing or not valid JSON.
 │
 ├─ 2. Validate required manifest fields
 │       name, version, description, author, isa, entry, toolchain, targets
 │       Abort on any missing field.
 │
 ├─ 3. Validate package name
 │       Must match: [a-z0-9][a-z0-9._-]{0,213}
 │       Abort if invalid.
 │
 ├─ 4. Validate version
 │       Must be a valid semver string (e.g. "1.0.0", "2.3.0-beta.1").
 │       Abort if not parseable as semver.
 │
 ├─ 5. Verify required paths exist inside the package
 │       manifest.json, README.md, docs/, tests/, sim/, toolchain/
 │       Abort if any required path is missing.
 │
 ├─ 6. Pack directory → .xsil (if input was a directory)
 │       gzip-tar of all files under the directory.
 │
 ├─ 7. Compute checksums
 │       checksumPayload = SHA-256 over all non-manifest files (sorted path order)
 │       checksumArchive = SHA-256 of the full .xsil archive
 │       Inject both into the in-memory manifest before upload.
 │
 ├─ 8. Read token from ~/.extensilica/config.toml
 │       Abort if not found.
 │
 └─ 9. POST /packages/:slug/versions  (multipart)
         with .xsil archive + manifest fields
```

`--dry-run` executes steps 1–7 and prints what would be uploaded, then exits without sending any request.

---

## Step 4 — Backend Validates and Stores

The backend receives the multipart request and runs a sequential validation pipeline before committing anything to storage.

```
Backend (POST /packages/:slug/versions)
 │
 ├─ A. Authenticate
 │       Verify Authorization: Bearer token.
 │       401 if token missing or invalid.
 │
 ├─ B. Resolve package
 │       SELECT Extension WHERE slug = :slug
 │
 │       If package does not exist yet:
 │         Auto-create it from the manifest fields.
 │         Set ownerId = req.user.id.
 │
 │       If package exists:
 │         Verify Extension.ownerId === req.user.id
 │         OR req.user is in Maintainers for this package.
 │         403 if neither.
 │
 ├─ C. Validate version format
 │       semver.valid(version) must be non-null.
 │       400 "version must be a valid semantic version (e.g. 1.0.0)." if not.
 │
 ├─ D. Enforce version uniqueness
 │       SELECT Version WHERE extensionId = ? AND version = ?
 │       409 "version 1.0.0 already exists. Versions are immutable." if found.
 │       (Even a yanked version blocks republishing the same version string.)
 │
 ├─ E. Validate manifest presence
 │       Unpack archive in memory; confirm manifest.json exists at root.
 │       400 "manifest.json not found in archive root." if missing.
 │
 ├─ F. Validate required manifest fields
 │       name, version, description, author, isa, entry, toolchain, targets
 │       400 "manifest is missing required field: <field>." if any absent.
 │
 ├─ G. Validate name matches slug
 │       manifest.name must equal the :slug URL parameter.
 │       400 "manifest name '<n>' does not match package slug '<slug>'." if not.
 │
 ├─ H. Validate author matches account
 │       manifest.author must equal req.user.username.
 │       400 "manifest author '<a>' does not match authenticated user '<u>'." if not.
 │
 ├─ I. Verify archive checksum
 │       Recompute SHA-256 of the received archive bytes.
 │       Compare with checksumArchive from the request body.
 │       400 "archive checksum mismatch." if they differ.
 │       (Catches corruption in transit.)
 │
 ├─ J. Write blob to file store
 │       Store .xsil at /<slug>/<version>.xsil
 │       Construct xsilUrl from configured base URL.
 │
 ├─ K. INSERT Version record
 │       version, changelog, xsilUrl, checksum, checksumPayload,
 │       isa, targets, toolchain, size, publishedAt
 │
 ├─ L. UPDATE Extension
 │       SET latestVersion = <version> if this is semver-greater than current latestVersion
 │       SET updatedAt = now()
 │       (Other fields — totalDownloads, weeklyDownloads — updated by background job.)
 │
 └─ M. Return 201
         { version, slug, xsilUrl, publishedAt }
```

### Validation error table

| Step | HTTP status | Error message |
|------|-------------|---------------|
| A | 401 | `"Authentication required."` |
| B (owner check) | 403 | `"You do not have permission to publish to this package."` |
| C | 400 | `"version must be a valid semantic version (e.g. 1.0.0)."` |
| D | 409 | `"version <v> already exists. Versions are immutable."` |
| E | 400 | `"manifest.json not found in archive root."` |
| F | 400 | `"manifest is missing required field: <field>."` |
| G | 400 | `"manifest name '<n>' does not match package slug '<slug>'."` |
| H | 400 | `"manifest author '<a>' does not match authenticated user '<u>'."` |
| I | 400 | `"archive checksum mismatch."` |
| J (I/O error) | 500 | `"Failed to store package artifact."` |
| K (DB error) | 500 | `"Failed to register version."` |

---

## Step 5 — Version Visible on Website

Once the backend returns `201`, the version is immediately available:

```
GET /packages/rvv-demo
  → latestVersion: "1.0.0"
  → versions: [ { version: "1.0.0", publishedAt: "...", ... } ]

Website: extensilica.com/package/rvv-demo
  → shows new version in the version history table
  → "Install command" updated to latest version
  → README rendered from readmeContent stored in the Version record
```

There is no publish queue, no moderation step, and no delay. Packages are live immediately.

---

## Package Naming Rules

| Rule | Detail |
|------|--------|
| Character set | Lowercase letters, digits, hyphens, dots, underscores: `[a-z0-9][a-z0-9._-]*` |
| Length | 1–214 characters |
| Uniqueness | Globally unique within the registry. Two users cannot hold the same name. First publisher claims the name. |
| Scoped names | Optional: `@<org>/<name>` format for organization-owned packages |
| Immutability | The name is permanent once claimed. Renaming is not supported. |
| Reserved names | Names that are confusingly similar to `xsil`, `extensilica`, or existing names may be rejected. |

### Name validation regex

```
^(?:@([a-z0-9_-]+)\/)?([a-z0-9][a-z0-9._-]{0,213})$
```

Group 1 (optional): organization scope  
Group 2: package name

---

## Versioning Rules

| Rule | Detail |
|------|--------|
| Format | [Semantic Versioning 2.0.0](https://semver.org/): `MAJOR.MINOR.PATCH` with optional pre-release and build metadata |
| Examples | `1.0.0`, `2.3.0-beta.1`, `0.0.1`, `1.0.0-rc.2+build.5` |
| Monotonicity | Not enforced. Publishers may publish `1.1.0` after `2.0.0` (e.g. backport branch). |
| Immutability | A published version can never be overwritten. The same `(slug, version)` pair is permanently blocked after first publish. |
| Yank | A publisher may yank a version via `POST /packages/:slug/versions/:version/yank`. The version record is not deleted; the CLI refuses to install it by default. |
| Latest resolution | `GET /packages/:slug` returns the highest non-yanked semver as `latestVersion`. |

---

## Republishing Rejection

Republishing the same version is always rejected with `409 Conflict`, regardless of whether the content changed:

```json
{
  "error": "version 1.0.0 already exists. Versions are immutable."
}
```

This applies even to yanked versions. If a version has been yanked and the publisher wants to "fix" it, they must publish a new version (e.g. `1.0.1`).

**Rationale:** Mutable versions break reproducibility. Anyone who previously installed `rvv-demo@1.0.0` can rely on getting the same bytes forever.

---

## Package Page Version History

Every package page at `extensilica.com/package/<slug>` must display the full version history. This is sourced from `GET /packages/:slug/versions`.

### Version history table

| Column | Source |
|--------|--------|
| Version | `PackageVersion.version` |
| Published | `PackageVersion.publishedAt` |
| Downloads | `PackageVersion.downloadCount` |
| Status | `isYanked ? "yanked" : "latest" / "previous"` |
| Changelog | `PackageVersion.changelog` (first 120 chars truncated) |
| Download link | `PackageVersion.xsilUrl` (direct `.xsil` download) |

### Version status labels

| Condition | Label |
|-----------|-------|
| Highest semver, not yanked | `latest` |
| Not highest, not yanked | (no badge) |
| `isYanked = true` | `yanked` (greyed out, not installable by default) |

Yanked versions remain in the history table for auditability. They are never deleted from the database.

---

## Multipart Upload Format

```
POST /packages/:slug/versions
Content-Type: multipart/form-data
Authorization: Bearer <token>

Parts:
  file              .xsil binary (required)
  version           "1.0.0" (required)
  changelog         "Added vector load/store ops" (optional)
  isa               "RV64GCV" (required)
  targets           '{"spike":{}}' (required, JSON string)
  toolchain         "riscv64-unknown-elf-gcc 14.2.0" (optional)
  keywords          "rvv,vector,spike" (optional, CSV)
  checksumPayload   "sha256:abc123..." (required)
  checksumArchive   "sha256:def456..." (required)
  size              "48320" (required, integer bytes as string)
```

Response on success (`201 Created`):

```json
{
  "version": "1.0.0",
  "slug": "rvv-demo",
  "xsilUrl": "https://files.extensilica.com/rvv-demo/1.0.0.xsil",
  "publishedAt": "2026-03-10T12:00:00.000Z"
}
```

---

## Full Publish Sequence Diagram

```
Developer          CLI                    Backend              File Store
    │               │                        │                     │
    ├─ xsil login ──►                         │                     │
    │               ├─ POST /auth/login ──────►                     │
    │               ◄── { token } ────────────┤                     │
    │               ├─ write config.toml       │                     │
    │               │                        │                     │
    ├─ xsil publish ./my-ext/                  │                     │
    │               │                        │                     │
    │               ├─ read manifest.json     │                     │
    │               ├─ validate fields        │                     │
    │               ├─ validate semver        │                     │
    │               ├─ check required paths   │                     │
    │               ├─ pack → .xsil           │                     │
    │               ├─ compute checksums      │                     │
    │               │                        │                     │
    │               ├─ POST /packages/rvv-demo/versions ────────────►
    │               │   (multipart: .xsil + metadata)               │
    │               │                        │                     │
    │               │                        ├─ authenticate        │
    │               │                        ├─ resolve package     │
    │               │                        ├─ validate semver     │
    │               │                        ├─ check duplicate     │
    │               │                        ├─ unpack, check manifest
    │               │                        ├─ verify checksum     │
    │               │                        │                     │
    │               │                        ├─ write blob ─────────►
    │               │                        │                     ├─ store rvv-demo/1.0.0.xsil
    │               │                        ◄──── xsilUrl ─────────┤
    │               │                        │                     │
    │               │                        ├─ INSERT Version      │
    │               │                        ├─ UPDATE Extension    │
    │               │                        │  (latestVersion)     │
    │               │                        │                     │
    │               ◄── 201 { version, xsilUrl, publishedAt } ──────┤
    │               │                        │                     │
    │               ├─ print success URL      │                     │
    ◄───────────────┤                        │                     │
    │  https://extensilica.com/package/rvv-demo                     │
    │               │                        │                     │
    │   (website immediately shows new version)                     │
```

---

## Error Recovery

| Scenario | Behaviour |
|----------|-----------|
| Network failure during upload | CLI retries up to 3 times with exponential back-off. If all fail, local `.xsil` file is preserved; re-run `xsil publish` to try again. |
| Backend returns 5xx | CLI prints the error and preserves the local archive. No partial state is committed (blob write and DB insert are rolled back if the blob write succeeds but the DB insert fails). |
| Duplicate version (409) | CLI prints the error and exits. No retry. Publisher must bump the version in `manifest.json`. |
| Auth failure (401) | CLI prompts to run `xsil login` again. |
| Checksum mismatch (400) | CLI recomputes and reprints the checksum. Likely indicates a bug in the CLI's packing step; file an issue. |

---

## `--dry-run` Mode

```bash
xsil publish ./my-extension/ --dry-run
```

Executes all CLI-local steps (manifest validation, semver check, path verification, packing, checksum computation) and prints what would be sent to the server, then exits without making any network request.

Useful for:
- Verifying the manifest before the first real publish
- Checking that all required paths are present
- Reviewing the computed checksums

Sample output:

```
✓ manifest valid
✓ version: 1.0.0 (semver)
✓ required paths present
✓ packed: my-extension-1.0.0.xsil (48 320 bytes)
✓ checksumPayload: sha256:e3b0c44298fc1c149...
✓ checksumArchive: sha256:9f86d081884c7d659...

Would publish to: https://api.extensilica.com/packages/rvv-demo/versions
Dry run — no request sent.
```
