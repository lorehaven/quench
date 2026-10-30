---
title: quench-desktop-atspi
parent: Crates
nav_order: 18
has_children: true
permalink: /crates/quench-desktop-atspi/
---

`quench-desktop-atspi` makes a native-renderer window readable by screen readers such as Orca on Linux. Screen readers talk to applications over a second D-Bus, the accessibility bus. This crate speaks the D-Bus wire protocol itself (no `libdbus`, no `zbus`), connects to that bus, registers the application with the AT-SPI registry and answers the `Accessible`, `Component`, `Action`, `Text`, `EditableText` and `Value` interfaces from the `AccessNode` tree that `quench-ui-native` produces.

Version: **0.1.5**. Edition 2024 (workspace). Linux only (`#![cfg(target_os = "linux")]`). No `unsafe`.

## Install

```toml
[dependencies]
quench-desktop-atspi = { version = "0.1.5", registry = "ennor" }
```

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `quench-ui-native` | `AccessNode`, `AccessAction`, `Role`: the tree the service publishes and the actions it queues. |

Dev-dependency: `quench-test`. No other third-party crates.

## Environment

| Variable | Read by | Meaning |
|---|---|---|
| `DBUS_SESSION_BUS_ADDRESS` | `Service::start` | Session bus address tried first. |
| (none) | `Service::start` | Fallback `unix:path=/run/user/<uid>/bus`, with the uid read from the owner of `/proc/self`. |

## Where to go next

* [Usage](usage.md): start, publish, carry out actions and edits.
* [API reference](api.md): every public item by module.
* [AT-SPI coverage](atspi.md): objects, interfaces, methods, properties, roles, states, signals.
* [Internals](internals.md): D-Bus wire encoding, authentication, threading.
* [Testing](testing.md).

The host that wires this into a window is [quench-desktop-native](../quench-desktop-native/index.md).

## Minimal example

```rust
use quench_desktop_atspi::Service;

let service = Service::start("My app")?;            // Err when there is no accessibility bus
// after each frame:
service.publish(renderer.accessibility_tree());
// on the program's thread:
for (id, action) in service.take_actions() { renderer.perform(id, action); }
for (id, text) in service.take_edits() { renderer.set_field_value(id, &text); }
# Ok::<(), std::io::Error>(())
```
