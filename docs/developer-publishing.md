# Publisher Guide

This guide explains how to publish a RISC-V package to ExtenSilica. You build the package locally; the registry hosts and distributes it.

## Publishing Workflow

### 1. Build your package content

Design and implement your RISC-V extension, toolchain, simulation assets, or documentation. Typical output:

- RTL (Verilog/SystemVerilog) and synthesized FPGA bitstream files (optional).
- A custom GCC or LLVM toolchain targeting your extended ISA (optional).
- Spike or QEMU simulation scripts and test assets.
- Markdown documentation.

### 2. Assemble the package directory

You can scaffold an unscoped package with **`xsil init <slug>`** (creates `./<slug>/` with `manifest.json`, `sim/run.sh`, `tests/run.sh`, `docs/`, `toolchain/README.md`, `.xsilignore`, and a fresh payload checksum). Use `--parent DIR` to place the folder elsewhere, `--author NAME` for `manifest.author`, and `--force` to replace an existing directory.

Otherwise create a directory conforming to the [XSil spec](../spec/xsil.md#3-archive-layout):

```
my-package/
  manifest.json   ← required
  README.md       ← recommended
  sim/            ← simulation assets, entry script
  toolchain/      ← custom toolchain (optional)
  bitstream/      ← FPGA bitstreams (optional)
  tests/          ← test suites (optional)
  docs/           ← documentation (optional)
```

### 3. Write `manifest.json`

Required fields:

```json
{
  "name": "my-package",
  "version": "1.0.0",
  "description": "Short description",
  "author": "your-username",
  "isa": "rv64gc",
  "targets": { "spike": {} },
  "entry": "sim/run.sh",
  "license": "MIT",
  "checksum": "<sha256-of-archive>"
}
```

See the [XSil spec](../spec/xsil.md) for all supported fields and rules.

### 4. Pack into `.xsil`

A `.xsil` file is a gzip-compressed tar archive of the package directory:

```bash
tar -czf my-package-1.0.0.xsil -C my-package .
```

The CLI will also pack the directory for you during `xsil publish`.

### 5. Authenticate

Log in with the CLI to get an API token:

```bash
xsil login
```

This stores your token in `~/.extensilica/token`.

### 6. Publish

```bash
xsil publish my-package-1.0.0.xsil
```

The CLI uploads the archive to the registry. The backend:

1. Verifies the manifest is present and valid.
2. Checks the version does not already exist for this package (versions are immutable).
3. Stores the `.xsil` file.
4. Registers the new version in the package catalog.

The new version is immediately visible on the package page and installable via `xsil install`.

---

## Version rules

- Versions must follow **semantic versioning** (`MAJOR.MINOR.PATCH`).
- You cannot republish the same version — create a new version number instead.
- To hide a broken version, use `xsil yank my-package@1.0.0`.
- The `latest` tag always resolves to the highest non-yanked semver version.

---

## Summary

| Step | Action |
|------|--------|
| 1 | Build extension content (RTL, toolchain, sim assets, docs, tests). |
| 2 | Assemble package directory conforming to the XSil spec. |
| 3 | Write `manifest.json` with required fields. |
| 4 | Pack into `.xsil` tarball. |
| 5 | Authenticate with `xsil login`. |
| 6 | Publish with `xsil publish <path>`. |

For package format details, see [spec/xsil.md](../spec/xsil.md).
For the full CLI reference, see [docs/cli-v3.md](./cli-v3.md).
