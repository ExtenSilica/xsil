/*
 * rvx-demo — Hello from RISC-V!
 *
 * A minimal C program that computes the first N Fibonacci numbers.
 * Designed to be compiled for RV64GC and simulated under Spike or QEMU.
 *
 * Build:
 *   riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d -O2 -static \
 *     -o sim/bin/hello.elf src/hello.c
 *
 * Run:
 *   spike pk sim/bin/hello.elf
 *   qemu-riscv64-static sim/bin/hello.elf
 */

#include <stdio.h>

#define N 10

static long fib(int n) {
    long a = 0, b = 1;
    for (int i = 0; i < n; i++) {
        long t = a + b;
        a = b;
        b = t;
    }
    return a;
}

int main(void) {
    puts("rvx-demo: Hello from RISC-V!");
    puts("");
    printf("Fibonacci sequence (first %d terms):\n", N);
    for (int i = 0; i < N; i++) {
        printf("  fib(%d) = %ld\n", i, fib(i));
    }
    puts("");
    puts("ISA: RV64GC  |  Simulated via Spike");
    return 0;
}
