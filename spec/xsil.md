# XSIL Package Format Specification v0.2 — Self-Resolving Packages

**Status:** Draft  
**Version:** 0.2  
**Last Updated:** April 2026

---

## 1. Definition

A **`.xsil` file** is a **versioned, self-describing package** for publishing and running reproducible RISC-V implementations.

A package may include simulation assets, tests, documentation, and optionally FPGA bitstreams and RTL sources.

**v0.2 focus:** packages are **self-resolving**. A package does not need to bundle every dependency inside the archive, but it **must declare every dependency required to run**, and the XSIL runtime/CLI must resolve those dependencies automatically (download, verify by hash, cache, execute) without manual user setup.

---

## 2. File Format

| Property | Value |
|----------|-------|
| Extension | `.xsil` |
| Encoding | GZIP-compressed tar archive (same bytes as `.tar.gz`, renamed) |
| Root entry | `manifest.json` must exist at the archive root |

---

## 3. Archive Layout

The following paths are **normative**. Omit empty directories. Content under each directory is publisher-defined; all paths referenced from `manifest.json` must resolve inside the archive.

```
manifest.json          (required) Package manifest
README.md              (required) Human-readable overview
docs/                  (required) Extended documentation
tests/                 (required) Test suite and vectors
sim/                   (required) Simulation scripts and assets
toolchain/             (required) Self-contained toolchain (compiler, headers, specs)
rtl/                   (optional) RTL sources, build scripts, and integration collateral (see §5.3)
bitstream/             (optional) FPGA bitstreams; only required when targets.fpga is present
assets/                (optional) Auxiliary files (data, waveforms, examples)
```

### Example

```
manifest.json
README.md
docs/isa-notes.md
sim/run.sh
sim/spike.yaml
tests/run.sh
tests/vectors/
toolchain/bin/riscv64-unknown-elf-gcc
bitstream/my-target.bit
```

---

## 4. Manifest (`manifest.json`)

The manifest is a UTF-8 JSON object at the archive root. It is the authoritative description of the package's identity, execution model, and integrity.

