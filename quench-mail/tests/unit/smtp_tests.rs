use crate::support::{Behavior, CERT_PEM, FakeServer, Tls, start};
use quench_mail::{Error, Mailer, MailerBuilder, Message, Security, dot_stuff};
use std::time::Duration;

fn message() -> Message {
    Message::builder()
        .from("Forge <noreply@example.com>")
        .to("alice@example.org")
        .subject("Confirm your address")
        .text("Follow the link.")
        .build()
        .unwrap()
}

fn client(server: &FakeServer) -> MailerBuilder {
    Mailer::builder("127.0.0.1")
        .port(server.addr.port())
        .timeout(Duration::from_secs(5))
}

/// Trusts the test certificate and validates it as `localhost`.
fn tls_client(server: &FakeServer, security: Security) -> MailerBuilder {
    client(server)
        .security(security)
        .tls_server_name("localhost")
        .add_root_certificate_pem(CERT_PEM)
        .unwrap()
}

fn login() -> Behavior {
    Behavior {
        credentials: Some(("noreply@example.com", "s3cret")),
        ..Behavior::default()
    }
}

// -- happy paths ----------------------------------------------------

#[tokio::test]
async fn sends_over_implicit_tls_with_a_login() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        ..login()
    })
    .await;
    let mailer = tls_client(&server, Security::Tls)
        .credentials("noreply@example.com", "s3cret")
        .hello_name("forge.test")
        .build()
        .unwrap();

    let response = mailer.send(&message()).await.expect("send");
    assert_eq!(response.reply.code, 250);
    assert_eq!(response.reply.enhanced.as_deref(), Some("2.0.0"));
    assert_eq!(response.message(), "queued as TEST1");

    let got = server.last().await;
    assert!(got.used_tls);
    assert_eq!(got.helo, "forge.test");
    assert_eq!(got.authed_as.as_deref(), Some("noreply@example.com"));
    assert_eq!(got.from, "noreply@example.com");
    assert_eq!(got.rcpt, ["alice@example.org"]);
    assert!(got.data.contains("Subject: Confirm your address\r\n"));
    assert!(got.data.ends_with("Follow the link.\r\n"));
}

#[tokio::test]
async fn sends_over_starttls_and_asks_again_after_the_upgrade() {
    let server = start(Behavior {
        tls: Tls::Upgrade,
        ..login()
    })
    .await;
    let mailer = tls_client(&server, Security::StartTls)
        .credentials("noreply@example.com", "s3cret")
        .build()
        .unwrap();

    mailer.send(&message()).await.expect("send");

    let got = server.last().await;
    assert!(got.used_tls, "the mail itself must travel over TLS");
    let commands: Vec<&str> = got.commands.iter().map(String::as_str).collect();
    let starttls = commands.iter().position(|c| *c == "STARTTLS").unwrap();
    let ehlos: Vec<usize> = commands
        .iter()
        .enumerate()
        .filter(|(_, c)| c.starts_with("EHLO"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        ehlos.len(),
        2,
        "EHLO before and after STARTTLS: {commands:?}"
    );
    assert!(ehlos[0] < starttls && starttls < ehlos[1]);
    let auth = commands.iter().position(|c| c.starts_with("AUTH")).unwrap();
    assert!(
        auth > ehlos[1],
        "login only after the upgrade: {commands:?}"
    );
}

#[tokio::test]
async fn sends_without_tls_or_login_when_configured_that_way() {
    let server = start(Behavior::default()).await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    mailer.send(&message()).await.expect("send");
    let got = server.last().await;
    assert!(!got.used_tls);
    assert_eq!(got.authed_as, None);
    assert!(!got.commands.iter().any(|c| c.starts_with("AUTH")));
}

#[tokio::test]
async fn falls_back_to_login_when_plain_is_not_offered() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        auth_mechanisms: vec!["LOGIN"],
        ..login()
    })
    .await;
    let mailer = tls_client(&server, Security::Tls)
        .credentials("noreply@example.com", "s3cret")
        .build()
        .unwrap();
    mailer.send(&message()).await.expect("send");
    let got = server.last().await;
    assert_eq!(got.authed_as.as_deref(), Some("noreply@example.com"));
    assert!(got.commands.contains(&"AUTH LOGIN".to_string()));
}

#[tokio::test]
async fn every_recipient_including_bcc_gets_a_rcpt_to() {
    let server = start(Behavior::default()).await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let message = Message::builder()
        .from("a@example.com")
        .to("b@example.org")
        .cc("c@example.org")
        .bcc("d@example.org")
        .text("hi")
        .build()
        .unwrap();
    mailer.send(&message).await.unwrap();
    assert_eq!(
        server.last().await.rcpt,
        ["b@example.org", "c@example.org", "d@example.org"]
    );
    assert!(!server.last().await.data.contains("d@example.org"));
}

