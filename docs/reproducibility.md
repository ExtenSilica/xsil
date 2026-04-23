# Reproducibility foundation

This document explains **why** many RISC-V extension artifacts today fail reproducibility, **how** the `.xsil` format and tooling address that gap, and **why** tying claims to **executable** packages matters for science and engineering.

---

## Why current RISC-V extensions are often not reproducible

“RISC-V extensions” in research and industry usually arrive as a **loose bundle of artifacts**: PDFs, Git repositories, Docker images, vendor IP, and ad‑hoc scripts. That is enough to *describe* an idea, but not enough for a third party to obtain the **same bits** and observe the **same behavior** on a reasonable timeline.

Typical failure modes:

| Gap | What goes wrong |
|-----|------------------|
| **Incomplete provenance** | A paper cites a commit or branch; the repo moves, submodules vanish, or “contact the authors” is required for scripts or bitstreams. |
| **Environment drift** | Toolchain versions, ISA strings, and simulator builds differ across machines; “it worked on my laptop” is not a reproducible standard. |
| **Opaque or missing binaries** | FPGA bitstreams and closed IP are not packaged with hashes, or cannot be legally redistributed—so no one can verify the *exact* artifact the paper used. |
| **Underspecified execution** | READMEs list manual steps; there is no single **entry** that a tool can run to validate the package end‑to‑end. |
| **Integrity vs intent** | Without a content hash over the payload, you cannot know whether what you downloaded matches what was reviewed or signed. |

In short: **description without a packaged, hashed, runnable unit** does not scale to third‑party verification.

---

## How XSIL fixes this

**XSIL** (`.xsil`) is a **reproducible package** format: a tarball with a **normative manifest**, a **payload hash** over the meaningful contents, and (when published) **registry metadata** pointing at a specific URL and hashes for that version.

Together, the CLI and registry turn “extension work” into something a machine can **fetch, validate, and execute** on a well‑defined path:

1. **Manifest** — Names, versions, ISA-related fields, entry points, optional targets (simulator vs FPGA), tests, and documentation live in one place (`manifest.json`), not only in prose.
2. **Payload integrity** — `payloadHash` (and optional package hashes) let anyone confirm they unpacked **the same bytes** the publisher intended.
3. **`xsil run`** — A single command runs the package’s declared **entry** (see [execution-model.md](./execution-model.md)): by default **simulation** is enough; FPGA is optional. That makes “reproduce the artifact’s behavior” a **CLI contract**, not a reading exercise.
4. **Registry** — Publishes **versioned metadata** (including tarball URLs) so `xsil run <package>` can resolve a concrete version without hunting through footnotes.

XSIL does not remove legal or commercial constraints on redistribution; it **standardizes** what *is* redistributed so reproducibility is *possible* where policy allows.

---

## Why executability matters

Reproducibility is not only “can I rebuild from source?”—for extensions it is “**can I run the same packaged behavior** and compare results?”

- **Science** — Claims about performance, correctness, or ISA behavior need a **shared executable substrate**. A paper alone is a hypothesis; a validated `.xsil` is closer to an **experimental protocol** others can rerun.
- **Engineering** — Teams need to bisect regressions, run CI, and gate releases. That requires a **deterministic entry** (`xsil run`, `xsil test`) tied to known hashes—not a wiki of manual steps.
- **Trust** — Integrity checks (hashes, optional signatures) bind **trust to bytes**. Executability binds **trust to observed behavior** on a reference path (simulator first, FPGA when opted in).

Without executability, “reproducible RISC-V extension” degrades into “trust the PDF.” XSIL aims for **inspectable, runnable artifacts**.

---

## Example: paper → XSIL → `xsil run`

A plausible end‑to‑end story:

1. **Paper** — Authors publish “RVFoo”: an ISA tweak + microbenchmarks. The PDF points to a **registry slug** (or a released `.xsil` file) instead of only a Git URL.
2. **XSIL** — Authors (or the community) pack bitstreams/toolchain/sim assets, write `manifest.json` with `entry`, `payloadHash`, `isa`, `targets`, and publish a **versioned** package to the registry.
3. **`xsil run`** — A reader runs:

   ```bash
   xsil run rvfoo-crypto
   ```

   The CLI fetches metadata, downloads the tarball, verifies hashes, selects a simulator by default (per [execution-model.md](./execution-model.md)), and executes the declared entry—reproducing the **packaged** behavior without re‑deriving the workflow from the paper.

The paper remains the **narrative**; the `.xsil` package is the **machine‑checkable artifact** that closes the reproducibility loop.

---

## Related

- [execution-model.md](./execution-model.md) — How `xsil run` chooses simulator vs FPGA.
- [spec/xsil.md](../spec/xsil.md) — Manifest and package layout (normative).
- [security-model.md](./security-model.md) — Integrity and optional signatures.
- [README.md](./README.md) — Documentation index.
