---
title: Testing
parent: quench-desktop-atspi
grand_parent: Crates
nav_order: 5
---

```bash
cargo test -p quench-desktop-atspi
```

Tests are in `tests/unit.rs` and use fake buses over temporary Unix sockets (a fake session bus that answers `GetAddress`, and a fake accessibility bus); they do not need a running accessibility service. The suites check: every D-Bus type survives the wire; hostile values are errors, not panics; message fields and body; EXTERNAL authentication between peers; the application/window/page tree; role, name, state and attributes; extents and hit testing; queued actions; the text interface; errors for missing things; focus signals; `Hello` and error replies; escaped and abstract addresses; the full registration through two fake buses; every role has a number and name; sliders answer `Value`; editing through `EditableText`; name, text, children, state and caret signals.

For code built on the crate, construct a `Shared`, hand `Service::serve` one end of a `UnixStream` pair (`Connection::new`) and call it as a screen reader would, with `Message::call` and `Connection::call`.
