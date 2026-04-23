# XSil execution model

This document defines **how `xsil` runs a package** after fetch and integrity validation. Normative package layout and manifest fields are in [spec/xsil.md](../spec/xsil.md).

## Command

```bash
xsil run <package>
```

`<package>` is a registry slug (e.g. `rvx-demo`), `slug@version`, or a path to a `.xsil` file.

Example:

```bash
xsil run rvx-demo
```

---

## Execution classes (priority)

The runtime distinguishes two **classes** of execution, in **strict priority order**:

| Priority | Class | Role |
|--------|--------|------|
| **1** | **Simulator** | Run the extension in software: instruction-set simulation (e.g. Spike), system or user emulation (e.g. QEMU), using assets under `sim/` and the bundled `toolchain/`. **This is the default.** |
| **2** | **FPGA** | Optional hardware path: program a bitstream and/or run board-specific flows under `bitstream/` when the user or manifest opts into FPGA execution. |

**Rules:**

1. **`xsil run` must succeed without any FPGA.** If only simulation is available, that is sufficient.
2. **Default target class is simulator.** The CLI selects a simulation backend unless the user explicitly requests FPGA (see below).
3. **Automatic target selection** — The implementation chooses a concrete backend (e.g. Spike vs QEMU) using manifest `targets` and on-disk layout, without requiring the user to name a target for the common case. For FPGA, any disambiguation uses **ids and paths declared in that package only**—there is no platform-wide board list.

---

## No platform board catalog

ExtenSilica **does not** maintain a global registry of “supported boards.” Optional FPGA hardware entries (e.g. manifest `targets.fpga` with publisher-defined bitstream paths) are **publisher-defined** inside each `.xsil`. The CLI must **only** interpret what that package’s manifest and `targets` declare; it must not substitute or validate ids against a built-in enum.

---

## Manifest: `targets`

The manifest’s `targets` object lists what the package supports. At minimum the spec defines:

| Key | Execution class |
|-----|-----------------|
| `spike` | Simulator |
| `qemu` | Simulator |
| `fpga` | FPGA (optional) |

Empty objects `{}` are allowed when behavior is implied by `sim/` or `docs/`.

---

## Automatic selection algorithm (normative intent)

When the user runs `xsil run <package>` **without** forcing FPGA:

1. **Prefer simulator** — If `targets.spike` and/or `targets.qemu` is present (or `sim/` contains a runnable flow documented for the default entry), select a **simulator** backend per implementation policy (e.g. prefer Spike if both exist, or follow `docs/`).
2. **Use manifest `entry`** — The `entry` field is interpreted in the context of the **selected class** (simulator by default), e.g. a script under `sim/` that launches Spike/QEMU with the packaged toolchain.
3. **FPGA only when requested** — FPGA is considered only if:
   - the user passes an explicit flag (e.g. `--target fpga` / `--board <id>` where `<id>` **matches an id declared in that package’s manifest**), **or**
   - the package declares **only** `fpga` and no simulator targets (unusual; `docs/` should explain), **or**
   - implementation-defined policy after printing a clear warning.

**FPGA is never required** for a conforming package that declares at least one simulator-related target or provides a default `entry` that runs in simulation.

---

## Summary

| Requirement | Behavior |
|-------------|----------|
| Automatic target selection | Yes — default path picks simulator unless overridden |
| Default | Simulator (priority 1) |
| FPGA | Optional (priority 2); not required for `xsil run` to work |

---

## Related

- [spec/xsil.md](../spec/xsil.md) — `entry`, `targets`, `sim/`, `toolchain/`, optional `bitstream/`
- [extensilica-architecture.md](./extensilica-architecture.md) — system context
