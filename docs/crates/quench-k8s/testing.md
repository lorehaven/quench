---
title: Testing
parent: quench-k8s
grand_parent: Crates
nav_order: 6
---

Two test binaries with `harness = false` (custom `quench_test` harness): `tests/api.rs` and `tests/config.rs`.

```bash
cargo test -p quench-k8s
```

## `tests/api.rs` (12 tests)

A mock API server built on `quench-http` records requests and answers scripted responses. Tests cover: `get`/`get_opt` paths and 404; paths for groups, cluster scope and odd names; the namespaced-needs-namespace error; list pagination and selectors; create/replace/patch/delete bodies, query strings and content types; logs as text and streaming; watch event decoding; bad watch lines as `Decode` errors; authentication headers on every request and re-read token files; non-`Status` failures; discovery of preferred versions with subresources skipped; discovery failing when the group list fails.

## `tests/config.rs` (9 tests)

Kubeconfig with token and embedded CA; client certificates, basic auth and token files; certificate files relative to the kubeconfig; every reported kubeconfig failure; service-account directory; environment-driven inference (`KUBECONFIG`, `HOME`, service host, in one test because the environment is process-wide); `guess_plural`; `Object` accessors; `lines` with errors and a missing final line break.

The previous flat page also stated that the client was checked read-only against a real k3s cluster; that is not reproducible from the repository and is *Unverified* here.

## Testing code that uses the crate

Point a `Config::new("http://127.0.0.1:<port>")` (no TLS needed for `http`) at a local `quench-http` server that answers the paths you use; `Auth::Bearer("token".into())` is enough for header assertions.
