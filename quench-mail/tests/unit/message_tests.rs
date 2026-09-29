use chrono::{TimeZone, Utc};
use quench_mail::{BuildError, Message, MessageBuilder};

fn base() -> MessageBuilder {
    Message::builder()
        .from("Forge <noreply@example.com>")
        .to("alice@example.org")
        .subject("Confirm your address")
        .text("Follow the link.")
        .date(Utc.with_ymd_and_hms(2026, 9, 29, 12, 0, 0).unwrap())
}

fn wire(message: &Message) -> String {
    String::from_utf8(message.formatted()).expect("wire form is ASCII")
}

fn headers(wire: &str) -> &str {
    wire.split("\r\n\r\n").next().unwrap()
}

#[test]
fn a_minimal_message_builds() {
    assert!(base().build().is_ok());
}

#[test]
fn missing_pieces_are_reported() {
    assert_eq!(
        Message::builder()
            .to("a@b.c")
            .text("x")
            .build()
            .unwrap_err(),
        BuildError::MissingFrom
    );
    assert_eq!(
        Message::builder()
            .from("a@b.c")
            .text("x")
            .build()
            .unwrap_err(),
        BuildError::NoRecipients
    );
    assert_eq!(
        Message::builder()
            .from("a@b.c")
            .to("d@e.f")
            .build()
            .unwrap_err(),
        BuildError::MissingBody
    );
}

#[test]
fn a_bcc_only_message_still_has_a_recipient() {
    assert!(
        Message::builder()
            .from("a@b.c")
            .bcc("d@e.f")
            .text("x")
            .build()
            .is_ok()
    );
}

#[test]
fn an_invalid_address_anywhere_fails_the_build_with_that_address() {
    for builder in [
        base().from("not an address"),
        base().to("nope"),
        base().cc("nope"),
        base().bcc("nope"),
        base().reply_to("nope"),
        base().envelope_from("nope"),
    ] {
        assert!(matches!(
            builder.build().unwrap_err(),
            BuildError::InvalidAddress(_)
        ));
    }
}

#[test]
fn the_first_error_wins() {
    let err = base().to("first-bad").to("second-bad").build().unwrap_err();
    assert_eq!(err, BuildError::InvalidAddress("first-bad".into()));
}

#[test]
fn header_injection_is_refused() {
    assert_eq!(
        base()
            .subject("hi\r\nBcc: evil@example.net")
            .build()
            .unwrap_err(),
        BuildError::HeaderInjection("subject")
    );
    assert_eq!(
        base()
            .header("X-Note", "a\nBcc: evil@example.net")
            .build()
            .unwrap_err(),
        BuildError::HeaderInjection("header value")
    );
    assert!(matches!(
        base().to("Eve\r\nBcc: x@y.z <e@e.ee>").build().unwrap_err(),
        BuildError::HeaderInjection(_)
    ));
}

#[test]
fn reserved_and_malformed_header_names_are_refused() {
    for name in [
        "From",
        "subject",
        "Content-Type",
        "MIME-Version",
        "Message-ID",
        "Date",
        "Bcc",
    ] {
        assert_eq!(
            base().header(name, "x").build().unwrap_err(),
            BuildError::ReservedHeader(name.to_string()),
            "{name}"
        );
    }
    for name in ["", "Has Space", "Colon:", "Ünicode"] {
        assert!(
            matches!(
                base().header(name, "x").build().unwrap_err(),
                BuildError::InvalidHeaderName(_)
            ),
            "{name:?}"
        );
    }
}

#[test]
fn envelope_defaults_to_the_from_address_and_lists_every_recipient_once() {
    let message = base()
        .to("Alice <alice@example.org>")
        .cc("bob@example.org")
        .bcc("ALICE@example.org")
        .bcc("carol@example.org")
        .build()
        .unwrap();
    let envelope = message.envelope();
    assert_eq!(envelope.from.as_str(), "noreply@example.com");
    let to: Vec<&str> = envelope.to.iter().map(|a| a.as_str()).collect();
    assert_eq!(
        to,
        [
            "alice@example.org",
            "alice@example.org",
            "bob@example.org",
            "carol@example.org"
        ]
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, a)| *a)
        .collect::<Vec<_>>()
    );
}

