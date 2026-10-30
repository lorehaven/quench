---
title: Configuration
parent: quench-mail
grand_parent: Crates
nav_order: 3
---

The crate reads no environment variables and no files. Everything is passed to `MailerBuilder` (from `Mailer::builder(host)`).

## Builder options

| Option | Type | Default | Meaning |
|---|---|---|---|
| `host` (argument of `Mailer::builder`) | `&str` | required | Host connected to. Must be non-empty and contain no control or whitespace character. |
| `.security(..)` | `Security` | `Security::Tls` | `Tls` (implicit TLS), `StartTls`, `None`. |
| `.port(..)` | `u16` | `465` for `Tls`, `587` for `StartTls`, `25` for `None` | The default follows the security mode chosen at `build()`. |
| `.tls_server_name(..)` | `&str` | the host | Name the server certificate is validated for (also the SNI name). Must be accepted by `quench_tls::server_name`. |
| `.credentials(user, password)` | `&str, &str` | none | Enables login. Used with `AUTH PLAIN` or `AUTH LOGIN`. |
| `.hello_name(..)` | `&str` | `localhost` | The name in `EHLO`/`HELO`. Must be non-empty with no whitespace or control chars. |
| `.timeout(..)` | `Duration` | 30 s | Limit for connecting, the TLS handshake, each write, and each wait for a reply. It is per step, not per send. |
| `.add_root_certificate_pem(..)` | `&str` | none | Adds trust anchors from PEM besides `webpki-roots`. Repeatable. |
| `.allow_plaintext_auth(..)` | `bool` | `false` | Allows credentials with `Security::None`. |

## Validation at `build()`

`build()` returns `Error::Config` for:

* empty (after trim) host: `"SMTP host is empty"`;
* whitespace or control characters in the host, whitespace or control characters or an empty value in the EHLO name: `"host and EHLO name must not contain whitespace"`;
* credentials with `Security::None` and `allow_plaintext_auth` false: `"refusing to send credentials without TLS (see allow_plaintext_auth)"`;
* an invalid TLS server name: `invalid TLS server name "<name>"`.

A failure to build the TLS client configuration is `Error::Tls(<text>)`.

`add_root_certificate_pem` returns `Error::Config` when the PEM is malformed (text from `quench_tls::root_store`) or when adding it did not increase the size of the root store (`"no certificate found in the PEM"`). The latter also happens for a PEM that is not a certificate.

## Trust

Server certificates are checked against `webpki-roots` (via `quench_tls`) plus the PEM roots added. The operating system bundle is not read (`ClientOptions::use_system_roots` stays `false`), and `accept_invalid_certs` stays `false`: there is no way to disable verification.

## Fixed limits (constants in `src/smtp.rs`, `src/mime.rs`)

| Limit | Value | Effect |
|---|---|---|
| Reply line | 4096 bytes | Longer: `Error::Protocol("reply line too long")`. |
| Reply lines | 100 | `Error::Protocol("reply has too many lines")`. |
| Encoded word | 45 raw bytes | Text per `=?UTF-8?B?...?=` word. |
| Header fold | 78 columns | `mime::fold_header`. |
| Quit wait | 2 s | Wait for the reply to `QUIT`; errors ignored. |
| Message size | the server's `SIZE` extension | Larger data: `Error::TooLarge`. No limit when the server does not advertise `SIZE`. |
