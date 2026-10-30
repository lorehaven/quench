---
title: quench-k8s
parent: Crates
nav_order: 27
has_children: true
permalink: /crates/quench-k8s/
---

`quench-k8s` (library `quench_k8s`) is a generic Kubernetes API client. It replaces `kube` and `k8s-openapi`: objects are plain `quench_serde::Value`s, so any kind, custom resources included, goes through the same calls to read, create, replace, patch, delete, watch, and read a pod's log. It authenticates from a kubeconfig (bearer token, token file, client certificate, basic) or from the pod's service account, and its HTTP and TLS come from `quench-client`.

* Version: `0.1.5` (`quench-k8s/Cargo.toml`); library name `quench_k8s`.
* No `[features]` table of its own. It enables the `tls` feature of `quench-client` and the `yaml` feature of `quench-serde`.

## Install

```toml
[dependencies]
quench-k8s = "0.1"
```

## Dependencies

| Dependency | Why |
|---|---|
| `quench-client` (feature `tls`) | HTTP transport (`quench_client::transport`): requests, TLS, client identity, streaming responses. |
| `quench-core` | `bytes::Bytes`, `stream` (`BoxStream`, `StreamExt`, `unfold`), `future::join_all`, `time::Time`, `base64`, `env`. |
| `quench-serde` (feature `yaml`) | `Value`, `Map`, JSON and YAML (kubeconfig), `urlencoded::percent_encode`. |

Dev-dependencies: `quench-test`, `quench-http` (mock API server), `tokio`.

## Minimal example

```rust
use quench_k8s::{ApiResource, Client, ListParams, Object};

async fn demo() -> Result<(), quench_k8s::Error> {
    let client = Client::infer()?;
    let pods = client.namespaced(&ApiResource::pod(), "default");
    for pod in pods.list(&ListParams::default().labels("app=web")).await?.items {
        println!("{}", Object(&pod).name().unwrap_or("?"));
    }
    Ok(())
}
```

## Where to go next

* [Usage](usage.md): list, get, create, patch, delete, logs, watch, discovery, custom resources.
* [API reference](api.md): every public item.
* [Configuration](configuration.md): kubeconfig and in-cluster resolution.
* [Errors](errors.md): `Error`, `Status`.
* [Internals](internals.md): paths, query strings, patch content types, watch decoding.
* [Testing](testing.md): the mock API server tests.

Not included: exec and attach, port forwarding, reflectors/informers, credential plugins (`exec`, `auth-provider`), typed models of the built-in kinds.
