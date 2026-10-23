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
    quench_log::debug!("Logging initialized with RUST_LOG={}", actual_level);
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

#[cfg(test)]
mod loader_tests {
    use super::{init, load_env_file, load_env_from_ancestors};

    #[test]
    fn existing_variables_win_over_the_file() {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let file = dir.path().join(".env");
        std::fs::write(
            &file,
            "QUENCH_TEST_ENV_NEW=from-file\nQUENCH_TEST_ENV_OLD=from-file\n",
        )
        .unwrap();
        quench_core::env::set("QUENCH_TEST_ENV_OLD", "from-process");
        quench_core::env::remove("QUENCH_TEST_ENV_NEW");
        load_env_file(&file);
        assert_eq!(
            quench_core::env::get_or("QUENCH_TEST_ENV_NEW", ""),
            "from-file"
        );
        assert_eq!(
            quench_core::env::get_or("QUENCH_TEST_ENV_OLD", ""),
            "from-process"
        );
        load_env_file(&dir.path().join("missing.env"));
        quench_core::env::remove("QUENCH_TEST_ENV_NEW");
        quench_core::env::remove("QUENCH_TEST_ENV_OLD");
    }

    /// Changes the working directory, so it is the only test here that does, and it puts it back.
    #[test]
    fn init_loads_the_nearest_env_file_and_installs_logging() {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "QUENCH_TEST_ANCESTOR=found\nLOG_FILE_ENABLED=false\nLOG_CONSOLE_ENABLED=false\n",
        )
        .unwrap();
        let service_dir = dir.path().join("docker/svc");
        std::fs::create_dir_all(&service_dir).unwrap();
        std::fs::write(service_dir.join(".env"), "QUENCH_TEST_SERVICE=found\n").unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        quench_core::env::remove("QUENCH_TEST_ANCESTOR");
        load_env_from_ancestors();
        let found = quench_core::env::get_or("QUENCH_TEST_ANCESTOR", "");

        std::env::set_current_dir(dir.path()).unwrap();
        quench_core::env::set("CARGO_BIN_NAME", "svc");
        init();
        let service = quench_core::env::get_or("QUENCH_TEST_SERVICE", "");
        std::env::set_current_dir(original).unwrap();

        assert_eq!(found, "found");
        assert_eq!(service, "found");
        assert!(quench_log::global_is_set());

        // No `.env` anywhere above a directory with none: nothing happens.
        let empty = quench_core::tmp::TempDir::new().unwrap();
        let before = std::env::current_dir().unwrap();
        std::env::set_current_dir(empty.path()).unwrap();
        load_env_from_ancestors();
        std::env::set_current_dir(before).unwrap();
        quench_core::env::remove("CARGO_BIN_NAME");
    }
}