#[test]
fn envelope_from_can_differ_from_the_header() {
    let message = base().envelope_from("bounces@example.com").build().unwrap();
    assert_eq!(message.envelope().from.as_str(), "bounces@example.com");
    assert!(wire(&message).contains("From: Forge <noreply@example.com>"));
    assert!(!wire(&message).contains("bounces@"));
}

#[test]
fn bcc_recipients_are_delivered_to_but_never_named() {
    let message = base().bcc("secret@example.net").build().unwrap();
    assert!(
        message
            .envelope()
            .to
            .iter()
            .any(|a| a.as_str() == "secret@example.net")
    );
    assert!(!wire(&message).contains("secret@example.net"));
    assert!(!wire(&message).to_ascii_lowercase().contains("bcc"));
}

#[test]
fn a_text_message_has_the_expected_headers_and_body() {
    let text = wire(&base().build().unwrap());
    let head = headers(&text);
    assert!(
        head.contains("Date: Tue, 29 Sep 2026 12:00:00 +0000"),
        "{head}"
    );
    assert!(head.contains("From: Forge <noreply@example.com>"));
    assert!(head.contains("To: alice@example.org"));
    assert!(head.contains("Subject: Confirm your address"));
    assert!(head.contains("MIME-Version: 1.0"));
    assert!(head.contains("Content-Type: text/plain; charset=utf-8"));
    assert!(head.contains("Content-Transfer-Encoding: 7bit"));
    assert!(text.ends_with("\r\n\r\nFollow the link.\r\n"));
}

#[test]
fn every_line_ends_in_crlf_and_the_message_is_ascii() {
    let message = base()
        .subject("Zażółć gęślą jaźń")
        .text("first\nsecond\rthird\r\nfourth zażółć")
        .html("<p>héllo</p>\n<p>wörld</p>")
        .build()
        .unwrap();
    let bytes = message.formatted();
    assert!(bytes.is_ascii());
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            assert!(i > 0 && bytes[i - 1] == b'\r', "bare LF at {i}");
        }
        if b == b'\r' {
            assert_eq!(bytes.get(i + 1), Some(&b'\n'), "bare CR at {i}");
        }
    }
}

#[test]
fn message_id_uses_the_sender_domain_and_is_unique() {
    let a = base().build().unwrap();
    let b = base().build().unwrap();
    assert!(a.message_id().starts_with('<') && a.message_id().ends_with("@example.com>"));
    assert_ne!(a.message_id(), b.message_id());
    assert!(wire(&a).contains(&format!("Message-ID: {}", a.message_id())));
}

#[test]
fn non_ascii_subject_and_names_become_encoded_words() {
    let message = base()
        .from("Zoë Łukasz <noreply@example.com>")
        .to("Żaneta <zaneta@example.org>")
        .subject("Potwierdź adres e-mail")
        .build()
        .unwrap();
    let text = wire(&message);
    let head = headers(&text);
    assert!(head.contains("Subject: =?UTF-8?B?"), "{head}");
    let unfolded = head.replace("\r\n ", " ");
    let line = |name: &str| {
        unfolded
            .split("\r\n")
            .find(|l| l.starts_with(name))
            .unwrap_or_else(|| panic!("no {name} header in {unfolded}"))
            .to_string()
    };
    assert!(line("From: ").contains("=?UTF-8?B?"), "{}", line("From: "));
    assert!(line("To: ").contains("=?UTF-8?B?"), "{}", line("To: "));
    assert!(line("To: ").contains("alice@example.org"));
    assert!(head.is_ascii());
}

#[test]
fn a_display_name_with_specials_is_quoted() {
    let message = base()
        .to("\"Liddell, Alice\" <alice@example.org>")
        .build()
        .unwrap();
    let text = wire(&message);
    let unfolded = headers(&text).replace("\r\n ", " ");
    assert!(
        unfolded.contains("To: alice@example.org, \"Liddell, Alice\" <alice@example.org>"),
        "{unfolded}"
    );
}

