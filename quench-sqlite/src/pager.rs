//! Page access: the file as numbered pages (the first is page 1), read through a small cache, and written in
//! transactions with a rollback journal.
//!
//! A transaction keeps every page it changes in memory until `commit`. The commit writes the journal (the
//! original of every changed page that existed before) and syncs it, writes the changed pages into the
//! database and syncs that, and then deletes the journal. A crash before the delete leaves a *hot journal*; the
//! next open writes the originals back and truncates the file to its old size, so a transaction is either all
//! there or not there at all. The journal is in the format the stock `sqlite3` reads, so it can also recover a
//! database this crate crashed on.

use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::header::{self, Header};

/// How many pages the cache keeps before it starts over.
const CACHE_PAGES: usize = 2_000;

/// The byte at which SQLite reserves a page for locking (1 GiB); databases that reach it are not written yet.
const LOCK_BYTE: u64 = 0x4000_0000;

const JOURNAL_MAGIC: [u8; 8] = [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];
const SECTOR: usize = 512;

/// The pages of one database file.
#[derive(Debug)]
pub struct Pager {
    path: PathBuf,
    file: File,
    writable: bool,
    header: Header,
    page_count: u32,
    cache: HashMap<u32, Arc<Vec<u8>>>,
    /// Pages changed in the open transaction.
    dirty: BTreeMap<u32, Arc<Vec<u8>>>,
    transaction: Option<Saved>,
    /// While a statement runs inside a transaction: what each page it touched held before, to undo it alone.
    undo: Option<StatementUndo>,
    /// Tests: the write that fails (and every one after it), to look at what a crash leaves behind.
    pub(crate) fail_at: Option<usize>,
    /// Pages read from the file (not from the cache), for tests of the access paths.
    pub(crate) file_reads: u64,
}

/// The pages and header of a transaction at one moment (pages are shared, so this is cheap).
#[derive(Debug, Clone)]
pub struct Snapshot {
    dirty: BTreeMap<u32, Arc<Vec<u8>>>,
    header: Header,
    page_count: u32,
}

/// What it takes to undo one statement: the pages it touched as they were (`None`: not changed in the transaction
/// yet), and the header and size before it.
#[derive(Debug)]
struct StatementUndo {
    pages: HashMap<u32, Option<Arc<Vec<u8>>>>,
    header: Header,
    page_count: u32,
}

/// What a rollback restores.
#[derive(Debug)]
struct Saved {
    header: Header,
    page_count: u32,
}

