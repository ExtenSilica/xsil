# ExtenSilica architecture (v2)

This document is the **concise system architecture** for the XSIL standard: format, tooling, registry, and flows. Normative details live in [spec/xsil.md](../spec/xsil.md) and the linked docs below.

---

## XSIL format (`.xsil`)

**XSIL** is a **gzip-compressed tar** whose root contains **`manifest.json`** plus a defined directory layout (simulation assets, toolchain, tests, documentation; optional FPGA/bitstream material). See **[spec/xsil.md](../spec/xsil.md)**.

| Concern | Role |
|--------|------|
| **Identity & version** | `name`, `version`, `description`, ISA-related fields |
| **Integrity** | `payloadHash` (and optional hashes/signatures as implemented) so unpacked bytes are verifiable |
| **Execution** | `entry` — how `xsil run` invokes the package; `targets` — simulator vs optional FPGA, **declared per package only** (no platform-wide hardware catalog) |
| **Reproducibility** | Self-contained paths under the archive; documented in [reproducibility.md](./reproducibility.md) |

The format is the **contract** between publishers and tools: same bytes → same validated payload and defined execution entrypoints.

---

## CLI (`xsil`)

The reference **`xsil`** binary ([`cli/`](../cli/)) implements **fetch → validate → run/test/install** against local paths, `.xsil` files, or registry-backed packages.

| Command | Purpose |
|---------|---------|
| **`xsil run`** | Resolve package, verify integrity, execute manifest `entry` (simulator-first; see [execution-model.md](./execution-model.md)) |
| **`xsil test`** | Run `testEntry` or `tests/run.sh` |
| **`xsil install`** | Install under a user directory for offline use |
| **`xsil info`** | Show metadata from registry and/or local install |

The CLI interprets **only** what the manifest and package tree declare (paths, targets, optional FPGA rows). It does not substitute publisher-defined ids against a global list.

---

## Registry

**Distribution** has two practical layers:

1. **Blob storage** — Where `.xsil` tarballs live (HTTP URL, object store, or npm-compatible registry depending on deployment). The CLI downloads by URL when given a registry slug.
2. **Metadata registry** — Optional HTTP service listing packages and versions: slugs, semver, tarball URL, optional per-version fields (e.g. `isa`, opaque `boards` JSON echoing the manifest). The store backend can expose **`GET /packages`**, **`GET /packages/:name`**, **`GET /packages/:name/:version`** for discovery and UI.

Trust endpoints (e.g. organization key, revocation lists) may live alongside the API for verification policy; they are **technical**, not commercial.

---

## Execution flow

End-to-end path for **`xsil run`** with a package specifier:

1. **Resolve** — The specifier is a registry slug, `slug@version`, a path to a `.xsil` file, or an **unpacked directory** with `manifest.json`.
2. **Fetch** (if remote) — Download tarball via metadata/registry; or read local file/tree.
3. **Validate** — Check payload hash (and optional package hash / signatures per implementation).
4. **Unpack** (if archive) — Extract to a temp directory; **local directories** are validated in place.
5. **Select execution class** — Default **simulator** (Spike/QEMU, etc.) per manifest `targets` and [execution-model.md](./execution-model.md); optional **FPGA** only when requested or declared without a sim path.
6. **Run `entry`** — Execute the shell command or script path from the manifest, with working directory at package root.
7. **Cleanup** — Remove temp dirs when the source was an archive; preserve user-owned directories.

---

## Developer flow

Typical loop for someone **publishing** an extension:

1. **Author** RTL, toolchain, sim scripts, tests — layout under a package root matching **[spec/xsil.md](../spec/xsil.md)**.
2. **Write `manifest.json`** — Required fields, `entry`, `targets`, integrity fields after hashing (see [publishing.md](./publishing.md)).
3. **Build** — Produce a `.xsil` archive (`xsil build` when implemented, or equivalent packing).
4. **Publish** — Upload the blob and register the new version with the metadata registry (`xsil publish` when implemented) so clients receive a stable URL and semver record.
5. **Document** — Keep `docs/` accurate so users and CI can reproduce behavior.

Optional: sign manifests and register keys for verification ([security-model.md](./security-model.md)).

---

## User flow

Typical loop for someone **consuming** an extension:

1. **Discover** — Browse metadata (API/UI) or use a known slug/version.
2. **Install or run** — `xsil install` to persist under `~/.extensilica/...`, or **`xsil run`** to fetch, validate, and execute the entry in one shot.
3. **Verify** — Rely on payload hash (and optional signatures) for integrity.
4. **Execute** — Simulator by default; opt into FPGA flows only as declared in the package.

Local development can point **`xsil run`** at a **directory** (e.g. [examples/rvx-demo](../examples/rvx-demo)) without publishing.

---

## Related

| Document | Topic |
|----------|--------|
| [spec/xsil.md](../spec/xsil.md) | Normative `.xsil` layout and manifest |
| [execution-model.md](./execution-model.md) | Simulator vs FPGA; no platform board catalog |
| [publishing.md](./publishing.md) | Build/publish intent |
| [reproducibility.md](./reproducibility.md) | Why XSIL + executability matter |
| [security-model.md](./security-model.md) | Hashes and optional signatures |
| [README.md](./README.md) | Documentation index |
