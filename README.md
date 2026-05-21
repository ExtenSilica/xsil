# xsil

Public **reference CLI** and documentation for the **`.xsil`** RISC-V package format.

- **`spec/xsil.md`** — normative package layout and manifest.
- **`cli/`** — `xsil` (Rust): `init`, `run`, `test`, `install`, `publish`, registry auth, and more.
- **`examples/rvx-demo/`** — minimal runnable package (`xsil run examples/rvx-demo`).

The ExtenSilica **registry, website, and hosted API** live in a separate **private** product repository. This repo is intentionally **tooling + format + examples** only.

Product positioning for the hosted platform (pre-silicon bridge, readiness levels) is documented in that private repo’s **`docs/pre-silicon-bridge.md`** / **`docs/pre-silicon-bridge-readiness.md`** when you have access.

## Install

From [crates.io](https://crates.io/crates/xsil) (recommended):

```bash
cargo install xsil
```

Works out of the box on **Rust 1.74+** (Ubuntu 22.04 / Debian 12 stock
toolchain). TLS is bundled in-binary via `rustls` — there is **no
runtime dependency on OpenSSL**, `libssl-dev`, or `pkg-config`. If you
want fully reproducible compilation that ignores any newer transitive
releases, add `--locked`:

```bash
cargo install --locked xsil
```

Or from a checkout of this repo:

```bash
cargo install --path cli
```

Or grab a pre-built binary from
[GitHub Releases](https://github.com/ExtenSilica/xsil/releases) (tags `cli/v*.*.*`)
or via the official installer:

```bash
curl -fsSL https://extensilica.com/install.sh | sh
```

## Quick commands

```bash
# Interactive scaffold (full wizard)
xsil new

# Non-interactive scaffold
xsil init my-extension
cd my-extension && xsil run .

# Run a bundled example (in this repo)
xsil run examples/rvx-demo

# Publish (dry run first)
xsil publish ./my-extension --dry-run

# Manage API tokens (one credential per device, CLI install, or bot)
xsil login --name "xsil-cli @ my-laptop"
xsil token list
xsil token create "ci-runner"
xsil token revoke 7
```

## License

The `xsil` crate is licensed under **ISC** (see `cli/Cargo.toml`). Example package `examples/rvx-demo` is **Apache-2.0** (see that tree).
