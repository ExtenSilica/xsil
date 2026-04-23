#!/bin/sh
# tests/run.sh — test suite for `xsil test rvx-demo`
#
# Invoked by the CLI as:
#   sh -c "sh tests/run.sh"
# with CWD set to the package root.
#
# Tests:
#   1. Simulation output contains the expected greeting
#   2. Simulation output contains the final Fibonacci value
#   3. Simulation output mentions the correct ISA string

set -e

PASS=0
FAIL=0

ok()   { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
fail() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

printf 'Running rvx-demo test suite...\n\n'

# Capture simulation output (suppress the demo-mode header lines that start with spaces)
OUTPUT=$(sh sim/run.sh 2>&1)

# ── Test 1: greeting ──────────────────────────────────────────────────────────
if printf '%s\n' "$OUTPUT" | grep -qF 'Hello from RISC-V!'; then
    ok 'greeting line present'
else
    fail 'greeting line missing — expected "Hello from RISC-V!"'
fi

# ── Test 2: final Fibonacci value ─────────────────────────────────────────────
if printf '%s\n' "$OUTPUT" | grep -qF 'fib(9) = 34'; then
    ok 'fib(9) = 34 correct'
else
    fail 'fib(9) value wrong or missing — expected "fib(9) = 34"'
fi

# ── Test 3: ISA string ────────────────────────────────────────────────────────
if printf '%s\n' "$OUTPUT" | grep -qF 'RV64GC'; then
    ok 'ISA string RV64GC present'
else
    fail 'ISA string missing — expected "RV64GC"'
fi

# ── Test 4: zero exit (implicit — set -e handles this) ───────────────────────
ok 'simulation exited 0'

# ── Summary ───────────────────────────────────────────────────────────────────
printf '\nResults: %d passed, %d failed\n' "$PASS" "$FAIL"

if [ "$FAIL" -gt 0 ]; then
    printf '\nSimulation output was:\n'
    printf '%s\n' "$OUTPUT" | sed 's/^/  /'
    exit 1
fi

printf 'All tests passed.\n'
exit 0
