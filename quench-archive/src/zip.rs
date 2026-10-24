//! zip archives: reading stored and deflated entries (with zip64 sizes and offsets), and writing them.
//!
//! [`Archive`] reads the central directory of anything that is `Read + Seek` (a file, a `Cursor`) and then
//! extracts single entries on demand, so looking at one entry of a large file reads only that entry.
//!
//! ```
//! use quench_archive::zip::{Archive, Writer};
//!
//! let mut writer = Writer::new();
//! writer.add("hello.txt", b"hello hello hello").unwrap();
//! let bytes = writer.finish().unwrap();
//!
//! let mut archive = Archive::new(std::io::Cursor::new(bytes)).unwrap();
//! assert_eq!(archive.read_by_name("hello.txt", 1024).unwrap(), b"hello hello hello");
//! ```

use crate::checksum::crc32;
use crate::deflate;
use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom};

const LOCAL_SIGNATURE: u32 = 0x0403_4B50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4B50;
const END_SIGNATURE: u32 = 0x0605_4B50;
const ZIP64_END_SIGNATURE: u32 = 0x0606_4B50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4B50;

const FLAG_ENCRYPTED: u16 = 0x0001;
const FLAG_UTF8: u16 = 0x0800;
const ZIP64_EXTRA: u16 = 0x0001;
const END_RECORD: usize = 22;
/// The end record is within this many bytes of the end: it is followed only by a comment of at most 64 KiB.
const END_SEARCH: u64 = 65_535 + END_RECORD as u64;

/// How an entry's data is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
    /// A method this crate does not decode (bzip2, lzma, ...), by its zip number.
    Other(u16),
}

/// What the central directory says about an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub name: String,
    pub method: Method,
    pub compressed_size: u64,
    pub size: u64,
    pub crc32: u32,
    offset: u64,
    encrypted: bool,
}

impl EntryInfo {
    /// Whether the entry is a directory (its name ends in `/`).
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

const fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"))
}

/// A zip file opened for reading.
#[derive(Debug)]
pub struct Archive<R> {
    reader: R,
    entries: Vec<EntryInfo>,
}

impl<R: Read + Seek> Archive<R> {
    /// Reads the central directory.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] when there is no zip structure, [`Error::Truncated`] for a cut file,
    /// [`Error::Unsupported`] for multi-disk archives, [`Error::Io`] when the reader fails.
    pub fn new(mut reader: R) -> Result<Self> {
        let length = reader.seek(SeekFrom::End(0))?;
        let tail_start = length.saturating_sub(END_SEARCH);
        reader.seek(SeekFrom::Start(tail_start))?;
        let mut tail = Vec::new();
        reader.by_ref().take(END_SEARCH).read_to_end(&mut tail)?;

        let end = tail
            .windows(4)
            .rposition(|w| u32_at(w, 0) == END_SIGNATURE)
            .filter(|&at| at + END_RECORD <= tail.len())
            .ok_or_else(|| Error::corrupt("not a zip file: no end-of-central-directory record"))?;
        let record = &tail[end..];
        if u16_at(record, 4) != 0 || u16_at(record, 6) != 0 {
            return Err(Error::Unsupported("multi-disk zip archives".to_string()));
        }
        let mut count = u64::from(u16_at(record, 10));
        let mut size = u64::from(u32_at(record, 12));
        let mut offset = u64::from(u32_at(record, 16));

        // Saturated fields mean the real values are in the zip64 record the locator points at.
        if count == 0xFFFF || size == 0xFFFF_FFFF || offset == 0xFFFF_FFFF {
            let locator_at = (tail_start + end as u64)
                .checked_sub(20)
                .ok_or_else(|| Error::corrupt("zip64 locator is missing"))?;
            reader.seek(SeekFrom::Start(locator_at))?;
            let mut locator = [0u8; 20];
            reader.read_exact(&mut locator).map_err(truncated)?;
            if u32_at(&locator, 0) != ZIP64_LOCATOR_SIGNATURE {
                return Err(Error::corrupt("zip64 locator is missing"));
            }
            reader.seek(SeekFrom::Start(u64_at(&locator, 8)))?;
            let mut zip64 = [0u8; 56];
            reader.read_exact(&mut zip64).map_err(truncated)?;
            if u32_at(&zip64, 0) != ZIP64_END_SIGNATURE {
                return Err(Error::corrupt("zip64 end record is missing"));
            }
            count = u64_at(&zip64, 32);
            size = u64_at(&zip64, 40);
            offset = u64_at(&zip64, 48);
        }

        if offset.checked_add(size).is_none_or(|end| end > length) {
            return Err(Error::corrupt("central directory lies outside the file"));
        }
        reader.seek(SeekFrom::Start(offset))?;
        let mut directory = Vec::new();
        reader.by_ref().take(size).read_to_end(&mut directory)?;
        let entries = parse_directory(&directory, count, length)?;
        Ok(Self { reader, entries })
    }

