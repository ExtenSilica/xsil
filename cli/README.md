# xsil

Reference CLI for the **`.xsil`** RISC-V ISA extension package format.

`xsil` lets you describe a custom RISC-V instruction set extension, scaffold a
runnable package (with Spike extension skeleton, opcodes, tests and examples),
run it locally, and publish it to a registry — without waiting for silicon.

## TL;DR

```bash
cargo install xsil
xsil new            # interactive wizard
xsil run ./my-ext   # build + run inside the generated package
```

## Install

```bash
cargo install xsil
```

Or grab a prebuilt binary from
[GitHub Releases](https://github.com/ExtenSilica/xsil/releases).

## `xsil new` — interactive wizard

The fastest way to start. The wizard asks for:

- package name, version, ISA base (e.g. `RV64GC`), license
- repository URL (mandatory)
- one or more custom instructions: mnemonic, format (R/I/S/B/U/J),
  opcode, `funct3`, `funct7`, operands, summary

It then generates a ready-to-build `.xsil` tree:

- `manifest.json` — package metadata + instruction list
- `opcodes.json` — machine-readable encoding table
- `opcodes.h` — C header with `.insn`-based inline-assembly macros
- `examples/<mnemonic>.S` — one runnable example per instruction
- `tests/instructions.S` — combined regression test
- `sim/spike-extension/` — Spike `extension_t` C++ skeleton + Makefile + README
- `toolchain/`, `LICENSE` (full SPDX text), `CHANGELOG.md`, `README.md`

Prefer non-interactive? Use `xsil init <name>` for a minimal scaffold.

## What is `.xsil`?

A `.xsil` is a reproducible, gzipped tarball with SHA-256 payload checksums
that bundles everything needed to describe and exercise a custom RISC-V
extension. The format spec lives at
<https://github.com/ExtenSilica/xsil/blob/main/spec/xsil.md>.

## Common commands

| Command | What it does |
| --- | --- |
| `xsil new` | Interactive wizard — generates a runnable skeleton |
| `xsil init <name>` | Non-interactive minimal scaffold |
| `xsil run <path>` | Execute a `.xsil` package locally |
| `xsil test <path>` | Run the package's test suite |
| `xsil install <pkg>` | Install a published package from the registry |
| `xsil publish <path>` | Upload a `.xsil` to the configured registry |
| `xsil login` | Authenticate against the registry |
| `xsil search <query>` | Search the registry |

The hosted registry, web UI, and an equivalent web wizard are at
<https://extensilica.com>.

## License

ISC — see `LICENSE`.
