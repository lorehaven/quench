//! tar archives: reading ustar, GNU (long names) and pax (extended headers), and writing ustar with GNU long
//! names.
//!
//! [`Reader`] works on a byte slice and hands out entries whose data borrows from it, so reading never
//! allocates for a file's content and an archive cannot make the reader allocate more than its own size.
//!
//! ```
//! use quench_archive::tar::{Builder, EntryKind, Reader};
//!
//! let mut builder = Builder::new();
//! builder.append_file("docs/readme.txt", b"hello").unwrap();
//! let bytes = builder.finish();
//!
//! let entries: Vec<_> = Reader::new(&bytes).collect::<Result<_, _>>().unwrap();
//! assert_eq!(entries[0].path, "docs/readme.txt");
//! assert_eq!(entries[0].kind, EntryKind::File);
//! assert_eq!(entries[0].data, b"hello");
//! ```

use crate::error::{Error, Result};

const BLOCK: usize = 512;
const NAME: usize = 100;
const PREFIX: usize = 155;

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    HardLink,
    CharDevice,
    BlockDevice,
    Fifo,
    /// A type flag this crate does not know; the entry is read as a file would be.
    Other(u8),
}

impl EntryKind {
    const fn from_flag(flag: u8) -> Self {
        match flag {
            0 | b'0' | b'7' => Self::File,
            b'1' => Self::HardLink,
            b'2' => Self::Symlink,
            b'3' => Self::CharDevice,
            b'4' => Self::BlockDevice,
            b'5' => Self::Directory,
            b'6' => Self::Fifo,
            other => Self::Other(other),
        }
    }

    const fn flag(self) -> u8 {
        match self {
            Self::File => b'0',
            Self::HardLink => b'1',
            Self::Symlink => b'2',
            Self::CharDevice => b'3',
            Self::BlockDevice => b'4',
            Self::Directory => b'5',
            Self::Fifo => b'6',
            Self::Other(flag) => flag,
        }
    }

    /// Whether the entry's size counts bytes of data that follow the header.
    const fn has_data(self) -> bool {
        !matches!(
            self,
            Self::Directory
                | Self::Symlink
                | Self::HardLink
                | Self::CharDevice
                | Self::BlockDevice
                | Self::Fifo
        )
    }

    /// Whether this is a regular file.
    #[must_use]
    pub const fn is_file(self) -> bool {
        matches!(self, Self::File)
    }

    /// Whether this is a directory.
    #[must_use]
    pub const fn is_dir(self) -> bool {
        matches!(self, Self::Directory)
    }
}

/// One member of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    /// Seconds since the Unix epoch.
    pub mtime: u64,
    /// The target of a symbolic or hard link.
    pub link: Option<String>,
    /// The file's content (empty for anything that is not a file).
    pub data: &'a [u8],
}

