---
title: Usage
parent: quench-config
grand_parent: Crates
nav_order: 1
---

## 1. Read a typed file

```rust
use quench_config::ConfigLoader;
#[derive(quench_serde::Deserialize)]
struct Settings { name: String, count: u32 }

let a: Settings = ConfigLoader::from_json_file("settings.json")?;
let b: Settings = ConfigLoader::from_yaml_file("settings.yaml")?;
let c: Settings = ConfigLoader::from_toml_file("settings.toml")?;
```

The loaders are associated functions (no prefix involved) and take `&str` paths. Pitfall: a missing file is `ConfigError::ReadError`, malformed content `ConfigError::ParseError` (see [Errors](errors.md)).

## 2. File, then environment, then default

```rust
let cfg: Settings = ConfigLoader::load_with_fallback(
    Some("config/settings.toml"),   // used only if it exists; format chosen by extension
    "SETTINGS_JSON",                // else a JSON document in this variable
    None,                           // else this default, or an error
)?;
```

Pitfalls: a file with an extension other than `json`, `yaml`, `yml`, `toml` is ignored; an environment variable that is not valid JSON for `T` is silently ignored; an existing file that fails to parse is returned as an error and the environment is not tried. The type must be `DeserializeOwned + Serialize`.

## 3. Prefixed environment values

```rust
let loader = ConfigLoader::new("GATEWAY");
let host = loader.env_string("HOST", "0.0.0.0");   // GATEWAY_HOST, then HOST, then the default
let port = loader.env_u64("PORT", 8080);           // unparseable text gives the default
let tls  = loader.env_bool("TLS", false);          // only true/1/yes/on are true
let origins = loader.env_list("ORIGINS", &["http://localhost"]);  // comma separated, trimmed
```

## 4. The ready-made `AppConfig`

```rust
let config = quench_config::AppConfig::from_env();  // APP_SERVICE_NAME, APP_PORT, ...
```

Defaults are in [Configuration](configuration.md).
