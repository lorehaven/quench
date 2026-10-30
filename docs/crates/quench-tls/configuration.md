---
title: Configuration
parent: quench-tls
grand_parent: Crates
nav_order: 3
---

## Environment variable

| Name | Read by | Meaning |
|---|---|---|
| `SSL_CERT_FILE` | `root_store` (when `ClientOptions::use_system_roots` is true), through `quench_core::env::var` | path to a PEM CA bundle. Used only when it points to an existing file; otherwise the fixed list below is searched. |

## System bundle search order

When `use_system_roots` is true and `SSL_CERT_FILE` is unset or not a file, the first existing path wins and only that file is loaded:

1. `/etc/ssl/certs/ca-certificates.crt`
2. `/etc/pki/tls/certs/ca-bundle.crt`
3. `/etc/ssl/ca-bundle.pem`
4. `/etc/ssl/cert.pem`
5. `/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem`

Not supported: Windows or macOS certificate stores (the crate reads PEM files only), `SSL_CERT_DIR`.

## Fixed choices (not configurable)

| Choice | Value |
|---|---|
| Crypto provider | `ring` |
| Protocol versions | rustls safe defaults: TLS 1.3 and 1.2 |
| Cipher suites | the `ring` provider's defaults |
| Server client-auth | none (`server_config`) |
| Mozilla roots | always included for clients (not when `accept_invalid_certs`) |

## Options struct

See [`ClientOptions`](api.md#clientoptions). `ClientOptions::default()` is: verification on, no extra roots, **system roots off**, no ALPN. Callers in this workspace: `quench-client` uses `use_system_roots: true` and ALPN `http/1.1`.
