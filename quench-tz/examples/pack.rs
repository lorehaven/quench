//! Rebuilds `data/zones.qtz` from a system zoneinfo directory:
//! `cargo run -p quench-tz --example pack -- /usr/share/zoneinfo quench-tz/data/zones.qtz`

use std::fs;
use std::path::Path;

use quench_tz::{ZoneData, pack};

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, ZoneData)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable directory")
        .flatten()
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = path
            .strip_prefix(root)
            .expect("under root")
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            // `posix/` and `right/` repeat every zone (the latter with leap seconds).
            if name != "posix" && name != "right" {
                walk(root, &path, out);
            }
            continue;
        }
        if matches!(name.as_str(), "localtime" | "posixrules") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        if !bytes.starts_with(b"TZif") {
            continue;
        }
        match ZoneData::from_tzif(&bytes) {
            Ok(data) => out.push((name, data)),
            Err(error) => eprintln!("skipping {name}: {error}"),
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .unwrap_or_else(|| "/usr/share/zoneinfo".to_string());
    let target = args.next().unwrap_or_else(|| "data/zones.qtz".to_string());
    let root = Path::new(&root);
    let version = fs::read_to_string(root.join("tzdata.zi"))
        .ok()
        .and_then(|text| text.lines().next().map(str::to_string))
        .and_then(|line| line.strip_prefix("# version ").map(str::to_string))
        .expect("tzdata.zi names the version");
    let mut zones = Vec::new();
    walk(root, root, &mut zones);
    let bytes = pack(&version, &zones);
    fs::write(&target, &bytes).expect("writable target");
    println!(
        "{} zones, version {version}, {} bytes -> {target}",
        zones.len(),
        bytes.len()
    );
}
