//! The 100-byte header at the start of page 1.

use crate::error::{Error, Result};

/// The first sixteen bytes of every database file.
pub const MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// The size of the header.
pub const SIZE: usize = 100;

/// The fields of the header this crate uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Bytes in a page: a power of two from 512 to 65536.
    pub page_size: u32,
    /// 1 for the rollback journal, 2 for the write-ahead log (for writing).
    pub write_version: u8,
    /// 1 for the rollback journal, 2 for the write-ahead log (for reading).
    pub read_version: u8,
    /// Unused bytes at the end of every page.
    pub reserved: u8,
    /// Counts the transactions that changed the file.
    pub change_counter: u32,
    /// The size of the database in pages (valid when `version_valid_for` equals the change counter).
    pub page_count: u32,
    /// The first page of the freelist, 0 when it is empty.
    pub freelist_trunk: u32,
    /// The pages on the freelist.
    pub freelist_count: u32,
    /// Changes whenever the schema does.
    pub schema_cookie: u32,
    /// 1 to 4: which features the schema uses.
    pub schema_format: u32,
    /// 1 for UTF-8 (the only encoding supported).
    pub text_encoding: u32,
    /// Free for the application; `PRAGMA user_version`.
    pub user_version: u32,
    /// The change counter the page count was written for.
    pub version_valid_for: u32,
}

const fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl Header {
    /// Parses the first page.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < SIZE {
            return Err(Error::NotADatabase(
                "shorter than the 100-byte header".into(),
            ));
        }
        if &bytes[..16] != MAGIC {
            return Err(Error::NotADatabase("the header text is missing".into()));
        }
        let raw = u32::from(u16::from_be_bytes([bytes[16], bytes[17]]));
        let page_size = if raw == 1 { 65_536 } else { raw };
        if !(512..=65_536).contains(&page_size) || !page_size.is_power_of_two() {
            return Err(Error::NotADatabase(format!("page size {raw} is not valid")));
        }
        if bytes[21] != 64 || bytes[22] != 32 || bytes[23] != 32 {
            return Err(Error::NotADatabase(
                "the payload fractions are not 64/32/32".into(),
            ));
        }
        let header = Self {
            page_size,
            write_version: bytes[18],
            read_version: bytes[19],
            reserved: bytes[20],
            change_counter: be32(bytes, 24),
            page_count: be32(bytes, 28),
            freelist_trunk: be32(bytes, 32),
            freelist_count: be32(bytes, 36),
            schema_cookie: be32(bytes, 40),
            schema_format: be32(bytes, 44),
            text_encoding: be32(bytes, 56),
            user_version: be32(bytes, 60),
            version_valid_for: be32(bytes, 92),
        };
        if usize::from(header.reserved) > (page_size as usize) - 480 {
            return Err(Error::NotADatabase("too many reserved bytes".into()));
        }
        if !(1..=4).contains(&header.schema_format) && header.schema_format != 0 {
            return Err(Error::Unsupported(format!(
                "schema format {}",
                header.schema_format
            )));
        }
        if header.text_encoding != 1 && header.text_encoding != 0 {
            return Err(Error::Unsupported(
                "text encoding other than UTF-8 (UTF-16 databases are not read)".into(),
            ));
        }
        Ok(header)
    }
}
