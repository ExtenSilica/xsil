/// Default registry base URL.
///
/// Resolution order at runtime (see `RegistryClient::from_config`):
///   1. The `registry` field in `~/.extensilica/config.json`, if set.
///   2. The `XSIL_REGISTRY` environment variable, if set.
///   3. This constant, baked into the binary.
///
/// Production binaries published to crates.io use the hosted registry at
/// `https://api.extensilica.com`. Developers running a local backend can
/// override either by writing a config file or by exporting
/// `XSIL_REGISTRY=http://localhost:3001` in their shell.
pub const DEFAULT_REGISTRY: &str = "https://api.extensilica.com";

/// Path to the CLI config file, relative to the user's home directory.
pub const CONFIG_RELATIVE_PATH: &str = ".extensilica/config.json";
