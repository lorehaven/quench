---
title: Testing
parent: quench-ui-web-macros
grand_parent: Crates
nav_order: 2
---

## Unit tests of the expansion

`src/expand.rs` has a `#[cfg(test)] mod tests` that calls `expand::plugin(attr, item)` on token streams parsed from strings and checks the text of the output:

* `a_trait_becomes_a_struct_of_calls_by_name`: `pub struct Echo ;`, `const NAME : & 'static str = "echo"`, the signature with `:: quench_ui_web :: BridgeError`, `bridge :: call ("echo" , "say"`, and `"text" : text , "times" : times`.
* `a_method_without_arguments_sends_an_empty_object`: `json ! ({ })`.
* `docs_and_visibility_are_kept`.
* `mistakes_are_reported_where_they_are`: the table of compile errors in the [reference](api.md), plus the bad attributes `""`, `echo`, `"Echo"`, `"1echo"`, `"my-echo"` and `struct S;`.

The crate uses the standard Rust test harness here (no `harness = false`).

## Tests of the expansion in use

`quench-ui-web/tests/unit/bridge_tests.rs` defines an `Echo` plugin with the attribute and drives it through the mock host: without a host the call fails with `unavailable`; a call is an envelope with the arguments by name; answers and errors are read back into the declared types. `quench-ui-web/tests/unit/plugin_tests.rs` checks every client in `quench_ui_web::plugins`, which are all declared with `#[plugin]`. See [Testing in quench-ui-web](../quench-ui-web/testing.md).

## Testing your own plugin client

```rust
#[quench_ui_web::plugin("echo")]
pub trait Echo { async fn say(text: String) -> Result<String, quench_ui_web::BridgeError>; }

// in a test:
quench_ui_web::mock::reset();
spawn(async { let _ = Echo::say("hi".into()).await; });
quench_ui_web::mock::run_ticks();
let sent = quench_ui_web::mock::bridge_sent();           // [{"id":0,"plugin":"echo","method":"say","args":{"text":"hi"}}]
quench_ui_web::mock::bridge_deliver(r#"{"id":0,"ok":"hi"}"#);
```

Compile-fail tests (trybuild) do not exist for this crate; the error cases are covered only by the string-level unit test above.
