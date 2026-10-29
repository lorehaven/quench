use super::support::*;
use quench_tool::linux::{init, package};
use quench_tool::packaging::{arch, rpm, source};
use quench_tool::project::Project;
use std::path::Path;

fn put(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

fn workspace_project(label: &str) -> Project {
    let p = project_named(label, "my-app", "web");
    std::fs::write(
        p.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"core\", \"app\", \"server\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"0.3.2\"\nedition = \"2024\"\n\n[workspace.dependencies]\nquench-ui = { path = \"/q/quench-ui\" }\nquench-core = { path = \"/q/quench-core\" }\n",
    )
    .unwrap();
    init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp"), None).unwrap();
    Project::find(&p.root).unwrap()
}

/// Puts the release binary where cargo would have.
fn built(p: &Project) {
    let dir = p.target_dir().join("release");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(package(p)), b"ELF-binary").unwrap();
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// The size of a header: its 16 bytes of intro, the index and the data.
fn header_len(bytes: &[u8], at: usize) -> usize {
    16 + 16 * u32_at(bytes, at + 8) + u32_at(bytes, at + 12)
}

#[quench_test::test]
fn the_arch_package_holds_the_package_info_the_binary_and_the_menu_entry() {
    let p = workspace_project("pkg-arch");
    built(&p);
    let path = arch(&Fake::default(), &mut Vec::new(), &p).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        "my-app-0.3.2-1-x86_64.pkg.tar.zst"
    );
    let tar = quench_archive::zstd::decompress(&std::fs::read(&path).unwrap(), 1 << 20).unwrap();
    let entries: Vec<_> = quench_archive::tar::Reader::new(&tar)
        .map(Result::unwrap)
        .collect();
    let names: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        names,
        [
            ".PKGINFO",
            "usr/bin/my-app",
            "usr/share/applications/dev.lorehaven.myapp.desktop"
        ],
        "the package info first, as pacman expects"
    );
    let info = String::from_utf8(entries[0].data.to_vec()).unwrap();
    for want in [
        "pkgname = my-app\n",
        "pkgver = 0.3.2-1\n",
        "arch = x86_64\n",
        "size = ",
    ] {
        assert!(info.contains(want), "{want:?} in {info}");
    }
    assert_eq!(
        (entries[1].data, entries[1].mode),
        (&b"ELF-binary"[..], 0o755)
    );
}

#[quench_test::test]
fn the_rpm_has_a_lead_two_headers_and_a_payload_the_signature_vouches_for() {
    let p = workspace_project("pkg-rpm");
    built(&p);
    let path = rpm(&Fake::default(), &mut Vec::new(), &p).unwrap();
    assert_eq!(path.file_name().unwrap(), "my-app-0.3.2-1.x86_64.rpm");
    let bytes = std::fs::read(&path).unwrap();

    assert_eq!(&bytes[..4], [0xed, 0xab, 0xee, 0xdb], "the lead's magic");
    assert_eq!(&bytes[10..21], b"my-app-0.3.");
    // The signature header follows the 96-byte lead and is padded to eight bytes.
    let sig_at = 96;
    assert_eq!(&bytes[sig_at..sig_at + 3], [0x8e, 0xad, 0xe8]);
    let sig_len = header_len(&bytes, sig_at).next_multiple_of(8);
    let header_at = sig_at + sig_len;
    assert_eq!(&bytes[header_at..header_at + 3], [0x8e, 0xad, 0xe8]);
    let header_end = header_at + header_len(&bytes, header_at);
    // Both headers start with their region tag (62 for the signature, 63 for the other): the version 4 shape.
    assert_eq!(u32_at(&bytes, sig_at + 16), 62);
    assert_eq!(u32_at(&bytes, header_at + 16), 63);

    // The signature carries a SHA-256 of the header; check it is that of these bytes.
    let digest = quench_crypto::hash::sha256(&bytes[header_at..header_end]);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let signature = String::from_utf8_lossy(&bytes[sig_at..sig_at + sig_len]);
    assert!(
        signature.contains(&hex),
        "the header digest is in the signature"
    );

    // The payload is gzip of a cpio archive holding the two files.
    let payload = quench_archive::gzip::decompress(&bytes[header_end..], 1 << 20).unwrap();
    assert_eq!(&payload[..6], b"070701");
    let text = String::from_utf8_lossy(&payload);
    assert!(
        text.contains("./usr/bin/my-app")
            && text.contains("./usr/share/applications/dev.lorehaven.myapp.desktop")
            && text.contains("TRAILER!!!"),
        "{text}"
    );
    assert_eq!(
        payload.len() % 4,
        0,
        "cpio entries are padded to four bytes"
    );
}

