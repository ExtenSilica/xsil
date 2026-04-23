# XSIL Package Format Specification v2.0

**Status:** Draft  
**Version:** 2.0  
**Last Updated:** March 2026

---

## 1. Definition

A **`.xsil` file** is a **versioned, self-describing package** for publishing and running reproducible RISC-V implementations.

A package may include simulation assets, toolchains, tests, documentation, and optionally FPGA bitstreams. It is the canonical unit of distribution in the ExtenSilica registry: one `.xsil` file maps to one package name at one immutable version, and contains everything needed to run or test that implementation without undeclared host dependencies.

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
| `entry` | string | Path (relative to unpacked root) to the primary execution script or binary. The CLI invokes this for `xsil run`. |
| `toolchain` | object | Describes the bundled toolchain (see §6). |
| `targets` | object | Supported execution backends (see §5). At least one target must be declared. |
| `checksums` | object | Integrity digests (see §7). |

### 4.2 Optional fields

| Field | Type | Description |
|-------|------|-------------|
| `license` | string | [SPDX license identifier](https://spdx.org/licenses/) (e.g. `Apache-2.0`, `MIT`). |
| `repository` | string | URL of the source repository. |
| `homepage` | string | URL of the project homepage or registry page. |
| `keywords` | array of string | Search tags. Lowercase, no spaces. Shown on the registry package page. |
| `readme` | string | Path to the README file inside the archive (default: `README.md`). |
| `testEntry` | string | Path to the test entry point. The CLI uses this for `xsil test`. Defaults to `tests/run.sh` if that file exists. |
| `payloadHash` | string | Legacy field: SHA-256 digest of all non-manifest files concatenated in sorted path order (`sha256:<hex>`). Superseded by `checksums.payload` in v2.0; both may be present for compatibility. |
| `payloadSize` | number | Total byte size of non-manifest files. Used by the CLI progress display. |

### 4.3 Full example

```json
{
  "name": "rvv-demo",
  "version": "1.2.0",
  "description": "RISC-V Vector extension demo with Spike simulation and test suite.",
  "author": "alice",
  "license": "Apache-2.0",
  "repository": "https://github.com/alice/rvv-demo",
  "homepage": "https://extensilica.com/package/rvv-demo",
  "keywords": ["rvv", "vector", "simulation", "spike"],
  "isa": "RV64GCV",
  "entry": "sim/run.sh",
  "testEntry": "tests/run.sh",
  "readme": "README.md",
  "toolchain": {
    "root": "toolchain",
    "version": "14.2.0",
    "triple": "riscv64-unknown-elf"
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

---

## 5. Targets

`targets` is an object whose keys name execution backends. The CLI uses this to dispatch `xsil run` and `xsil test` to the correct executor.

### 5.1 Standard target keys

| Key | Executor | Notes |
|-----|----------|-------|
| `spike` | [Spike RISC-V ISA Simulator](https://github.com/riscv-software-src/riscv-isa-sim) | Most common default for simulation packages. |
| `qemu` | [QEMU](https://www.qemu.org/) system or user-mode | Use `machine` and `cpu` subfields as needed. |
| `fpga` | FPGA synthesis and programming flow | FPGA support is **optional**. See §5.2. |

Each target value is a JSON object with backend-specific keys. An empty object `{}` is valid when defaults are fully documented in `docs/`.

### 5.2 FPGA targets

FPGA support is **optional**. If `targets.fpga` is present:

- `bitstream` (string) — path to the bitstream file inside the archive (typically under `bitstream/`).
- `id` (string) — publisher-defined hardware target identifier. This is **not** a value from a platform board catalog.

**No platform board catalog.** ExtenSilica does not maintain a global registry of board identifiers. Hardware target IDs are declared exclusively by the publisher inside each package. The CLI must use only what the package declares, and must never reject a package on the basis of an unknown board name.

---

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
