# Changelog

All notable changes to the `xsil` CLI. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] — 2026-05-11

### Added

- `xsil login` now mints a **named** API token on the registry (default
  `xsil-cli @ <hostname>`, overridable with `--name <label>`). Each
  device / CLI install / CI bot can therefore hold its own credential
  without invalidating other sessions, mirroring how `npm`, `cargo` and
  `gh` handle authentication. Hostname is detected without new
  dependencies (`$HOSTNAME` → `/etc/hostname` → `hostname` command).
- New `xsil token` subcommand for managing tokens directly from the
  shell:
  - `xsil token list`   — print every token on the current account
    (live and revoked), with `id`, name, created and last-used columns.
  - `xsil token create <name>` — mint a fresh token; the raw value is
    printed **once**, with a copy-this-now banner.
  - `xsil token revoke <id>` — revoke a single token (idempotent; tells
    you when the token was already revoked).

### Compatibility

- These features require a registry running the `ApiToken` schema (commits
  `ff0fbe4` + `8611652` in the ExtenSilica registry repo, deployed to
  `api.extensilica.com`). Running `xsil token *` against an older
  registry will return `404 /auth/me/tokens`. `xsil login` itself remains
  backwards compatible: older registries simply ignore the new `name`
  field in the request body.

## [0.2.1] — 2026-05-09

### Documentation

- Document the `standardStatus` / `authority` prompts in `xsil new` and the
  matching `--standard-status` / `--authority` flags. These were already
  required by the CLI as of v0.2.0; the crates.io front-page README simply
  didn't describe them.
- Document that **provenance** (`portStatus`: `seeded` / `community_port` /
  `claimed` / `official` / `archived`) is a registry-side field. The CLI
  does not read or write it, and the registry will ignore any `portStatus`
  value you place in `manifest.json`. Tracks xsil spec v2.2 (§4.8).

### No behavioural changes

The CLI binary is identical to v0.2.0 in semantics and on-disk output. This
release is a documentation refresh published so the crates.io page matches
what the tool actually does.

## [0.2.0] — 2026-05-08

### Added

- `xsil new` now prompts (and accepts `--standard-status` / `--authority`
  flags) for an honest classification of the extension's relationship to the
  RISC-V standard. Both fields are mandatory and propagate into
  `manifest.json` at the top level. Unknown `standardStatus` values are
  rejected.
- Default `xsil init` scaffold now populates `standardStatus = "custom"`
  and `authority = "TODO: who defines this extension?"` so the resulting
  package validates out of the box.

## [0.1.0] — 2026-04

Initial public release on crates.io.
