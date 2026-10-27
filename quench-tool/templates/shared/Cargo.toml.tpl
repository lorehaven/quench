[package]
name = "{{ name }}-shared"
version.workspace = true
edition.workspace = true

[lib]
name = "{{ snake }}_shared"
path = "src/lib.rs"

[dependencies]
quench-fullstack = { workspace = true }
quench-serde = { workspace = true }
