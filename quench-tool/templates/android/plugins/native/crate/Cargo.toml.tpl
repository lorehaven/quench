[package]
name = "{{ name }}-native"
version.workspace = true
edition.workspace = true

# The program the Android host starts for native work (`quench android build-native` copies it into
# the app as libquench_native.so). It speaks length-prefixed frames on stdin and stdout.
[[bin]]
name = "quench_native"
path = "src/main.rs"

[dependencies]
quench-android = { workspace = true }
quench-core = { workspace = true }
quench-serde = { workspace = true }
