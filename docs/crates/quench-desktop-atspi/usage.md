---
title: Usage
parent: quench-desktop-atspi
grand_parent: Crates
nav_order: 1
---

*The examples need a session with the accessibility bus (`org.a11y.Bus`) running. The book's authors did not start one.*

## 1. Start the service

```rust
let service = match quench_desktop_atspi::Service::start("My app") {
    Ok(s) => Some(s),
    Err(e) => { eprintln!("no accessibility: {e}"); None }   // the app still works
};
```

`start` asks the session bus for the accessibility bus address (`org.a11y.Bus.GetAddress`), connects and authenticates there (EXTERNAL), says `Hello`, registers with `org.a11y.atspi.Socket.Embed` and starts a thread named `atspi` that answers calls. All of it uses a 5 s timeout; answering has none.

The `app_name` is the application's and the window's accessible name.

## 2. Publish the page

Call `publish(tree)` whenever the renderer says the page changed (the host `quench-desktop-native` does it after each drawn frame). `publish` replaces the snapshot the answering thread reads, and sends signals for what changed since the last tree: name, text, caret, checked/expanded/selected, children added/removed, and focus changes.

## 3. Carry out what a screen reader asks

```rust
if service.take_asked() { /* a screen reader is active: poll quickly */ }
for (id, action) in service.take_actions() { renderer.perform(id, action); }
for (id, text) in service.take_edits() { renderer.set_field_value(id, &text); }
```

Actions (click, focus, scroll into view) and edits (a new whole text for a field) are queued by the answering thread; they are carried out on the program's thread. `take_asked` is true if anything has called an object since the last call, which tells a host that an assistive technology is attached.

## 4. Serve over a peer connection (tests, or a custom bus)

```rust
let shared = Shared::new("app");
shared.set_name(":1.7");
let service = Service::serve(connection, shared);
```

`Connection::new(stream, name)` takes an already authenticated `UnixStream`; `authenticate_client`/`authenticate_server` perform the EXTERNAL handshake for a direct connection between two peers (as the tests do).

## 5. Pitfalls

* `Service::start` fails when the accessibility service is not running; handle the error and continue without accessibility.
* The tree is a snapshot: ids are the renderer's `AccessNode::id`; the document node has id 0 and its parent is the window. A node that no longer exists answers `org.freedesktop.DBus.Error.UnknownObject`.
* Only little-endian messages are accepted (`only little-endian messages are supported`).