#[quench_test::test]
fn the_source_tarball_leaves_out_build_output_and_keys() {
    let p = workspace_project("pkg-source");
    for (path, data) in [
        ("app/Cargo.toml", "[package]\n"),
        ("app/src/lib.rs", "// code\n"),
        ("target/debug/junk", "x"),
        ("dist/old.deb", "x"),
        ("android/app/build/out.apk", "x"),
        ("release.keystore", "secret"),
        ("android/local.properties", "sdk.dir=/x"),
    ] {
        put(&p.root.join(path), data.as_bytes());
    }
    let mut out = Vec::new();
    let path = source(&mut out, &p).unwrap();
    assert_eq!(path.file_name().unwrap(), "my-app-0.3.2.tar.gz");
    let tar = quench_archive::gzip::decompress(&std::fs::read(&path).unwrap(), 1 << 20).unwrap();
    let names: Vec<String> = quench_archive::tar::Reader::new(&tar)
        .map(|e| e.unwrap().path)
        .collect();
    for want in [
        "my-app-0.3.2/app/Cargo.toml",
        "my-app-0.3.2/app/src/lib.rs",
        "my-app-0.3.2/Cargo.toml",
        "my-app-0.3.2/.quench.toml",
    ] {
        assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
    }
    for never in ["target", "dist", "keystore", "local.properties", "out.apk"] {
        assert!(
            !names.iter().any(|n| n.contains(never)),
            "{never} in {names:?}"
        );
    }
}

#[quench_test::test]
fn a_project_inside_a_workspace_brings_a_manifest_cut_down_to_its_crates() {
    let repo = tmp("pkg-source-flat");
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace]\nmembers = [\n  \"cli/*\",\n  \"services/demo/app\",\n  \"services/demo/core\",\n  \"tests/*\",\n]\nresolver = \"2\"\n\n[workspace.dependencies]\nx = \"1\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("Cargo.lock"), "# lock\n").unwrap();
    let root = repo.join("services/demo");
    for dir in ["app", "core"] {
        put(
            &root.join(dir).join("Cargo.toml"),
            b"[package]\nname = \"demo\"\nversion = \"0.2.0\"\n",
        );
    }
    put(
        &root.join(".quench.toml"),
        b"name = \"demo\"\ntemplate = \"web\"\n",
    );
    put(
        &root.join("desktop/Cargo.toml"),
        b"[package]\nname = \"demo-desktop\"\nversion = \"0.2.0\"\n",
    );
    let p = Project::find(&root).unwrap();
    assert!(p.flat());
    let path = source(&mut Vec::new(), &p).unwrap();
    assert_eq!(path.file_name().unwrap(), "demo-0.2.0.tar.gz");
    let tar = quench_archive::gzip::decompress(&std::fs::read(&path).unwrap(), 1 << 20).unwrap();
    let entries: Vec<_> = quench_archive::tar::Reader::new(&tar)
        .map(Result::unwrap)
        .collect();
    let find = |name: &str| {
        entries
            .iter()
            .find(|e| e.path == name)
            .unwrap_or_else(|| panic!("{name} in the tarball"))
    };
    let manifest = String::from_utf8(find("demo-0.2.0/Cargo.toml").data.to_vec()).unwrap();
    assert!(
        manifest.contains("\"services/demo/app\"")
            && manifest.contains("\"services/demo/core\"")
            && !manifest.contains("cli/*")
            && !manifest.contains("tests/*")
            && manifest.contains("[workspace.dependencies]"),
        "{manifest}"
    );
    assert_eq!(find("demo-0.2.0/Cargo.lock").data, b"# lock\n");
    find("demo-0.2.0/services/demo/app/Cargo.toml");
}

