# xsil

Reference CLI for the **`.xsil`** RISC-V ISA extension package format.

`xsil` lets you describe a custom RISC-V instruction set extension, scaffold a
runnable package, run tests against it, and publish it to a registry — without
waiting for silicon.

## Install

```bash
cargo install xsil
```

Or grab a prebuilt binary from
[GitHub Releases](https://github.com/ExtenSilica/xsil/releases).

## Quick start

```bash
# Interactive wizard: name, ISA, instructions, opcodes, license, ...
xsil new

# Or non-interactive scaffold
xsil init my-extension

# Run, test, publish
cd my-extension
xsil run .
xsil test .
xsil publish . --dry-run
```

## What is .xsil?

A `.xsil` is a reproducible, signed, gzipped tarball that bundles everything
needed to describe and exercise a custom RISC-V extension:

- `manifest.json` — package metadata, ISA base, instruction list
- `opcodes.json` / `opcodes.h` — encoding (opcode / funct3 / funct7) + C macros
- `examples/<mnemonic>.S` — runnable assembly sample per instruction
- `tests/instructions.S` — combined regression test
- `sim/spike-extension/` — Spike (`extension_t`) C++ skeleton + Makefile
- `toolchain/`, `LICENSE`, `CHANGELOG.md`, `README.md`

The format spec lives at
<https://github.com/ExtenSilica/xsil/blob/main/spec/xsil.md>.

## Common commands

| Command | What it does |
| --- | --- |
| `xsil new` | Interactive wizard — generates a richer skeleton |
| `xsil init <name>` | Non-interactive scaffold |
| `xsil run <path>` | Execute a `.xsil` package locally |
| `xsil test <path>` | Run the package's test suite |
| `xsil install <pkg>` | Install a published package from the registry |
| `xsil publish <path>` | Upload a `.xsil` to the configured registry |
| `xsil login` | Authenticate against the registry |

The hosted registry, web UI, and wizard are at
<https://extensilica.com>.

## License

ISC — see `LICENSE`.
