[package]
name = "{{ name }}-server"
version.workspace = true
edition.workspace = true

[[bin]]
name = "{{ name }}-server"
path = "src/main.rs"

[features]
# The wasm module is built into the binary (`quench build --release` does this).
embed = []

[dependencies]
{{ name }}-app = { workspace = true }
{% if fullstack %}{{ name }}-shared = { workspace = true }
{% endif %}quench-fullstack = { workspace = true }
quench-http = { workspace = true }
tokio = { workspace = true }