    /// Every entry, in directory order.
    #[must_use]
    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }

    /// The position of the entry called `name`.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    /// Extracts the entry called `name`.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] when there is no such entry, else as [`read`](Self::read).
    pub fn read_by_name(&mut self, name: &str, limit: u64) -> Result<Vec<u8>> {
        let index = self
            .index_of(name)
            .ok_or_else(|| Error::corrupt(format!("no entry called {name}")))?;
        self.read(index, limit)
    }

    /// Extracts entry `index`, checking its size and CRC-32, and refusing entries that declare more than
    /// `limit` bytes.
    ///
    /// # Errors
    ///
    /// [`Error::Limit`] past `limit`, [`Error::Unsupported`] for encrypted entries or other methods,
    /// [`Error::Checksum`] when the content does not match, [`Error::Corrupt`] / [`Error::Truncated`] for
    /// damage.
    ///
    /// # Panics
    ///
    /// When `index` is not an entry of [`entries`](Self::entries).
    pub fn read(&mut self, index: usize, limit: u64) -> Result<Vec<u8>> {
        let entry = self.entries[index].clone();
        if entry.encrypted {
            return Err(Error::Unsupported("encrypted zip entries".to_string()));
        }
        if entry.size > limit {
            return Err(Error::Limit(format!(
                "{} is {} bytes, more than {limit}",
                entry.name, entry.size
            )));
        }

        self.reader.seek(SeekFrom::Start(entry.offset))?;
        let mut local = [0u8; 30];
        self.reader.read_exact(&mut local).map_err(truncated)?;
        if u32_at(&local, 0) != LOCAL_SIGNATURE {
            return Err(Error::corrupt("local header signature"));
        }
        let skip = i64::from(u16_at(&local, 26)) + i64::from(u16_at(&local, 28));
        self.reader.seek(SeekFrom::Current(skip))?;

        let mut compressed = Vec::new();
        self.reader
            .by_ref()
            .take(entry.compressed_size)
            .read_to_end(&mut compressed)?;
        if compressed.len() as u64 != entry.compressed_size {
            return Err(Error::Truncated);
        }

        let size = usize::try_from(entry.size).map_err(|_| Error::Limit("size".to_string()))?;
        let data = match entry.method {
            Method::Stored => compressed,
            Method::Deflated => deflate::decompress(&compressed, size)?,
            Method::Other(number) => {
                return Err(Error::Unsupported(format!(
                    "zip compression method {number}"
                )));
            }
        };
        if data.len() != size {
            return Err(Error::Checksum(format!(
                "{} has the wrong size",
                entry.name
            )));
        }
        if crc32(&data) != entry.crc32 {
            return Err(Error::Checksum(format!("{} CRC-32", entry.name)));
        }
        Ok(data)
    }
}

fn truncated(error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        Error::Truncated
    } else {
        Error::from(error)
    }
}

fn parse_directory(directory: &[u8], count: u64, file_length: u64) -> Result<Vec<EntryInfo>> {
    let mut entries = Vec::new();
    let mut at = 0;
    for _ in 0..count {
        let header = directory.get(at..at + 46).ok_or(Error::Truncated)?;
        if u32_at(header, 0) != CENTRAL_SIGNATURE {
            return Err(Error::corrupt("central directory entry signature"));
        }
        let flags = u16_at(header, 8);
        let method = match u16_at(header, 10) {
            0 => Method::Stored,
            8 => Method::Deflated,
            other => Method::Other(other),
        };
        let mut compressed_size = u64::from(u32_at(header, 20));
        let mut size = u64::from(u32_at(header, 24));
        let name_length = usize::from(u16_at(header, 28));
        let extra_length = usize::from(u16_at(header, 30));
        let comment_length = usize::from(u16_at(header, 32));
        let mut offset = u64::from(u32_at(header, 42));

        let variable = directory
            .get(at + 46..at + 46 + name_length + extra_length + comment_length)
            .ok_or(Error::Truncated)?;
        let name_bytes = &variable[..name_length];
        let extra = &variable[name_length..name_length + extra_length];

        // The zip64 extra field carries, in this order, only those values the header saturated.
        let mut fields = extra;
        while fields.len() >= 4 {
            let (id, length) = (u16_at(fields, 0), usize::from(u16_at(fields, 2)));
            let body = fields.get(4..4 + length).ok_or(Error::Truncated)?;
            fields = &fields[4 + length..];
            if id == ZIP64_EXTRA {
                let mut cursor = 0;
                for value in [&mut size, &mut compressed_size, &mut offset] {
                    let saturated = *value == 0xFFFF_FFFF;
                    if saturated {
                        let raw = body.get(cursor..cursor + 8).ok_or(Error::Truncated)?;
                        *value = u64_at(raw, 0);
                        cursor += 8;
                    }
                }
            }
        }
        if offset >= file_length {
            return Err(Error::corrupt("entry starts outside the file"));
        }

        let name = if flags & FLAG_UTF8 != 0 {
            String::from_utf8(name_bytes.to_vec())
                .map_err(|_| Error::corrupt("an entry name is not valid UTF-8"))?
        } else {
            String::from_utf8_lossy(name_bytes).into_owned()
        };
        entries.push(EntryInfo {
            name,
            method,
            compressed_size,
            size,
            crc32: u32_at(header, 16),
            offset,
            encrypted: flags & FLAG_ENCRYPTED != 0,
        });
        at += 46 + name_length + extra_length + comment_length;
    }
    Ok(entries)
}

