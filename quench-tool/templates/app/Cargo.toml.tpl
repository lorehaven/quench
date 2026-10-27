[package]
name = "{{ name }}-app"
version.workspace = true
edition.workspace = true

[lib]
name = "{{ snake }}_app"
path = "src/lib.rs"
# `cdylib` is the module the browser loads; `rlib` is what the server renders pages from.
crate-type = ["cdylib", "rlib"]

[dependencies]
{{ name }}-core = { workspace = true }
{% if fullstack %}{{ name }}-shared = { workspace = true }
{% endif %}quench-ui = { workspace = true }
quench-ui-web = { workspace = true }
