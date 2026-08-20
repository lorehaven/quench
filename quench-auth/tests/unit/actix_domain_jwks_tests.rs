//! Unit tests for `actix/domain/jwks.rs`.
//!
//! `JwksVerifier` only ever talks HTTP, so instead of mocking it out we spin
//! up a tiny real HTTP server on localhost per test - no new dev-dependency
//! needed, and it exercises the actual request/parse path.
//!
//! `env_lock::ENV_LOCK` is deliberately held across the one `.await` in the
//! `GATEHOUSE_URL` test below - each test runs on its own thread here, so
//! nothing else can deadlock on it.
#![allow(clippy::await_holding_lock)]

use base64::Engine;
use quench_auth::actix::domain::jwks::JwksVerifier;
use quench_auth::actix::domain::jwt::KeyResolver;
use quench_auth::actix::domain::signing::generate_signing_key;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn encode_x(public_key: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key)
}

/// Serves `body` as a 200 JSON response to every request it receives, and
/// hands back a counter so a test can assert whether a second call actually
/// went back to the network or was served from cache.
fn spawn_server(body: String) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_in_thread = hits.clone();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            hits_in_thread.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (format!("http://{addr}"), hits)
}

#[tokio::test]
async fn resolves_a_known_kid_from_the_published_set_and_then_from_cache() {
    let key = generate_signing_key();
    let body = serde_json::json!({
        "keys": [{"kid": "abc", "x": encode_x(&key.public_key)}]
    })
    .to_string();
    let (url, hits) = spawn_server(body);

    let verifier = JwksVerifier::new(&url, true).await.expect("verifier");
    assert!(verifier.resolve("abc").await.is_some());
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Second lookup of the same kid must be served from cache, not refetched.
    assert!(verifier.resolve("abc").await.is_some());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_unknown_kid_is_none_and_the_miss_itself_is_cached() {
    let key = generate_signing_key();
    let body = serde_json::json!({
        "keys": [{"kid": "someone-else", "x": encode_x(&key.public_key)}]
    })
    .to_string();
    let (url, hits) = spawn_server(body);

    let verifier = JwksVerifier::new(&url, true).await.expect("verifier");
    assert!(verifier.resolve("missing").await.is_none());
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // The negative result is cached too, so this must not hit the network again.
    assert!(verifier.resolve("missing").await.is_none());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_malformed_jwks_response_resolves_to_none() {
    let (url, _hits) = spawn_server("not json".to_string());

    let verifier = JwksVerifier::new(&url, true).await.expect("verifier");
    assert!(verifier.resolve("abc").await.is_none());
}

#[tokio::test]
async fn an_unreachable_gatehouse_resolves_to_none_instead_of_panicking() {
    // Nothing is listening on this port - the request itself fails.
    let verifier = JwksVerifier::new("http://127.0.0.1:1", true)
        .await
        .expect("verifier");
    assert!(verifier.resolve("abc").await.is_none());
}

#[tokio::test]
async fn new_accepts_both_tls_verify_settings() {
    let key = generate_signing_key();
    let body = serde_json::json!({
        "keys": [{"kid": "abc", "x": encode_x(&key.public_key)}]
    })
    .to_string();
    let (url, _hits) = spawn_server(body);

    assert!(JwksVerifier::new(&url, false).await.is_ok());
    assert!(JwksVerifier::new(&url, true).await.is_ok());
}

#[tokio::test]
async fn from_env_requires_gatehouse_url_to_be_set() {
    // `GATEHOUSE_URL` is also touched by realm/sso_client tests in this
    // binary - shared lock to keep this from racing them.
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("GATEHOUSE_URL");
    assert!(JwksVerifier::from_env().await.is_err());
}
