# Toolchain — rvx-demo

`rvx-demo` uses an **external** toolchain (`"external": true` in `manifest.json`).
No compiler binaries are bundled in this package — this keeps the demo small.
Production packages should bundle their toolchain under `toolchain/` instead.

---

## Required tools

| Tool | Purpose | Min version |
|------|---------|-------------|
| `riscv64-unknown-elf-gcc` | Cross-compiler (C → RV64GC ELF) | 12.0 |
| `spike` | RISC-V ISA Simulator | 1.1 |
| `pk` | Proxy kernel for Spike user-mode | 1.0 |

**Alternative to Spike:** `qemu-riscv64-static` or `qemu-riscv64` may be used instead.
The `sim/run.sh` entry point tries all available simulators in order.

---

## Installation

### Ubuntu / Debian

```bash
sudo apt-get update
sudo apt-get install gcc-riscv64-unknown-elf spike qemu-user-static
```

> **Note:** The `spike` package is not always in default repos.
> Build from source if needed: https://github.com/riscv-software-src/riscv-isa-sim

### macOS (Homebrew)

```bash
brew tap riscv-software-src/riscv
brew install riscv-tools qemu
```

### From source (any Linux)

```bash
# riscv-gnu-toolchain
git clone https://github.com/riscv-collab/riscv-gnu-toolchain
cd riscv-gnu-toolchain
./configure --prefix=/opt/riscv
make -j$(nproc)
export PATH=/opt/riscv/bin:$PATH

# Spike (ISS)
git clone https://github.com/riscv-software-src/riscv-isa-sim
cd riscv-isa-sim && mkdir build && cd build
../configure --prefix=/opt/riscv
make install

# Proxy kernel
git clone https://github.com/riscv-software-src/riscv-pk
cd riscv-pk && mkdir build && cd build
../configure --prefix=/opt/riscv --host=riscv64-unknown-elf
make install
```

---

## Compile the demo manually

From the `rvx-demo` package root:

```bash
riscv64-unknown-elf-gcc \
  -march=rv64gc -mabi=lp64d \
  -O2 -static \
  -o sim/bin/hello.elf \
  src/hello.c
```

Then run:

```bash
spike pk sim/bin/hello.elf
# or
qemu-riscv64-static sim/bin/hello.elf
```
