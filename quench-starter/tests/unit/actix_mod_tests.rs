//! Unit tests for `actix/mod.rs`.

use quench_starter::actix::load_tls;

#[test]
fn missing_cert_and_key_files_yield_no_tls_config() {
    assert!(load_tls("/nonexistent/cert.pem", "/nonexistent/key.pem").is_none());
}

#[test]
fn a_cert_file_that_is_not_valid_pem_yields_no_tls_config() {
    let dir = std::env::temp_dir().join(format!(
        "quench-starter-load-tls-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, "not a certificate").unwrap();
    std::fs::write(&key_path, "not a key").unwrap();

    assert!(load_tls(&cert_path, &key_path).is_none());

    std::fs::remove_dir_all(&dir).ok();
}
