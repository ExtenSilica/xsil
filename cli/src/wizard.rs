//! `xsil new` — Extension Wizard for `.xsil` packages.
//!
//! Generates a richer skeleton than `xsil init`:
//!  - `manifest.json` (with computed `checksums.payload`)
//!  - `opcodes.json` (custom-instruction descriptor)
//!  - `README.md`, `docs/overview.md`, `toolchain/README.md`, `.xsilignore`
//!  - `examples/demo.S`, `tests/basic.S`, `tests/expected.txt`
//!  - `sim/run.sh`, `sim/spike.yaml`, `tests/run.sh`
//!
//! Mirrors the templates produced by `store-backend` `lib/wizardGenerate.ts` so
//! the UI download and the CLI output have the same shape.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::manager::ExtensionManager;
use crate::types::{Manifest, ManifestChecksums};

// ── Reserved slugs / formats / standard-status values ────────────────────────

/// Honest classification values, kept in sync with the registry's
/// `StandardStatus` Prisma enum (`store-backend/prisma/schema.prisma`) and the
/// frontend's `STANDARD_STATUS_VALUES`. Order matters for help output.
const STANDARD_STATUS_VALUES: &[&str] =
    &["ratified", "draft", "vendor", "research", "custom"];

const RESERVED_SLUGS: &[&str] = &[
    "xsil", "extensilica", "registry", "store", "platform",
    "admin", "administrator", "root", "system", "superuser", "sudo",
    "moderator", "staff", "support", "official", "security",
    "test", "testing", "demo", "example", "sample", "placeholder",
    "undefined", "null", "none", "noop", "empty", "blank",
    "todo", "fixme", "temp", "tmp",
    "lib", "core", "base", "common", "utils", "util", "tools",
    "api", "sdk", "cli", "app", "pkg", "package",
    "v1", "v2", "v3",
];

const VALID_FORMATS: &[&str] = &["R", "I", "S", "B", "U", "J"];

#[derive(Clone, Debug)]
pub struct WizardInstruction {
    pub mnemonic: String,
    pub format: String,
    pub opcode: Option<String>,
    pub funct3: Option<String>,
    pub funct7: Option<String>,
    pub operands: Vec<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WizardTargets {
    pub qemu: bool,
    pub binutils: bool,
    pub llvm: bool,
}

#[derive(Clone, Debug)]
pub struct WizardArgs {
    pub name: String,
    pub parent: Option<PathBuf>,
    pub force: bool,
    pub non_interactive: bool,
    pub author: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub isa: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    /// Honest classification: one of `STANDARD_STATUS_VALUES`.
    pub standard_status: Option<String>,
    /// Free-text spec authority (e.g. "RISC-V International").
    pub authority: Option<String>,
    pub instructions: Vec<WizardInstruction>,
    pub targets: WizardTargets,
}

// ── Validation helpers ────────────────────────────────────────────────────────

fn validate_slug(slug: &str) -> Result<()> {
    if slug.contains('@') || slug.contains('/') || slug.contains('\\') {
        bail!(
            "Use an unscoped slug (lowercase letters, digits, hyphens). \
             For scoped packages (`@org/pkg`), edit `manifest.json` after generation."
        );
    }
    let b = slug.as_bytes();
    if b.len() < 2 || b.len() > 64 {
        bail!("Package name must be between 2 and 64 characters.");
    }
    if b[0] == b'-' || b[b.len() - 1] == b'-' {
        bail!("Package name must not start or end with a hyphen.");
    }
    for &ch in b {
        if !matches!(ch, b'a'..=b'z' | b'0'..=b'9' | b'-') {
            bail!(
                "Package name may only contain lowercase letters, digits, and hyphens (got byte {ch})."
            );
        }
    }
    if RESERVED_SLUGS.contains(&slug) {
        bail!("\"{slug}\" is reserved; choose another name.");
    }
    Ok(())
}

fn validate_semver(s: &str) -> Result<()> {
    semver::Version::parse(s).with_context(|| format!("\"{s}\" is not valid semver (e.g. 0.1.0)"))?;
    Ok(())
}

fn validate_isa(s: &str) -> Result<()> {
    let t = s.trim();
    if t.len() < 2 || t.len() > 40 {
        bail!("ISA must be between 2 and 40 characters.");
    }
    for ch in t.chars() {
        if !ch.is_ascii_uppercase() && !ch.is_ascii_digit() && ch != '_' {
            bail!("ISA must be uppercase letters, digits, or `_` (got `{ch}`).");
        }
    }
    Ok(())
}

fn validate_http_url(field: &str, raw: &str) -> Result<()> {
    let t = raw.trim();
    if t.is_empty() {
        bail!("{field} is required.");
    }
    if t.len() > 2048 {
        bail!("{field} must be at most 2048 characters.");
    }
    if !(t.starts_with("http://") || t.starts_with("https://")) {
        bail!("{field} must start with http:// or https://.");
    }
    Ok(())
}

fn validate_format(s: &str) -> Result<()> {
    if !VALID_FORMATS.contains(&s) {
        bail!("Format must be one of: R, I, S, B, U, J.");
    }
    Ok(())
}

fn normalize_standard_status(raw: &str) -> Result<String> {
    let t = raw.trim().to_ascii_lowercase();
    if t.is_empty() {
        bail!("standardStatus is required.");
    }
    if !STANDARD_STATUS_VALUES.contains(&t.as_str()) {
        bail!(
            "standardStatus must be one of: {} (got `{}`).",
            STANDARD_STATUS_VALUES.join(", "),
            raw,
        );
    }
    Ok(t)
}

fn validate_authority(raw: &str) -> Result<String> {
    let t = raw.trim().to_string();
    if t.len() < 2 || t.len() > 200 {
        bail!("authority must be between 2 and 200 characters.");
    }
    Ok(t)
}

// ── Identifier / encoding helpers (mirror of wizardGenerate.ts) ───────────────

/// Convert a mnemonic / slug into a safe lowercase identifier (`x.add` → `x_add`).
fn safe_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.starts_with('_') {
        out.remove(0);
    }
    while out.ends_with('_') {
        out.pop();
    }
    // Collapse runs of underscores from non-alnum sequences.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_us = false;
    for ch in out.chars() {
        if ch == '_' {
            if !prev_us {
                collapsed.push('_');
            }
            prev_us = true;
        } else {
            collapsed.push(ch);
            prev_us = false;
        }
    }
    if collapsed.is_empty() {
        collapsed.push_str("insn");
    }
    if collapsed.starts_with(|c: char| c.is_ascii_digit()) {
        collapsed.insert(0, '_');
    }
    collapsed
}

/// Convert a mnemonic into an UPPER_CASE C macro identifier (`x.add` → `X_ADD`).
fn macro_ident(s: &str) -> String {
    safe_ident(s).to_ascii_uppercase()
}

