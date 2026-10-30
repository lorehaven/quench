---
title: Testing
parent: quench-tls
grand_parent: Crates
nav_order: 6
---

Tests are in `tests/tls.rs` (custom harness: `harness = false`, `#[quench_test::test]`) plus one unit test in `src/lib.rs` (`installing_the_default_provider_is_idempotent`).

| Area | Tests |
|---|---|
| handshake and data | `a_full_handshake_carries_data_both_ways`, `large_transfers_survive_the_session_buffer_limits`, `a_tiny_pipe_exercises_backpressure_in_both_directions` |
| verification | `an_untrusted_certificate_fails_the_client_handshake`, `a_name_mismatch_fails_the_client_handshake`, `accepting_any_certificate_skips_verification`, `tls_1_2_only_servers_work_with_and_without_verification` |
| error paths | `plain_text_sent_to_a_tls_server_is_rejected`, `a_peer_that_vanishes_mid_handshake_is_an_eof_error`, `truncation_without_close_notify_is_reported`, `reading_after_close_notify_keeps_returning_eof`, read/write/flush/zero-write errors and `handshakes_fail_cleanly_when_the_transport_breaks` (a `Flaky` transport) |
| helpers | `server_names_accept_dns_and_ip_literals`, `trust_stores_include_webpki_roots_and_extras`, `system_roots_honour_ssl_cert_file`, `missing_or_broken_server_credentials_load_as_none`, `pem_helpers_parse_and_reject`, `acceptors_and_connectors_are_cheap_to_clone` |
| mutual TLS | `a_client_certificate_is_presented_when_one_is_configured`, `without_a_client_certificate_a_mutual_server_refuses`, `a_bad_client_identity_is_described` |

Fixtures in `tests/fixtures/`: `ca.pem`, `server.pem` / `server.key` (a `localhost` / `127.0.0.1` leaf, ECDSA P-256, valid for 100 years), and for mutual TLS `client-ca.pem`, `client.pem`, `client.key` (`CN=quench-test-client`). The CA keys were discarded; the keys are test-only. `tests/fixtures/README.md` says they were made for the `quench-proto` TLS tests.

```bash
cargo test -p quench-tls
```

## Testing code that uses the crate

Use the same approach as the tests: bind `127.0.0.1:0`, accept with a `TlsAcceptor` built from fixture files, and connect with a `TlsConnector` whose `ClientOptions.extra_roots_pem` contains the fixture CA (or `accept_invalid_certs: true` when verification is not under test). For transport failures, wrap an in-memory `tokio::io::duplex` in a type that errors on demand, as `Flaky` does.
