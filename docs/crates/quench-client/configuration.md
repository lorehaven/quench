---
title: Configuration
parent: quench-client
grand_parent: Crates
nav_order: 5
---

The crate reads **no environment variables of its own** and has no config file. Everything is set in code. The TLS layer (`quench-tls`) reads `SSL_CERT_FILE` (see below).

## Options

| Option | Where | Default | Notes |
|---|---|---|---|
| base URL | `HttpClient::new/builder`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient::builder` | required | concatenated with each path |
| `tls_verify` | the four typed builders | `true` | `false` = `danger_accept_invalid_certs(true)` |
| request timeout | `transport::ClientBuilder::timeout`, `RequestBuilder::timeout` | none | typed clients set none |
| redirects | `ClientBuilder::redirect` | `Redirect::Limited(10)` | typed clients use the default |
| `max_response_bytes` | `transport::ClientBuilder` | unlimited | not on the blocking client or typed clients |
| `user_agent` | `ClientBuilder` | none | no default `User-Agent` is sent |
| extra root CA | `add_root_certificate_pem` | none | PEM bytes, repeatable |
| client certificate | `client_identity_pem(cert, key)` | none | PEM bytes |
| `token_url`, `client_id`, `client_secret` | `ClientCredentialsClientBuilder` | `""` | |
| `username`, `password` | `BasicAuthClientBuilder` | `""` | |

## Constants (private)

| Constant | Value | Meaning |
|---|---|---|
| `MAX_REDIRECTS` | `10` | default hop limit |
| `MAX_IDLE_PER_HOST` | `8` | idle connections kept per `(scheme, host, port)` |
| `IDLE_TTL` | `60 s` | idle connection lifetime |
| `REDIRECT_DRAIN_LIMIT` | `64 KiB` | a redirect body is read up to this size so the connection can be reused |
| token safety margin | `30 s` | `ClientCredentialsClient` refreshes this long before `expires_in` |

## TLS

With the `tls` feature the connector is built from `quench_tls::ClientOptions { accept_invalid_certs, extra_roots_pem, use_system_roots: true, alpn_protocols: ["http/1.1"] }`. Trust roots: Mozilla (`webpki-roots`), the OS CA bundle (`$SSL_CERT_FILE` if it points to a file, else the first existing of `/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt`, `/etc/ssl/ca-bundle.pem`, `/etc/ssl/cert.pem`, `/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem`), then `add_root_certificate_pem` entries. Only HTTP/1.1 is offered through ALPN. See [quench-tls](../quench-tls/configuration.md).

## URL rules

* Schemes: `http` and `https` only; other schemes give ``Error::Request("unsupported URL scheme `x`")``, a relative URL ``Error::Request("`u` is not an absolute URL")``.
* Default ports: 80 and 443. IPv6 literals in brackets work.
* A URL fragment (`#...`) is removed before sending.
* Connections use `TCP_NODELAY`.
