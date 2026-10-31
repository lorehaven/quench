//! B-trees: the cell layout of table and index pages, the payload that spills onto overflow pages, and walking
//! a tree in key order.

use crate::error::{Error, Result};
use crate::pager::Pager;
use crate::varint;

/// A tree this deep is a cycle or garbage; real trees are a handful of levels.
const MAX_DEPTH: usize = 64;

/// The four kinds of b-tree page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    TableLeaf,
    TableInterior,
    IndexLeaf,
    IndexInterior,
}

impl Kind {
    /// The flag byte of the page.
    pub(crate) const fn flag(self) -> u8 {
        match self {
            Self::TableLeaf => 13,
            Self::TableInterior => 5,
            Self::IndexLeaf => 10,
            Self::IndexInterior => 2,
        }
    }

    pub(crate) const fn from_flag(flag: u8) -> Option<Self> {
        match flag {
            13 => Some(Self::TableLeaf),
            5 => Some(Self::TableInterior),
            10 => Some(Self::IndexLeaf),
            2 => Some(Self::IndexInterior),
            _ => None,
        }
    }

    pub(crate) const fn is_leaf(self) -> bool {
        matches!(self, Self::TableLeaf | Self::IndexLeaf)
    }

    pub(crate) const fn is_index(self) -> bool {
        matches!(self, Self::IndexLeaf | Self::IndexInterior)
    }
}

/// The header of a b-tree page.
#[derive(Debug, Clone, Copy)]
pub struct PageHeader {
    pub kind: Kind,
    pub cell_count: usize,
    /// Interior pages: the page right of the last cell.
    pub right_child: u32,
    /// Where the cell pointer array starts.
    pointers: usize,
}

fn be16(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([b[at], b[at + 1]]))
}

pub const fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl PageHeader {
    /// Parses the header of page `number`, whose bytes are `page`.
    pub fn parse(number: u32, page: &[u8]) -> Result<Self> {
        let base = if number == 1 { crate::header::SIZE } else { 0 };
        let flag = *page
            .get(base)
            .ok_or_else(|| Error::corrupt(number, "page too short"))?;
        let kind = Kind::from_flag(flag).ok_or_else(|| {
            Error::corrupt(number, format!("page type {flag} is not a b-tree page"))
        })?;
        let header_len = if kind.is_leaf() { 8 } else { 12 };
        if base + header_len > page.len() {
            return Err(Error::corrupt(number, "page too short for its header"));
        }
        let cell_count = be16(page, base + 3);
        let pointers = base + header_len;
        if pointers + 2 * cell_count > page.len() {
            return Err(Error::corrupt(number, "cell pointers run past the page"));
        }
        let right_child = if kind.is_leaf() {
            0
        } else {
            be32(page, base + 8)
        };
        Ok(Self {
            kind,
            cell_count,
            right_child,
            pointers,
        })
    }

    /// Where cell `i` starts in the page.
    pub fn cell_offset(&self, number: u32, page: &[u8], i: usize) -> Result<usize> {
        let at = be16(page, self.pointers + 2 * i);
        if at >= page.len() {
            return Err(Error::corrupt(number, "cell pointer outside the page"));
        }
        Ok(at)
    }
}

/// How much of a payload of `total` bytes stays on the page (the rest goes to overflow pages).
pub const fn local_size(kind: Kind, usable: usize, total: usize) -> usize {
    let (max, min) = if kind.is_index() {
        ((usable - 12) * 64 / 255 - 23, (usable - 12) * 32 / 255 - 23)
    } else {
        (usable - 35, (usable - 12) * 32 / 255 - 23)
    };
    if total <= max {
        return total;
    }
    let k = min + (total - min) % (usable - 4);
    if k <= max { k } else { min }
}

/// A cell, read: the left child of an interior cell, the key of a table cell, and the payload.
#[derive(Debug)]
pub struct Cell {
    pub left_child: u32,
    pub rowid: i64,
    pub payload: Vec<u8>,
}

/// Reads the cell at `at`, following the overflow chain if its payload is not all there.
pub fn read_cell(
    pager: &mut Pager,
    number: u32,
    page: &[u8],
    kind: Kind,
    at: usize,
) -> Result<Cell> {
    let bad = |why: &str| Error::corrupt(number, why.to_string());
    let mut pos = at;
    let left_child = if kind.is_leaf() || kind == Kind::TableInterior {
        0
    } else {
        pos += 4;
        be32(page, at.min(page.len().saturating_sub(4)))
    };
    if kind == Kind::TableInterior {
        let child = be32(page, at.min(page.len().saturating_sub(4)));
        let (key, _) = varint::get(
            page.get(at + 4..)
                .ok_or_else(|| bad("cell past the page"))?,
        )
        .ok_or_else(|| bad("cell key"))?;
        return Ok(Cell {
            left_child: child,
            rowid: key as i64,
            payload: Vec::new(),
        });
    }
    let (total, n) = varint::get(page.get(pos..).ok_or_else(|| bad("cell past the page"))?)
        .ok_or_else(|| bad("payload size"))?;
    pos += n;
    let total = usize::try_from(total).map_err(|_| bad("payload size"))?;
    let rowid = if kind == Kind::TableLeaf {
        let (key, n) = varint::get(page.get(pos..).ok_or_else(|| bad("cell past the page"))?)
            .ok_or_else(|| bad("rowid"))?;
        pos += n;
        key as i64
    } else {
        0
    };
    let usable = pager.usable_size();
    let local = local_size(kind, usable, total);
    let end = pos
        .checked_add(local)
        .filter(|&e| e <= page.len())
        .ok_or_else(|| bad("payload past the page"))?;
    let mut payload = Vec::with_capacity(total);
    payload.extend_from_slice(&page[pos..end]);
    if local < total {
        let mut next = be32(page, end.min(page.len() - 4));
        if end + 4 > page.len() {
            return Err(bad("overflow pointer past the page"));
        }
        let mut hops = 0_u32;
        while payload.len() < total {
            hops += 1;
            if next == 0 || hops > pager.page_count() {
                return Err(bad("overflow chain ends early or loops"));
            }
            let overflow = pager.page(next)?;
            let take = (total - payload.len()).min(usable - 4);
            payload.extend_from_slice(&overflow[4..4 + take]);
            next = be32(&overflow, 0);
        }
    }
    Ok(Cell {
        left_child,
        rowid,
        payload,
    })
}

