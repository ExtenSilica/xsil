# CLI Reference v3 — `xsil`

**Binary:** `xsil`  
**Language:** Rust  
**Date:** March 2026  
**Branch:** `pivot/npmjs-like-xsil-registry`  
**Related:** [publishing-flow-v3.md](./publishing-flow-v3.md), [spec/xsil.md](../spec/xsil.md)

This file is the **canonical** CLI reference for this repository.

---

## Overview

`xsil` is the command-line interface for the ExtenSilica registry. It handles everything from account authentication to publishing packages, installing them, and running or testing them locally.

```
xsil <command> [args] [--dry-run]
```

The global `--dry-run` flag is accepted by all commands. It performs all local validation steps (manifest parsing, checksum computation, packing) without executing, uploading, or writing to disk.

---

## Commands

### `xsil login`

Authenticate with the registry. Prompts for email and password (password is not echoed), calls `POST /auth/login`, and stores the returned API token in `~/.extensilica/config.json`.

```bash
xsil login
```

**Flow:**

```
Email: alice@example.com
Password: (hidden)
✔ Logged in as alice.
```

Each login generates a **new token** and invalidates the previous one. The token is stored locally and used automatically by all subsequent authenticated commands.

---

### `xsil logout`

Invalidate the current API token on the server (`POST /auth/logout`) and remove it from the local config file.

```bash
xsil logout
```

After logout, any command that requires authentication will fail until `xsil login` is run again.

---

### `xsil whoami`

Print the profile of the currently authenticated user.

```bash
xsil whoami
```

**Output:**

```
  Username : alice
  Email    : alice@example.com
  Bio      : RISC-V enthusiast
  Member since : 2026-03-10T12:00:00.000Z
```

Returns an error if not logged in.

---

### `xsil publish <path>`

Pack and publish a package to the registry. Requires authentication.

```bash
xsil publish ./my-extension/
xsil publish ./my-extension-1.0.0.xsil
xsil publish ./my-extension/ --changelog "Added vector load/store ops"
xsil publish ./my-extension/ --dry-run
```

**Accepted inputs:**

| Input | Behaviour |
|-------|-----------|
| Unpacked directory | CLI reads `manifest.json`, validates fields, packs into `.xsil`, computes checksums, uploads |
| Pre-built `.xsil` file | CLI extracts and validates `manifest.json` from the archive, computes checksums, uploads as-is |

**Local validation steps (always run, including `--dry-run`):**

1. Read and parse `manifest.json`.
2. Validate required fields: `name`, `version`, `description`, `author`, `entry`.
3. Validate `version` is valid semver.
4. Pack directory into `.xsil` gzip-tar (if input is a directory).
5. Compute `checksumPayload` — SHA-256 over all non-manifest files.
6. Compute `checksumArchive` — SHA-256 of the full `.xsil` archive.
7. Print summary of what would be uploaded.

**Upload (skipped on `--dry-run`):**

Calls `POST /packages/<name>/versions` with a multipart body containing the `.xsil` binary and all metadata fields.

**Example output:**

```
➤ Packing ./my-extension/...
✔ rvv-demo v1.0.0 (48320 bytes)
  checksumPayload : sha256:e3b0c44298fc1c...
  checksumArchive : sha256:9f86d081884c7d...
➤ Uploading to registry...
✔ Published: rvv-demo v1.0.0
  https://files.extensilica.com/rvv-demo/1.0.0.xsil
```

---

### `xsil install <package>`

Download and install a package under `~/.extensilica/extensions/<name>/<version>/`.

```bash
xsil install rvv-demo
xsil install rvv-demo@1.1.0
xsil install ./rvv-demo-1.0.0.xsil
xsil install rvv-demo --force
```

**Package argument formats:**

| Format | Example |
|--------|---------|
| Registry slug (latest) | `rvv-demo` |
| Slug at version | `rvv-demo@1.1.0` |
| Local `.xsil` file | `./rvv-demo-1.0.0.xsil` |

**Flags:**

