//! zip reading against archives written by Info-ZIP and Python, and writing read back by this reader.

use quench_archive::Error;
use quench_archive::zip::{Archive, Method, Writer};
use std::io::Cursor;

fn open(bytes: &[u8]) -> Archive<Cursor<Vec<u8>>> {
    Archive::new(Cursor::new(bytes.to_vec())).expect("a readable zip")
}

#[test]
fn reads_deflated_entries() {
    let mut archive = open(include_bytes!("fixtures/deflated.zip"));
    let names: Vec<_> = archive.entries().iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["text.txt", "random.bin", "sub/", "sub/ünï.txt"]);

    let text = archive.read_by_name("text.txt", 1 << 20).unwrap();
    assert_eq!(text, b"hello hello hello hello hello hello\n");
    assert_eq!(archive.entries()[0].method, Method::Deflated);

    // Random bytes do not compress, so zip stores them.
    let random = archive.read_by_name("random.bin", 1 << 20).unwrap();
    assert_eq!(random.len(), 3000);
    assert_eq!(archive.entries()[1].method, Method::Stored);

    assert!(archive.entries()[2].is_dir());
    assert!(!archive.entries()[0].is_dir());
    assert_eq!(archive.read(2, 10).unwrap(), b"");
    assert_eq!(
        archive.read_by_name("sub/ünï.txt", 100).unwrap(),
        "ünï\n".as_bytes()
    );
    assert_eq!(archive.index_of("random.bin"), Some(1));
    assert_eq!(archive.index_of("missing"), None);
    assert!(matches!(
        archive.read_by_name("missing", 10),
        Err(Error::Corrupt(_))
    ));
}

#[test]
fn reads_stored_entries_and_an_archive_comment() {
    let mut archive = open(include_bytes!("fixtures/stored.zip"));
    assert_eq!(archive.entries()[0].method, Method::Stored);
    assert_eq!(archive.read(0, 100).unwrap().len(), 36);

    // The end record is found past a comment.
    let mut commented = open(include_bytes!("fixtures/comment.zip"));
    assert_eq!(commented.read(0, 100).unwrap().len(), 36);
}

#[test]
fn reads_zip64_extra_fields() {
    let mut archive = open(include_bytes!("fixtures/forced64.zip"));
    assert_eq!(archive.read_by_name("big.txt", 2000).unwrap().len(), 1100);
}

/// Rewrites the end record of a small archive into zip64 form.
fn zip64_end(zip: &[u8]) -> Vec<u8> {
    let end = zip.len() - 22;
    let record = &zip[end..];
    let count = u64::from(u16::from_le_bytes([record[10], record[11]]));
    let size = u64::from(u32::from_le_bytes(record[12..16].try_into().unwrap()));
    let offset = u64::from(u32::from_le_bytes(record[16..20].try_into().unwrap()));

    let mut out = zip[..end].to_vec();
    let zip64_at = out.len() as u64;
    out.extend_from_slice(&0x0606_4B50u32.to_le_bytes());
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&[45, 0, 45, 0]);
    out.extend_from_slice(&[0; 8]);
    for value in [count, count, size, offset] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0x0706_4B50u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&zip64_at.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());

    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn reads_a_zip64_end_record() {
    let mut writer = Writer::new();
    writer.add("a", b"zip sixty four").unwrap();
    let zip = zip64_end(&writer.finish().unwrap());
    let mut archive = open(&zip);
    assert_eq!(archive.read_by_name("a", 100).unwrap(), b"zip sixty four");

    // Saturated fields without the zip64 records are damage.
    let mut broken = zip.clone();
    let at = broken.len() - 22 - 20 - 56;
    broken[at..at + 4].copy_from_slice(&[0; 4]);
    assert!(matches!(
        Archive::new(Cursor::new(broken)),
        Err(Error::Corrupt(_))
    ));
    let mut no_locator = zip;
    let at = no_locator.len() - 22 - 20;
    no_locator[at..at + 4].copy_from_slice(&[0; 4]);
    assert!(matches!(
        Archive::new(Cursor::new(no_locator)),
        Err(Error::Corrupt(_))
    ));
    let tiny = {
        let mut z = vec![0u8; 22];
        z[..4].copy_from_slice(&0x0605_4B50u32.to_le_bytes());
        z[10..12].copy_from_slice(&0xFFFFu16.to_le_bytes());
        z
    };
    assert!(matches!(
        Archive::new(Cursor::new(tiny)),
        Err(Error::Corrupt(_))
    ));
}

