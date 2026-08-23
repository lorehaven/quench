/// Initialize logging with configurable verbosity via RUST_LOG environment variable.
///
/// Loads `.env` files (project root, then any service-specific
/// `docker/<bin>/.env`) before handing off to `quench_log::init()`, which
/// reads `RUST_LOG` plus the rest of its own configuration - console and
/// rolling-file (JSON by default) output, both on by default. See
/// `quench-log`'s own docs for the full list of variables.
pub fn init() {
    // Try to load .env from common locations for local development
    let _ = dotenvy::dotenv(); // Load from project root

    // Try to load from service-specific .env file if it exists
    if let Ok(bin_name) = std::env::var("CARGO_BIN_NAME") {
        let service_env_path = format!("docker/{}/", bin_name);
        let env_file = std::path::Path::new(&service_env_path).join(".env");
        if env_file.exists() {
            let _ = dotenvy::from_path(&env_file);
        }
    }

    quench_log::init();

    let actual_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing::debug!("Logging initialized with RUST_LOG={}", actual_level);
}
