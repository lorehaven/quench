[package]
name = "{{ name }}-surface"
version.workspace = true
edition.workspace = true

# The program the Android host starts to draw the screen (`quench android build-native` copies it into the
# app as libquench_surface.so). It reads touches and keys from stdin and writes frames to stdout.
[[bin]]
name = "quench_surface"
path = "src/main.rs"

[dependencies]
quench-android-surface = { workspace = true }
quench-style = { workspace = true }
quench-ui = { workspace = true }
quench-ui-native = { workspace = true }
