---
title: Internals
parent: quench-desktop-atspi
grand_parent: Crates
nav_order: 4
---

## Threads

`Service::serve` starts one thread (`atspi`) that loops on `Connection::recv` and answers through a `Sender`. It reads the snapshot under a mutex. The program's thread calls `publish`, `take_actions`, `take_edits`, `take_asked`. Signals are sent from the program's thread through a second `Sender` of the same connection (a shared writer under a mutex and a shared serial counter), so replies and signals do not interleave inside a message.

## Registration

1. Session bus: `DBUS_SESSION_BUS_ADDRESS`, else `unix:path=/run/user/<uid>/bus`.
2. `org.a11y.Bus.GetAddress` at `/org/a11y/bus` -> the accessibility bus address.
3. Connect, EXTERNAL authentication, `Hello` -> unique name.
4. `org.a11y.atspi.Socket.Embed((so))` on `org.a11y.atspi.Registry` with our root; the reply's `(so)` is the registry's root.

## D-Bus encoding

Little-endian only. Alignment counts from the start of the message: bytes 1, int16 2, bool/int32/uint32/string/object path/array 4, int64/uint64/double/struct/dict entry 8. Strings are a `u32` length, the bytes and a NUL; signatures a `u8` length, bytes and NUL; booleans are 4 bytes. Arrays are a `u32` byte length, padding to the element alignment, then the elements. The header is `l`, type, flags, version 1, body length, serial and an array of `(byte, variant)` fields, padded to 8 bytes.

Limits when reading: header fields 65536 bytes, body 16 MiB, arrays 64 MiB (`1 << 26`), nesting depth 32, authentication lines 4096 bytes.

## Diffing

`publish` compares the old and new trees for nodes present in both (by id, in ascending order): name; the value of a text box (common prefix and suffix are kept, the middle is a delete and an insert); the caret (when present); `checked`, `expanded`, `selected` (when the new value is present); the list of child ids (removed children get `remove` events, new ones get `add`).

## Limits of the model

* One window and one document per application.
* Ids are the renderer's node ids; they are valid until the next publish.
* The answering thread never calls into the renderer (it is not `Send`); everything it needs is in the snapshot.