#[test]
fn several_recipients_share_a_header() {
    let message = base()
        .to("b@example.org")
        .cc("c@example.org")
        .cc("d@example.org")
        .reply_to("Support <help@example.com>")
        .build()
        .unwrap();
    let text = wire(&message);
    assert!(text.contains("To: alice@example.org, b@example.org"));
    assert!(text.contains("Cc: c@example.org, d@example.org"));
    assert!(text.contains("Reply-To: Support <help@example.com>"));
}

#[test]
fn long_recipient_lists_fold_below_78_columns() {
    let mut builder = base();
    for i in 0..12 {
        builder = builder.to(&format!("recipient{i}@example.org"));
    }
    let text = wire(&builder.build().unwrap());
    for line in headers(&text).split("\r\n") {
        assert!(line.len() <= 78, "{line:?}");
    }
}

#[test]
fn custom_headers_are_written() {
    let message = base()
        .header("Auto-Submitted", "auto-generated")
        .header("X-Forge-Kind", "verification")
        .build()
        .unwrap();
    let text = wire(&message);
    assert!(text.contains("Auto-Submitted: auto-generated"));
    assert!(text.contains("X-Forge-Kind: verification"));
}

#[test]
fn non_ascii_body_is_quoted_printable() {
    let message = base().text("Zażółć gęślą jaźń").build().unwrap();
    let text = wire(&message);
    assert!(text.contains("Content-Transfer-Encoding: quoted-printable"));
    assert!(text.contains("Za=C5=BC=C3=B3=C5=82=C4=87"));
}

#[test]
fn html_only_is_a_single_html_part() {
    let message = Message::builder()
        .from("a@b.c")
        .to("d@e.f")
        .html("<p>hi</p>")
        .build()
        .unwrap();
    let text = wire(&message);
    assert!(text.contains("Content-Type: text/html; charset=utf-8"));
    assert!(!text.contains("multipart"));
}

#[test]
fn text_and_html_make_multipart_alternative_with_html_last() {
    let message = base().html("<p>Follow the link.</p>").build().unwrap();
    let text = wire(&message);
    let head = headers(&text);
    let boundary = head
        .split("boundary=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("boundary in the Content-Type header");
    assert!(head.contains("Content-Type: multipart/alternative;"));

    let opening = format!("--{boundary}\r\n");
    let closing = format!("--{boundary}--\r\n");
    assert_eq!(text.matches(&opening).count(), 2);
    assert!(text.ends_with(&closing));

    let plain = text.find("text/plain").unwrap();
    let html = text.find("text/html").unwrap();
    assert!(plain < html, "plain part must precede the html part");
    assert!(text.contains("Follow the link.\r\n--"));
    assert!(text.contains("<p>Follow the link.</p>\r\n--"));
}

#[test]
fn boundaries_differ_between_messages() {
    let boundary = |m: &Message| {
        wire(m)
            .split("boundary=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(str::to_string)
            .unwrap()
    };
    let a = base().html("<p>x</p>").build().unwrap();
    let b = base().html("<p>x</p>").build().unwrap();
    assert_ne!(boundary(&a), boundary(&b));
}

#[test]
fn a_body_that_is_not_newline_terminated_still_ends_in_crlf() {
    let text = wire(&base().text("no newline").build().unwrap());
    assert!(text.ends_with("no newline\r\n"));
    let text = wire(&base().text("has newline\n").build().unwrap());
    assert!(text.ends_with("has newline\r\n"));
    assert!(!text.ends_with("\r\n\r\n"));
}

#[test]
fn prebuilt_mailboxes_keep_their_display_name_verbatim() {
    use quench_mail::{Address, Mailbox};
    let from = Mailbox::new(
        Some("Zoë <not a tag> \"Z\"".to_string()),
        Address::new("noreply", "example.com").unwrap(),
    );
    let to = Mailbox::from(Address::new("alice", "example.org").unwrap());
    let message = Message::builder()
        .from_mailbox(from)
        .to_mailbox(to)
        .text("hi")
        .build()
        .unwrap();
    let text = String::from_utf8(message.formatted()).unwrap();
    assert!(text.contains("From: =?UTF-8?B?"), "{text}");
    assert!(text.contains("To: alice@example.org"));
    assert_eq!(message.envelope().from.as_str(), "noreply@example.com");
}