### 4.1 Required fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Registry slug. Lowercase alphanumeric and hyphens. Stable across versions. Unique within the registry. |
| `version` | string | [Semantic Versioning 2.0.0](https://semver.org/) (`MAJOR.MINOR.PATCH`). Each published version is immutable. |
| `description` | string | Short human-readable description (plain text, ≤ 280 characters). |
| `author` | string | Publisher name or username. Must match the authenticated registry user for publication. |
| `isa` | string | RISC-V ISA string (e.g. `RV64GCV`, `RV32IMAC_Xcustom`). |
| `execution` | object | Execution block for `xsil run` / `xsil test` (see §4.4). |
| `toolchain` | object | Describes the bundled toolchain (see §6). |
| `targets` | object | Supported execution backends (see §5). At least one target must be declared. |
| `checksums` | object | Integrity digests (see §7). |
| `resolution` | object | Reproducibility policy (bundled/resolved/host-dependent) (see §4.5). |

### 4.2 Optional fields

| Field | Type | Description |
|-------|------|-------------|
| `license` | string | [SPDX license identifier](https://spdx.org/licenses/) (e.g. `Apache-2.0`, `MIT`). |
| `repository` | string | URL of the source repository. |
| `homepage` | string | URL of the project homepage or registry page. |
| `standardStatus` | string | Honest classification of the extension's relationship to the RISC-V standard (see §4.7). One of: `ratified`, `draft`, `vendor`, `research`, `custom`. Required by the registry from spec v2.1; tooling SHOULD warn when missing. |
| `authority` | string | Free-text identifier of the body that defines the extension (e.g. `RISC-V International`, `T-Head / XuanTie`, `University of Michigan`). Length 2–200 characters. Required alongside `standardStatus`. |
| `keywords` | array of string | Search tags. Lowercase, no spaces. Shown on the registry package page. |
| `readme` | string | Path to the README file inside the archive (default: `README.md`). |
| `testEntry` | string | **Legacy**. Deprecated by `execution.testEntry`. |
| `payloadHash` | string | Legacy field: SHA-256 digest of all non-manifest files concatenated in sorted path order (`sha256:<hex>`). Superseded by `checksums.payload` in v2.0; both may be present for compatibility. |
| `payloadSize` | number | Total byte size of non-manifest files. Used by the CLI progress display. |
| `dependencies` | object | Dependency declarations used when `resolution.mode` is `resolved` (see §4.6). |

### 4.3 Full example (resolved)

```json
{
  "name": "rvv-demo",
  "version": "1.2.0",
  "description": "RISC-V Vector extension demo with Spike simulation and test suite.",
  "author": "alice",
  "license": "Apache-2.0",
  "repository": "https://github.com/alice/rvv-demo",
  "homepage": "https://extensilica.com/package/rvv-demo",
  "standardStatus": "ratified",
  "authority": "RISC-V International",
  "keywords": ["rvv", "vector", "simulation", "spike"],
  "isa": "RV64GCV",
  "execution": {
    "entry": "sh sim/run.sh",
    "testEntry": "sh tests/run.sh",
    "env": {
      "PATH": "$XSIL_TOOLCHAIN_ROOT/bin:$XSIL_SPIKE_ROOT/bin:$PATH"
    }
  },
  "readme": "README.md",
  "toolchain": {
    "root": "toolchain",
    "version": "14.2.0",
    "triple": "riscv64-unknown-elf"
  },
  "resolution": {
    "mode": "resolved"
  },
  "dependencies": {
    "tools": [
      {
        "name": "spike",
        "version": "1.1.1-xsil.3",
        "platforms": {
          "linux-x86_64": {
            "url": "https://registry.extensilica.dev/tools/spike/1.1.1-xsil.3/spike-linux-x86_64.tar.zst",
            "sha256": "..."
          }
        }
      }
    ]
  },
  "targets": {
    "spike": { "config": "sim/spike.yaml" },
    "qemu":  { "machine": "virt", "cpu": "rv64,v=true" },
    "fpga":  { "id": "my-target", "bitstream": "bitstream/my-target.bit" }
  },
  "checksums": {
    "payload": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "archive": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
  },
  "payloadSize": 48320
}
```

### 4.4 Execution (v0.2)

`execution` defines how `xsil run` and `xsil test` launch a package after unpack and dependency resolution.

| Field | Required | Type | Meaning |
|-------|----------|------|---------|
| `entry` | Yes (for runnable packages) | string | Command or script path to run. |
| `testEntry` | No | string | Command or script path for tests. |
| `workdir` | No | string | Working directory relative to package root (default: `.`). |
| `env` | No | object | Environment variables. Supports `$XSIL_*_ROOT` expansion by the runtime. |

**Backwards compatibility:** legacy top-level `entry` / `testEntry` MAY be present for older tooling. When both are present, `execution.*` takes precedence.

### 4.5 Resolution (v0.2)

`resolution` declares the reproducibility policy for the package.

| Mode | Meaning |
|------|---------|
| `bundled` | Everything needed to run is inside the `.xsil` archive. |
| `resolved` | Dependencies are declared and are resolved automatically by the runtime (download + hash verify + cache). |
| `host-dependent` | Depends on pre-installed host tools. Allowed, but not eligible for runnable/reproducible badges. |

`resolution.mode` MUST NOT be `latest` or any moving target.

### 4.6 Dependencies (v0.2)

When `resolution.mode` is `resolved`, the manifest SHOULD include `dependencies` so the runtime can resolve tools automatically.

#### `dependencies.tools[]`

Each entry declares a tool artifact pinned by version and verified by hash per platform:

| Field | Required | Meaning |
|-------|----------|---------|
| `name` | Yes | Tool identifier (publisher-defined). |
| `version` | Yes | Pinned version string (MUST NOT be `latest`). |
| `platforms` | Yes | Map of platform → artifact descriptor. |

Each platform artifact MUST include:

- `url` (string)
- `sha256` (string) — required; URLs without hashes MUST be rejected.

Tool Registry is the default trusted source, but external URLs are permitted only with hashes and SHOULD be displayed with a warning in UIs.

### 4.7 Standard status (v2.1)

`standardStatus` and `authority` express the package's honest relationship to the RISC-V standard. Both fields are stored at the top level of `manifest.json`, and the registry persists them on the package record so consumers can filter and audit the catalog.

| Value | Meaning | Typical authority |
|-------|---------|-------------------|
| `ratified` | Frozen specification approved by RISC-V International. The encoding is locked; future changes go into a separate extension. | `RISC-V International` |
| `draft` | Working draft from RISC-V International or another standards body. Not ratified yet — the encoding may still change. | `RISC-V International`, vendor consortium |
| `vendor` | Custom extension defined and shipped by a commercial vendor. | `T-Head / XuanTie`, `SiFive`, `Qualcomm`, `Andes`, `Ventana`, … |
| `research` | Academic / experimental extension, often used in research papers or pre-production prototypes. | university or research group |
| `custom` | Bespoke or one-off extension that does not fit the categories above. | individual or team |

Hard rules:

- A publisher MUST NOT label a package `ratified` unless RISC-V International has actually frozen the specification.
- The registry SHOULD reject manifests whose `standardStatus` is unknown or absent (legacy v0.x manifests are accepted but rendered as "unclassified").
- `authority` is free-text and must be 2–200 characters. The registry trims surrounding whitespace before persisting.
- The CLI's `xsil new` and the web wizard both prompt for these fields and refuse to scaffold a package without them.

Tooling SHOULD surface `standardStatus` prominently in the package UI and group catalog statistics by it.

---

## 5. Targets

`targets` is an object whose keys name execution backends. The CLI uses this to dispatch `xsil run` and `xsil test` to the correct executor.

### 5.1 Standard target keys

| Key | Executor | Notes |
|-----|----------|-------|
| `spike` | [Spike RISC-V ISA Simulator](https://github.com/riscv-software-src/riscv-isa-sim) | Most common default for simulation packages. |
| `qemu` | [QEMU](https://www.qemu.org/) system or user-mode | Use `machine` and `cpu` subfields as needed. |
| `fpga` | FPGA synthesis and programming flow | FPGA support is **optional**. See §5.2. |
| `rtl` | RTL implementation | Optional, but first-class for discovery and registry badges. See §5.3. |

Each target value is a JSON object with backend-specific keys. An empty object `{}` is valid when defaults are fully documented in `docs/`.

### 5.2 FPGA targets

FPGA support is **optional**. If `targets.fpga` is present:

- `bitstream` (string) — path to the bitstream file inside the archive (typically under `bitstream/`).
- `id` (string) — publisher-defined hardware target identifier. This is **not** a value from a platform board catalog.

**No platform board catalog.** ExtenSilica does not maintain a global registry of board identifiers. Hardware target IDs are declared exclusively by the publisher inside each package. The CLI must use only what the package declares, and must never reject a package on the basis of an unknown board name.

---

### 5.3 RTL targets (optional)

RTL sources may be shipped inside the `.xsil` archive for reproducibility, review, and downstream integration. This is **orthogonal** to simulation/emulation and FPGA: a package may provide RTL without providing bitstreams, and may provide bitstreams without providing RTL sources.

If `targets.rtl` is present, the package should include a top-level `rtl/` directory containing the implementation and any build/test wrappers.

Recommended fields (not all are required; publishers may add additional keys):

| Field | Type | Meaning |
|-------|------|---------|
| `language` | string | e.g. `sv`, `verilog`, `chisel`, `vhdl` |
| `top` | string | top module/entity name (publisher-defined) |
| `root` | string | path to RTL root inside the archive (default: `rtl`) |
| `build` | string | path to a build script inside the archive (e.g. `rtl/build.sh`) |
| `test` | string | path to a RTL test entry (e.g. `rtl/test.sh`) |
| `docs` | string | path to integration docs (e.g. `docs/rtl.md`) |

**Normative intent:** `targets.rtl` exists so the registry and tooling can reliably identify “this package ships RTL” and present it as a capability badge. It does not require the CLI to synthesize RTL as part of `xsil run`.

## 6. Toolchain

The `toolchain` object locates the self-contained toolchain bundled inside the archive.

| Key | Required | Description |
|-----|----------|-------------|
| `root` | Yes | Directory containing the toolchain root (typically `toolchain/`). |
| `version` | No | Toolchain version string (e.g. `14.2.0`). |
| `triple` | No | GCC target triple (e.g. `riscv64-unknown-elf`). |

The `toolchain/` tree must supply everything needed to build and run the package's tests without fetching additional compilers from the network. If a package intentionally relies on a host-installed toolchain, that dependency must be documented in `docs/` and the `toolchain` object must set `"external": true`.

---

## 7. Checksums

The `checksums` object records cryptographic digests for integrity verification.

| Key | Description |
|-----|-------------|
| `payload` | SHA-256 over all non-manifest files, concatenated in lexicographic path order: `sha256:<hex>`. |
| `archive` | SHA-256 of the complete `.xsil` archive file: `sha256:<hex>`. |

The CLI recomputes `checksums.payload` (or the legacy `payloadHash`) on every `xsil install` and `xsil run`. If the recomputed value does not match the manifest value, execution is aborted regardless of the source (registry, local file, or directory).

Both `checksums.payload` and the legacy `payloadHash` field may be present simultaneously during the v1.x → v2.0 transition period. If both are present, `checksums.payload` takes precedence.

---

## 8. Versioning

- Versions follow [Semantic Versioning 2.0.0](https://semver.org/).
- Each published version is **immutable**. Once a version is registered with the registry, its `.xsil` bytes and manifest cannot be changed.
- A version may be **yanked** (revoked) by the publisher; the CLI will refuse to install yanked versions unless `--override-security` is explicitly passed.
- The registry always surfaces the latest non-yanked version as the default for `xsil install <name>`.

---

## 9. Registry Integration

When published to the ExtenSilica registry:

- The `name` field becomes the canonical registry slug.
- The `author` field must match the authenticated publisher account.
- The registry stores the `.xsil` archive in blob storage and indexes all manifest fields for search and display.
- The package page at `extensilica.com/package/<name>` displays `description`, `keywords`, `readme`, `license`, `repository`, `homepage`, and the version history table.

---

## 10. Normative Rules

1. **Self-describing** — A package must be fully interpretable from its `manifest.json` alone, without external documentation or registry lookup. All execution paths must resolve to files inside the archive.
2. **Versioned** — Every package must declare a `version`. The registry rejects uploads without a valid semver version.
3. **Runnable or testable** — After unpacking, `entry` must produce a defined execution path, and `testEntry` (or `tests/run.sh`) must be executable by `xsil test`. Placeholder scripts that exit 0 without doing meaningful work satisfy the format requirement but are discouraged.
4. **Self-contained toolchain** — Unless `toolchain.external` is `true`, all required compilers and build tools must be included under `toolchain/`.
5. **Integrity** — `checksums.payload` (or `payloadHash`) must be present and accurate. The CLI enforces this on every execution.
6. **No platform assumptions** — The package must not assume any globally registered board name, platform enum, or ExtenSilica-managed hardware catalog. Hardware targets are publisher-defined.

---

## 11. What This Specification Does Not Define

- **Licensing or payments** — Not part of the format. The `license` field is metadata only.
- **Marketplaces, storefronts, or access control** — Not part of the format.
- **Publisher signatures or organization trust chains** — Optional; not required for publication or execution.
- **A mandatory CLI implementation** — The reference implementation uses `xsil run <package>`; exact flags are defined by that tool, not by this document.
- **Host operating system requirements** — Packages may document OS assumptions in `docs/`; the format itself is OS-agnostic.

---

## Document History

| Version | Date | Summary |
|---------|------|---------|
| 1.0 | 2025-03 | Initial format: gzip-tar, `manifest.json`, layout `sim/`, `toolchain/`, `tests/`, `docs/`, optional `bitstream/`; fields `name`, `version`, `isa`, `entry`, `targets`, `toolchain`, `description`; targets `spike`, `qemu`, `fpga`. |
| 2.0 | 2026-03 | Added required fields `author`, `checksums`; optional fields `license`, `repository`, `homepage`, `keywords`, `readme`, `testEntry`; added `assets/` directory; introduced `checksums` object superseding legacy `payloadHash`; added §8 Versioning, §9 Registry Integration; removed commercial/trust sections; clarified FPGA optionality and no-platform-board-catalog rule. |
| 2.1 | 2026-05 | Added `standardStatus` and `authority` (§4.2 + §4.7) for honest classification of ratified / draft / vendor / research / custom extensions. Both fields are required by the registry and prompted for by `xsil new` and the web wizard. |
