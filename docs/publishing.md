# XSIL publishing workflow

This document defines the **minimal publishing path** for a conforming `.xsil` package: **`xsil build`** (produce a signed, hashed archive) and **`xsil publish`** (distribute it to a registry). Normative layout and manifest fields are in **[spec/xsil.md](../spec/xsil.md)**; this page ties them to the two commands.

> **Implementation note:** The reference `xsil` CLI is evolving toward these commands. If your build does not expose `build` / `publish` yet, you can still assemble a directory that matches the spec, run validation locally, and produce a `.xsil` tarball using project tooling—**the package requirements below are what `xsil build` is intended to enforce.**

---

## Minimal workflow

```text
  source tree          xsil build              xsil publish
  (package dir)   ──────────────────►   .xsil file   ──────────────►   registry / users
```

```bash
# 1) From a package directory that matches the XSil layout (see below)
xsil build ./my-extension

# 2) Upload the resulting archive and register a new version
xsil publish ./my-extension-1.0.0.xsil
```

Together, **`build`** answers “is this directory a valid, reproducible package artifact?” and **`publish`** answers “how do others fetch this exact version?”

---

## `xsil build`

**Purpose:** Turn a **package root directory** into a single **`.xsil`** file (gzip-compressed tar) that satisfies the specification: correct layout, valid `manifest.json`, consistent paths, and a **payload digest** (and optional signatures) so consumers can verify they unpacked the same bytes.

**Typical responsibilities (normative intent):**

1. **Validate** — Parse `manifest.json`; ensure required fields exist; ensure paths referenced by `execution.entry` (or legacy `entry`), `targets`, and `toolchain.root` exist; ensure required top-level directories are present (`sim/`, `toolchain/`, `tests/`, `docs/` per spec).
2. **Pack** — Create the archive with `manifest.json` at the root and directory contents as specified.
3. **Hash** — Compute `payloadHash` (per project policy—commonly SHA-256 over payload files) and write/update manifest fields so `xsil run` / `xsil test` can verify integrity.
4. **Sign (optional)** — If publisher signing is enabled, add signature material to the manifest after hashing.
5. **Emit** — Write `name-version.xsil` (or a path given by `-o` / `--output`).

**Common options (illustrative):**

| Flag | Role |
|------|------|
| `--output`, `-o` | Write the `.xsil` to a specific path |
| `--dry-run` | Validate only; do not write an archive |
| `--key` | Private key for signing when producing a signed package |

---

## `xsil publish`

**Purpose:** Take a **built `.xsil`** file and **publish** it: upload the blob to durable storage (implementation-defined) and **register** package metadata and version (slug, semver, tarball URL, hashes) so clients can run:

```bash
xsil run my-pkg@1.0.0
```

**Typical responsibilities:**

1. **Verify** — Re-check manifest, payload hash, and optional signatures before upload.
2. **Upload** — Store the `.xsil` bytes at a stable URL (or hand off to an npm/OCI-compatible registry, depending on deployment).
3. **Register** — Create or update the **version record** in the ExtenSilica **metadata registry** (`GET /packages/...`).

Environment variables (examples): registry base URL, API token if the registry requires authentication for writes.

---

## Package structure

A **package root** is a directory that will become the root of the tarball. It **must** match **[spec/xsil.md §3 Archive layout](../spec/xsil.md#3-archive-layout)**.

| Path | Required | Role |
|------|----------|------|
| **`manifest.json`** | **Yes** | Identity, ISA, entry, targets, toolchain pointer, description |
| **`sim/`** | **Yes** | Simulation assets (Spike/QEMU scripts, configs, launchers) |
| **`toolchain/`** | **Yes** | Bundled compiler/tooling so builds do not depend on undeclared host SDKs |
| **`tests/`** | **Yes** | Tests and vectors (`xsil test` uses `testEntry` or e.g. `tests/run.sh`) |
| **`docs/`** | **Yes** | Human-readable instructions and ISA notes |
| **`bitstream/`** | No | FPGA bitstreams when `fpga` is a target |

Omit empty directories only if your tooling and the spec allow it; many publishers keep minimal placeholders (e.g. `docs/README.md`) so the tree is obviously complete.

---

## Required files

At minimum:

1. **`manifest.json`** — All **required** manifest fields in **[spec/xsil.md §4](../spec/xsil.md#4-manifest-manifestjson)** (`name`, `version`, `isa`, `entry`, `targets`, `toolchain`, `description`).
2. **Content for every path** referenced from the manifest — e.g. the `entry` script, files under `toolchain.root`, paths inside `targets.spike` / `targets.qemu` / `targets.fpga`.
3. **Tests** — At least one runnable test entry (manifest `testEntry` or conventional `tests/run.sh`) if you expect `xsil test` to be used.

After **`xsil build`**, the manifest should also carry **`payloadHash`** (and optional signature fields) consistent with the packed bytes.

---

## How to define the entry point

The primary run hook is the manifest field **`execution.entry`** (or legacy `entry`): a command/script that **`xsil run`** executes after unpack and dependency resolution.

- Prefer a **shell script** under `sim/` that invokes Spike, QEMU, or a thin wrapper around your bundled `toolchain/`.
- The script must be **executable** on the target OS or invoked explicitly (e.g. `bash sim/run.sh`) if documented in `docs/`.
- **`xsil run`** uses **`entry`** together with **`targets`** to choose simulator vs FPGA (see [execution-model.md](./execution-model.md)): **simulator is the default**; FPGA only when requested or when no sim path exists.

Optional: **`execution.testEntry`** — command for **`xsil test`** (if not using the default `tests/run.sh`).

---

## How to include simulator or FPGA

Declare capabilities in **`targets`** (see **[spec/xsil.md §5](../spec/xsil.md#5-targets-targets)**):

| Key | Meaning |
|-----|---------|
| **`spike`** | Spike (or compatible ISS) — put configs/scripts under `sim/`; reference them from this object if needed |
| **`qemu`** | QEMU — machine/user-mode options as needed |
| **`fpga`** | FPGA — typically references bitstreams under **`bitstream/`** |

**Simulator-first:** Packages intended for broad reproducibility should declare **`spike` and/or `qemu`** so `xsil run` can succeed **without** hardware. Empty objects `{}` are allowed when behavior is implied by `sim/` and `docs/`.

**FPGA:** Add **`targets.fpga`** with paths to bitstream files (usually under `bitstream/`). Flash/probe commands belong in **`docs/`** and/or optional per-manifest `boards[]` rows. Identifiers are **publisher-defined**—ExtenSilica does not ship a board catalog. **FPGA is optional** for a conforming package that provides a simulator path.

---

## Related

- **[spec/xsil.md](../spec/xsil.md)** — Normative format (layout, manifest, targets, toolchain).
- **[execution-model.md](./execution-model.md)** — How `xsil run` picks simulator vs FPGA.
- **[reproducibility.md](./reproducibility.md)** — Why packaged, hashed, runnable artifacts matter.
- **[developer-publishing.md](./developer-publishing.md)** — Broader narrative (RTL, synthesis, signing); may overlap—**spec + this doc** take precedence for layout and commands.
