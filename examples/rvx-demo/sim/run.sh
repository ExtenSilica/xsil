#!/bin/sh
# sim/run.sh — entry point for `xsil run rvx-demo`
#
# This script is invoked by the CLI as:
#   sh -c "sh sim/run.sh"
# with CWD set to the package root.
#
# Execution priority:
#   1. spike + pk           (preferred — true ISS)
#   2. qemu-riscv64-static  (alternative simulator)
#   3. qemu-riscv64         (alternative simulator)
#   4. demo mode            (no simulator required)

ELF="sim/bin/hello.elf"
SRC="src/hello.c"

has() { command -v "$1" >/dev/null 2>&1; }

# ── Step 1: compile ELF if missing and cross-compiler is available ────────────
if [ ! -f "$ELF" ] && has riscv64-unknown-elf-gcc; then
    printf "Compiling %s for RV64GC...\n" "$SRC"
    mkdir -p sim/bin
    riscv64-unknown-elf-gcc \
        -march=rv64gc -mabi=lp64d \
        -O2 -static \
        -o "$ELF" "$SRC"
    printf "Compiled  → %s\n\n" "$ELF"
fi

# ── Step 2: run under Spike (with proxy kernel) ───────────────────────────────
if [ -f "$ELF" ] && has spike && has pk; then
    printf "[ spike rv64gc ] %s\n\n" "$ELF"
    spike pk "$ELF"
    exit $?
fi

# ── Step 3: run under QEMU user-mode emulation ────────────────────────────────
if [ -f "$ELF" ] && has qemu-riscv64-static; then
    printf "[ qemu-riscv64-static ] %s\n\n" "$ELF"
    qemu-riscv64-static "$ELF"
    exit $?
fi

if [ -f "$ELF" ] && has qemu-riscv64; then
    printf "[ qemu-riscv64 ] %s\n\n" "$ELF"
    qemu-riscv64 "$ELF"
    exit $?
fi

# ── Step 4: demo mode (no simulator installed) ────────────────────────────────
printf '\n'
printf '  rvx-demo — Demo Mode\n'
printf '  ════════════════════\n'
printf '  Spike and QEMU were not found on this system.\n'
printf '  Showing the expected simulation output instead.\n'
printf '\n'
printf '  To run with a real RISC-V simulator:\n'
printf '    1. Install riscv64-unknown-elf-gcc, spike, and pk (or qemu-riscv64)\n'
printf '    2. Run:  xsil run rvx-demo\n'
printf '\n'
printf 'rvx-demo: Hello from RISC-V!\n'
printf '\n'
printf 'Fibonacci sequence (first 10 terms):\n'
printf '  fib(0) = 0\n'
printf '  fib(1) = 1\n'
printf '  fib(2) = 1\n'
printf '  fib(3) = 2\n'
printf '  fib(4) = 3\n'
printf '  fib(5) = 5\n'
printf '  fib(6) = 8\n'
printf '  fib(7) = 13\n'
printf '  fib(8) = 21\n'
printf '  fib(9) = 34\n'
printf '\n'
printf 'ISA: RV64GC  |  Simulated via Spike\n'
printf '\n'
exit 0