/// Reads the entries of an archive held in memory.
///
/// The iterator ends at the end-of-archive marker (or the end of the data) and yields one error, then ends,
/// for a damaged archive.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    position: usize,
    done: bool,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            done: false,
        }
    }

    fn block(&self) -> Result<Option<&'a [u8]>> {
        if self.position == self.data.len() {
            return Ok(None);
        }
        self.data
            .get(self.position..self.position + BLOCK)
            .map(Some)
            .ok_or(Error::Truncated)
    }

    /// Takes `size` bytes of data and skips its padding.
    fn take(&mut self, size: u64) -> Result<&'a [u8]> {
        let size = usize::try_from(size).map_err(|_| Error::Truncated)?;
        let end = self.position.checked_add(size).ok_or(Error::Truncated)?;
        let data = self.data.get(self.position..end).ok_or(Error::Truncated)?;
        self.position = end
            .saturating_add((BLOCK - size % BLOCK) % BLOCK)
            .min(self.data.len());
        Ok(data)
    }

    fn next_entry(&mut self) -> Result<Option<Entry<'a>>> {
        let mut long_name: Option<String> = None;
        let mut long_link: Option<String> = None;
        let mut pax = Pax::default();
        loop {
            let Some(block) = self.block()? else {
                return Ok(None);
            };
            if block.iter().all(|&b| b == 0) {
                return Ok(None);
            }
            self.position += BLOCK;
            verify_checksum(block)?;

            let flag = block[156];
            let size = pax.size.map_or_else(|| number(&block[124..136]), Ok)?;
            match flag {
                b'L' | b'K' => {
                    let text = c_string(self.take(size)?)?;
                    if flag == b'L' {
                        long_name = Some(text);
                    } else {
                        long_link = Some(text);
                    }
                    continue;
                }
                b'x' => {
                    pax.parse(self.take(size)?)?;
                    continue;
                }
                // Global extended headers describe the whole archive; nothing here depends on them.
                b'g' => {
                    self.take(size)?;
                    continue;
                }
                _ => {}
            }

            let kind = EntryKind::from_flag(flag);
            let mut path = pax
                .path
                .take()
                .or(long_name)
                .map_or_else(|| header_path(block), Ok)?;
            if kind.is_dir() && !path.ends_with('/') {
                path.push('/');
            }
            let link = pax
                .link
                .take()
                .or(long_link)
                .map_or_else(|| c_string(&block[157..257]), Ok)?;
            let data = if kind.has_data() {
                self.take(size)?
            } else {
                &[]
            };
            return Ok(Some(Entry {
                path,
                kind,
                size,
                mode: u32::try_from(number(&block[100..108])? & 0o7777).unwrap_or(0),
                uid: pax.uid.map_or_else(|| number(&block[108..116]), Ok)?,
                gid: pax.gid.map_or_else(|| number(&block[116..124]), Ok)?,
                mtime: pax.mtime.map_or_else(|| number(&block[136..148]), Ok)?,
                link: (!link.is_empty()).then_some(link),
                data,
            }));
        }
    }
}

impl<'a> Iterator for Reader<'a> {
    type Item = Result<Entry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.next_entry() {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

/// The values of a pax extended header that override the next entry's own.
#[derive(Default)]
struct Pax {
    path: Option<String>,
    link: Option<String>,
    size: Option<u64>,
    uid: Option<u64>,
    gid: Option<u64>,
    mtime: Option<u64>,
}

impl Pax {
    /// Parses `<length> <key>=<value>\n` records.
    fn parse(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            let space = data
                .iter()
                .position(|&b| b == b' ')
                .ok_or_else(|| Error::corrupt("pax record without a length"))?;
            let length: usize = std::str::from_utf8(&data[..space])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| Error::corrupt("pax record length"))?;
            if length <= space + 1 || length > data.len() {
                return Err(Error::corrupt("pax record length"));
            }
            let record = &data[space + 1..length];
            data = &data[length..];
            let record = record.strip_suffix(b"\n").unwrap_or(record);
            let Some(equals) = record.iter().position(|&b| b == b'=') else {
                return Err(Error::corrupt("pax record without a value"));
            };
            let (key, value) = (&record[..equals], &record[equals + 1..]);
            let text = || {
                std::str::from_utf8(value)
                    .map(str::to_string)
                    .map_err(|_| Error::corrupt("pax value is not valid UTF-8"))
            };
            let integer = || {
                text()?
                    .split('.')
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| Error::corrupt("pax number"))
            };
            match key {
                b"path" => self.path = Some(text()?),
                b"linkpath" => self.link = Some(text()?),
                b"size" => self.size = Some(integer()?),
                b"uid" => self.uid = Some(integer()?),
                b"gid" => self.gid = Some(integer()?),
                b"mtime" => self.mtime = Some(integer()?),
                _ => {}
            }
        }
        Ok(())
    }
}

/// A NUL-terminated string field.
fn c_string(field: &[u8]) -> Result<String> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map(str::to_string)
        .map_err(|_| Error::corrupt("a path is not valid UTF-8"))
}

fn header_path(block: &[u8]) -> Result<String> {
    let name = c_string(&block[..NAME])?;
    // Only POSIX ustar headers use the prefix field this way; old GNU headers keep other data there.
    if &block[257..263] == b"ustar\0" {
        let prefix = c_string(&block[345..345 + PREFIX])?;
        if !prefix.is_empty() {
            return Ok(format!("{prefix}/{name}"));
        }
    }
    Ok(name)
}