/// A PNG that is only its signature and the start of its header: enough to name its size.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes
}

fn with_icon(label: &str, icon: &[u8]) -> Project {
    let p = workspace_project(label);
    put(&p.root.join("icon.png"), icon);
    let manifest = p.root.join(".quench.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replacen("[linux]\n", "[linux]\nicon = \"icon.png\"\n", 1),
    )
    .unwrap();
    let p = Project::find(&p.root).unwrap();
    built(&p);
    p
}

#[quench_test::test]
fn an_icon_is_installed_in_the_theme_and_named_by_the_menu_entry() {
    let p = with_icon("pkg-icon", &png(256, 256));
    let icon_path = "usr/share/icons/hicolor/256x256/apps/dev.lorehaven.myapp.png";

    let arch_pkg = arch(&Fake::default(), &mut Vec::new(), &p).unwrap();
    let tar =
        quench_archive::zstd::decompress(&std::fs::read(&arch_pkg).unwrap(), 1 << 20).unwrap();
    let entries: Vec<_> = quench_archive::tar::Reader::new(&tar)
        .map(Result::unwrap)
        .collect();
    assert!(entries.iter().any(|e| e.path == icon_path), "arch");
    let entry = entries
        .iter()
        .find(|e| e.path.ends_with(".desktop"))
        .unwrap();
    assert!(
        String::from_utf8_lossy(entry.data).contains("Icon=dev.lorehaven.myapp\n"),
        "the menu entry names the icon"
    );

    let deb_pkg = quench_tool::linux::deb(&Fake::default(), &mut Vec::new(), &p).unwrap();
    let ar = std::fs::read(&deb_pkg).unwrap();
    let data = quench_archive::gzip::decompress(
        // data.tar.gz is the last member: its bytes start 60 bytes (one header) after its name.
        &ar[ar.windows(12).position(|w| w == b"data.tar.gz ").unwrap() + 60..],
        1 << 20,
    );
    let data = data.unwrap();
    let names: Vec<String> = quench_archive::tar::Reader::new(&data)
        .map(|e| e.unwrap().path)
        .collect();
    assert!(
        names.iter().any(|n| n == &format!("./{icon_path}")),
        "deb: {names:?}"
    );

    let rpm_pkg = rpm(&Fake::default(), &mut Vec::new(), &p).unwrap();
    let bytes = std::fs::read(&rpm_pkg).unwrap();
    let sig_len = header_len(&bytes, 96).next_multiple_of(8);
    let header_at = 96 + sig_len;
    let payload = quench_archive::gzip::decompress(
        &bytes[header_at + header_len(&bytes, header_at)..],
        1 << 20,
    )
    .unwrap();
    assert!(
        String::from_utf8_lossy(&payload).contains(&format!("./{icon_path}")),
        "rpm"
    );
}

#[quench_test::test]
fn an_icon_that_is_not_a_square_png_is_refused() {
    let p = with_icon("pkg-icon-wide", &png(256, 128));
    let e = arch(&Fake::default(), &mut Vec::new(), &p)
        .unwrap_err()
        .to_string();
    assert!(e.contains("must be square"), "{e}");
    let p = with_icon("pkg-icon-text", b"not a png at all, not at all");
    let e = arch(&Fake::default(), &mut Vec::new(), &p)
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a PNG"), "{e}");
}