#[test]
fn the_writer_and_the_reader_agree() {
    let big: Vec<u8> = (0..20_000u32)
        .flat_map(|i| (i % 251).to_le_bytes())
        .collect();
    let mut writer = Writer::new();
    writer.add("dir/", b"ignored").unwrap();
    writer.add("dir/big.bin", &big).unwrap();
    writer.add("empty", b"").unwrap();
    writer.add("ünï.txt", "ünï".as_bytes()).unwrap();
    let bytes = writer.finish().unwrap();

    let mut archive = open(&bytes);
    assert_eq!(archive.entries().len(), 4);
    assert!(archive.entries()[0].is_dir());
    assert_eq!(archive.entries()[1].method, Method::Deflated);
    assert_eq!(archive.entries()[1].size, big.len() as u64);
    assert!(archive.entries()[1].compressed_size < archive.entries()[1].size);
    assert_eq!(archive.read_by_name("dir/big.bin", 1 << 20).unwrap(), big);
    assert_eq!(archive.read_by_name("empty", 1).unwrap(), b"");
    assert_eq!(
        archive.read_by_name("ünï.txt", 100).unwrap(),
        "ünï".as_bytes()
    );

    let mut writer = Writer::new();
    assert!(writer.add("", b"").is_err());
    assert_eq!(format!("{writer:?}"), "Writer(0 entries)");
}

#[test]
fn limits_and_damage_are_errors() {
    let mut writer = Writer::new();
    writer.add("a.txt", &b"abc".repeat(1000)).unwrap();
    let zip = writer.finish().unwrap();

    let mut archive = open(&zip);
    assert!(matches!(archive.read(0, 2999), Err(Error::Limit(_))));
    assert_eq!(archive.read(0, 3000).unwrap().len(), 3000);

    // Flip a byte of the entry data: the CRC (or the deflate stream) notices.
    let mut flipped = zip.clone();
    flipped[40] ^= 0x55;
    let result = open(&flipped).read(0, 10_000);
    assert!(
        matches!(result, Err(Error::Checksum(_) | Error::Corrupt(_))),
        "{result:?}"
    );

    // Not a zip at all, and a zip cut off.
    assert!(matches!(
        Archive::new(Cursor::new(b"not a zip file".to_vec())),
        Err(Error::Corrupt(_))
    ));
    assert!(matches!(
        Archive::new(Cursor::new(zip[..zip.len() - 10].to_vec())),
        Err(Error::Corrupt(_))
    ));
    // A wrong local header signature.
    let mut local = zip.clone();
    local[0] = 0;
    assert!(matches!(
        open(&local).read(0, 10_000),
        Err(Error::Corrupt(_))
    ));
    // A central directory entry whose signature is wrong.
    let mut central = zip.clone();
    let directory = u32::from_le_bytes(
        central[central.len() - 6..central.len() - 2]
            .try_into()
            .unwrap(),
    );
    central[directory as usize] = 0;
    assert!(matches!(
        Archive::new(Cursor::new(central)),
        Err(Error::Corrupt(_))
    ));
    // A directory that points outside the file.
    let mut outside = zip;
    let n = outside.len();
    outside[n - 6..n - 2].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Archive::new(Cursor::new(outside)),
        Err(Error::Corrupt(_))
    ));
}

/// Edits one flag of the first central directory entry.
fn central_edit(zip: &[u8], edit: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut out = zip.to_vec();
    let directory =
        u32::from_le_bytes(out[out.len() - 6..out.len() - 2].try_into().unwrap()) as usize;
    edit(&mut out[directory..directory + 46]);
    out
}

#[test]
fn encryption_and_other_methods_are_unsupported() {
    let mut writer = Writer::new();
    writer.add("a.txt", &b"abc".repeat(100)).unwrap();
    let zip = writer.finish().unwrap();

    let encrypted = central_edit(&zip, |h| h[8] |= 1);
    assert!(matches!(
        open(&encrypted).read(0, 1000),
        Err(Error::Unsupported(_))
    ));

    let bzip = central_edit(&zip, |h| h[10] = 12);
    let mut archive = open(&bzip);
    assert_eq!(archive.entries()[0].method, Method::Other(12));
    assert!(matches!(archive.read(0, 1000), Err(Error::Unsupported(_))));

    // A declared size that is not the real one.
    let lying = central_edit(&zip, |h| h[24] = h[24].wrapping_add(1));
    assert!(matches!(
        open(&lying).read(0, 10_000),
        Err(Error::Checksum(_) | Error::Corrupt(_) | Error::Limit(_))
    ));

    // A name that claims UTF-8 and is not.
    let bad_name = central_edit(&zip, |h| h[9] |= 0x08);
    let mut raw = bad_name;
    let directory =
        u32::from_le_bytes(raw[raw.len() - 6..raw.len() - 2].try_into().unwrap()) as usize;
    raw[directory + 46] = 0xFF;
    assert!(matches!(
        Archive::new(Cursor::new(raw)),
        Err(Error::Corrupt(_))
    ));

    // A multi-disk archive.
    let mut disks = zip;
    let n = disks.len();
    disks[n - 18] = 1;
    assert!(matches!(
        Archive::new(Cursor::new(disks)),
        Err(Error::Unsupported(_))
    ));
}
