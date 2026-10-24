//! The rustls glue against a real handshake, using the fixture CA and `localhost` leaf.

use quench_tls::{
    ClientOptions, TlsAcceptor, TlsConnector, client_config, client_config_with_identity,
    load_server_config, root_store, server_name,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn acceptor() -> TlsAcceptor {
    let mut config = load_server_config(fixture("server.pem"), fixture("server.key"))
        .expect("fixture cert and key load");
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    TlsAcceptor::from(config)
}

fn trusting() -> TlsConnector {
    let options = ClientOptions {
        extra_roots_pem: vec![std::fs::read(fixture("ca.pem")).unwrap()],
        alpn_protocols: vec![b"http/1.1".to_vec()],
        ..ClientOptions::default()
    };
    TlsConnector::new(client_config(&options).unwrap())
}

#[tokio::test]
async fn a_full_handshake_carries_data_both_ways() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    let server = tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.unwrap();
        assert_eq!(tls.alpn_protocol(), Some(&b"http/1.1"[..]));
        let mut buf = [0u8; 5];
        tls.read_exact(&mut buf).await.unwrap();
        tls.write_all(b"pong!").await.unwrap();
        tls.flush().await.unwrap();
        tls.shutdown().await.unwrap();
        buf
    });
    let mut tls = trusting()
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap();
    assert!(format!("{tls:?}").contains("TlsStream"));
    assert!(tls.get_ref().1.protocol_version().is_some());
    tls.write_all(b"ping!").await.unwrap();
    let mut reply = Vec::new();
    tls.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, b"pong!");
    assert_eq!(&server.await.unwrap(), b"ping!");
}

#[tokio::test]
async fn large_transfers_survive_the_session_buffer_limits() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let acceptor = acceptor();
    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let expected = payload.clone();
    let server = tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.unwrap();
        let mut got = Vec::new();
        tls.read_to_end(&mut got).await.unwrap();
        got
    });
    let mut tls = trusting()
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap();
    tls.write_all(&payload).await.unwrap();
    tls.shutdown().await.unwrap();
    assert_eq!(server.await.unwrap(), expected);
}

#[tokio::test]
async fn an_untrusted_certificate_fails_the_client_handshake() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    tokio::spawn(async move {
        let _ = acceptor.accept(server_io).await;
    });
    let connector = TlsConnector::new(client_config(&ClientOptions::default()).unwrap());
    let err = connector
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
}

#[tokio::test]
async fn a_name_mismatch_fails_the_client_handshake() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    tokio::spawn(async move {
        let _ = acceptor.accept(server_io).await;
    });
    let err = trusting()
        .connect(server_name("not-localhost.example").unwrap(), client_io)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
}

#[tokio::test]
async fn accepting_any_certificate_skips_verification() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.unwrap();
        tls.write_all(b"hi").await.unwrap();
        tls.shutdown().await.unwrap();
    });
    let connector = TlsConnector::from(
        client_config(&ClientOptions {
            accept_invalid_certs: true,
            ..ClientOptions::default()
        })
        .unwrap(),
    );
    let mut tls = connector
        .connect(server_name("whatever.example").unwrap(), client_io)
        .await
        .unwrap();
    let mut got = String::new();
    tls.read_to_string(&mut got).await.unwrap();
    assert_eq!(got, "hi");
}

#[tokio::test]
async fn plain_text_sent_to_a_tls_server_is_rejected() {
    let (mut client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    let server = tokio::spawn(async move { acceptor.accept(server_io).await.map(|_| ()) });
    client_io
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    assert!(server.await.unwrap().is_err());
}

#[tokio::test]
async fn a_peer_that_vanishes_mid_handshake_is_an_eof_error() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    drop(client_io);
    let err = acceptor().accept(server_io).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
}

