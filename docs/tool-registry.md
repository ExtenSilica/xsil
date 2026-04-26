# XSIL Tool Registry API (minimal shape)

This document defines a **minimal HTTP API** for hosting and resolving tool artifacts referenced by XSIL `manifest.json` `dependencies.tools[]`.

It is intentionally small: packages can still use external URLs, but the Tool Registry is the **default trusted source** and enables deduplication + caching across packages.

## Goals

- Provide immutable, versioned tool artifacts (`spike`, `qemu`, `riscv-llvm`, `verilator`, …).
- Support multiple platforms (`linux-x86_64`, `linux-aarch64`, `macos-aarch64`, …).
- Enable deterministic resolution: **no `latest`**, always pinned versions and **sha256 verification**.

## Concepts

- **Tool**: logical name (e.g. `spike`, `riscv-gnu-toolchain`, `spike-mojov`)
- **Version**: immutable version string (publisher-defined, must be pinned; e.g. `14.2.0-xsil.1`)
- **Platform**: runtime tuple (e.g. `linux-x86_64`)
- **Artifact**: downloadable archive containing a tool root directory (typically includes `bin/`)

## Endpoints (read-only)

### List tool versions

`GET /tools/<name>`

Response:

```json
{
  "name": "spike",
  "versions": ["1.1.1-xsil.3", "1.1.1-xsil.2"]
}
```

### Describe a tool version

`GET /tools/<name>/<version>`

Response:

```json
{
  "name": "spike",
  "version": "1.1.1-xsil.3",
  "platforms": ["linux-x86_64", "linux-aarch64", "macos-aarch64"]
}
```

### Get artifact metadata for a platform

`GET /tools/<name>/<version>/<platform>`

Response:

```json
{
  "name": "spike",
  "version": "1.1.1-xsil.3",
  "platform": "linux-x86_64",
  "url": "https://registry.extensilica.dev/tools/spike/1.1.1-xsil.3/spike-linux-x86_64.tar.zst",
  "sha256": "…",
  "contentType": "application/zstd",
  "format": "tar.zst"
}
```

### Download the artifact (optional convenience)

`GET /tools/<name>/<version>/<platform>/download`

- Either streams bytes directly **or** redirects (302) to the durable blob URL in the `url` field above.

## Normative rules for clients (xsil)

- Tools MUST be pinned by `name` + `version`.
- The client MUST verify `sha256` after download.
- The client MUST cache extracted tools keyed by `(name, version, platform, sha256)`.
- The client MUST NOT accept `version: "latest"` for tool resolution.

## Mapping into XSIL manifests

Packages should reference Tool Registry artifacts via `dependencies.tools[]`:

```json
{
  "resolution": { "mode": "resolved" },
  "dependencies": {
    "tools": [
      {
        "name": "spike",
        "version": "1.1.1-xsil.3",
        "platforms": {
          "linux-x86_64": {
            "url": "https://registry.extensilica.dev/tools/spike/1.1.1-xsil.3/spike-linux-x86_64.tar.zst",
            "sha256": "…"
          }
        }
      }
    ]
  }
}
```

