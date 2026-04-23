/// Default registry base URL. Overridden by the `registry` field in
/// ~/.extensilica/config.json.
///
/// IMPORTANT — before cutting a production release binary, change this to:
///   "https://api.extensilica.com"
pub const DEFAULT_REGISTRY: &str = "http://localhost:3001";

/// Path to the CLI config file, relative to the user's home directory.
pub const CONFIG_RELATIVE_PATH: &str = ".extensilica/config.json";
