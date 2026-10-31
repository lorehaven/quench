# quench-config
Flexible configuration management for multiple sources.

## What it is
`quench-config` is a small shared helper for loading configuration: JSON, YAML and TOML files and typed environment variables through one API, instead of each service hand-rolling `std::env::var` plus `.parse()`. `ConfigLoader` reads typed files and prefixed environment values; `AppConfig` is a ready-made configuration read from the environment.

It depends on `quench-core` and `quench-serde` (with its `yaml` feature).

## Use cases
- Read a typed settings file in JSON, YAML or TOML.
- Fall back from a file to an environment variable to a default.
- Read prefixed environment values as strings, numbers, booleans or lists.
- Load the ready-made `AppConfig` from the environment.

## Example
```rust
use quench_config::ConfigLoader;

#[derive(quench_serde::Deserialize)]
struct Settings { name: String, count: u32 }

let a: Settings = ConfigLoader::from_toml_file("settings.toml")?;

let loader = ConfigLoader::new("GATEWAY");
let host = loader.env_string("HOST", "0.0.0.0");   // GATEWAY_HOST, then HOST, then the default
let port = loader.env_u64("PORT", 8080);
let tls  = loader.env_bool("TLS", false);
let origins = loader.env_list("ORIGINS", &["http://localhost"]);

let config = quench_config::AppConfig::from_env();
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-config/