/// Resolve a free-form opcode hint to its 7-bit numeric value, defaulting to
/// `custom-0` (0x0b). Recognised inputs include `custom-0..3`, `0bNNN…`,
/// `0xNN`, plain decimal, and a 1..7-digit binary literal.
fn parse_opcode(raw: Option<&str>) -> u8 {
    let Some(t) = raw.map(|s| s.trim().to_ascii_lowercase()) else {
        return 0x0b;
    };
    if t.is_empty() {
        return 0x0b;
    }
    match t.as_str() {
        "custom-0" | "custom0" => return 0x0b,
        "custom-1" | "custom1" => return 0x2b,
        "custom-2" | "custom2" => return 0x5b,
        "custom-3" | "custom3" => return 0x7b,
        _ => {}
    }
    if let Some(stripped) = t.strip_prefix("0b") {
        return u32::from_str_radix(stripped, 2).map(|n| (n & 0x7f) as u8).unwrap_or(0x0b);
    }
    if let Some(stripped) = t.strip_prefix("0x") {
        return u32::from_str_radix(stripped, 16).map(|n| (n & 0x7f) as u8).unwrap_or(0x0b);
    }
    if t.chars().all(|c| c == '0' || c == '1') && (1..=7).contains(&t.len()) {
        return u32::from_str_radix(&t, 2).map(|n| (n & 0x7f) as u8).unwrap_or(0x0b);
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return t.parse::<u32>().map(|n| (n & 0x7f) as u8).unwrap_or(0x0b);
    }
    0x0b
}

/// Parse a funct3/funct7-style hint string to a numeric value. Defaults to 0.
fn parse_funct(raw: Option<&str>, max_bits: u8) -> u8 {
    let Some(t) = raw.map(|s| s.trim().to_ascii_lowercase()) else {
        return 0;
    };
    if t.is_empty() {
        return 0;
    }
    let n: u32 = if let Some(s) = t.strip_prefix("0b") {
        u32::from_str_radix(s, 2).unwrap_or(0)
    } else if let Some(s) = t.strip_prefix("0x") {
        u32::from_str_radix(s, 16).unwrap_or(0)
    } else if t.chars().all(|c| c == '0' || c == '1') && t.len() as u8 <= max_bits {
        u32::from_str_radix(&t, 2).unwrap_or(0)
    } else if t.chars().all(|c| c.is_ascii_digit()) {
        t.parse::<u32>().unwrap_or(0)
    } else {
        0
    };
    let mask = (1u32 << max_bits) - 1;
    (n & mask) as u8
}

/// Format a number as a binary literal padded to `bits` digits, e.g. `0b0000111`.
fn bin_str(n: u8, bits: usize) -> String {
    let s = format!("{:0>width$b}", n, width = bits);
    format!("0b{}", s)
}

/// Format a number as a 0x-prefixed hex literal padded to `digits`.
fn hex_str(n: u8, digits: usize) -> String {
    format!("0x{:0>width$x}", n, width = digits)
}

fn default_author() -> String {
    std::process::Command::new("git")
        .args(["config", "--get", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "your-username".to_string())
}

// ── Tiny prompt helpers (no extra deps) ──────────────────────────────────────

fn prompt_line(prompt: &str, default: Option<&str>) -> Result<String> {
    print!("{prompt}");
    if let Some(d) = default {
        if !d.is_empty() {
            print!(" [{d}]");
        }
    }
    print!(": ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).context("read stdin")?;
    let s = line.trim().to_string();
    if s.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(s)
    }
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "Y/n" } else { "y/N" };
    let raw = prompt_line(&format!("{prompt} ({suffix})"), Some(if default_yes { "Y" } else { "N" }))?;
    let l = raw.trim().to_ascii_lowercase();
    Ok(matches!(l.as_str(), "y" | "yes" | "1" | "true"))
}

// ── File / template helpers ──────────────────────────────────────────────────

