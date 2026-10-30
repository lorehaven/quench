---
title: Testing
parent: quench-fullstack-macros
grand_parent: Crates
nav_order: 4
---

`src/expand.rs` contains a `#[cfg(test)] mod tests` that runs the expansion as plain `proc_macro2` functions and compares token text. Six tests:

1. the server gets the function, a route and a registration (no `with_user`/`FromRequest` for a public function);
2. the browser gets a stub with the same signature and the two derives;
3. `auth` looks the user up before arguments are read (`from_http` for required, `. ok ()` for optional);
4. the path can be chosen;
5. each mistake is reported (not async, generic, `self`, pattern parameter, non-`Result` return, bad path, space in path, unknown option, bad `auth`, non-function item);
6. a function with no arguments gets an empty argument struct.

```bash
cargo test -p quench-fullstack-macros
```

Behavioural tests with real functions are in `quench-fullstack` (`tests/unit/function_tests.rs`, `tests/unit/functions.rs`).
