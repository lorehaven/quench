# quench-desktop-atspi
AT-SPI accessibility for the native renderer on Linux, with the D-Bus wire protocol written by hand.

## What it is
`quench-desktop-atspi` makes a native-renderer window readable by screen readers such as Orca. Screen readers talk to applications over the accessibility bus; this crate speaks the D-Bus wire protocol itself (no `libdbus`, no `zbus`), connects to that bus, registers the application with the AT-SPI registry and answers the `Accessible`, `Component`, `Action`, `Text`, `EditableText` and `Value` interfaces.

It publishes the `AccessNode` tree produced by `quench-ui-native`, and queues the actions and edits a screen reader requests for the program to carry out. The host `quench-desktop-native` uses it after each drawn frame.

## Use cases
- Make a native Quench window accessible to Orca.
- Publish the page's accessibility tree after each frame.
- Carry out click, focus and scroll actions requested by assistive technology.
- Apply text edits to fields from a screen reader.
- Detect that a screen reader is attached and poll faster.

## Example
```rust
let service = match quench_desktop_atspi::Service::start("My app") {
    Ok(s) => Some(s),
    Err(e) => { eprintln!("no accessibility: {e}"); None }   // the app still works
};
```

```rust
// service.publish(tree) after each drawn frame, then:
if service.take_asked() { /* a screen reader is active: poll quickly */ }
for (id, action) in service.take_actions() { renderer.perform(id, action); }
for (id, text) in service.take_edits() { renderer.set_field_value(id, &text); }
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-desktop-atspi/