#[tokio::test]
async fn truncation_without_close_notify_is_reported() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.unwrap();
        tls.write_all(b"partial").await.unwrap();
        tls.flush().await.unwrap();
        // Dropped without close_notify.
    });
    let mut tls = trusting()
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap();
    let mut buf = Vec::new();
    let err = tls.read_to_end(&mut buf).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    assert_eq!(buf, b"partial");
}

#[tokio::test]
async fn reading_after_close_notify_keeps_returning_eof() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = acceptor();
    tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.unwrap();
        tls.shutdown().await.unwrap();
    });
    let mut tls = trusting()
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap();
    let mut buf = [0u8; 8];
    assert_eq!(tls.read(&mut buf).await.unwrap(), 0);
    assert_eq!(tls.read(&mut buf).await.unwrap(), 0);
    assert_eq!(tls.write(&[]).await.unwrap(), 0);
}

#[test]
fn server_names_accept_dns_and_ip_literals() {
    assert!(server_name("example.com").is_ok());
    assert!(server_name("127.0.0.1").is_ok());
    assert!(server_name("[::1]").is_ok());
    assert!(server_name("not a host!").is_err());
    assert!(server_name("").is_err());
}

#[test]
fn trust_stores_include_webpki_roots_and_extras() {
    let base = root_store(&ClientOptions::default()).unwrap();
    assert!(base.len() > 100, "webpki-roots are present");
    let with_extra = root_store(&ClientOptions {
        extra_roots_pem: vec![std::fs::read(fixture("ca.pem")).unwrap()],
        ..ClientOptions::default()
    })
    .unwrap();
    assert_eq!(with_extra.len(), base.len() + 1);
    let with_system = root_store(&ClientOptions {
        use_system_roots: true,
        ..ClientOptions::default()
    })
    .unwrap();
    assert!(with_system.len() >= base.len());
    assert!(
        root_store(&ClientOptions {
            extra_roots_pem: vec![
                b"-----BEGIN CERTIFICATE-----\n!!!\n-----END CERTIFICATE-----\n".to_vec()
            ],
            ..ClientOptions::default()
        })
        .is_err()
    );
    // A PEM with no certificates adds nothing.
    assert_eq!(
        root_store(&ClientOptions {
            extra_roots_pem: vec![b"no pem here".to_vec()],
            ..ClientOptions::default()
        })
        .unwrap()
        .len(),
        base.len()
    );
}

#[test]
fn system_roots_honour_ssl_cert_file() {
    // SAFETY-free: tests in this binary that read the variable are the two below, serialised by one lock.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap();
    quench_core::env::set("SSL_CERT_FILE", fixture("ca.pem"));
    let with_file = root_store(&ClientOptions {
        use_system_roots: true,
        ..ClientOptions::default()
    })
    .unwrap();
    quench_core::env::remove("SSL_CERT_FILE");
    let base = root_store(&ClientOptions::default()).unwrap();
    assert!(with_file.len() > base.len());
}

#[test]
fn missing_or_broken_server_credentials_load_as_none() {
    assert!(load_server_config(fixture("nope.pem"), fixture("server.key")).is_none());
    assert!(load_server_config(fixture("server.pem"), fixture("nope.key")).is_none());
    assert!(
        load_server_config(fixture("ca.pem"), fixture("server.key")).is_none(),
        "a key that does not match the certificate"
    );
    assert!(load_server_config(fixture("README.md"), fixture("server.key")).is_none());
}

#[test]
fn acceptors_and_connectors_are_cheap_to_clone() {
    let _copy = acceptor();
    let from_arc = TlsAcceptor::from(Arc::new(
        load_server_config(fixture("server.pem"), fixture("server.key")).unwrap(),
    ));
    assert!(format!("{from_arc:?}").contains("TlsAcceptor"));
    assert!(format!("{:?}", trusting()).contains("TlsConnector"));
}

// ---- failures in the transport underneath -------------------------------------

mod flaky {
    use super::*;
    use quench_tls::{pem_certificates, pem_private_key, server_config};
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

