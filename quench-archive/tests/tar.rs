//! tar reading against archives written by GNU tar, and writing read back by this reader.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_archive::Error;
use quench_archive::tar::{Builder, Entry, EntryKind, Header, Reader};

fn read(bytes: &[u8]) -> Vec<Entry<'_>> {
    Reader::new(bytes)
        .collect::<Result<_, _>>()
        .expect("a readable archive")
}

fn find<'a, 'b>(entries: &'b [Entry<'a>], path: &str) -> &'b Entry<'a> {
    entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("no {path}"))
}

#[quench_test::test]
fn reads_a_ustar_archive() {
    let bytes = include_bytes!("fixtures/ustar.tar");
    let entries = read(bytes);
    let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["hello.txt", "docs/", "docs/big.txt", "link"]);

    let hello = find(&entries, "hello.txt");
    assert_eq!(hello.kind, EntryKind::File);
    assert_eq!(hello.data, b"hello\n");
    assert_eq!(
        (hello.mode, hello.uid, hello.gid, hello.mtime),
        (0o644, 0, 0, 1_700_000_000)
    );

    let docs = find(&entries, "docs/");
    assert!(docs.kind.is_dir() && !docs.kind.is_file() && docs.data.is_empty());

    assert_eq!(find(&entries, "docs/big.txt").data.len(), 3000);
    let link = find(&entries, "link");
    assert_eq!(link.kind, EntryKind::Symlink);
    assert_eq!(link.link.as_deref(), Some("hello.txt"));
}

fn deep() -> String {
    format!("{}leaf.txt", "deeply/".repeat(18))
}

#[quench_test::test]
fn reads_gnu_long_names() {
    let bytes = include_bytes!("fixtures/gnu.tar");
    let entries = read(bytes);
    assert_eq!(find(&entries, &deep()).data, b"deep\n");
    assert!(deep().len() > 100);
}

#[quench_test::test]
fn reads_pax_headers() {
    let bytes = include_bytes!("fixtures/pax.tar");
    let entries = read(bytes);
    assert_eq!(find(&entries, &deep()).data, b"deep\n");
    // Non-ASCII names come through the pax `path` record.
    assert_eq!(find(&entries, "ünï/ü.txt").data, b"uni\n");
}

#[quench_test::test]
fn the_writer_and_the_reader_agree() {
    let long_name = format!("{}file.txt", "directory/".repeat(30));
    let mut builder = Builder::new();
    builder.append_file("a.txt", b"first").unwrap();
    builder.append_file(&long_name, b"long").unwrap();
    // Splits into a ustar prefix and a name.
    let split = format!("{}/{}", "p".repeat(120), "n".repeat(60));
    builder.append_file(&split, b"split").unwrap();
    builder
        .append(&Header::directory("dir"), b"ignored")
        .unwrap();
    builder
        .append(&Header::symlink("dir/ln", &"t".repeat(150)), b"ignored")
        .unwrap();
    builder
        .append(
            &Header {
                mode: 0o755,
                uid: 1000,
                gid: 100,
                mtime: 1_234_567_890,
                ..Header::file("exec")
            },
            b"#!/bin/sh\n",
        )
        .unwrap();
    builder.append_file("empty", b"").unwrap();
    let bytes = builder.finish();
    assert_eq!(bytes.len() % 512, 0);

    let entries = read(&bytes);
    assert_eq!(find(&entries, "a.txt").data, b"first");
    assert_eq!(find(&entries, &long_name).data, b"long");
    assert_eq!(find(&entries, &split).data, b"split");
    let dir = find(&entries, "dir/");
    assert_eq!(
        (dir.kind, dir.mode, dir.data.len()),
        (EntryKind::Directory, 0o755, 0)
    );
    let link = find(&entries, "dir/ln");
    assert_eq!(link.kind, EntryKind::Symlink);
    assert_eq!(link.link, Some("t".repeat(150)));
    let exec = find(&entries, "exec");
    assert_eq!(
        (exec.mode, exec.uid, exec.gid, exec.mtime),
        (0o755, 1000, 100, 1_234_567_890)
    );
    assert_eq!(find(&entries, "empty").data, b"");
}

#[quench_test::test]
fn large_numbers_use_base_256() {
    let mut builder = Builder::new();
    builder
        .append(
            &Header {
                uid: u64::MAX >> 8,
                mtime: 1 << 40,
                ..Header::file("x")
            },
            b"",
        )
        .unwrap();
    let bytes = builder.finish();
    let entry = &read(&bytes)[0];
    assert_eq!((entry.uid, entry.mtime), (u64::MAX >> 8, 1 << 40));
}