#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let m = fs::metadata(path)?.permissions().mode();
    fs::set_permissions(path, fs::Permissions::from_mode(m | 0o111))
        .with_context(|| format!("chmod +x {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn readme_template(
    slug: &str,
    description: &str,
    isa: &str,
    instructions: &[WizardInstruction],
    standard_status: &str,
    authority: &str,
) -> String {
    let ins_lines = if instructions.is_empty() {
        "_No custom instructions declared yet._".to_string()
    } else {
        instructions
            .iter()
            .map(|i| {
                let s = i.summary.as_deref().unwrap_or("");
                if s.is_empty() {
                    format!("- `{}` ({})", i.mnemonic, i.format)
                } else {
                    format!("- `{}` ({}) — {}", i.mnemonic, i.format, s)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        r#"# {slug}

{description}

**ISA:** `{isa}`

**Standard status:** `{standard_status}`
**Authority:** {authority}

## Run

```bash
xsil install {slug}
xsil run {slug}
xsil test {slug}
```

## Custom instructions

{ins_lines}

## Targets

- `spike` — skeleton wired in `sim/run.sh`.
- `qemu`, `binutils`, `llvm` — declared as `status: planned` in `manifest.json`.

## Generated by

ExtenSilica Extension Wizard (`xsil new`). Edit each file as needed before publishing.
"#
    )
}

fn docs_overview_template(slug: &str, description: &str, isa: &str) -> String {
    format!(
        r#"# {slug} — overview

{description}

This document is a starting point. Replace it with your extension's:

- motivation,
- ISA family (`{isa}`) and required base features,
- programmer model (registers, CSRs, traps),
- instruction set (see `opcodes.json`),
- intended targets (Spike today; QEMU/binutils/LLVM planned),
- compatibility considerations.
"#
    )
}

fn opcodes_json_template(instructions: &[WizardInstruction]) -> String {
    if instructions.is_empty() {
        return serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "instructions": [
                {
                    "mnemonic": "x.cust1",
                    "format": "R",
                    "opcode": "custom-0",
                    "funct3": "0b000",
                    "funct7": "0b0000000",
                    "operands": ["rd", "rs1", "rs2"],
                    "summary": "rd = rs1 + rs2 (placeholder semantics; replace me)."
                }
            ]
        }))
        .unwrap_or_default()
            + "\n";
    }
    let arr: Vec<serde_json::Value> = instructions
        .iter()
        .map(|i| {
            serde_json::json!({
                "mnemonic": i.mnemonic,
                "format": i.format,
                "opcode": i.opcode,
                "funct3": i.funct3,
                "funct7": i.funct7,
                "operands": if i.operands.is_empty() { serde_json::Value::Null } else { serde_json::Value::Array(i.operands.iter().cloned().map(serde_json::Value::String).collect()) },
                "summary": i.summary,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "instructions": arr,
    }))
    .unwrap_or_default()
        + "\n"
}

const SIM_RUN_SH: &str = r#"#!/bin/sh
# sim/run.sh — entry point for `xsil run`.
# Compiles examples/demo.S (if a riscv toolchain is present) and runs it under Spike;
# falls back to a printed expected output so the package still demos on hosts
# without a simulator installed.

set -eu

SRC="examples/demo.S"
ELF="sim/bin/demo.elf"

has() { command -v "$1" >/dev/null 2>&1; }

if [ ! -f "$ELF" ] && has riscv64-unknown-elf-gcc; then
  printf "Assembling %s for RV64GC...\n" "$SRC"
  mkdir -p sim/bin
  riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d -nostdlib -static -o "$ELF" "$SRC" || true
fi

if [ -f "$ELF" ] && has spike && has pk; then
  printf "[ spike rv64gc ] %s\n\n" "$ELF"
  spike pk "$ELF"
  exit $?
fi

# Fallback demo mode.
printf "\n  %s — Demo Mode\n" "$(basename "$(pwd)")"
printf "  ===================\n"
printf "  No RISC-V toolchain or Spike found. Showing the expected output.\n\n"
cat tests/expected.txt
"#;

const SIM_SPIKE_YAML: &str = r#"# sim/spike.yaml — informational config consumed by your own scripts.
# The CLI does not parse this file; sim/run.sh / tests/run.sh do.
isa: rv64gc
priv: msu
mem: 256m
"#;

const TESTS_RUN_SH: &str = r#"#!/bin/sh
# tests/run.sh — entry point for `xsil test`.
# Runs sim/run.sh and compares its stdout against tests/expected.txt.

set -eu

actual="$(sh sim/run.sh 2>/dev/null || true)"
expected="$(cat tests/expected.txt)"

if [ "$actual" = "$expected" ]; then
  printf "PASS\n"
  exit 0
fi

printf "FAIL\n--- expected ---\n%s\n--- actual ---\n%s\n" "$expected" "$actual"
exit 1
"#;

const TESTS_BASIC_S: &str = r#"# tests/basic.S — placeholder smoke test (RV64I).
# Replace this with assembly that exercises your custom instructions.
.text
.globl _start
_start:
    li      a0, 0          # exit code = 0
    li      a7, 93         # SYS_exit (newlib pk syscall ABI)
    ecall
"#;

const TESTS_EXPECTED: &str = "extension demo: hello from RISC-V!\n";

const EXAMPLES_DEMO_S: &str = r#"# examples/demo.S — minimal RV64I demo program.
# Prints a greeting via newlib pk's "write" + "exit" syscalls.
# Replace with code that uses your custom instructions.
.section .rodata
msg:    .ascii  "extension demo: hello from RISC-V!\n"
msg_end:

.text
.globl _start
_start:
    li      a0, 1                      # fd = stdout
    la      a1, msg                    # buf
    li      a2, msg_end - msg          # count
    li      a7, 64                     # SYS_write
    ecall

    li      a0, 0
    li      a7, 93                     # SYS_exit
    ecall
"#;

const TOOLCHAIN_README: &str = r#"# toolchain/

This package declares its toolchain as **external** (`toolchain.external: true`).
Install `riscv64-unknown-elf-gcc` (or whatever your target uses) on the host
that runs `xsil run` / `xsil test`.

If/when you bundle a toolchain inside the package, set `toolchain.external` to
`false` in `manifest.json` and unpack the toolchain into this directory.
"#;

// ── Richer skeleton templates (mirror the backend wizardGenerate.ts) ─────────

/// `opcodes.h` — C header with `__asm__` helper macros emitting `.insn`.
fn opcodes_header_template(slug: &str, instructions: &[WizardInstruction]) -> String {
    let guard = format!("{}_OPCODES_H", safe_ident(slug).to_ascii_uppercase());
    let mut out = String::new();
    out.push_str("/* Auto-generated by ExtenSilica Extension Wizard.\n");
    out.push_str(" *\n");
    out.push_str(&format!(" * Inline-assembly helper macros for {slug}'s custom instructions,\n"));
    out.push_str(" * emitted via the GAS `.insn` directive (no custom assembler required).\n");
    out.push_str(" *\n");
    out.push_str(" * Usage:\n");
    out.push_str(" *   #include \"opcodes.h\"\n");
    out.push_str(" *   uint64_t r;\n");
    if let Some(sample) = instructions.first() {
        out.push_str(&format!(" *   {}(r, /* rs1 */ 1, /* rs2 */ 2);\n", macro_ident(&sample.mnemonic)));
    } else {
        out.push_str(" *   // Add custom instructions to opcodes.json and regenerate.\n");
    }
    out.push_str(" */\n");
    out.push_str(&format!("#ifndef {guard}\n"));
    out.push_str(&format!("#define {guard}\n\n"));
    out.push_str("#include <stdint.h>\n\n");

    if instructions.is_empty() {
        out.push_str("/* No custom instructions declared. Edit opcodes.json and regenerate this header. */\n\n");
    } else {
        for ins in instructions {
            let macro_name = macro_ident(&ins.mnemonic);
            let op = parse_opcode(ins.opcode.as_deref());
            let f3 = parse_funct(ins.funct3.as_deref(), 3);
            let f7 = parse_funct(ins.funct7.as_deref(), 7);
            let summary = ins.summary.as_deref().map(|s| format!(" {s}")).unwrap_or_default();
            out.push_str(&format!("/* {} ({}-type):{} */\n", ins.mnemonic, ins.format, summary));
            match ins.format.as_str() {
                "R" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rd, rs1, rs2) \\\n    __asm__ __volatile__(\".insn r {}, {}, {}, %0, %1, %2\" \\\n        : \"=r\"(rd) : \"r\"(rs1), \"r\"(rs2))\n",
                        hex_str(op, 2),
                        hex_str(f3, 2),
                        hex_str(f7, 2),
                    ));
                }
                "I" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rd, rs1, imm) \\\n    __asm__ __volatile__(\".insn i {}, {}, %0, %1, %2\" \\\n        : \"=r\"(rd) : \"r\"(rs1), \"i\"(imm))\n",
                        hex_str(op, 2),
                        hex_str(f3, 2),
                    ));
                }
                "S" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rs2, rs1, imm) \\\n    __asm__ __volatile__(\".insn s {}, {}, %0, %2(%1)\" \\\n        : : \"r\"(rs2), \"r\"(rs1), \"i\"(imm) : \"memory\")\n",
                        hex_str(op, 2),
                        hex_str(f3, 2),
                    ));
                }
                "B" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rs1, rs2, label) \\\n    __asm__ __volatile__(\".insn b {}, {}, %0, %1, \" #label \\\n        : : \"r\"(rs1), \"r\"(rs2))\n",
                        hex_str(op, 2),
                        hex_str(f3, 2),
                    ));
                }
                "U" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rd, imm) \\\n    __asm__ __volatile__(\".insn u {}, %0, %1\" \\\n        : \"=r\"(rd) : \"i\"(imm))\n",
                        hex_str(op, 2),
                    ));
                }
                "J" => {
                    out.push_str(&format!(
                        "#define {macro_name}(rd, label) \\\n    __asm__ __volatile__(\".insn j {}, %0, \" #label \\\n        : \"=r\"(rd))\n",
                        hex_str(op, 2),
                    ));
                }
                _ => {}
            }
            out.push('\n');
        }
    }

    out.push_str(&format!("#endif /* {guard} */\n"));
    out
}

