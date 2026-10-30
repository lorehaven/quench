---
title: Internals
parent: quench-android
grand_parent: Crates
nav_order: 4
---

## Wire format

All integers are big-endian. A **frame** is a 4-byte unsigned length and that many bytes.

```text
request  = frame(operation name, UTF-8) frame(input bytes)
answer   = frame( status byte | payload )
status 0 = success, payload is the result
status 1 = failure, payload is the message (UTF-8)
```

The program keeps answering requests until its input ends. The Kotlin host (`Native.invoke`) sends exactly one request per process and then closes the pipe, so in practice a process answers once. If the child dies before an answer, the host reports "the native program ended without an answer" followed by up to 4096 bytes of its standard error (or `exit <code>`).

## Limits

* A frame body is at most `MAX_FRAME = 64 << 20` bytes (64 MiB) on the program side; larger frames end the session with `InvalidData`.
* The Kotlin host's default time limit is 60 seconds per call (`timeoutSeconds`); on expiry the process is destroyed forcibly.

## Isolation

The native code is not loaded into the app: a crash or hang ends only that process. Handlers run on the thread that serves; a panic is caught and becomes a failure answer, serving goes on. The program is installed as `libquench_native.so` under `jniLibs/<abi>/` (built by `quench android build-native`) because Android only extracts files of that name shape, and run from `applicationInfo.nativeLibraryDir`.

## Registry

A process-global `Mutex<HashMap<&'static str, Handler>>` in a `OnceLock`. The built-ins are registered once, by a `Once` that `call` triggers before every lookup. Consequence: an app that registers a built-in name (for example `core.echo`) before the first `call` is overwritten when the built-ins register; one registered after the first `call` replaces the built-in. `run` calls `init` before serving, so names registered in `init` that equal a built-in lose. *Unverified by a test:* this follows from reading `call` and `builtins`.

## `tz.next`

Uses `quench_tz::Zone::resolve` on the local reading (`Single`, `Ambiguous { earlier, later }`, `Gap`). A daily reminder therefore stays at the same wall-clock time through both daylight-saving changes, which `AlarmManager` cannot do (it counts elapsed time).
