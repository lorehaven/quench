---
title: Testing
parent: quench-fullstack
grand_parent: Crates
nav_order: 9
---

One test binary, `tests/unit.rs` (`[[test]] name = "unit"`, `harness = false`, `quench_test::main!()`), with 76 `#[quench_test::test]` functions in the modules `assets_tests`, `dev_tests`, `document_tests`, `error_tests`, `function_tests`, `guard_tests`, `routing_tests`, `session_tests`, `stub_tests`, plus helpers `support` and `functions` (real `#[server]` functions used by the tests). The library also contains `#[cfg(test)] quench_test::main!();`.

```bash
cargo test -p quench-fullstack
```

Covered: hashing (including a published SHA-256 vector), asset paths and replacement, caching headers, 304s, gzip and its round trip, the document, nonce freshness, the policy, escaping with hostile input, the endpoint end to end; every data shape of a server function through the real client against the real server in one process (the client runs on the in-memory host of `quench-ui-web` and its fetches are handed to the endpoint); arguments the function cannot read, each error kind with its status, the generic internal error, the protocol checks (`guard`), and sessions (cookie and bearer, expired, tampered, foreign-signed and wrong-audience tokens, `auth` and `auth = "optional"`, one refresh for concurrent calls, a dead refresh token ending the session, sign-in and sign-out, the stub token service).

## Testing your own server functions

* Register your functions by linking the crate; build `UiApp::new(Ui::new(wasm))`.
* Use `testing::StubGatehouse::new(&[("alice", "pw")])`: serve it as an endpoint, provide `stub.jwt()` in the container, then sign in through `client::sign_in` or `Bearer::login`.
* `stub.set_access_ttl(-120)` gives already-expired access tokens to exercise the refresh path; `stub.revoke_all()` kills all refresh tokens; `login_count()`/`refresh_count()` assert how often the flows ran.
* `client::reset()` clears the thread-local client state between tests.

## Browser tests

`scripts/ui-web-test.fish` exists in the repository (headless Chromium run; its check count and behaviour were not verified for this book), and `examples/ui_web_app` and `examples/ui_web_server` are the example pair (the server example depends on `quench-fullstack`). *Unverified:* the previous page's claims of 75 browser checks and 96% line coverage.
