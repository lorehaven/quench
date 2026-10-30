---
title: Quench
nav_order: 1
permalink: /
---

Quench is a Rust framework of shared crates with a strict dependency policy: nothing beyond `std`, `libc`, `getrandom`, `tokio`, `rustls` with `ring` and `webpki-roots`, and the compile-time `proc-macro2`/`syn`/`quote`. It covers the whole stack of an application: configuration, logging, HTTP server and client, database access, authentication, a reactive UI toolkit that runs in the browser, on the desktop and on Android, and the tools that package the result.

The book has three parts:

* [Guides](guides/index.md): how the pieces fit together and how to build with them.
* [Crates](crates/index.md): one section per crate, written like a crates.io page - overview, usage, and a reference of the whole public API.
* The `quench` command-line tool is documented with its crate, `quench-tool`.

Forge, the set of services and tools built on Quench, has its own book.
