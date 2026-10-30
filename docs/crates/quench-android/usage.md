---
title: Usage
parent: quench-android
grand_parent: Crates
nav_order: 1
---

Worked examples, from the common case to testing without a device.

## 1. The program's `main`

Goal: answer operations from the Kotlin host.

```rust
fn register_ops() {
    quench_android::register("greet", |input| {
        let name = std::str::from_utf8(input).map_err(|e| e.to_string())?;
        Ok(format!("hello, {name}").into_bytes())
    });
}

fn main() -> std::process::ExitCode {
    quench_android::run(register_ops)
}
```

`run` calls `register_ops`, then serves standard input and output until the host closes them. The built-in operations (`core.version`, `core.echo`, `tz.next`) are registered on first use of `call`, so they are always available.

Pitfall: anything written to standard output corrupts the framing. Use `eprintln!`; the host includes up to 4096 bytes of standard error in the message when the program dies without an answer.

## 2. Calling it from Kotlin

The generated `dev.quench.Native` object runs the program (`libquench_native.so` in the app's native library directory):

```kotlin
val bytes = Native.invoke(context, "greet", "Ada".toByteArray(), timeoutSeconds = 60)
```

It throws `NativeException` when the operation fails (message = the handler's message), the program is not in the build, it cannot be started, or it ends without an answer. Each call starts a new process; the host does not keep one running.

## 3. Operations in a background job

A job named `<task>` of the `jobs` plugin runs the native operation `job.<task>` (see [quench-tool plugins](../quench-tool/plugins.md)). Register it like any other:

```rust
quench_android::register("job.sync", |_input| {
    // ... work ...
    Ok(Vec::new())
});
```

## 4. Next local time (`tz.next`)

```json
{"zone": "Europe/Warsaw", "time": "08:00", "after_ms": 1793000000000}
```

answers `{"at_ms": <milliseconds since the epoch>}`. From Rust, call [`ops::next_local`](api.md#opsnext_local) directly:

```rust
let at_ms = quench_android::ops::next_local("Europe/Warsaw", 8, 0, 1_793_000_000_000)?;
```

## 5. Testing handlers without a device

`serve` takes any `Read` and `Write`:

```rust
let mut input: Vec<u8> = Vec::new();
for part in [b"core.echo".as_slice(), b"hi".as_slice()] {
    input.extend((part.len() as u32).to_be_bytes());
    input.extend(part);
}
let mut output = Vec::new();
quench_android::serve(&mut input.as_slice(), &mut output).unwrap();
// output = 00 00 00 03, 0x00, b"hi"
assert_eq!(output, [0, 0, 0, 3, 0, b'h', b'i']);
```

Or call `quench_android::call("core.echo", b"hi")` and `encode` the result.
