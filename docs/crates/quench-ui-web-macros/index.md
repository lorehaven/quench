---
title: quench-ui-web-macros
parent: Crates
nav_order: 59
has_children: true
---

`quench-ui-web-macros` (library `quench_ui_web_macros`, a `proc-macro` crate) provides the `#[plugin]` attribute: a typed client for a native plugin of the host app, written as a trait of `async fn` declarations. Depend on [`quench-ui-web`](../quench-ui-web/index.md), which re-exports it as `quench_ui_web::plugin`; the expansion refers to `::quench_ui_web`, so it only compiles where that crate is a dependency (or in `quench-ui-web` itself).

Version: **0.1.0**. Edition: 2024 (workspace). No `rust-version`. Features: none.

## Install

```toml
[dependencies]
quench-ui-web = { version = "0.1.7", registry = "ennor" }   # re-exports #[plugin]
```

Depending on `quench-ui-web-macros = { version = "0.1.0", registry = "ennor" }` directly works only together with `quench-ui-web`.

## Dependencies

| Crate | Why |
|---|---|
| `proc-macro2`, `quote`, `syn` | parsing the trait and generating the tokens (workspace versions). |

## Where to go next

* [Reference: `#[plugin]`](api.md): arguments, expansion, errors.
* [Testing](testing.md).
* The runtime half: [`bridge`](../quench-ui-web/api-bridge.md) and the [typed clients](../quench-ui-web/api-plugins.md).

## Minimal example

From `quench-ui-web/tests/unit/bridge_tests.rs`:

```rust
use quench_serde::{Deserialize, Serialize};
use quench_ui_web::BridgeError;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Device { sdk: u32, model: String }

#[quench_ui_web::plugin("echo")]
pub trait Echo {
    /// Returns the text.
    async fn say(text: String) -> Result<String, BridgeError>;
    async fn device() -> Result<Device, BridgeError>;
    async fn add(a: i64, b: i64) -> Result<i64, BridgeError>;
}

// let reply = Echo::say("hi".into()).await?;   // sends {"id":0,"plugin":"echo","method":"say","args":{"text":"hi"}}
```
