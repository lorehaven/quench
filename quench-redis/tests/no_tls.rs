//! Without the `tls` feature a TLS configuration is refused instead of silently going plain.
#![cfg(not(feature = "tls"))]

use quench_redis::{Config, Connection, Error};
use tokio::net::TcpListener;

#[tokio::test]
async fn rediss_urls_fail_to_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Config::from_url(&format!("rediss://127.0.0.1:{port}")).unwrap();
    let err = Connection::connect(&config).await.unwrap_err();
    assert!(
        matches!(&err, Error::Tls(m) if m.contains("`tls` feature")),
        "{err}"
    );
}
