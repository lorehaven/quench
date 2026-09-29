use quench_mail::{Address, BuildError, Mailbox};

#[test]
fn a_plain_address_splits_into_local_and_domain() {
    let address: Address = "alice@example.com".parse().unwrap();
    assert_eq!(address.local(), "alice");
    assert_eq!(address.domain(), "example.com");
    assert_eq!(address.as_str(), "alice@example.com");
    assert_eq!(address.to_string(), "alice@example.com");
}

#[test]
fn common_local_part_characters_are_accepted() {
    for ok in [
        "first.last@example.com",
        "user+tag@example.com",
        "o'brien@example.com",
        "a_b-c@sub.example.co.uk",
        "x@localhost",
        "1@2.io",
    ] {
        assert!(ok.parse::<Address>().is_ok(), "{ok}");
    }
}

#[test]
fn malformed_addresses_are_rejected() {
    let long_local = format!("{}@example.com", "a".repeat(65));
    let long_domain = format!("a@{}.com", "b".repeat(64));
    for bad in [
        "",
        "plain",
        "@example.com",
        "alice@",
        "a b@example.com",
        "alice@exa mple.com",
        ".alice@example.com",
        "alice.@example.com",
        "al..ice@example.com",
        "alice@-example.com",
        "alice@example-.com",
        "alice@exa_mple.com",
        "alice@.com",
        "alice@example..com",
        "\"quoted\"@example.com",
        "ali(ce)@example.com",
        "zoë@example.com",
        "alice@exämple.com",
        &long_local,
        &long_domain,
    ] {
        assert!(
            bad.parse::<Address>().is_err(),
            "{bad:?} should be rejected"
        );
    }
}

#[test]
fn line_breaks_and_smtp_command_syntax_cannot_smuggle_in() {
    for bad in [
        "a@b.c\r\nBcc: x@y.z",
        "a@b.c\nRCPT TO:<x@y.z>",
        "a@b.c>\r\nMAIL FROM:<x@y.z",
        "a>b@c.d",
        "a<b@c.d",
        "a@b.c\0",
    ] {
        assert!(bad.parse::<Address>().is_err(), "{bad:?}");
        assert!(bad.parse::<Mailbox>().is_err(), "{bad:?}");
    }
}

#[test]
fn address_new_validates_like_parsing() {
    assert!(Address::new("alice", "example.com").is_ok());
    assert_eq!(
        Address::new("alice", "bad domain").unwrap_err(),
        BuildError::InvalidAddress("alice@bad domain".into())
    );
}

#[test]
fn mailbox_forms_parse() {
    let bare: Mailbox = "alice@example.com".parse().unwrap();
    assert_eq!(bare.name, None);

    let angled: Mailbox = "<alice@example.com>".parse().unwrap();
    assert_eq!(angled.name, None);
    assert_eq!(angled.address.as_str(), "alice@example.com");

    let named: Mailbox = "Alice Liddell <alice@example.com>".parse().unwrap();
    assert_eq!(named.name.as_deref(), Some("Alice Liddell"));

    let padded: Mailbox = "  Alice   <  alice@example.com  >  ".parse().unwrap();
    assert_eq!(padded.name.as_deref(), Some("Alice"));
    assert_eq!(padded.address.as_str(), "alice@example.com");
}

#[test]
fn quoted_display_names_are_unquoted() {
    let m: Mailbox = "\"Liddell, Alice\" <alice@example.com>".parse().unwrap();
    assert_eq!(m.name.as_deref(), Some("Liddell, Alice"));

    let escaped: Mailbox = r#""Al \"the\" Great" <al@example.com>"#.parse().unwrap();
    assert_eq!(escaped.name.as_deref(), Some(r#"Al "the" Great"#));
}

#[test]
fn non_ascii_display_names_are_kept_for_later_encoding() {
    let m: Mailbox = "Zoë Łukasz <zoe@example.com>".parse().unwrap();
    assert_eq!(m.name.as_deref(), Some("Zoë Łukasz"));
}

#[test]
fn malformed_mailboxes_are_rejected() {
    for bad in [
        "Alice <alice@example.com",
        "Alice <alice@example.com> trailing",
        "\"Alice <alice@example.com>",
        "Alice <>",
        "Alice <not-an-address>",
        "Ali\u{7}ce <alice@example.com>",
        "Ali\r\nce <alice@example.com>",
    ] {
        assert!(bad.parse::<Mailbox>().is_err(), "{bad:?}");
    }
}

#[test]
fn mailbox_display_is_readable() {
    let named: Mailbox = "Alice <alice@example.com>".parse().unwrap();
    assert_eq!(named.to_string(), "Alice <alice@example.com>");
    let bare: Mailbox = "alice@example.com".parse().unwrap();
    assert_eq!(bare.to_string(), "alice@example.com");
    assert_eq!(Mailbox::from(bare.address.clone()), bare);
}
