# rvx-demo

> **The first official ExtenSilica demo package.**
> A minimal RISC-V program that computes Fibonacci numbers — compiled for RV64GC and run under Spike.

[![ISA](https://img.shields.io/badge/ISA-RV64GC-blue)](https://github.com/riscv/riscv-isa-manual)
[![License](https://img.shields.io/badge/license-Apache--2.0-green)](LICENSE)

---

## Quick start

```bash
# Install and run from the registry
xsil install rvx-demo
xsil run rvx-demo

# Or run directly from source (no install needed)
xsil run examples/rvx-demo
```

Expected output:

```
rvx-demo: Hello from RISC-V!

Fibonacci sequence (first 10 terms):
  fib(0) = 0
  fib(1) = 1
  fib(2) = 1
  fib(3) = 2
  fib(4) = 3
  fib(5) = 5
  fib(6) = 8
  fib(7) = 13
  fib(8) = 21
  fib(9) = 34

ISA: RV64GC  |  Simulated via Spike
```

---

## What this demonstrates

| Aspect | Detail |
|--------|--------|
| **Package format** | Valid `.xsil` layout with all required directories |
| **Manifest fields** | All fields required for registry publication |
| **Simulation** | C program → RV64GC ELF → Spike / QEMU |
| **Testing** | Output validation via `xsil test` |
| **Portability** | Demo-mode fallback — runs on any machine, even without Spike |

---

## Running tests

```bash
xsil test rvx-demo
```

The test suite runs `sim/run.sh`, captures the output, and checks for:
- The greeting line
- The correct final Fibonacci value (`fib(9) = 34`)
- The ISA string (`RV64GC`)

Tests pass on any machine — if Spike/QEMU are not installed, demo mode is used.

---

## Simulator requirements

`rvx-demo` tries simulators in this order:

| Simulator | Install |
|-----------|---------|
| `spike` + `pk` (preferred) | `apt install gcc-riscv64-unknown-elf spike` |
| `qemu-riscv64-static` | `apt install qemu-user-static` |
| Demo mode | *(always available — no install needed)* |

See [`toolchain/README.md`](toolchain/README.md) for full installation instructions including macOS and from-source builds.

---

## Package structure

```
rvx-demo/
├── manifest.json        Package identity and execution model
├── README.md            This file (rendered on the registry page)
├── src/
│   └── hello.c          RISC-V C source
├── sim/
│   ├── run.sh           Entry point (xsil run)
│   └── spike.yaml       Spike flags reference
├── tests/
│   ├── run.sh           Test suite (xsil test)
│   └── expected.txt     Reference output
├── toolchain/
│   └── README.md        Toolchain installation guide
└── docs/
    └── overview.md      Technical overview
```

---

## Publishing

```bash
xsil login
xsil publish examples/rvx-demo --changelog "Initial release"
```

---

## License

Apache-2.0 — see the [ExtenSilica registry](https://extensilica.com/package/rvx-demo) for details.