/// Calls `f(rowid, payload)` for every row of the table tree at `root`, in rowid order.
pub fn scan_table(
    pager: &mut Pager,
    root: u32,
    f: &mut dyn FnMut(i64, &[u8]) -> Result<()>,
) -> Result<()> {
    walk_table(pager, root, 0, f)
}

fn walk_table(
    pager: &mut Pager,
    number: u32,
    depth: usize,
    f: &mut dyn FnMut(i64, &[u8]) -> Result<()>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::corrupt(number, "the tree is too deep (a loop?)"));
    }
    let page = pager.page(number)?;
    let header = PageHeader::parse(number, &page)?;
    match header.kind {
        Kind::TableLeaf => {
            for i in 0..header.cell_count {
                let at = header.cell_offset(number, &page, i)?;
                let cell = read_cell(pager, number, &page, header.kind, at)?;
                f(cell.rowid, &cell.payload)?;
            }
            Ok(())
        }
        Kind::TableInterior => {
            for i in 0..header.cell_count {
                let at = header.cell_offset(number, &page, i)?;
                let cell = read_cell(pager, number, &page, header.kind, at)?;
                walk_table(pager, cell.left_child, depth + 1, f)?;
            }
            walk_table(pager, header.right_child, depth + 1, f)
        }
        _ => Err(Error::corrupt(number, "an index page in a table tree")),
    }
}

/// Calls `f(entry)` for every entry of the index tree at `root`, in key order.
pub fn scan_index(
    pager: &mut Pager,
    root: u32,
    f: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    walk_index(pager, root, 0, f)
}

fn walk_index(
    pager: &mut Pager,
    number: u32,
    depth: usize,
    f: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::corrupt(number, "the tree is too deep (a loop?)"));
    }
    let page = pager.page(number)?;
    let header = PageHeader::parse(number, &page)?;
    match header.kind {
        Kind::IndexLeaf => {
            for i in 0..header.cell_count {
                let at = header.cell_offset(number, &page, i)?;
                f(&read_cell(pager, number, &page, header.kind, at)?.payload)?;
            }
            Ok(())
        }
        Kind::IndexInterior => {
            for i in 0..header.cell_count {
                let at = header.cell_offset(number, &page, i)?;
                let cell = read_cell(pager, number, &page, header.kind, at)?;
                walk_index(pager, cell.left_child, depth + 1, f)?;
                f(&cell.payload)?;
            }
            walk_index(pager, header.right_child, depth + 1, f)
        }
        _ => Err(Error::corrupt(number, "a table page in an index tree")),
    }
}

/// The payload of the row with `rowid` in the table tree at `root`.
pub fn table_get(pager: &mut Pager, root: u32, rowid: i64) -> Result<Option<Vec<u8>>> {
    let mut number = root;
    for _ in 0..=MAX_DEPTH {
        let page = pager.page(number)?;
        let header = PageHeader::parse(number, &page)?;
        match header.kind {
            Kind::TableInterior => {
                // The keys are the largest rowid of each left subtree: take the first cell whose key is >= rowid.
                let (mut lo, mut hi) = (0, header.cell_count);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let at = header.cell_offset(number, &page, mid)?;
                    let cell = read_cell(pager, number, &page, header.kind, at)?;
                    if cell.rowid < rowid {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                number = if lo == header.cell_count {
                    header.right_child
                } else {
                    let at = header.cell_offset(number, &page, lo)?;
                    read_cell(pager, number, &page, header.kind, at)?.left_child
                };
            }
            Kind::TableLeaf => {
                let (mut lo, mut hi) = (0, header.cell_count);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let at = header.cell_offset(number, &page, mid)?;
                    let cell = read_cell(pager, number, &page, header.kind, at)?;
                    match cell.rowid.cmp(&rowid) {
                        std::cmp::Ordering::Equal => return Ok(Some(cell.payload)),
                        std::cmp::Ordering::Less => lo = mid + 1,
                        std::cmp::Ordering::Greater => hi = mid,
                    }
                }
                return Ok(None);
            }
            _ => return Err(Error::corrupt(number, "an index page in a table tree")),
        }
    }
    Err(Error::corrupt(number, "the tree is too deep (a loop?)"))
}
