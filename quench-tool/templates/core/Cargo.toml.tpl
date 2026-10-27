[package]
name = "{{ name }}-core"
version.workspace = true
edition.workspace = true
autotests = false

[lib]
name = "{{ snake }}_core"
path = "src/lib.rs"

[[test]]
name = "unit"
path = "tests/unit.rs"
harness = false

[dev-dependencies]
quench-test = { workspace = true }
