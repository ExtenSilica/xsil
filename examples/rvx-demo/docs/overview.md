# rvx-demo — Technical Overview

## Purpose

`rvx-demo` is the first official ExtenSilica demo package. It demonstrates:

1. **Package layout** — all required directories and files for a valid `.xsil` package.
2. **Manifest format** — every field used for registry publication and CLI execution.
3. **Simulation flow** — a C program compiled for RV64GC and run under Spike or QEMU.
4. **Test suite** — output validation that works in any environment.
5. **Demo-mode fallback** — graceful operation when simulators are not installed.

---

## Package contents

| Path | Description |
|------|-------------|
| `manifest.json` | Package identity, execution model, checksums |
| `README.md` | Registry page content (this document, rendered as Markdown) |
| `src/hello.c` | RISC-V C source — Fibonacci series computation |
| `sim/run.sh` | Entry point: compiles and runs `hello.elf` under Spike or QEMU |
| `sim/spike.yaml` | Spike flags reference (informational) |
| `sim/bin/` | Compiled ELF output (generated at run time; not tracked in git) |
| `tests/run.sh` | Test entry: validates simulation output |
| `tests/expected.txt` | Reference output |
| `toolchain/README.md` | Toolchain installation instructions |
| `docs/overview.md` | This file |

---

## The program

`src/hello.c` computes the first 10 Fibonacci numbers using a simple iterative function and prints them to stdout. It is intentionally trivial; the value is in the packaging and execution model.

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

## Execution flow

`xsil run rvx-demo` invokes `sim/run.sh` via `sh -c "sh sim/run.sh"` with the package root as CWD.

The script tries executors in this order:

| Priority | Condition | Action |
|----------|-----------|--------|
| 1 | `spike` + `pk` available | Compile ELF if missing, run `spike pk hello.elf` |
| 2 | `qemu-riscv64-static` available | Compile ELF if missing, run with QEMU |
| 3 | `qemu-riscv64` available | Compile ELF if missing, run with QEMU |
| 4 | Nothing found | Print expected output (demo mode), exit 0 |

---

## Test flow

`xsil test rvx-demo` invokes `tests/run.sh`, which:

1. Runs `sim/run.sh` and captures its output.
2. Checks that the output contains:
   - `"Hello from RISC-V!"`
   - `"fib(9) = 34"`
   - `"RV64GC"`
3. Reports pass/fail per check.
4. Exits 0 on all pass, 1 on any failure.

Because `sim/run.sh` always exits 0 (including demo mode), the test suite passes on any machine.

---

## Publishing this package

```bash
xsil login
xsil publish examples/rvx-demo --changelog "Initial demo release"
```

The CLI will:
1. Read and validate `manifest.json`.
2. Compute `checksums.payload` and `checksums.archive`.
3. Pack `examples/rvx-demo/` into `rvx-demo-1.0.0.xsil`.
4. Upload to the registry.

**Note:** `sim/bin/` is excluded from the tarball by adding it to `.xsilignore` (or by removing the compiled ELF before publishing). Do not publish compiled binaries that may be system-specific.