    const OK: u8 = 0;
    const READ_ERROR: u8 = 1;
    const WRITE_ERROR: u8 = 2;
    const WRITE_ZERO: u8 = 3;
    const FLUSH_ERROR: u8 = 4;

    /// A pipe whose behaviour can be switched mid-test.
    struct Flaky {
        inner: DuplexStream,
        mode: Arc<AtomicU8>,
    }

    impl AsyncRead for Flaky {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.mode.load(Ordering::SeqCst) == READ_ERROR {
                return Poll::Ready(Err(io::Error::other("read failed")));
            }
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for Flaky {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.mode.load(Ordering::SeqCst) {
                WRITE_ERROR => Poll::Ready(Err(io::Error::other("write failed"))),
                WRITE_ZERO => Poll::Ready(Ok(0)),
                _ => Pin::new(&mut self.inner).poll_write(cx, buf),
            }
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            if self.mode.load(Ordering::SeqCst) == FLUSH_ERROR {
                return Poll::Ready(Err(io::Error::other("flush failed")));
            }
            Pin::new(&mut self.inner).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
    }

    /// A completed client session over a `Flaky` pipe, with the server end serving `serve`.
    async fn connected<F, Fut>(serve: F) -> (quench_tls::TlsStream<Flaky>, Arc<AtomicU8>)
    where
        F: FnOnce(quench_tls::TlsStream<DuplexStream>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (client_io, server_io) = tokio::io::duplex(1 << 16);
        let acceptor = acceptor();
        tokio::spawn(async move {
            let tls = acceptor.accept(server_io).await.unwrap();
            serve(tls).await;
        });
        let mode = Arc::new(AtomicU8::new(OK));
        let io = Flaky {
            inner: client_io,
            mode: mode.clone(),
        };
        (
            trusting()
                .connect(server_name("localhost").unwrap(), io)
                .await
                .unwrap(),
            mode,
        )
    }

    #[tokio::test]
    async fn a_read_error_from_the_transport_surfaces() {
        let (mut tls, mode) = connected(|mut s| async move {
            let _ = s.read_u8().await;
        })
        .await;
        mode.store(READ_ERROR, Ordering::SeqCst);
        let mut buf = [0u8; 4];
        let err = tls.read(&mut buf).await.unwrap_err();
        assert_eq!(err.to_string(), "read failed");
    }

    #[tokio::test]
    async fn write_errors_surface_from_write_flush_and_shutdown() {
        let (mut tls, mode) = connected(|mut s| async move {
            let mut sink = Vec::new();
            let _ = s.read_to_end(&mut sink).await;
        })
        .await;
        mode.store(WRITE_ERROR, Ordering::SeqCst);
        assert!(tls.write_all(b"data").await.is_err());
        assert!(tls.flush().await.is_err());
        assert!(tls.shutdown().await.is_err());
    }

