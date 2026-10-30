---
title: Testing
parent: quench-mail
grand_parent: Crates
nav_order: 6
---

The crate has one integration-test binary, `tests/unit.rs`, declared in `Cargo.toml` as `[[test]] name = "unit" path = "tests/unit.rs" harness = false`, with `quench_test::main!()` as the harness. Modules: `address_tests`, `message_tests`, `mime_tests`, `smtp_tests` and the helper `support`.

```bash
cargo test -p quench-mail
```

## The fake server

`tests/unit/support/mod.rs` is an in-process scriptable SMTP server on real sockets and real TLS, using the throwaway `localhost` certificate and key in `tests/fixtures/`. A `Behavior` struct sets the scenario:

| Field | Meaning |
|---|---|
| `tls: Tls` | `None`, `Implicit` or `Upgrade` (STARTTLS). |
| `auth_mechanisms` | Default `["PLAIN", "LOGIN"]`. |
| `credentials` | Required login, or accept anyone. |
| `size` | Advertised `SIZE`. |
| `bad_rcpt_reply` | Reply for recipients containing `bad` (default `550 5.1.1 no such mailbox`). |
| `data_reply` | Reply after DATA (default `250 2.0.0 queued as TEST1`). |
| `silent`, `hang_up` | Never speak; hang up after the greeting. |
| `inject_after_starttls` | Bytes appended in the clear to the `220` for STARTTLS. |
| `hide_starttls` | Do not advertise STARTTLS. |

The server records what it received. The test helpers `CERT_PEM` (the fixture certificate) lets tests use `add_root_certificate_pem`.

## What the SMTP tests cover

Implicit TLS with login, STARTTLS (EHLO asked again after the upgrade), no-TLS no-login, AUTH LOGIN fallback, Bcc `RCPT TO`, `test_connection`, cloned mailers, leading-dot lines and `dot_stuff`, wrong password (`Auth`, not transient), permanent and temporary recipient refusals, refusal after DATA, `SIZE` limit, untrusted certificate, wrong certificate name, STARTTLS required but hidden, injection after the STARTTLS reply, credentials without TLS refused at build, no supported login mechanism, timeout, hang-up as `Protocol`, refused connection as transient `Io`, default ports per security mode, invalid configuration, password absent from `Debug` output.

## Testing code that uses the crate

`Message` building and `formatted()` need no network and are deterministic apart from the `Date`, Message-ID and multipart boundary; use `MessageBuilder::date(Time)` to pin the date. For delivery, point a `Mailer` with `Security::None` at a local listener, or copy the fake-server approach above. Nothing in the public API provides a mock transport.