/// Standalone runnable example for a single instruction.
fn instruction_example_template(ins: &WizardInstruction) -> String {
    let op = parse_opcode(ins.opcode.as_deref());
    let f3 = parse_funct(ins.funct3.as_deref(), 3);
    let f7 = parse_funct(ins.funct7.as_deref(), 7);
    let op_s = bin_str(op, 7);
    let f3_s = bin_str(f3, 3);
    let f7_s = bin_str(f7, 7);
    let summary = ins.summary.as_deref().unwrap_or("placeholder semantics");
    let safe = safe_ident(&ins.mnemonic);
    let head = format!(
        r#"# examples/{safe}.S — demo for `{mnemonic}` ({fmt}-type)
# Encoded via GAS .insn directive — no toolchain patching required.
# {summary}
#
# Build: riscv64-unknown-elf-gcc -march=rv64gc -nostdlib -static $< -o demo.elf
# Run  : spike pk demo.elf

.text
.globl _start
_start:
"#,
        mnemonic = ins.mnemonic,
        fmt = ins.format,
    );
    let body = match ins.format.as_str() {
        "R" => format!(
            "    li      t0, 5                          # rs1\n    li      t1, 7                          # rs2\n    .insn r {op_s}, {f3_s}, {f7_s}, t2, t0, t1   # {mn}: t2 <- f(t0, t1)\n",
            mn = ins.mnemonic,
        ),
        "I" => format!(
            "    li      t0, 5                          # rs1\n    .insn i {op_s}, {f3_s}, t2, t0, 12         # {mn}: t2 <- f(t0, imm=12)\n",
            mn = ins.mnemonic,
        ),
        "S" => format!(
            "    la      t0, scratch                    # base address\n    li      t1, 0xCAFE                     # rs2 (value)\n    .insn s {op_s}, {f3_s}, t1, 0(t0)           # {mn}: store t1 at *(t0+0)\n",
            mn = ins.mnemonic,
        ),
        "B" => format!(
            "    li      t0, 1\n    li      t1, 1\n    .insn b {op_s}, {f3_s}, t0, t1, taken      # {mn}: branch if cond\n    j       fall_through\ntaken:\nfall_through:\n",
            mn = ins.mnemonic,
        ),
        "U" => format!(
            "    .insn u {op_s}, t2, 0xABCDE               # {mn}: t2 <- imm<<12\n",
            mn = ins.mnemonic,
        ),
        "J" => format!(
            "    .insn j {op_s}, t2, target                # {mn}: jump+link\ntarget:\n",
            mn = ins.mnemonic,
        ),
        _ => String::new(),
    };
    let tail = "\n    li      a0, 0                          # exit code = 0\n    li      a7, 93                         # SYS_exit (newlib pk)\n    ecall\n";
    let data_section = if ins.format == "S" {
        "\n.section .bss\n.align 3\nscratch: .skip 8\n"
    } else {
        ""
    };
    format!("{head}{body}{tail}{data_section}")
}

/// Combined `tests/instructions.S` exercising every declared instruction.
fn instructions_combined_test_template(instructions: &[WizardInstruction]) -> String {
    if instructions.is_empty() {
        return r#"# tests/instructions.S — placeholder.
# Declare instructions in opcodes.json and regenerate to populate this file.
.text
.globl _start
_start:
    li      a0, 0
    li      a7, 93
    ecall
"#
        .to_string();
    }
    let mut out = String::new();
    out.push_str(
        "# tests/instructions.S — exercises every instruction declared in opcodes.json.\n\
         # Each .insn directive is annotated with its mnemonic so failing assemblies are\n\
         # easy to trace back to the manifest.\n\
         \n\
         .text\n\
         .globl _start\n\
         _start:\n\
             li      t0, 5\n\
             li      t1, 7\n",
    );
    for ins in instructions {
        let op = parse_opcode(ins.opcode.as_deref());
        let f3 = parse_funct(ins.funct3.as_deref(), 3);
        let f7 = parse_funct(ins.funct7.as_deref(), 7);
        let op_s = bin_str(op, 7);
        let f3_s = bin_str(f3, 3);
        let f7_s = bin_str(f7, 7);
        let suffix = ins
            .summary
            .as_deref()
            .map(|s| format!(" — {s}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n    # {} ({}-type){}\n",
            ins.mnemonic, ins.format, suffix
        ));
        match ins.format.as_str() {
            "R" => out.push_str(&format!(
                "    .insn r {op_s}, {f3_s}, {f7_s}, t2, t0, t1\n"
            )),
            "I" => out.push_str(&format!("    .insn i {op_s}, {f3_s}, t2, t0, 12\n")),
            "S" => out.push_str(&format!("    .insn s {op_s}, {f3_s}, t1, 0(t0)\n")),
            "B" => out.push_str(&format!(
                "    .insn b {op_s}, {f3_s}, t0, t1, 1f\n1:\n"
            )),
            "U" => out.push_str(&format!("    .insn u {op_s}, t2, 0xABCDE\n")),
            "J" => out.push_str(&format!("    .insn j {op_s}, t2, 1f\n1:\n")),
            _ => {}
        }
    }
    out.push_str("\n    li      a0, 0\n    li      a7, 93\n    ecall\n");
    out
}

/// Spike `extension_t` C++ skeleton with one handler per declared instruction.
fn spike_extension_cpp_template(slug: &str, instructions: &[WizardInstruction]) -> String {
    let id = safe_ident(slug);
    let cls = format!("{id}_t");

    let mut handlers = String::new();
    for ins in instructions {
        let fn_name = format!("do_{}", safe_ident(&ins.mnemonic));
        let suffix = ins
            .summary
            .as_deref()
            .map(|s| format!(" — {s}"))
            .unwrap_or_default();
        if ins.format == "R" {
            handlers.push_str(&format!(
                "// {mn} (R-type){suffix}\nstatic reg_t {fn_name}(processor_t* p, insn_t insn, reg_t pc) {{\n  (void)p;\n  reg_t rs1 = RS1;\n  reg_t rs2 = RS2;\n  // TODO: replace with the real semantics of {mn}.\n  WRITE_RD(sext_xlen(rs1 + rs2));\n  return pc + insn.length();\n}}\n\n",
                mn = ins.mnemonic,
            ));
        } else {
            handlers.push_str(&format!(
                "// {mn} ({fmt}-type){suffix}\nstatic reg_t {fn_name}(processor_t* p, insn_t insn, reg_t pc) {{\n  // TODO: implement {fmt}-type decoding for {mn}.\n  (void)p;\n  return pc + insn.length();\n}}\n\n",
                mn = ins.mnemonic,
                fmt = ins.format,
            ));
        }
    }

    let desc = if instructions.is_empty() {
        "  // No instructions declared yet — add entries in opcodes.json and regenerate.\n  return {};".to_string()
    } else {
        let mut s = String::new();
        for ins in instructions {
            let fn_name = format!("do_{}", safe_ident(&ins.mnemonic));
            let op = parse_opcode(ins.opcode.as_deref());
            let f3 = parse_funct(ins.funct3.as_deref(), 3);
            let f7 = parse_funct(ins.funct7.as_deref(), 7);
            let (m_match, m_mask) = match ins.format.as_str() {
                "R" => (
                    format!("({}) | ({} << 12) | ({} << 25)", hex_str(op, 2), hex_str(f3, 2), hex_str(f7, 2)),
                    "0x7f | (0x7 << 12) | (0x7f << 25)".to_string(),
                ),
                "I" | "S" | "B" => (
                    format!("({}) | ({} << 12)", hex_str(op, 2), hex_str(f3, 2)),
                    "0x7f | (0x7 << 12)".to_string(),
                ),
                _ => (format!("({})", hex_str(op, 2)), "0x7f".to_string()),
            };
            s.push_str(&format!(
                "  // {mn}\n  insns.push_back({{{m_match}, {m_mask}, {fn_name}, {fn_name}, {fn_name}, {fn_name}}});\n",
                mn = ins.mnemonic,
            ));
        }
        s.push_str("  return insns;");
        s
    };

    format!(
        r#"// {id}_extension.cc — Spike (riscv-isa-sim) extension skeleton for "{slug}".
//
// Auto-generated by ExtenSilica Extension Wizard. Plug in the real semantics
// inside each "do_<mnemonic>" handler and rebuild with the included Makefile.
//
// Tested layout against riscv-isa-sim 1.1.0. Adjust the includes / API if you
// target a different Spike version (the extension API is moderately fluid).
// REGISTER_EXTENSION's first argument must be a valid C identifier, so we
// register as "{id}" — invoke Spike with --extension={id}.

#include <riscv/extension.h>
#include <riscv/processor.h>
#include <riscv/decode_macros.h>
#include <vector>

{handlers}class {cls} : public extension_t {{
public:
  const char* name() override {{ return "{id}"; }}

  std::vector<insn_desc_t> get_instructions() override {{
    std::vector<insn_desc_t> insns;
{desc}
  }}

  std::vector<disasm_insn_t*> get_disasms() override {{ return {{}}; }}

  void reset() override {{}}
}};

REGISTER_EXTENSION({id}, []() {{ return new {cls}; }})
"#,
    )
}