/// An octal field (NUL- or space-terminated), or GNU's base-256 form when the top bit is set.
fn number(field: &[u8]) -> Result<u64> {
    if field[0] & 0x80 != 0 {
        if field[0] & 0x40 != 0 {
            return Err(Error::Unsupported("negative tar number".to_string()));
        }
        let mut value = u64::from(field[0] & 0x3F);
        for &byte in &field[1..] {
            value = value
                .checked_mul(256)
                .and_then(|v| v.checked_add(u64::from(byte)))
                .ok_or_else(|| Error::corrupt("tar number overflows"))?;
        }
        return Ok(value);
    }
    let text = field
        .iter()
        .copied()
        .skip_while(|&b| b == b' ')
        .take_while(|&b| b != 0 && b != b' ')
        .collect::<Vec<u8>>();
    if text.is_empty() {
        return Ok(0);
    }
    let mut value = 0u64;
    for byte in text {
        if !(b'0'..=b'7').contains(&byte) {
            return Err(Error::corrupt("tar number is not octal"));
        }
        value = value
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(byte - b'0')))
            .ok_or_else(|| Error::corrupt("tar number overflows"))?;
    }
    Ok(value)
}

fn verify_checksum(block: &[u8]) -> Result<()> {
    let stored = number(&block[148..156])?;
    let (mut unsigned, mut signed) = (0u64, 0i64);
    for (i, &byte) in block.iter().enumerate() {
        let byte = if (148..156).contains(&i) { b' ' } else { byte };
        unsigned += u64::from(byte);
        signed += i64::from(byte.cast_signed());
    }
    if stored == unsigned || i64::try_from(stored).is_ok_and(|s| s == signed) {
        Ok(())
    } else {
        Err(Error::Checksum("tar header".to_string()))
    }
}

// ---------------------------------------------------------------- writing

/// The metadata of an entry to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub path: String,
    pub kind: EntryKind,
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    /// Seconds since the Unix epoch.
    pub mtime: u64,
    /// The target of a symbolic or hard link.
    pub link: Option<String>,
}

impl Header {
    /// A regular file: mode 0644, owner root, modification time 0 (what makes archives reproducible).
    #[must_use]
    pub fn file(path: &str) -> Self {
        Self {
            path: path.to_string(),
            kind: EntryKind::File,
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime: 0,
            link: None,
        }
    }

    /// A directory: mode 0755.
    #[must_use]
    pub fn directory(path: &str) -> Self {
        Self {
            kind: EntryKind::Directory,
            mode: 0o755,
            ..Self::file(path)
        }
    }

    /// A symbolic link to `target`: mode 0777.
    #[must_use]
    pub fn symlink(path: &str, target: &str) -> Self {
        Self {
            kind: EntryKind::Symlink,
            mode: 0o777,
            link: Some(target.to_string()),
            ..Self::file(path)
        }
    }
}

/// Writes an archive into memory.
#[derive(Debug, Default)]
pub struct Builder {
    out: Vec<u8>,
}

impl Builder {
    #[must_use]
    pub const fn new() -> Self {
        Self { out: Vec::new() }
    }