// ---------------------------------------------------------------- writing

/// The DOS date 1980-01-01 and time 00:00, so archives are reproducible.
const DOS_DATE: u16 = 0x0021;

struct Written {
    name: String,
    method: u16,
    crc: u32,
    compressed: u32,
    size: u32,
    offset: u32,
}

/// Writes a zip archive into memory. Entries are deflated when that is smaller, else stored; sizes and
/// offsets are limited to 4 GiB (no zip64 output).
#[derive(Default)]
pub struct Writer {
    out: Vec<u8>,
    written: Vec<Written>,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Writer({} entries)", self.written.len())
    }
}

impl Writer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            out: Vec::new(),
            written: Vec::new(),
        }
    }

    /// Adds an entry; a name ending in `/` is a directory and `data` is ignored.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] for an empty name, [`Error::Limit`] when the archive would pass what zip's 32-bit
    /// fields and 65,535 entries can describe.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<()> {
        if name.is_empty() {
            return Err(Error::corrupt("a zip entry needs a name"));
        }
        if self.written.len() >= 0xFFFF {
            return Err(Error::Limit("more than 65535 zip entries".to_string()));
        }
        let data = if name.ends_with('/') { &[] } else { data };
        let packed = deflate::compress(data);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() {
            (8, &packed)
        } else {
            (0, data)
        };
        let too_big = || Error::Limit("zip output passes 4 GiB".to_string());
        let offset = u32::try_from(self.out.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let compressed = u32::try_from(body.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        let flags = if name.is_ascii() { 0 } else { FLAG_UTF8 };
        let name_length = u16::try_from(name.len()).map_err(|_| too_big())?;

        self.out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
        self.out
            .extend_from_slice(&(if method == 8 { 20u16 } else { 10 }).to_le_bytes());
        self.out.extend_from_slice(&flags.to_le_bytes());
        self.out.extend_from_slice(&method.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out.extend_from_slice(&DOS_DATE.to_le_bytes());
        self.out.extend_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&compressed.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&name_length.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(body);

        self.written.push(Written {
            name: name.to_string(),
            method,
            crc,
            compressed,
            size,
            offset,
        });
        Ok(())
    }

    /// The finished archive.
    ///
    /// # Errors
    ///
    /// [`Error::Limit`] when the central directory would pass 4 GiB.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let directory_offset = u32::try_from(self.out.len())
            .map_err(|_| Error::Limit("zip output passes 4 GiB".to_string()))?;
        for entry in &self.written {
            let flags = if entry.name.is_ascii() { 0 } else { FLAG_UTF8 };
            self.out.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
            self.out.extend_from_slice(&20u16.to_le_bytes()); // made by
            self.out
                .extend_from_slice(&(if entry.method == 8 { 20u16 } else { 10 }).to_le_bytes());
            self.out.extend_from_slice(&flags.to_le_bytes());
            self.out.extend_from_slice(&entry.method.to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes());
            self.out.extend_from_slice(&DOS_DATE.to_le_bytes());
            self.out.extend_from_slice(&entry.crc.to_le_bytes());
            self.out.extend_from_slice(&entry.compressed.to_le_bytes());
            self.out.extend_from_slice(&entry.size.to_le_bytes());
            self.out
                .extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            self.out.extend_from_slice(&[0; 2 + 2 + 2 + 2 + 4]); // extra, comment, disk, internal, external
            self.out.extend_from_slice(&entry.offset.to_le_bytes());
            self.out.extend_from_slice(entry.name.as_bytes());
        }
        let directory_size = u32::try_from(self.out.len())
            .map_err(|_| Error::Limit("zip output passes 4 GiB".to_string()))?
            - directory_offset;
        let count = self.written.len() as u16;
        self.out.extend_from_slice(&END_SIGNATURE.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]); // disk numbers
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&directory_size.to_le_bytes());
        self.out.extend_from_slice(&directory_offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        Ok(self.out)
    }
}