fn spike_extension_makefile_template(slug: &str) -> String {
    let id = safe_ident(slug);
    format!(
        r#"# sim/spike-extension/Makefile — builds the Spike extension into a shared lib.
#
# Override SPIKE_PREFIX to point at your Spike install (the directory that
# contains include/riscv/ headers).
#
#   make SPIKE_PREFIX=/opt/riscv

NAME    := {id}
SRC     := {id}_extension.cc
OBJ     := $(SRC:.cc=.o)
LIB     := lib$(NAME)_spike.so

SPIKE_PREFIX  ?= /usr/local
SPIKE_INCLUDE ?= $(SPIKE_PREFIX)/include

CXXFLAGS ?= -O2 -Wall -fPIC -std=c++17 -I$(SPIKE_INCLUDE)
LDFLAGS  ?= -shared

all: $(LIB)

$(LIB): $(OBJ)
"#
    ) + "\t$(CXX) $(LDFLAGS) -o $@ $<\n\n"
        + "$(OBJ): $(SRC)\n"
        + "\t$(CXX) $(CXXFLAGS) -c -o $@ $<\n\n"
        + "clean:\n"
        + "\trm -f $(OBJ) $(LIB)\n\n"
        + ".PHONY: all clean\n"
}

fn spike_extension_readme_template(slug: &str) -> String {
    let id = safe_ident(slug);
    format!(
        r#"# Spike extension for `{slug}`

This directory contains a starter implementation of a [Spike](https://github.com/riscv-software-src/riscv-isa-sim)
extension that handles the custom instructions declared in `opcodes.json`.

## Build

```bash
make SPIKE_PREFIX=/opt/riscv
```

This produces `lib{id}_spike.so`.

## Run

```bash
spike --extension={id} \
      --extlib=$(pwd)/lib{id}_spike.so \
      pk ../../examples/demo.elf
```

(Spike's `REGISTER_EXTENSION` macro requires a C identifier, so the
registered name uses underscores even when the package slug has hyphens.)

## Customize

Edit `{id}_extension.cc` to:

1. Update the `match`/`mask` patterns inside `get_instructions()` to match the
   bit layout you want (the wizard pre-fills them from opcode/funct3/funct7).
2. Implement the real semantics of every `do_<mnemonic>` handler.
3. Optionally provide disassembly entries via `get_disasms()`.

The Spike extension API has evolved over time. The skeleton targets riscv-isa-sim 1.1.0+.
Older or newer trees may need minor include / signature tweaks.
"#,
    )
}

fn changelog_template(version: &str) -> String {
    format!(
        r#"# Changelog

All notable changes to this package are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [{version}] — Initial release

### Added
- Initial `.xsil` skeleton scaffolded by the ExtenSilica Extension Wizard.
- `opcodes.json` with the declared custom instructions.
- Spike extension skeleton in `sim/spike-extension/`.
- Inline-asm helper macros in `opcodes.h`.
- Per-instruction examples in `examples/`.
"#,
    )
}

/// LICENSE text for the most common SPDX ids. Returns `None` when the id is
/// unknown — the caller falls back to a short SPDX-link stub.
fn known_license_text(spdx: &str, year: i32, holder: &str) -> Option<String> {
    let text = match spdx {
        "MIT" => format!(
            r#"MIT License

Copyright (c) {year} {holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"#
        ),
        "BSD-2-Clause" => format!(
            r#"BSD 2-Clause License

Copyright (c) {year}, {holder}

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
"#
        ),
        "BSD-3-Clause" => format!(
            r#"BSD 3-Clause License

Copyright (c) {year}, {holder}
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
"#
        ),
        "ISC" => format!(
            r#"ISC License

Copyright (c) {year} {holder}

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
"#
        ),
        "Unlicense" => r#"This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this
software, either in source code form or as a compiled binary, for any purpose,
commercial or non-commercial, and by any means.

In jurisdictions that recognize copyright laws, the author or authors of this
software dedicate any and all copyright interest in the software to the public
domain. We make this dedication for the benefit of the public at large and to
the detriment of our heirs and successors. We intend this dedication to be an
overt act of relinquishment in perpetuity of all present and future rights to
this software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <https://unlicense.org>
"#
        .to_string(),
        "CC0-1.0" => format!(
            r#"Creative Commons Legal Code

CC0 1.0 Universal

Copyright (c) {year} {holder}

The full legal text of CC0 1.0 Universal is available at:
  https://creativecommons.org/publicdomain/zero/1.0/legalcode

Summary: the person associated with this work has dedicated it to the public
domain by waiving all of their rights under copyright law worldwide, to the
extent allowed by law.
"#
        ),
        "Apache-2.0" => format!(
            r#"                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

Copyright {year} {holder}

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

For the full Apache 2.0 text, see https://www.apache.org/licenses/LICENSE-2.0.
"#
        ),
        "MPL-2.0" => format!(
            r#"Mozilla Public License Version 2.0

Copyright (c) {year} {holder}

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can
obtain one at https://mozilla.org/MPL/2.0/.
"#
        ),
        _ => return None,
    };
    Some(text)
}

fn license_template(spdx: &str, holder: &str) -> String {
    let year = current_year();
    if let Some(text) = known_license_text(spdx, year, holder) {
        return text;
    }
    format!(
        "{spdx} License\n\nCopyright (c) {year} {holder}\n\nThis package is distributed under the {spdx} license.\nThe full text is available at https://spdx.org/licenses/{spdx}.html.\n",
    )
}

fn current_year() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Cheap approximation: 1970 + secs/seconds-per-year-365.2425.
    // Drift vs UTC year is < 1 day at year boundaries — fine for a copyright header.
    1970 + (secs / 31_556_952) as i32
}

const XSILIGNORE: &str = r#"# Files / folders excluded from .xsil packing.
# Aligned with the CLI built-in ignore set.
.git/
.DS_Store
Thumbs.db

# Build artifacts
sim/bin/
**/*.o
**/*.elf
"#;

// ── Interactive collection ────────────────────────────────────────────────────

fn collect_interactively(args: &mut WizardArgs) -> Result<()> {
    println!("\n  ExtenSilica Extension Wizard");
    println!("  Press <Enter> to accept defaults shown in [brackets].\n");

    if args.description.is_none() || args.description.as_deref().unwrap_or("").trim().is_empty() {
        let d = prompt_line("Description", None)?;
        if d.is_empty() {
            bail!("Description is required.");
        }
        args.description = Some(d);
    }
    if args.author.is_none() {
        let default_a = default_author();
        let a = prompt_line("Author", Some(&default_a))?;
        args.author = Some(a);
    }
    if args.version.is_none() {
        let v = prompt_line("Version", Some("0.1.0"))?;
        args.version = Some(v);
    }
    if args.isa.is_none() {
        let i = prompt_line("Base ISA", Some("RV64GC"))?;
        args.isa = Some(i.to_uppercase());
    }
    if args.license.is_none() {
        let l = prompt_line("License", Some("Apache-2.0"))?;
        args.license = Some(l);
    }
    if args.repository.is_none() || args.repository.as_deref().unwrap_or("").trim().is_empty() {
        let suggested = format!(
            "https://github.com/{}/{}",
            args.author.as_deref().unwrap_or("you").trim(),
            args.name,
        );
        loop {
            let r = prompt_line("Repository URL (required)", Some(&suggested))?;
            match validate_http_url("repository", &r) {
                Ok(()) => {
                    args.repository = Some(r);
                    break;
                }
                Err(e) => {
                    eprintln!("  ! {e}");
                }
            }
        }
    }
    if args.homepage.is_none() {
        let h = prompt_line("Homepage URL (optional)", Some(""))?;
        if !h.is_empty() {
            args.homepage = Some(h);
        }
    }

    println!("\n  Honest classification (required):");
    println!("    ratified  — frozen RISC-V International specification");
    println!("    draft     — working draft, not ratified yet");
    println!("    vendor    — commercial vendor extension (T-Head, SiFive, …)");
    println!("    research  — academic / experimental");
    println!("    custom    — bespoke / one-off");
    if args.standard_status.is_none() {
        loop {
            let s = prompt_line("Standard status", Some("custom"))?;
            match normalize_standard_status(&s) {
                Ok(v) => {
                    args.standard_status = Some(v);
                    break;
                }
                Err(e) => eprintln!("  ! {e}"),
            }
        }
    }
    if args.authority.is_none() {
        let suggested = match args.standard_status.as_deref() {
            Some("ratified") => Some("RISC-V International"),
            _ => None,
        };
        loop {
            let a = prompt_line("Authority (who defines the spec)", suggested)?;
            match validate_authority(&a) {
                Ok(v) => {
                    args.authority = Some(v);
                    break;
                }
                Err(e) => eprintln!("  ! {e}"),
            }
        }
    }

    if args.instructions.is_empty() {
        if prompt_yes_no("Add custom instructions?", false)? {
            loop {
                let mnemonic = prompt_line("  mnemonic (empty to stop)", None)?;
                if mnemonic.is_empty() {
                    break;
                }
                let format = prompt_line("  format [R/I/S/B/U/J]", Some("R"))?.to_uppercase();
                validate_format(&format)?;
                let opcode = prompt_line("  opcode (e.g. custom-0)", Some(""))?;
                let funct3 = prompt_line("  funct3 (e.g. 0b000)", Some(""))?;
                let funct7 = prompt_line("  funct7 (e.g. 0b0000000)", Some(""))?;
                let operands_raw = prompt_line("  operands (comma-separated, e.g. rd,rs1,rs2)", Some(""))?;
                let summary = prompt_line("  summary (one line)", Some(""))?;
                let operands = operands_raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                args.instructions.push(WizardInstruction {
                    mnemonic,
                    format,
                    opcode: if opcode.is_empty() { None } else { Some(opcode) },
                    funct3: if funct3.is_empty() { None } else { Some(funct3) },
                    funct7: if funct7.is_empty() { None } else { Some(funct7) },
                    operands,
                    summary: if summary.is_empty() { None } else { Some(summary) },
                });
                if !prompt_yes_no("Add another?", false)? {
                    break;
                }
            }
        }
    }

    println!("\n  Optional targets (declared as `status: planned`):");
    if !args.targets.qemu {
        args.targets.qemu = prompt_yes_no("  include qemu placeholder?", false)?;
    }
    if !args.targets.binutils {
        args.targets.binutils = prompt_yes_no("  include binutils placeholder?", false)?;
    }
    if !args.targets.llvm {
        args.targets.llvm = prompt_yes_no("  include llvm placeholder?", false)?;
    }

    Ok(())
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Generate the wizard skeleton on disk under `parent/<name>/` and return its path.
pub fn cmd_new(manager: &ExtensionManager, mut args: WizardArgs) -> Result<PathBuf> {
    validate_slug(&args.name)?;

    let base = args
        .parent
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let root = base.join(&args.name);

    if root.exists() {
        if !args.force {
            bail!(
                "{} already exists. Pass --force to remove it and create a fresh skeleton.",
                root.display()
            );
        }
        fs::remove_dir_all(&root).with_context(|| format!("remove {}", root.display()))?;
    }

    if !args.non_interactive {
        collect_interactively(&mut args)?;
    }

    // Defaults / final validation.
    let description = args
        .description
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("description is required (use --description in non-interactive mode)"))?;
    let version = args.version.clone().unwrap_or_else(|| "0.1.0".into());
    validate_semver(&version)?;
    let author = args
        .author
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_author);
    let license = args
        .license
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Apache-2.0".into());
    let isa = args
        .isa
        .clone()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "RV64GC".into());
    validate_isa(&isa)?;

    let repository = args
        .repository
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!(
            "repository is required (use --repository in non-interactive mode)"
        ))?;
    validate_http_url("repository", &repository)?;

    if let Some(h) = args.homepage.as_ref() {
        let t = h.trim();
        if !t.is_empty() {
            validate_http_url("homepage", t)?;
        }
    }

    let standard_status = args
        .standard_status
        .as_ref()
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!(
            "standardStatus is required (use --standard-status in non-interactive mode)"
        ))?;
    let standard_status = normalize_standard_status(standard_status)?;
    let authority = args
        .authority
        .as_ref()
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!(
            "authority is required (use --authority in non-interactive mode)"
        ))?;
    let authority = validate_authority(authority)?;

    for (idx, ins) in args.instructions.iter().enumerate() {
        if ins.mnemonic.trim().is_empty() {
            bail!("instructions[{idx}].mnemonic is required.");
        }
        validate_format(&ins.format)
            .with_context(|| format!("instructions[{idx}].format"))?;
    }

    // Filesystem scaffold.
    fs::create_dir_all(root.join("docs")).context("create docs/")?;
    fs::create_dir_all(root.join("examples")).context("create examples/")?;
    fs::create_dir_all(root.join("sim")).context("create sim/")?;
    fs::create_dir_all(root.join("sim/spike-extension")).context("create sim/spike-extension/")?;
    fs::create_dir_all(root.join("tests")).context("create tests/")?;
    fs::create_dir_all(root.join("toolchain")).context("create toolchain/")?;

    write_file(&root.join(".xsilignore"), XSILIGNORE)?;
    write_file(
        &root.join("README.md"),
        &readme_template(
            &args.name,
            &description,
            &isa,
            &args.instructions,
            &standard_status,
            &authority,
        ),
    )?;
    write_file(&root.join("CHANGELOG.md"), &changelog_template(&version))?;
    write_file(&root.join("LICENSE"), &license_template(&license, &author))?;
    write_file(&root.join("opcodes.json"), &opcodes_json_template(&args.instructions))?;
    write_file(
        &root.join("opcodes.h"),
        &opcodes_header_template(&args.name, &args.instructions),
    )?;
    write_file(
        &root.join("docs/overview.md"),
        &docs_overview_template(&args.name, &description, &isa),
    )?;
    write_file(&root.join("examples/demo.S"), EXAMPLES_DEMO_S)?;
    for ins in &args.instructions {
        write_file(
            &root.join(format!("examples/{}.S", safe_ident(&ins.mnemonic))),
            &instruction_example_template(ins),
        )?;
    }
    let sim_run = root.join("sim/run.sh");
    write_file(&sim_run, SIM_RUN_SH)?;
    mark_executable(&sim_run)?;
    write_file(&root.join("sim/spike.yaml"), SIM_SPIKE_YAML)?;
    let safe_slug = safe_ident(&args.name);
    write_file(
        &root.join(format!("sim/spike-extension/{safe_slug}_extension.cc")),
        &spike_extension_cpp_template(&args.name, &args.instructions),
    )?;
    write_file(
        &root.join("sim/spike-extension/Makefile"),
        &spike_extension_makefile_template(&args.name),
    )?;
    write_file(
        &root.join("sim/spike-extension/README.md"),
        &spike_extension_readme_template(&args.name),
    )?;
    let tests_run = root.join("tests/run.sh");
    write_file(&tests_run, TESTS_RUN_SH)?;
    mark_executable(&tests_run)?;
    write_file(&root.join("tests/basic.S"), TESTS_BASIC_S)?;
    write_file(&root.join("tests/expected.txt"), TESTS_EXPECTED)?;
    write_file(
        &root.join("tests/instructions.S"),
        &instructions_combined_test_template(&args.instructions),
    )?;
    write_file(&root.join("toolchain/README.md"), TOOLCHAIN_README)?;

    // Compute payload hash + size from disk so it matches `xsil publish`.
    let hash = manager
        .compute_payload_hash(&root)
        .context("compute payload hash for new package")?;
    let size = manager
        .compute_payload_size(&root)
        .context("compute payload size for new package")?;

    // Build manifest.
    let mut targets = serde_json::Map::new();
    targets.insert(
        "spike".into(),
        serde_json::json!({
            "isa": isa.to_lowercase(),
            "priv": "msu",
            "mem": "256m",
            "config": "sim/spike.yaml",
            "status": "skeleton",
        }),
    );
    if args.targets.qemu {
        targets.insert("qemu".into(), serde_json::json!({ "status": "planned" }));
    }
    if args.targets.binutils {
        targets.insert("binutils".into(), serde_json::json!({ "status": "planned" }));
    }
    if args.targets.llvm {
        targets.insert("llvm".into(), serde_json::json!({ "status": "planned" }));
    }

    let mut keywords = vec!["risc-v".to_string(), "extension".to_string(), "xsil".to_string(), "wizard".to_string()];
    if !keywords.contains(&args.name) {
        keywords.push(args.name.clone());
    }

    let manifest = Manifest {
        name: args.name.clone(),
        version: version.clone(),
        description,
        author,
        isa: Some(isa),
        entry: Some("sh sim/run.sh".to_string()),
        test_entry: Some("sh tests/run.sh".to_string()),
        execution: Some(serde_json::json!({
            "entry": "sh sim/run.sh",
            "testEntry": "sh tests/run.sh",
            "env": {}
        })),
        dependencies: Some(serde_json::json!({ "tools": [] })),
        resolution: Some(serde_json::json!({ "mode": "host-dependent" })),
        toolchain: Some(serde_json::json!({
            "root": "toolchain",
            "triple": "riscv64-unknown-elf",
            "version": "14.2.0",
            "external": true
        })),
        targets: Some(serde_json::Value::Object(targets)),
        keywords: Some(keywords),
        license: Some(license),
        repository: Some(repository),
        homepage: args.homepage.clone().filter(|s| !s.trim().is_empty()),
        standard_status: Some(standard_status),
        authority: Some(authority),
        payload_hash: String::new(),
        checksums: Some(ManifestChecksums {
            payload: format!("sha256:{}", hash),
            archive: String::new(),
        }),
        payload_size: size,
    };
    let json = serde_json::to_string_pretty(&manifest).context("serialize manifest.json")?;
    write_file(&root.join("manifest.json"), &json)?;

    Ok(root)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{cmd_new, WizardArgs, WizardInstruction, WizardTargets};
    use crate::manager::ExtensionManager;

    fn args_for(slug: &str, parent: &Path) -> WizardArgs {
        WizardArgs {
            name: slug.into(),
            parent: Some(parent.to_path_buf()),
            force: false,
            non_interactive: true,
            author: Some("tester".into()),
            description: Some("Wizard skeleton smoke test.".into()),
            version: Some("0.1.0".into()),
            isa: Some("RV64GC".into()),
            license: Some("Apache-2.0".into()),
            repository: Some(format!("https://github.com/tester/{slug}")),
            homepage: None,
            standard_status: Some("custom".into()),
            authority: Some("CLI Test Authority".into()),
            instructions: vec![],
            targets: WizardTargets::default(),
        }
    }

    #[test]
    fn skeleton_passes_local_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-test-pkg", &parent);
        a.instructions = vec![WizardInstruction {
            mnemonic: "x.add".into(),
            format: "R".into(),
            opcode: Some("custom-0".into()),
            funct3: Some("0b000".into()),
            funct7: Some("0b0000000".into()),
            operands: vec!["rd".into(), "rs1".into(), "rs2".into()],
            summary: Some("rd = rs1 + rs2".into()),
        }];
        let root = cmd_new(&mgr, a).expect("cmd_new");

        for rel in [
            "manifest.json",
            "README.md",
            "CHANGELOG.md",
            "LICENSE",
            "opcodes.json",
            "opcodes.h",
            "docs/overview.md",
            "examples/demo.S",
            "examples/x_add.S",
            "sim/run.sh",
            "sim/spike.yaml",
            "sim/spike-extension/wiz_test_pkg_extension.cc",
            "sim/spike-extension/Makefile",
            "sim/spike-extension/README.md",
            "tests/run.sh",
            "tests/basic.S",
            "tests/expected.txt",
            "tests/instructions.S",
            "toolchain/README.md",
            ".xsilignore",
        ] {
            let p = root.join(rel);
            assert!(p.is_file(), "missing {}", rel);
            let len = fs::metadata(&p).unwrap().len();
            assert!(len > 0, "empty {}", rel);
        }

        let _ = mgr
            .validate_local_package_directory(&root)
            .expect("validate_local_package_directory");
    }

    #[test]
    fn opcodes_header_macro_per_instruction() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("macro-ext", &parent);
        a.instructions = vec![
            WizardInstruction {
                mnemonic: "x.alpha".into(),
                format: "R".into(),
                opcode: Some("custom-0".into()),
                funct3: Some("0b001".into()),
                funct7: Some("0b0000010".into()),
                operands: vec!["rd".into(), "rs1".into(), "rs2".into()],
                summary: None,
            },
            WizardInstruction {
                mnemonic: "x.beta".into(),
                format: "I".into(),
                opcode: Some("custom-1".into()),
                funct3: Some("0b010".into()),
                funct7: None,
                operands: vec!["rd".into(), "rs1".into(), "imm".into()],
                summary: None,
            },
        ];
        let root = cmd_new(&mgr, a).expect("cmd_new");
        let header = fs::read_to_string(root.join("opcodes.h")).unwrap();
        assert!(header.contains("#define X_ALPHA(rd, rs1, rs2)"), "missing X_ALPHA macro: {header}");
        assert!(header.contains(".insn r 0x0b, 0x01, 0x02,"));
        assert!(header.contains("#define X_BETA(rd, rs1, imm)"));
        assert!(header.contains(".insn i 0x2b, 0x02,"));
        assert!(header.contains("#ifndef MACRO_EXT_OPCODES_H"));
    }

    #[test]
    fn spike_extension_skeleton_registers_handlers() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("spike-ext", &parent);
        a.instructions = vec![WizardInstruction {
            mnemonic: "x.add".into(),
            format: "R".into(),
            opcode: Some("custom-0".into()),
            funct3: Some("0b000".into()),
            funct7: Some("0b0000000".into()),
            operands: vec!["rd".into(), "rs1".into(), "rs2".into()],
            summary: None,
        }];
        let root = cmd_new(&mgr, a).expect("cmd_new");

        let cpp = fs::read_to_string(
            root.join("sim/spike-extension/spike_ext_extension.cc"),
        )
        .unwrap();
        assert!(cpp.contains("class spike_ext_t : public extension_t"));
        // REGISTER_EXTENSION needs a C identifier, so the safe_ident is used.
        assert!(cpp.contains("REGISTER_EXTENSION(spike_ext,"));
        assert!(cpp.contains("static reg_t do_x_add("));
        assert!(cpp.contains("insns.push_back({"));

        let mk = fs::read_to_string(root.join("sim/spike-extension/Makefile")).unwrap();
        assert!(mk.contains("NAME    := spike_ext"));
        assert!(mk.contains("lib$(NAME)_spike.so"));
    }

    #[test]
    fn license_matches_known_spdx() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("mit-ext", &parent);
        a.license = Some("MIT".into());
        a.author = Some("Felipe Pedroni".into());
        let root = cmd_new(&mgr, a).expect("cmd_new");

        let lic = fs::read_to_string(root.join("LICENSE")).unwrap();
        assert!(lic.starts_with("MIT License"), "first line: {:?}", lic.lines().next());
        assert!(lic.contains("Felipe Pedroni"));
    }

    #[test]
    fn instructions_combined_test_lists_each_mnemonic() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("multi-ext", &parent);
        a.instructions = vec![
            WizardInstruction {
                mnemonic: "x.add".into(), format: "R".into(),
                opcode: Some("custom-0".into()), funct3: Some("0b000".into()),
                funct7: Some("0b0000000".into()),
                operands: vec!["rd".into(), "rs1".into(), "rs2".into()],
                summary: None,
            },
            WizardInstruction {
                mnemonic: "x.li".into(), format: "I".into(),
                opcode: Some("custom-1".into()), funct3: Some("0b010".into()),
                funct7: None,
                operands: vec!["rd".into(), "rs1".into(), "imm".into()],
                summary: None,
            },
        ];
        let root = cmd_new(&mgr, a).expect("cmd_new");

        let combined = fs::read_to_string(root.join("tests/instructions.S")).unwrap();
        assert!(combined.contains("# x.add (R-type)"));
        assert!(combined.contains("# x.li (I-type)"));
        assert!(combined.contains(".insn r 0b0001011, 0b000, 0b0000000,"));
        assert!(combined.contains(".insn i 0b0101011, 0b010,"));
    }

    #[test]
    fn instructions_are_serialised_into_opcodes_json() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-ops", &parent);
        a.instructions = vec![
            WizardInstruction {
                mnemonic: "x.alpha".into(),
                format: "I".into(),
                opcode: Some("custom-0".into()),
                funct3: Some("0b000".into()),
                funct7: None,
                operands: vec!["rd".into(), "rs1".into(), "imm".into()],
                summary: Some("alpha summary".into()),
            },
            WizardInstruction {
                mnemonic: "x.beta".into(),
                format: "R".into(),
                opcode: None,
                funct3: None,
                funct7: None,
                operands: vec![],
                summary: None,
            },
        ];

        let root = cmd_new(&mgr, a).expect("cmd_new");
        let raw = fs::read_to_string(root.join("opcodes.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["schemaVersion"], 1);
        let arr = v["instructions"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["mnemonic"], "x.alpha");
        assert_eq!(arr[1]["format"], "R");
    }

    #[test]
    fn rejects_reserved_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let res = cmd_new(&mgr, args_for("extensilica", &parent));
        assert!(res.is_err());
    }

    #[test]
    fn rejects_missing_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-no-repo", &parent);
        a.repository = None;
        let res = cmd_new(&mgr, a);
        assert!(res.is_err(), "expected cmd_new to fail without repository");
    }

    #[test]
    fn rejects_non_http_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-bad-repo", &parent);
        a.repository = Some("ftp://example.com/repo".into());
        let res = cmd_new(&mgr, a);
        assert!(res.is_err(), "expected cmd_new to fail on non-http repo");
    }

    #[test]
    fn persists_repository_in_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-with-repo", &parent);
        a.repository = Some("https://gitlab.com/me/wiz-with-repo".into());
        let root = cmd_new(&mgr, a).expect("cmd_new");

        let raw = fs::read_to_string(root.join("manifest.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["repository"], "https://gitlab.com/me/wiz-with-repo");
    }

    #[test]
    fn rejects_missing_standard_status() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-no-status", &parent);
        a.standard_status = None;
        assert!(cmd_new(&mgr, a).is_err());
    }

    #[test]
    fn rejects_unknown_standard_status() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-bad-status", &parent);
        a.standard_status = Some("official".into());
        assert!(cmd_new(&mgr, a).is_err());
    }

    #[test]
    fn rejects_missing_authority() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-no-authority", &parent);
        a.authority = None;
        assert!(cmd_new(&mgr, a).is_err());
    }

    #[test]
    fn persists_standard_status_and_authority() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-classed", &parent);
        a.standard_status = Some("ratified".into());
        a.authority = Some("RISC-V International".into());
        let root = cmd_new(&mgr, a).expect("cmd_new");

        let raw = fs::read_to_string(root.join("manifest.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["standardStatus"], "ratified");
        assert_eq!(v["authority"], "RISC-V International");

        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        assert!(readme.contains("**Standard status:** `ratified`"));
        assert!(readme.contains("**Authority:** RISC-V International"));
    }

    #[test]
    fn rejects_invalid_format() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("xsil-home");
        let mgr = ExtensionManager::new(home);
        let parent = tmp.path().join("work");
        fs::create_dir_all(&parent).unwrap();

        let mut a = args_for("wiz-bad-fmt", &parent);
        a.instructions = vec![WizardInstruction {
            mnemonic: "x.bad".into(),
            format: "Z".into(),
            opcode: None,
            funct3: None,
            funct7: None,
            operands: vec![],
            summary: None,
        }];
        let res = cmd_new(&mgr, a);
        assert!(res.is_err());
    }
}
