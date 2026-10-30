---
title: API
parent: quench-config
grand_parent: Crates
nav_order: 2
---

All public items of `src/lib.rs`.

## `ConfigError` and `Result`

```rust
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]     ReadError(String),
    #[error("Failed to parse config: {0}")]         ParseError(String),
    #[error("Missing required config value: {0}")]  MissingValue(String),
    #[error("Invalid config value: {0}")]           InvalidValue(String),
}
pub type Result<T> = std::result::Result<T, ConfigError>;
```

The `Error` derive is `quench_core`'s. `InvalidValue` is declared but never constructed by this crate.

## `ConfigLoader`

```rust
#[derive(Debug)]
pub struct ConfigLoader { env_prefix: String }
```

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new(env_prefix: &str) -> Self` | |
| `from_json_file` | `pub fn from_json_file<T: DeserializeOwned>(path: &str) -> Result<T>` | `read_to_string`, then `quench_serde::json::from_str`. Errors: `ReadError("{path}: {io error}")`, `ParseError("JSON parse error: {e}")` |
| `from_yaml_file` | `pub fn from_yaml_file<T: DeserializeOwned>(path: &str) -> Result<T>` | via `quench_serde::yaml::from_str`; `ParseError("YAML parse error: {e}")` |
| `from_toml_file` | `pub fn from_toml_file<T: DeserializeOwned>(path: &str) -> Result<T>` | via `quench_serde::toml::from_str`; `ParseError("TOML parse error: {e}")` |
| `load_with_fallback` | `pub fn load_with_fallback<T: DeserializeOwned + Serialize>(file_path: Option<&str>, env_var: &str, default: Option<T>) -> Result<T>` | 1) if `file_path` is `Some` and exists, the extension (case-insensitive) picks the loader: `json`, `yaml`/`yml`, `toml`; the loader's result is returned as is. 2) else if `env_var` is set and parses as JSON `T`, that. 3) else `default`, or `MissingValue("No config found in file or env var {env_var}")` |
| `env_string` | `#[must_use] pub fn env_string(&self, key: &str, default: &str) -> String` | `{prefix}_{key}`, then `key`, then `default` (variables via `quench_core::env::var`, so non-UTF-8 counts as unset) |
| `env_u64` | `#[must_use] pub fn env_u64(&self, key: &str, default: u64) -> u64` | `env_string` parsed; `default` on parse failure |
| `env_bool` | `#[must_use] pub fn env_bool(&self, key: &str, default: bool) -> bool` | lower-cased value is true only for `true`, `1`, `yes`, `on`; any other present value is false (the default applies only when the key is unset) |
| `env_list` | `#[must_use] pub fn env_list(&self, key: &str, default: &[&str]) -> Vec<String>` | value (or the defaults joined with `,`) split on `,`, trimmed, empty items dropped |

## `AppConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub service_name: String,
    pub port: u16,
    pub database_url: String,
    pub log_level: String,
    #[serde(default)] pub debug: bool,
}
impl AppConfig { #[must_use] pub fn from_env() -> Self }
```

`from_env` builds `ConfigLoader::new("APP")` and reads `SERVICE_NAME` (default `unknown`), `PORT` (`8080`; a value that does not fit `u16` gives `8080`), `DATABASE_URL` (empty), `LOG_LEVEL` (`info`), `DEBUG` (`false`), each as `APP_<KEY>` first, then `<KEY>`.