| Flag | Behaviour |
|------|-----------|
| `--force` | Reinstall even if the version is already present; allow installing an older version |
| `--override-security` | Install yanked versions (not recommended) |
| `--dry-run` | Resolve and validate without installing |

**Integrity:** After downloading, `xsil install` unpacks the archive and recomputes the `payloadHash` from the extracted files. If it doesn't match the value in `manifest.json`, installation is aborted.

---

### `xsil run <package>`

Fetch (if needed), verify integrity, and execute the `entry` command from `manifest.json`.

```bash
xsil run rvv-demo
xsil run rvv-demo@1.1.0
xsil run ./rvv-demo-1.0.0.xsil
xsil run ./rvv-demo/
xsil run rvv-demo --dry-run
```

**Resolution order:**

1. If the argument is a directory with `manifest.json` → validate in place, execute `entry`.
2. If the argument is a `.xsil` file → unpack to temp, validate, execute, clean up.
3. Otherwise → treat as registry slug, fetch, download, unpack to temp, validate, execute, clean up.

**Integrity:** Same payload hash check as `install`. If hash verification fails, execution is refused.

**Execution:** Runs `entry` as a shell command with the package root as the working directory:

```bash
sh -c "<entry>"
```

Example: `"entry": "sim/run.sh"` → `sh -c sim/run.sh` in the package root.

---

### `xsil test <package>`

Fetch (if needed), verify integrity, and run the test suite.

```bash
xsil test rvv-demo
xsil test ./rvv-demo/
```

**Test entry resolution:**

1. If `testEntry` is set in `manifest.json`, use that.
2. Otherwise, fall back to `tests/run.sh` if the file exists.
3. If neither is available, exit with an error.

**Execution:** Same shell invocation as `run`, with the test entry point instead of `entry`.

---

### `xsil info <package>`

Display registry metadata and local install status for a package. Calls **`GET /packages/:slug`** and prints fields the registry stores per package and per version (including v0.2 **`resolutionMode`**, **`targets`**, **`toolchain`**, **`dependencies`**, and **`execution`** echoes from the published manifest).

```bash
xsil info rvv-demo
xsil info rvv-demo@1.1.0
xsil info @acme/cool-ext
```

**Package-level lines (when present in JSON):** `homepageUrl`, **`org`** (`@slug` + display name), **`totalDownloads`**, **`weeklyDownloads`**, plus name, slug, author, description, keywords, license, repository.

**Latest-version snapshot (no `@version` on the slug):** after the `Latest:` line, a block **── Latest version (v…) — registry metadata ──** includes **Readiness** (**`readinessLevel`** when present, otherwise inferred the same way as the webapp) plus a **Capabilities** checklist (Runnable / testEntry / repro modes / toolchain / simulators / RTL / FPGA / Tests). It then summarises **Resolution**, **Targets** (keys or array entries), **Toolchain** (one line; detects `external`), **Dependencies** (count of `tools` when JSON lists them), **`entry` / `testEntry`** when `execution` is valid JSON.

**Pinned version (`slug@ver`):** a **── Version … ──** section includes ISA, downloads, published date, the same **Readiness + Capabilities** block, the same reproducibility block, checksum prefix, changelog first line, yank state, and `xsil install slug@ver`.

**Output (illustrative):**

```
➤ Fetching info for rvv-demo...
  Name        : rvv-demo
  Slug        : rvv-demo
  Author      : alice
  Description : RISC-V Vector extension demo with Spike simulation.
  Keywords    : rvv, vector, spike
  License     : Apache-2.0
  Repository  : https://github.com/alice/rvv-demo
  Homepage    : https://example.com/rvv-demo
  Organization: @acme (ACME Labs)
  Downloads   : 1427
  Weekly dl   : 42
  Versions    : 3
  Latest      : 1.2.0

  ── Latest version (v1.2.0) — registry metadata ──
  Readiness   : RL4 — Testable
  Capabilities:
    ✓ Runnable (entry declared)
    ✓ Testable (testEntry declared)
    ✓ Repro: bundled
    ✗ Repro: resolved
    ✗ Repro: host-dependent
    ✓ Toolchain: bundled
    ✗ Toolchain: external
    ✓ Sim: Spike
    ✗ Emu: QEMU
    ✗ RTL
    ✗ FPGA
    ✓ Tests
  Resolution  : bundled — reproducible; no resolved tool downloads
  Targets     : spike
  Toolchain   : riscv64-unknown-elf (bundled)
  Dependencies: 2 tool(s) declared in manifest
  entry         : ./scripts/run.sh
  testEntry     : ./scripts/test.sh

  Available   :
    1.2.0 (RV64GCV — 842 downloads) (latest)
    1.1.0 (RV64GCV — 431 downloads)
    1.0.0 (RV64GCV — 154 downloads)
  Installed   : 1.1.0 at /home/alice/.extensilica/extensions/rvv-demo/1.1.0
```