    /// Appends a regular file with the default [`Header::file`] metadata.
    ///
    /// # Errors
    ///
    /// As [`append`](Self::append).
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.append(&Header::file(path), data)
    }

    /// Appends an entry; `data` is its content and is ignored (and not written) unless the entry is a file.
    ///
    /// Paths longer than ustar's 100 + 155 characters (or not splittable at a `/`) get a GNU long-name entry
    /// before them, as GNU tar writes.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] for an empty path or one containing a NUL.
    pub fn append(&mut self, header: &Header, data: &[u8]) -> Result<()> {
        if header.path.is_empty() || header.path.contains('\0') {
            return Err(Error::corrupt("a tar path cannot be empty or contain NUL"));
        }
        let link = header.link.as_deref().unwrap_or("");
        let data = if header.kind.has_data() { data } else { &[] };

        let mut path = header.path.clone();
        if header.kind.is_dir() && !path.ends_with('/') {
            path.push('/');
        }

        let (name, prefix) = split_path(&path).unwrap_or_else(|| {
            self.long_entry(b'L', &path);
            (truncated(&path, NAME), String::new())
        });
        let link_field = if link.len() > NAME {
            self.long_entry(b'K', link);
            truncated(link, NAME)
        } else {
            link.to_string()
        };

        let mut block = [0u8; BLOCK];
        put_text(&mut block[..NAME], &name);
        put_octal(&mut block[100..108], u64::from(header.mode & 0o7777));
        put_number(&mut block[108..116], header.uid);
        put_number(&mut block[116..124], header.gid);
        put_number(&mut block[124..136], data.len() as u64);
        put_number(&mut block[136..148], header.mtime);
        block[156] = header.kind.flag();
        put_text(&mut block[157..257], &link_field);
        block[257..263].copy_from_slice(b"ustar\0");
        block[263..265].copy_from_slice(b"00");
        put_text(&mut block[345..345 + PREFIX], &prefix);
        seal(&mut block);

        self.out.extend_from_slice(&block);
        self.out.extend_from_slice(data);
        self.pad(data.len());
        Ok(())
    }

    /// A GNU `L`/`K` entry carrying a name that does not fit its header.
    fn long_entry(&mut self, flag: u8, text: &str) {
        let mut block = [0u8; BLOCK];
        put_text(&mut block[..NAME], "././@LongLink");
        put_octal(&mut block[100..108], 0o644);
        put_number(&mut block[108..116], 0);
        put_number(&mut block[116..124], 0);
        let length = text.len() + 1;
        put_number(&mut block[124..136], length as u64);
        put_number(&mut block[136..148], 0);
        block[156] = flag;
        block[257..265].copy_from_slice(b"ustar  \0");
        seal(&mut block);
        self.out.extend_from_slice(&block);
        self.out.extend_from_slice(text.as_bytes());
        self.out.push(0);
        self.pad(length);
    }

    fn pad(&mut self, length: usize) {
        self.out
            .resize(self.out.len() + (BLOCK - length % BLOCK) % BLOCK, 0);
    }

    /// The finished archive, with its end-of-archive marker.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.out.resize(self.out.len() + 2 * BLOCK, 0);
        self.out
    }
}

/// The longest prefix of `text` that fits `limit` bytes without splitting a character.
fn truncated(text: &str, limit: usize) -> String {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Splits a path into ustar's `(name, prefix)` when it fits.
fn split_path(path: &str) -> Option<(String, String)> {
    if path.len() <= NAME {
        return Some((path.to_string(), String::new()));
    }
    // The prefix is everything before some `/`; the rest must fit the name field.
    path.match_indices('/')
        .map(|(i, _)| i)
        .find(|&i| i <= PREFIX && path.len() - i - 1 <= NAME && path.len() - i - 1 > 0)
        .map(|i| (path[i + 1..].to_string(), path[..i].to_string()))
}

fn put_text(field: &mut [u8], text: &str) {
    field[..text.len()].copy_from_slice(text.as_bytes());
}

fn put_octal(field: &mut [u8], value: u64) {
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    field[..digits].copy_from_slice(text.as_bytes());
}

/// Octal when it fits, else GNU's base-256.
fn put_number(field: &mut [u8], value: u64) {
    let digits = field.len() - 1;
    if value < 1u64 << (3 * digits) {
        put_octal(field, value);
    } else {
        let bytes = value.to_be_bytes();
        let start = field.len() - bytes.len();
        field[start..].copy_from_slice(&bytes);
        field[0] = 0x80;
    }
}

fn seal(block: &mut [u8; BLOCK]) {
    block[148..156].fill(b' ');
    let sum: u64 = block.iter().map(|&b| u64::from(b)).sum();
    let text = format!("{sum:06o}\0 ");
    block[148..156].copy_from_slice(text.as_bytes());
}