#[quench_test::test]
fn the_writer_refuses_unusable_paths() {
    let mut builder = Builder::new();
    assert!(builder.append_file("", b"").is_err());
    assert!(builder.append_file("a\0b", b"").is_err());
}

#[quench_test::test]
fn damaged_archives_are_errors() {
    let mut builder = Builder::new();
    builder.append_file("a.txt", b"data").unwrap();
    let good = builder.finish();

    // A header checksum that does not match.
    let mut flipped = good.clone();
    flipped[0] ^= 1;
    assert!(matches!(
        Reader::new(&flipped).next(),
        Some(Err(Error::Checksum(_)))
    ));
    // A header cut short.
    assert!(matches!(
        Reader::new(&good[..100]).next(),
        Some(Err(Error::Truncated))
    ));
    // Data cut short.
    assert!(matches!(
        Reader::new(&good[..513]).next(),
        Some(Err(Error::Truncated))
    ));
    // The iterator ends after an error.
    let mut reader = Reader::new(&good[..100]);
    assert!(reader.next().is_some());
    assert!(reader.next().is_none());
    // A non-octal number.
    let mut bad_number = good.clone();
    bad_number[100] = b'9';
    reseal(&mut bad_number);
    assert!(matches!(
        Reader::new(&bad_number).next(),
        Some(Err(Error::Corrupt(_)))
    ));
    // A path that is not UTF-8.
    let mut bad_path = good.clone();
    bad_path[0] = 0xFF;
    reseal(&mut bad_path);
    assert!(matches!(
        Reader::new(&bad_path).next(),
        Some(Err(Error::Corrupt(_)))
    ));
    // An archive with no end marker still ends at the data.
    let unterminated = &good[..512 + 512];
    assert_eq!(read(unterminated).len(), 1);
    assert_eq!(Reader::new(&[]).count(), 0);
}

/// Recomputes the checksum of the first header after a test edited it.
fn reseal(archive: &mut [u8]) {
    archive[148..156].fill(b' ');
    let sum: u32 = archive[..512].iter().map(|&b| u32::from(b)).sum();
    archive[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

#[quench_test::test]
fn a_huge_declared_size_is_truncation_not_allocation() {
    let mut builder = Builder::new();
    builder.append(&Header::file("big"), &[]).unwrap();
    let mut bytes = builder.finish();
    // Claim 2^40 bytes of data in base-256.
    bytes[124] = 0x80;
    bytes[125..135].fill(0);
    bytes[133] = 0x01;
    reseal(&mut bytes);
    assert!(matches!(
        Reader::new(&bytes).next(),
        Some(Err(Error::Truncated))
    ));
}

#[quench_test::test]
fn pax_records_override_the_header() {
    let record = |key: &str, value: &str| {
        let body = format!(" {key}={value}\n");
        let mut length = body.len() + 1;
        while format!("{length}{body}").len() != length {
            length = format!("{length}{body}").len();
        }
        format!("{length}{body}")
    };
    let pax = [
        record("path", "overridden/name.txt"),
        record("linkpath", "elsewhere"),
        record("uid", "4242"),
        record("gid", "2424"),
        record("mtime", "1700000000.5"),
        record("comment", "ignored"),
    ]
    .concat();

    let mut builder = Builder::new();
    builder.append_file("plain", b"x").unwrap();
    let inner = builder.finish();
    // Build: pax header (type x) + the file entry.
    let mut header = [0u8; 512];
    header[..13].copy_from_slice(b"PaxHeader/pax");
    header[100..107].copy_from_slice(b"0000644");
    header[124..135].copy_from_slice(format!("{:011o}", pax.len()).as_bytes());
    header[156] = b'x';
    header[257..263].copy_from_slice(b"ustar\0");
    header[148..156].fill(b' ');
    let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut archive = header.to_vec();
    archive.extend_from_slice(pax.as_bytes());
    archive.resize(archive.len().next_multiple_of(512), 0);
    archive.extend_from_slice(&inner);

    let entries = read(&archive);
    let entry = &entries[0];
    assert_eq!(entry.path, "overridden/name.txt");
    assert_eq!(entry.link.as_deref(), Some("elsewhere"));
    assert_eq!(
        (entry.uid, entry.gid, entry.mtime),
        (4242, 2424, 1_700_000_000)
    );
    assert_eq!(entry.data, b"x");
}

quench_test::main!();
