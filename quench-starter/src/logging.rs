/// Initialize logging with configurable verbosity via `RUST_LOG` environment variable.
///
/// Loads `.env` files (project root, then any service-specific
/// `docker/<bin>/.env`) before handing off to `quench_log::init()`, which
/// reads `RUST_LOG` plus the rest of its own configuration - console and
/// rolling-file (JSON by default) output, both on by default. See
/// `quench-log`'s own docs for the full list of variables.
pub fn init() {
    // Try to load .env from common locations for local development
    load_env_from_ancestors(); // Load from project root

    // Try to load from service-specific .env file if it exists
    if let Ok(bin_name) = std::env::var("CARGO_BIN_NAME") {
        let service_env_path = format!("docker/{bin_name}/");
        let env_file = std::path::Path::new(&service_env_path).join(".env");
        if env_file.exists() {
            load_env_file(&env_file);
        }
    }

    quench_log::init();

    let actual_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing::debug!("Logging initialized with RUST_LOG={}", actual_level);
}

/// `.env` from the working directory or the nearest ancestor that has one.
fn load_env_from_ancestors() {
    let Ok(mut dir) = std::env::current_dir() else {
        return;
    };
    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            load_env_file(&candidate);
            return;
        }
        if !dir.pop() {
            return;
        }
    }
}

/// Sets every `KEY=VALUE` line of `path` that isn't already in the
/// environment (a real variable always wins over the file). Understands
/// `#` comments, an `export ` prefix, and single / double quoted values.
#[allow(unsafe_code)] // `set_var`, see the SAFETY note below
fn load_env_file(path: &std::path::Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for (key, value) in parse_env(&content) {
        if std::env::var_os(&key).is_none() {
            // SAFETY: logging is initialised first thing in `main`, before
            // any other thread reads or writes the environment.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

fn parse_env(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let value = value.trim();
            let value = match value.chars().next() {
                Some(q @ ('"' | '\'')) => value[1..]
                    .find(q)
                    .map_or_else(|| value[1..].to_string(), |end| value[1..=end].to_string()),
                _ => value
                    .split(" #")
                    .next()
                    .unwrap_or("")
                    .trim_end()
                    .to_string(),
            };
            Some((key.to_string(), value))
        })
        .collect()
}

#[cfg(test)]
mod env_tests {
    use super::parse_env;

    #[test]
    fn parses_comments_exports_and_quotes() {
        let env = parse_env("# c\nA=1\nexport B=\"x y\" # tail\nC='z'\nD=v # note\n\nbad\n");
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("A"), Some("1"));
        assert_eq!(get("B"), Some("x y"));
        assert_eq!(get("C"), Some("z"));
        assert_eq!(get("D"), Some("v"));
        assert_eq!(env.len(), 4);
    }
}