fn journal_path(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

const fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn checksum(nonce: u32, page: &[u8]) -> u32 {
    let mut sum = nonce;
    let mut i = page.len() as isize - 200;
    while i > 0 {
        sum = sum.wrapping_add(u32::from(page[i as usize]));
        i -= 200;
    }
    sum
}

impl Pager {
    /// Opens the file for reading. A database with a hot journal is refused: only a writer may recover it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path.as_ref(), false)
    }

    /// Opens the file for reading and writing, holding it exclusively, and recovers a hot journal.
    pub fn open_rw(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path.as_ref(), true)
    }

    fn open_with(path: &Path, writable: bool) -> Result<Self> {
        let mut file = OpenOptions::new().read(true).write(writable).open(path)?;
        if writable {
            file.try_lock().map_err(|e| match e {
                std::fs::TryLockError::WouldBlock => Error::Busy,
                std::fs::TryLockError::Error(e) => Error::Io(e),
            })?;
            recover(path, &mut file)?;
        } else if hot_journal(path) {
            return Err(Error::Unsupported(
                "the database has a hot journal (a write was interrupted): open it for writing to recover".into(),
            ));
        }
        let mut first = [0_u8; header::SIZE];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut first).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                Error::NotADatabase("the file is shorter than a header".into())
            } else {
                Error::Io(e)
            }
        })?;
        let header = Header::parse(&first)?;
        if header.write_version > 2 || header.read_version > 2 {
            return Err(Error::Unsupported(format!(
                "file format versions {}/{}",
                header.write_version, header.read_version
            )));
        }
        if header.read_version == 2 {
            return Err(Error::Unsupported(
                "a database in WAL mode (the write-ahead log is not read yet)".into(),
            ));
        }
        let len = file.metadata()?.len();
        let by_size = u32::try_from(len / u64::from(header.page_size))
            .map_err(|_| Error::NotADatabase("more pages than 2^32".into()))?;
        // The header's page count is only trusted when it was written by the last change.
        let page_count =
            if header.page_count != 0 && header.version_valid_for == header.change_counter {
                header.page_count
            } else {
                by_size
            };
        if page_count == 0 {
            return Err(Error::NotADatabase("no pages".into()));
        }
        Ok(Self {
            path: path.to_path_buf(),
            file,
            writable,
            header,
            page_count,
            cache: HashMap::new(),
            dirty: BTreeMap::new(),
            transaction: None,
            undo: None,
            fail_at: None,
            file_reads: 0,
        })
    }

    /// Makes a new database file (it must not exist) with an empty schema, and opens it for writing.
    pub fn create(path: impl AsRef<Path>, page_size: u32) -> Result<Self> {
        let path = path.as_ref();
        if !(512..=65_536).contains(&page_size) || !page_size.is_power_of_two() {
            return Err(Error::Unsupported(format!(
                "page size {page_size}: use a power of two from 512 to 65536"
            )));
        }
        let mut page = vec![0_u8; page_size as usize];
        page[..16].copy_from_slice(header::MAGIC);
        page[16..18].copy_from_slice(
            &(if page_size == 65_536 {
                1
            } else {
                page_size as u16
            })
            .to_be_bytes(),
        );
        page[18] = 1;
        page[19] = 1;
        page[21] = 64;
        page[22] = 32;
        page[23] = 32;
        page[24..28].copy_from_slice(&1_u32.to_be_bytes());
        page[28..32].copy_from_slice(&1_u32.to_be_bytes());
        page[44..48].copy_from_slice(&4_u32.to_be_bytes());
        page[56..60].copy_from_slice(&1_u32.to_be_bytes());
        page[92..96].copy_from_slice(&1_u32.to_be_bytes());
        page[96..100].copy_from_slice(&3_045_000_u32.to_be_bytes());
        // An empty table leaf for `sqlite_schema`.
        page[header::SIZE] = 13;
        // The cell content starts at the end of the page (0 stands for 65536).
        let content = u16::try_from(page_size).unwrap_or(0).to_be_bytes();
        page[header::SIZE + 5..header::SIZE + 7].copy_from_slice(&content);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&page)?;
        file.sync_all()?;
        drop(file);
        Self::open_rw(path)
    }

    /// The parsed header.
    #[must_use]
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// The header, to change it inside a transaction (it is written to page 1 at commit).
    pub fn header_mut(&mut self) -> Result<&mut Header> {
        self.need_transaction("header change")?;
        Ok(&mut self.header)
    }

    /// Bytes in a page.
    #[must_use]
    pub const fn page_size(&self) -> usize {
        self.header.page_size as usize
    }

    /// Bytes of a page that hold data: the page less the reserved region.
    #[must_use]
    pub const fn usable_size(&self) -> usize {
        self.page_size() - self.header.reserved as usize
    }

    /// Pages in the database.
    #[must_use]
    pub const fn page_count(&self) -> u32 {
        self.page_count
    }

    /// Page `number` (1-based), as the open transaction sees it.
    pub fn page(&mut self, number: u32) -> Result<Arc<Vec<u8>>> {
        if number == 0 || number > self.page_count {
            return Err(Error::corrupt(
                number,
                format!("page number outside 1..={}", self.page_count),
            ));
        }
        if let Some(page) = self.dirty.get(&number) {
            return Ok(Arc::clone(page));
        }
        if let Some(page) = self.cache.get(&number) {
            return Ok(Arc::clone(page));
        }
        let page = Arc::new(self.read_from_file(number)?);
        if self.cache.len() >= CACHE_PAGES {
            self.cache.clear();
        }
        self.cache.insert(number, Arc::clone(&page));
        Ok(page)
    }

    fn read_from_file(&mut self, number: u32) -> Result<Vec<u8>> {
        self.file_reads += 1;
        let size = self.page_size();
        let mut buf = vec![0_u8; size];
        self.file
            .seek(SeekFrom::Start(u64::from(number - 1) * size as u64))?;
        self.file.read_exact(&mut buf).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                Error::corrupt(number, "the file ends inside the page")
            } else {
                Error::Io(e)
            }
        })?;
        Ok(buf)
    }

    const fn need_transaction(&self, what: &'static str) -> Result<()> {
        if !self.writable {
            return Err(Error::ReadOnly("the database is open for reading"));
        }
        if self.transaction.is_none() {
            return Err(Error::ReadOnly(what));
        }
        Ok(())
    }

    /// Starts a transaction.
    pub fn begin(&mut self) -> Result<()> {
        if !self.writable {
            return Err(Error::ReadOnly("the database is open for reading"));
        }
        if self.transaction.is_some() {
            return Err(Error::ReadOnly("a transaction is already open"));
        }
        if u64::from(self.page_count) * self.page_size() as u64 >= LOCK_BYTE {
            return Err(Error::Unsupported(
                "databases of 1 GiB and more are not written yet".into(),
            ));
        }
        self.transaction = Some(Saved {
            header: self.header.clone(),
            page_count: self.page_count,
        });
        Ok(())
    }

    /// Sets page `number` to `bytes` (exactly one page) in the open transaction.
    pub fn write_page(&mut self, number: u32, bytes: Vec<u8>) -> Result<()> {
        self.need_transaction("page write outside a transaction")?;
        if number == 0 || number > self.page_count || bytes.len() != self.page_size() {
            return Err(Error::corrupt(
                number,
                "write of a page that does not exist",
            ));
        }
        self.put(number, Arc::new(bytes));
        Ok(())
    }

    /// A page for new content: one from the freelist, or a new page at the end of the file.
    pub fn allocate(&mut self) -> Result<u32> {
        self.need_transaction("allocation outside a transaction")?;
        let trunk = self.header.freelist_trunk;
        if trunk != 0 {
            let page = self.page(trunk)?;
            let count = be32(&page, 4);
            let number = if count > 0 {
                let leaf = be32(&page, 8 + 4 * (count as usize - 1));
                let mut changed = (*page).clone();
                changed[4..8].copy_from_slice(&(count - 1).to_be_bytes());
                self.put(trunk, Arc::new(changed));
                leaf
            } else {
                self.header.freelist_trunk = be32(&page, 0);
                trunk
            };
            self.header.freelist_count = self.header.freelist_count.saturating_sub(1);
            if number == 0 || number > self.page_count {
                return Err(Error::corrupt(
                    trunk,
                    "the freelist names a page that does not exist",
                ));
            }
            self.put(number, Arc::new(vec![0; self.page_size()]));
            return Ok(number);
        }
        if (u64::from(self.page_count) + 1) * self.page_size() as u64 >= LOCK_BYTE {
            return Err(Error::Unsupported(
                "databases of 1 GiB and more are not written yet".into(),
            ));
        }
        self.page_count += 1;
        let blank = Arc::new(vec![0; self.page_size()]);
        self.put(self.page_count, blank);
        Ok(self.page_count)
    }

    /// Puts page `number` on the freelist.
    pub fn free(&mut self, number: u32) -> Result<()> {
        self.need_transaction("free outside a transaction")?;
        if number < 2 || number > self.page_count {
            return Err(Error::corrupt(
                number,
                "freeing a page that cannot be freed",
            ));
        }
        let trunk = self.header.freelist_trunk;
        let max_leaves = self.usable_size() / 4 - 2;
        if trunk != 0 {
            let page = self.page(trunk)?;
            let count = be32(&page, 4) as usize;
            if count < max_leaves {
                let mut changed = (*page).clone();
                changed[8 + 4 * count..12 + 4 * count].copy_from_slice(&number.to_be_bytes());
                changed[4..8].copy_from_slice(&(count as u32 + 1).to_be_bytes());
                self.put(trunk, Arc::new(changed));
                self.put(number, Arc::new(vec![0; self.page_size()]));
                self.header.freelist_count += 1;
                return Ok(());
            }
        }
        let mut page = vec![0_u8; self.page_size()];
        page[0..4].copy_from_slice(&trunk.to_be_bytes());
        self.put(number, Arc::new(page));
        self.header.freelist_trunk = number;
        self.header.freelist_count += 1;
        Ok(())
    }

    /// Forgets every cached page.
    #[cfg(test)]
    pub(crate) fn drop_cache(&mut self) {
        self.cache.clear();
    }

    /// Puts a page into the transaction, remembering what it held for the statement being run.
    fn put(&mut self, number: u32, page: Arc<Vec<u8>>) {
        if let Some(undo) = &mut self.undo {
            undo.pages
                .entry(number)
                .or_insert_with(|| self.dirty.get(&number).cloned());
        }
        self.dirty.insert(number, page);
    }

    /// Starts a statement inside the open transaction: a failure can be undone by [`abort_statement`] without
    /// touching what the transaction did before. Costs nothing until pages change.
    ///
    /// [`abort_statement`]: Self::abort_statement
    pub fn begin_statement(&mut self) -> Result<()> {
        self.need_transaction("statement outside a transaction")?;
        self.undo = Some(StatementUndo {
            pages: HashMap::new(),
            header: self.header.clone(),
            page_count: self.page_count,
        });
        Ok(())
    }

    /// The statement succeeded: its changes stay in the transaction.
    pub fn end_statement(&mut self) {
        self.undo = None;
    }

    /// The statement failed: every page it changed goes back as it was.
    pub fn abort_statement(&mut self) {
        let Some(undo) = self.undo.take() else { return };
        for (number, before) in undo.pages {
            match before {
                Some(page) => {
                    self.dirty.insert(number, page);
                }
                None => {
                    self.dirty.remove(&number);
                }
            }
        }
        self.header = undo.header;
        self.page_count = undo.page_count;
    }

    /// A point inside the open transaction to go back to (a statement that fails, a savepoint).
    pub fn snapshot(&self) -> Result<Snapshot> {
        self.need_transaction("snapshot outside a transaction")?;
        Ok(Snapshot {
            dirty: self.dirty.clone(),
            header: self.header.clone(),
            page_count: self.page_count,
        })
    }

    /// Goes back to a snapshot taken in this transaction.
    pub fn restore(&mut self, snapshot: Snapshot) -> Result<()> {
        self.need_transaction("restore outside a transaction")?;
        self.undo = None;
        self.dirty = snapshot.dirty;
        self.header = snapshot.header;
        self.page_count = snapshot.page_count;
        Ok(())
    }

    /// Throws the open transaction away.
    pub fn rollback(&mut self) {
        let Some(saved) = self.transaction.take() else {
            return;
        };
        self.undo = None;
        self.dirty.clear();
        self.cache.clear();
        self.header = saved.header;
        self.page_count = saved.page_count;
    }

    /// Makes the transaction permanent. Fails with the file untouched or recoverable (see the module text).
    pub fn commit(&mut self) -> Result<()> {
        let Some(saved) = self.transaction.take() else {
            return Err(Error::ReadOnly("no transaction is open"));
        };
        if self.dirty.is_empty() && self.header == saved.header {
            return Ok(());
        }
        let result = self.write_commit(&saved);
        if result.is_err() {
            // What the file holds is for the next open to sort out; this handle is no longer usable.
            self.dirty.clear();
            self.cache.clear();
        }
        result
    }

    /// A step that is not a write (a sync, the delete): the failure point, when reached, stops here.
    fn barrier(&mut self) -> Result<()> {
        if self.step()? {
            return Err(Error::Io(std::io::Error::other("injected failure")));
        }
        Ok(())
    }

    fn step(&mut self) -> Result<bool> {
        match &mut self.fail_at {
            None => Ok(false),
            Some(0) => Err(Error::Io(std::io::Error::other("injected failure"))),
            Some(n) => {
                *n -= 1;
                // The write that is about to be the last one lands torn.
                Ok(*n == 0)
            }
        }
    }

    fn write_commit(&mut self, saved: &Saved) -> Result<()> {
        let size = self.page_size();
        // Page 1 carries the header fields.
        let mut first = (*self.page(1)?).clone();
        self.header.change_counter = saved.header.change_counter.wrapping_add(1);
        self.header.version_valid_for = self.header.change_counter;
        self.header.page_count = self.page_count;
        first[24..28].copy_from_slice(&self.header.change_counter.to_be_bytes());
        first[28..32].copy_from_slice(&self.header.page_count.to_be_bytes());
        first[32..36].copy_from_slice(&self.header.freelist_trunk.to_be_bytes());
        first[36..40].copy_from_slice(&self.header.freelist_count.to_be_bytes());
        first[40..44].copy_from_slice(&self.header.schema_cookie.to_be_bytes());
        first[60..64].copy_from_slice(&self.header.user_version.to_be_bytes());
        first[92..96].copy_from_slice(&self.header.version_valid_for.to_be_bytes());
        self.dirty.insert(1, Arc::new(first));

        // 1. The journal: the original of every page that is changed and was there before.
        let journal = journal_path(&self.path);
        let originals: Vec<u32> = self
            .dirty
            .keys()
            .copied()
            .filter(|&n| n <= saved.page_count)
            .collect();
        let nonce = self.header.change_counter.wrapping_mul(2_654_435_761)
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
        let mut j = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&journal)?;
        let mut head = vec![0_u8; SECTOR];
        head[..8].copy_from_slice(&JOURNAL_MAGIC);
        head[8..12].copy_from_slice(&(originals.len() as u32).to_be_bytes());
        head[12..16].copy_from_slice(&nonce.to_be_bytes());
        head[16..20].copy_from_slice(&saved.page_count.to_be_bytes());
        head[20..24].copy_from_slice(&(SECTOR as u32).to_be_bytes());
        head[24..28].copy_from_slice(&(size as u32).to_be_bytes());
        let torn = self.step()?;
        write_maybe_torn(&mut j, &head, torn)?;
        for n in originals {
            let original = self.read_from_file(n)?;
            let mut record = Vec::with_capacity(size + 8);
            record.extend_from_slice(&n.to_be_bytes());
            record.extend_from_slice(&original);
            record.extend_from_slice(&checksum(nonce, &original).to_be_bytes());
            let torn = self.step()?;
            write_maybe_torn(&mut j, &record, torn)?;
        }
        self.barrier()?;
        j.sync_all()?;
        drop(j);

        // 2. The database.
        let pages: Vec<(u32, Arc<Vec<u8>>)> = self
            .dirty
            .iter()
            .map(|(n, p)| (*n, Arc::clone(p)))
            .collect();
        for (n, page) in pages {
            self.file
                .seek(SeekFrom::Start(u64::from(n - 1) * size as u64))?;
            let torn = self.step()?;
            write_maybe_torn(&mut self.file, &page, torn)?;
        }
        self.barrier()?;
        self.file.sync_all()?;

        // 3. The commit point: the journal goes.
        self.barrier()?;
        std::fs::remove_file(&journal)?;
        for (n, page) in std::mem::take(&mut self.dirty) {
            self.cache.insert(n, page);
        }
        Ok(())
    }
}

