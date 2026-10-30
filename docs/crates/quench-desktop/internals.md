---
title: Internals
parent: quench-desktop
grand_parent: Crates
nav_order: 5
---

## Origin and security

* The page is served from the custom scheme `quench` (host `app`); the scheme is registered secure and CORS-enabled in webkit's security manager, so the page is a secure context that can `fetch('quench://app/...')`. The Android host serves `https://app.quench.local/` instead; the bundle is the same.
* The bridge answers only pages whose origin is `quench://app` or the dev server's (`is_app_origin`). Messages from other origins are refused and logged; answers are not delivered to other pages (the delivery script checks the view's current URI).
* Links out of the app: only `https`, `http` and `mailto` go to `xdg-open`; the view never navigates away.
* The user agent ends with `QuenchHost/1`, which is how the page knows a host is there (the Android host does the same).
* The crate registers no scheme other than `quench` and sets no webkit settings besides the user agent and developer extras.

## The bridge on the desktop

Page to host: `window.webkit.messageHandlers.quench.postMessage(<string>)`. Host to page: the host evaluates `window.__quenchBridge(<JSON string literal>)` in the page. The page-side wrapper is documented with `quench-ui-web` in the Quench book. Answers may be produced on any thread; `Bridge` sends them through a `Deliver` that posts the evaluation to the GTK main loop with `Remote::post`.

## Threading

GTK is single-threaded: `Gtk`, `Window`, `WebView` are `!Send`/`!Sync` (they carry `PhantomData<*const ()>`), and `Gtk::init` hands out at most one value per thread. `Remote::post` is the only way for other threads (plugins finishing work) to reach the view. Closures given to GTK are boxed, freed by a destroy notify, and run under `catch_unwind` so a panic never unwinds into C.

## Control socket

With `QUENCH_DESKTOP_CONTROL=<path>` a **debug build** listens on that Unix socket (mode `0600`). Each line a client sends is a script to run in the page; the reply is one line, `{"ok":<value as JSON>}` or `{"error":"..."}`. Scripts that do not finish in 20 seconds answer `{"error":"the script did not finish in 20 s"}`; a closed window answers `{"error":"the window is closed"}`. A release build ignores the variable. It is how `scripts/linux-e2e.fish` drives the gallery (outside this book).

## Dev mode

`App::dev(url)` loads the dev server in the window and allows it as an app origin; the page reloads itself when the server restarts (the server's `Ui::dev_reload`). The scheme server is still registered.

## Files

`ffi.rs` declares the C functions it uses by hand (no `-sys` crates); the crate links `gtk-3` and `webkit2gtk-4.1` when the `window` feature is on. Everything else is safe Rust.