    #[tokio::test]
    async fn a_transport_that_accepts_zero_bytes_is_a_write_zero_error() {
        let (mut tls, mode) = connected(|mut s| async move {
            let mut sink = Vec::new();
            let _ = s.read_to_end(&mut sink).await;
        })
        .await;
        mode.store(WRITE_ZERO, Ordering::SeqCst);
        let err = tls.write_all(b"data").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[tokio::test]
    async fn flush_errors_from_the_transport_surface() {
        let (mut tls, mode) = connected(|mut s| async move {
            let mut sink = Vec::new();
            let _ = s.read_to_end(&mut sink).await;
        })
        .await;
        tls.write_all(b"fine").await.unwrap();
        mode.store(FLUSH_ERROR, Ordering::SeqCst);
        assert!(tls.flush().await.is_err());
    }

    #[tokio::test]
    async fn handshakes_fail_cleanly_when_the_transport_breaks() {
        let (client_io, _server_io) = tokio::io::duplex(1 << 16);
        let mode = Arc::new(AtomicU8::new(WRITE_ERROR));
        let io = Flaky {
            inner: client_io,
            mode,
        };
        assert!(
            trusting()
                .connect(server_name("localhost").unwrap(), io)
                .await
                .is_err()
        );

        let (client_io, _server_io) = tokio::io::duplex(1 << 16);
        let mode = Arc::new(AtomicU8::new(READ_ERROR));
        let io = Flaky {
            inner: client_io,
            mode,
        };
        assert!(
            trusting()
                .connect(server_name("localhost").unwrap(), io)
                .await
                .is_err()
        );

        let (client_io, _server_io) = tokio::io::duplex(1 << 16);
        let io = Flaky {
            inner: client_io,
            mode: Arc::new(AtomicU8::new(WRITE_ZERO)),
        };
        let err = trusting()
            .connect(server_name("localhost").unwrap(), io)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[tokio::test]
    async fn a_tiny_pipe_exercises_backpressure_in_both_directions() {
        let (client_io, server_io) = tokio::io::duplex(64);
        let acceptor = acceptor();
        let payload = vec![7u8; 50_000];
        let expected = payload.clone();
        let server = tokio::spawn(async move {
            let mut tls = acceptor.accept(server_io).await.unwrap();
            let mut got = vec![0u8; 50_000];
            tls.read_exact(&mut got).await.unwrap();
            tls.write_all(&got).await.unwrap();
            tls.shutdown().await.unwrap();
            got
        });
        let tls = trusting()
            .connect(server_name("localhost").unwrap(), client_io)
            .await
            .unwrap();
        let (mut rd, mut wr) = tokio::io::split(tls);
        let write = async {
            wr.write_all(&payload).await.unwrap();
            wr.flush().await.unwrap();
        };
        let mut echoed = vec![0u8; 50_000];
        let read = async {
            rd.read_exact(&mut echoed).await.unwrap();
        };
        tokio::join!(read, write);
        assert_eq!(echoed, expected);
        assert_eq!(server.await.unwrap(), expected);
    }

    #[tokio::test]
    async fn tls_1_2_only_servers_work_with_and_without_verification() {
        let chain = pem_certificates(&std::fs::read(fixture("server.pem")).unwrap()).unwrap();
        let key = pem_private_key(&std::fs::read(fixture("server.key")).unwrap()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls12 = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS12])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap();
        let acceptor = TlsAcceptor::from(tls12);
        for options in [
            ClientOptions {
                extra_roots_pem: vec![std::fs::read(fixture("ca.pem")).unwrap()],
                ..ClientOptions::default()
            },
            ClientOptions {
                accept_invalid_certs: true,
                ..ClientOptions::default()
            },
        ] {
            let (client_io, server_io) = tokio::io::duplex(1 << 16);
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let mut tls = acceptor.accept(server_io).await.unwrap();
                tls.write_all(b"v1.2").await.unwrap();
                tls.shutdown().await.unwrap();
            });
            let connector = TlsConnector::new(client_config(&options).unwrap());
            let mut tls = connector
                .connect(server_name("localhost").unwrap(), client_io)
                .await
                .unwrap();
            assert_eq!(
                tls.get_ref().1.protocol_version(),
                Some(rustls::ProtocolVersion::TLSv1_2)
            );
            let mut got = String::new();
            tls.read_to_string(&mut got).await.unwrap();
            assert_eq!(got, "v1.2");
        }
    }

    #[test]
    fn pem_helpers_parse_and_reject() {
        let cert = std::fs::read(fixture("server.pem")).unwrap();
        assert_eq!(pem_certificates(&cert).unwrap().len(), 1);
        assert!(
            pem_certificates(b"-----BEGIN CERTIFICATE-----\n@@@\n-----END CERTIFICATE-----\n")
                .is_err()
        );
        assert!(pem_certificates(b"nothing").unwrap().is_empty());
        let key = std::fs::read(fixture("server.key")).unwrap();
        assert!(pem_private_key(&key).is_ok());
        assert!(pem_private_key(b"no key here").is_err());
        // A key that does not belong to the chain is refused when the config is built.
        let chain = pem_certificates(&std::fs::read(fixture("ca.pem")).unwrap()).unwrap();
        assert!(server_config(chain, pem_private_key(&key).unwrap()).is_err());
    }
}