fn write_maybe_torn(file: &mut impl Write, bytes: &[u8], torn: bool) -> Result<()> {
    if torn {
        file.write_all(&bytes[..bytes.len() / 2])?;
        file.flush()?;
        return Err(Error::Io(std::io::Error::other(
            "injected failure (torn write)",
        )));
    }
    file.write_all(bytes)?;
    Ok(())
}

fn hot_journal(db: &Path) -> bool {
    let Ok(mut j) = File::open(journal_path(db)) else {
        return false;
    };
    let mut magic = [0_u8; 8];
    j.read_exact(&mut magic).is_ok() && magic == JOURNAL_MAGIC
}

/// Rolls an interrupted transaction back: the originals go back, the file gets its old size, the journal goes.
fn recover(db: &Path, file: &mut File) -> Result<()> {
    let path = journal_path(db);
    let Ok(mut j) = File::open(&path) else {
        return Ok(());
    };
    let mut head = vec![0_u8; SECTOR];
    if j.read_exact(&mut head).is_err() || head[..8] != JOURNAL_MAGIC {
        // Not a journal that was ever completed (or one that was zeroed): nothing was changed in the database.
        drop(j);
        std::fs::remove_file(&path)?;
        return Ok(());
    }
    let mut count = be32(&head, 8);
    let nonce = be32(&head, 12);
    let initial = be32(&head, 16);
    let sector = be32(&head, 20) as usize;
    let size = be32(&head, 24) as usize;
    if sector < 512 || !size.is_power_of_two() || !(512..=65_536).contains(&size) {
        return Err(Error::corrupt(0, "the journal header is malformed"));
    }
    let len = j.metadata()?.len() as usize;
    let record = size + 8;
    if count == u32::MAX || (sector + count as usize * record) > len {
        count = (len.saturating_sub(sector) / record) as u32;
    }
    j.seek(SeekFrom::Start(sector as u64))?;
    let mut buf = vec![0_u8; record];
    for _ in 0..count {
        if j.read_exact(&mut buf).is_err() {
            break;
        }
        let number = be32(&buf, 0);
        let data = &buf[4..4 + size];
        if be32(&buf, 4 + size) != checksum(nonce, data) || number == 0 {
            break;
        }
        file.seek(SeekFrom::Start(u64::from(number - 1) * size as u64))?;
        file.write_all(data)?;
    }
    file.set_len(u64::from(initial) * size as u64)?;
    file.sync_all()?;
    drop(j);
    std::fs::remove_file(&path)?;
    Ok(())
}