#[tokio::test]
async fn test_connection_logs_in_without_sending_anything() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        ..login()
    })
    .await;
    let mailer = tls_client(&server, Security::Tls)
        .credentials("noreply@example.com", "s3cret")
        .build()
        .unwrap();
    mailer.test_connection().await.expect("connection works");
    let got = server.last().await;
    assert_eq!(got.authed_as.as_deref(), Some("noreply@example.com"));
    assert!(got.data.is_empty());
    assert!(!got.commands.iter().any(|c| c.starts_with("MAIL")));
}

#[tokio::test]
async fn a_mailer_can_be_cloned_and_reused() {
    let server = start(Behavior::default()).await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let copy = mailer.clone();
    mailer.send(&message()).await.unwrap();
    copy.send(&message()).await.unwrap();
    server.wait_for(2).await;
    assert_eq!(server.sessions(), 2);
}

// -- data integrity -------------------------------------------------

#[tokio::test]
async fn lines_starting_with_a_dot_survive_the_trip() {
    let server = start(Behavior::default()).await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let message = Message::builder()
        .from("a@example.com")
        .to("b@example.org")
        .text(".hidden\n.\n..two\nafter the lone dot")
        .build()
        .unwrap();
    mailer.send(&message).await.unwrap();
    let data = server.last().await.data;
    assert!(
        data.contains("\r\n.hidden\r\n.\r\n..two\r\nafter the lone dot\r\n"),
        "{data:?}"
    );
}

#[test]
fn dot_stuffing_doubles_leading_dots_and_terminates() {
    assert_eq!(dot_stuff(b"a\r\n.b\r\n"), b"a\r\n..b\r\n.\r\n");
    assert_eq!(dot_stuff(b".x"), b"..x\r\n.\r\n");
    assert_eq!(dot_stuff(b"a\r\n.\r\nb"), b"a\r\n..\r\nb\r\n.\r\n");
    assert_eq!(dot_stuff(b"no dots\r\n"), b"no dots\r\n.\r\n");
    assert_eq!(dot_stuff(b""), b"\r\n.\r\n");
    assert_eq!(dot_stuff(b"mid.dot\r\n"), b"mid.dot\r\n.\r\n");
}

// -- refusals -------------------------------------------------------

#[tokio::test]
async fn a_wrong_password_is_an_auth_error_and_not_retryable() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        ..login()
    })
    .await;
    let mailer = tls_client(&server, Security::Tls)
        .credentials("noreply@example.com", "wrong")
        .build()
        .unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
    assert!(!err.is_transient());
    let reply = err.reply().unwrap();
    assert_eq!(reply.code, 535);
    assert_eq!(reply.enhanced.as_deref(), Some("5.7.8"));
    assert!(
        server.last().await.data.is_empty(),
        "nothing sent after a failed login"
    );
}

#[tokio::test]
async fn a_permanent_recipient_refusal_is_reported_and_not_retryable() {
    let server = start(Behavior::default()).await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let message = Message::builder()
        .from("a@example.com")
        .to("bad@example.org")
        .text("x")
        .build()
        .unwrap();
    let err = mailer.send(&message).await.unwrap_err();
    match &err {
        Error::Rejected { stage, reply } => {
            assert_eq!(*stage, "RCPT TO");
            assert_eq!(reply.code, 550);
            assert_eq!(reply.enhanced.as_deref(), Some("5.1.1"));
            assert_eq!(reply.message(), "no such mailbox");
        }
        other => panic!("{other:?}"),
    }
    assert!(!err.is_transient());
    assert!(err.to_string().contains("RCPT TO"), "{err}");
}

#[tokio::test]
async fn a_temporary_refusal_is_retryable() {
    let server = start(Behavior {
        bad_rcpt_reply: "451 4.3.0 try again later",
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let message = Message::builder()
        .from("a@example.com")
        .to("bad@example.org")
        .text("x")
        .build()
        .unwrap();
    let err = mailer.send(&message).await.unwrap_err();
    assert!(err.is_transient(), "{err:?}");
}

#[tokio::test]
async fn a_refusal_after_data_is_reported() {
    let server = start(Behavior {
        data_reply: "554 5.7.1 message looks like spam",
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    match err {
        Error::Rejected { stage, reply } => {
            assert_eq!(stage, "end of DATA");
            assert_eq!(reply.code, 554);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_message_over_the_advertised_size_is_not_sent() {
    let server = start(Behavior {
        size: Some(100),
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::TooLarge { limit: 100, .. }), "{err:?}");
    assert!(
        !server
            .last()
            .await
            .commands
            .iter()
            .any(|c| c.starts_with("MAIL"))
    );
}

// -- TLS safety -----------------------------------------------------

#[tokio::test]
async fn an_untrusted_certificate_is_refused() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        ..Behavior::default()
    })
    .await;
    // No extra root: the throwaway certificate is not in the public store.
    let mailer = client(&server)
        .security(Security::Tls)
        .tls_server_name("localhost")
        .build()
        .unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Tls(_)), "{err:?}");
}

#[tokio::test]
async fn a_certificate_for_another_name_is_refused() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server)
        .security(Security::Tls)
        .tls_server_name("someone-else.example")
        .add_root_certificate_pem(CERT_PEM)
        .unwrap()
        .build()
        .unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Tls(_)), "{err:?}");
}