Yanked versions are excluded from the "Available" list.

---

### `xsil search <query>`

Search the registry by name, description, or keyword.

```bash
xsil search rvv
xsil search "vector extension" --limit 20
```

**Flags:**

| Flag | Default | Behaviour |
|------|---------|-----------|
| `--limit` | `10` | Maximum number of results to display |

**Output:**

```
➤ Searching for "rvv"...
  rvv-demo 1.2.0 — RISC-V Vector extension demo with Spike simulation.
  rvv-int  0.9.1 — Integer vector operations demo.
```

---

## Package Argument Formats

All commands that accept a `<package>` argument support four input forms:

| Form | Example | Behaviour |
|------|---------|-----------|
| Registry slug | `rvv-demo` | Resolve latest non-yanked version from registry |
| Slug @ version | `rvv-demo@1.1.0` | Pin to exact version |
| Local `.xsil` file | `./rvv-demo-1.0.0.xsil` | Read from disk; no registry lookup |
| Unpacked directory | `./rvv-demo/` | Validate and run in place; no download |

---

## Config File

`~/.extensilica/config.json`

```json
{
  "registry": "https://api.extensilica.com",
  "token": "a3f8c2d1..."
}
```

| Key | Default | Description |
|-----|---------|-------------|
| `registry` | `http://localhost:3001` | Base URL of the registry API |
| `token` | `null` | API token from the last successful `xsil login` |

The file is created automatically on first login. To point the CLI at a self-hosted registry, edit `registry` directly.

---

## Install Directory

`~/.extensilica/extensions/<name>/<version>/`

Each installed package version gets its own directory. The manifest, simulation assets, toolchain, and tests are all preserved as-is from the archive.

---

## Integrity Model

Every `install`, `run`, and `test` command validates the **payload** hash (non-manifest files) before executing:

1. Compute SHA-256 over all non-manifest files (sorted lexicographically by path).
2. Compare with `manifest.checksums.payload` (v2) or `manifest.payloadHash` (v1).
3. If they differ → abort with an error.

**What this promises:** when validation succeeds, the **extracted package files** covered by that checksum match what the author published for that version—you do not get a silent substitution of those files. It does **not** imply every run is bitwise-identical across hosts or time (resolved toolchains, host-dependent resolution, OS caches, and anything outside the payload hash can still differ). If validation fails, the CLI aborts with an explicit integrity error.

For target selection and when `xsil run` may still fail after a good checksum, see [execution-model.md](./execution-model.md).

---

## What Was Removed

The following were present in previous CLI versions and are explicitly removed in v3:

| Removed | Reason |
|---------|--------|
| Ed25519 developer signatures | Organization trust chain from v1; not needed for an open registry |
| Certificate verification | Same; trust model was tied to licensing |
| Revocation list fetching | Was `/trust/revocations`; trust endpoint removed |
| `flash_command` / FPGA flash | Execution-model artifact from v2; FPGA targets remain optional metadata |
| License/payment checks | Monetization; removed entirely |
| `--override-security` for revoked keys | Replaced by `--override-security` for yanked versions only |
| `generate-keys`, `pack`, `sign` commands | Signing workflow removed; publish handles packing |
| `auth.json` (old token file) | Replaced by `config.json` (token + registry URL together) |
