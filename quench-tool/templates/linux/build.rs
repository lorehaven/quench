//! Embeds the app's bundle (`../dist/www`, from `quench build --bundle`) into the binary: it writes
//! `bundle.rs` into OUT_DIR, with one `include_bytes!` per file. Without a bundle the window is empty.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, prefix: &str, into: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if entry.path().is_dir() {
            walk(&entry.path(), &path, into);
        } else {
            into.push((path, entry.path()));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let www = manifest.parent().unwrap_or(&manifest).join("dist/www");
    println!("cargo:rerun-if-changed={}", www.display());
    let mut files = Vec::new();
    walk(&www, "", &mut files);
    if files.is_empty() {
        println!("cargo:warning=no bundle in dist/www: run `quench build --bundle` first");
    }
    let mut code = String::from("pub fn bundle() -> quench_desktop::Bundle {\n    quench_desktop::Bundle::new()\n");
    for (name, path) in &files {
        code.push_str(&format!("        .with({name:?}, &include_bytes!({:?})[..])\n", path.display().to_string()));
    }
    code.push_str("}\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    std::fs::write(out.join("bundle.rs"), code).expect("write bundle.rs");
}