#[tokio::test]
async fn starttls_is_required_when_configured_even_if_the_server_hides_it() {
    let server = start(Behavior {
        tls: Tls::Upgrade,
        hide_starttls: true,
        ..Behavior::default()
    })
    .await;
    let mailer = tls_client(&server, Security::StartTls).build().unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err:?}");
    assert!(
        server.last().await.data.is_empty(),
        "must not fall back to plaintext"
    );
}

#[tokio::test]
async fn data_injected_after_the_starttls_reply_is_rejected() {
    let server = start(Behavior {
        tls: Tls::Upgrade,
        inject_after_starttls: Some("250 injected"),
        ..Behavior::default()
    })
    .await;
    let mailer = tls_client(&server, Security::StartTls).build().unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Protocol(_)), "{err:?}");
}

#[test]
fn credentials_without_tls_are_refused_at_build_time() {
    let err = Mailer::builder("mail.example.com")
        .security(Security::None)
        .credentials("u", "p")
        .build()
        .unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err:?}");
    assert!(
        Mailer::builder("mail.example.com")
            .security(Security::None)
            .credentials("u", "p")
            .allow_plaintext_auth(true)
            .build()
            .is_ok()
    );
}

#[tokio::test]
async fn credentials_are_not_sent_when_the_server_offers_no_login() {
    let server = start(Behavior {
        tls: Tls::Implicit,
        auth_mechanisms: vec![],
        ..Behavior::default()
    })
    .await;
    let mailer = tls_client(&server, Security::Tls)
        .credentials("u", "p")
        .build()
        .unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err:?}");
    assert!(
        !server
            .last()
            .await
            .commands
            .iter()
            .any(|c| c.starts_with("AUTH"))
    );
}

// -- robustness -----------------------------------------------------

#[tokio::test]
async fn a_silent_server_times_out() {
    let server = start(Behavior {
        silent: true,
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server)
        .security(Security::None)
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let started = std::time::Instant::now();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err:?}");
    assert!(err.is_transient());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_server_that_hangs_up_is_a_protocol_error() {
    let server = start(Behavior {
        hang_up: true,
        ..Behavior::default()
    })
    .await;
    let mailer = client(&server).security(Security::None).build().unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Protocol(_) | Error::Io(_)), "{err:?}");
}

#[tokio::test]
async fn a_refused_connection_is_a_retryable_io_error() {
    let mailer = Mailer::builder("127.0.0.1")
        .port(1)
        .security(Security::None)
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let err = mailer.send(&message()).await.unwrap_err();
    assert!(matches!(err, Error::Io(_)), "{err:?}");
    assert!(err.is_transient());
}

// -- configuration --------------------------------------------------

#[test]
fn default_ports_follow_the_security_mode() {
    // Not observable without connecting; build must at least succeed for each.
    for security in [Security::Tls, Security::StartTls, Security::None] {
        assert!(
            Mailer::builder("mail.example.com")
                .security(security)
                .build()
                .is_ok()
        );
    }
}

#[test]
fn nonsense_configuration_is_rejected() {
    assert!(matches!(
        Mailer::builder("").build().unwrap_err(),
        Error::Config(_)
    ));
    assert!(matches!(
        Mailer::builder("bad host").build().unwrap_err(),
        Error::Config(_)
    ));
    assert!(matches!(
        Mailer::builder("h\r\nQUIT").build().unwrap_err(),
        Error::Config(_)
    ));
    assert!(matches!(
        Mailer::builder("mail.example.com")
            .hello_name("bad name")
            .build()
            .unwrap_err(),
        Error::Config(_)
    ));
    assert!(matches!(
        Mailer::builder("mail.example.com")
            .add_root_certificate_pem("not a certificate")
            .unwrap_err(),
        Error::Config(_)
    ));
}

#[test]
fn the_password_never_shows_up_in_debug_output() {
    let mailer = Mailer::builder("mail.example.com")
        .credentials("noreply@example.com", "hunter2-secret")
        .build()
        .unwrap();
    let shown = format!("{mailer:?}");
    assert!(!shown.contains("hunter2-secret"), "{shown}");
    assert!(shown.contains("mail.example.com"));
}