/// A server that requires a client certificate signed by the fixture client CA.
fn mutual_acceptor() -> TlsAcceptor {
    use rustls::RootCertStore;
    use rustls::server::WebPkiClientVerifier;

    let mut roots = RootCertStore::empty();
    for cert in
        quench_tls::pem_certificates(&std::fs::read(fixture("client-ca.pem")).unwrap()).unwrap()
    {
        roots.add(cert).unwrap();
    }
    let verifier = WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()
    .unwrap();
    let chain =
        quench_tls::pem_certificates(&std::fs::read(fixture("server.pem")).unwrap()).unwrap();
    let key = quench_tls::pem_private_key(&std::fs::read(fixture("server.key")).unwrap()).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_client_cert_verifier(verifier)
    .with_single_cert(chain, key)
    .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

fn server_options() -> ClientOptions {
    ClientOptions {
        extra_roots_pem: vec![std::fs::read(fixture("ca.pem")).unwrap()],
        ..ClientOptions::default()
    }
}

#[tokio::test]
async fn a_client_certificate_is_presented_when_one_is_configured() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = mutual_acceptor();
    let server = tokio::spawn(async move {
        let mut tls = acceptor
            .accept(server_io)
            .await
            .expect("the client certificate is accepted");
        let mut buf = [0u8; 2];
        tls.read_exact(&mut buf).await.unwrap();
        tls.write_all(b"ok").await.unwrap();
        tls.shutdown().await.unwrap();
    });
    let config = client_config_with_identity(
        &server_options(),
        &std::fs::read(fixture("client.pem")).unwrap(),
        &std::fs::read(fixture("client.key")).unwrap(),
    )
    .unwrap();
    let mut tls = TlsConnector::new(config)
        .connect(server_name("localhost").unwrap(), client_io)
        .await
        .unwrap();
    tls.write_all(b"hi").await.unwrap();
    let mut reply = Vec::new();
    tls.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, b"ok");
    server.await.unwrap();
}

#[tokio::test]
async fn without_a_client_certificate_a_mutual_server_refuses() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let acceptor = mutual_acceptor();
    let server = tokio::spawn(async move { acceptor.accept(server_io).await.is_err() });
    let anonymous = TlsConnector::new(client_config(&server_options()).unwrap());
    let outcome = async {
        let mut tls = anonymous
            .connect(server_name("localhost").unwrap(), client_io)
            .await?;
        tls.write_all(b"hi").await?;
        let mut buf = Vec::new();
        tls.read_to_end(&mut buf).await?;
        Ok::<_, std::io::Error>(buf)
    }
    .await;
    assert!(server.await.unwrap(), "the server rejected the handshake");
    assert!(outcome.is_err() || outcome.unwrap().is_empty());
}

#[test]
fn a_bad_client_identity_is_described() {
    let options = server_options();
    let cert = std::fs::read(fixture("client.pem")).unwrap();
    let key = std::fs::read(fixture("client.key")).unwrap();
    assert!(
        client_config_with_identity(&options, b"", &key)
            .unwrap_err()
            .contains("no certificate")
    );
    assert!(
        client_config_with_identity(&options, &cert, b"not a key")
            .unwrap_err()
            .contains("private key")
    );
    // A key that does not belong to the certificate is refused too.
    let other_key = std::fs::read(fixture("server.key")).unwrap();
    assert!(client_config_with_identity(&options, &cert, &other_key).is_err());
    // With verification off the identity is still sent.
    let lax = ClientOptions {
        accept_invalid_certs: true,
        ..ClientOptions::default()
    };
    assert!(client_config_with_identity(&lax, &cert, &key).is_ok());
}
