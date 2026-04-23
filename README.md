# xsil

Public **reference CLI** and documentation for the **`.xsil`** RISC-V package format.

- **`spec/xsil.md`** — normative package layout and manifest.
- **`cli/`** — `xsil` (Rust): `init`, `run`, `test`, `install`, `publish`, registry auth, and more.
- **`examples/rvx-demo/`** — minimal runnable package (`xsil run examples/rvx-demo`).

The ExtenSilica **registry, website, and hosted API** live in a separate **private** product repository. This repo is intentionally **tooling + format + examples** only.

## Install

From [GitHub Releases](https://github.com/extensilica/xsil/releases) (tags `cli/v*.*.*`) or:

```bash
cargo install --path cli
```

Official installers (pinned to this org/repo):

```bash
curl -fsSL https://extensilica.com/install.sh | sh
```

## Quick commands

```bash
xsil init my-extension
cd my-extension && xsil run .

xsil run examples/rvx-demo
xsil publish ./my-extension --dry-run
```

## License

The `xsil` crate is licensed under **ISC** (see `cli/Cargo.toml`). Example package `examples/rvx-demo` is **Apache-2.0** (see that tree).
