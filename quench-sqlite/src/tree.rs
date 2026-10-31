//! Changing b-trees: insert with page splits, delete with merging, and the cells that spill onto overflow pages.
//!
//! A page is rebuilt whole from its cells whenever it changes (`Node`), which keeps the layout simple: the
//! cells are packed at the end of the page, in order, with no free blocks. A node that no longer fits is split
//! into pages that do, and the dividers go up to the parent (`settle`); a non-root node that empties is merged
//! with a neighbour (`rebalance`). The root keeps its page number throughout, since the schema names it.

use std::cmp::Ordering;
use std::sync::Arc;

use crate::btree::{Kind, PageHeader, be32, local_size};
use crate::error::{Error, Result};
use crate::pager::Pager;
use crate::record::{self, Value};
use crate::varint;

/// A tree this deep is a cycle or garbage.
const MAX_DEPTH: usize = 64;

/// The two kinds of tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tree {
    Table,
    Index,
}

/// A page, taken apart: its cells as they are stored, and for an interior page the page right of the last cell.
#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    cells: Vec<Vec<u8>>,
    right: u32,
}

/// A step of a descent: the page, and which pointer of it was taken (`cells.len()` is the right child).
#[derive(Debug, Clone, Copy)]
struct Frame {
    page: u32,
    slot: usize,
}

fn internal(page: u32, what: &str) -> Error {
    Error::corrupt(page, format!("internal: {what}"))
}

const fn child(cell: &[u8]) -> u32 {
    be32(cell, 0)
}

fn set_child(cell: &mut [u8], page: u32) {
    cell[..4].copy_from_slice(&page.to_be_bytes());
}

/// The bytes of the cell at `at`.
fn cell_len(kind: Kind, usable: usize, page: &[u8], at: usize, number: u32) -> Result<usize> {
    let bad = || Error::corrupt(number, "a cell runs past the page");
    let get = |from: usize| page.get(from..).and_then(varint::get).ok_or_else(bad);
    let len = match kind {
        Kind::TableInterior => 4 + get(at + 4)?.1,
        Kind::TableLeaf => {
            let (total, n1) = get(at)?;
            let (_, n2) = get(at + n1)?;
            let total = total as usize;
            let local = local_size(kind, usable, total);
            n1 + n2 + local + if local < total { 4 } else { 0 }
        }
        Kind::IndexLeaf | Kind::IndexInterior => {
            let skip = if kind == Kind::IndexInterior { 4 } else { 0 };
            let (total, n1) = get(at + skip)?;
            let total = total as usize;
            let local = local_size(kind, usable, total);
            skip + n1 + local + if local < total { 4 } else { 0 }
        }
    };
    if at + len > page.len() {
        return Err(bad());
    }
    Ok(len)
}

/// A page read without copying its cells: where each one lies in the page.
struct Raw {
    page: Arc<Vec<u8>>,
    kind: Kind,
    spans: Vec<(usize, usize)>,
    right: u32,
}

impl Raw {
    const fn len(&self) -> usize {
        self.spans.len()
    }

    fn cell(&self, i: usize) -> &[u8] {
        let (start, len) = self.spans[i];
        &self.page[start..start + len]
    }

    /// The page as a node, for changing it.
    fn node(&self) -> Node {
        Node {
            kind: self.kind,
            cells: (0..self.len()).map(|i| self.cell(i).to_vec()).collect(),
            right: self.right,
        }
    }
}

fn load_raw(pager: &mut Pager, number: u32) -> Result<Raw> {
    let page = pager.page(number)?;
    let header = PageHeader::parse(number, &page)?;
    let usable = pager.usable_size();
    let mut spans = Vec::with_capacity(header.cell_count);
    for i in 0..header.cell_count {
        let at = header.cell_offset(number, &page, i)?;
        spans.push((at, cell_len(header.kind, usable, &page, at, number)?));
    }
    Ok(Raw {
        page,
        kind: header.kind,
        spans,
        right: header.right_child,
    })
}

fn load(pager: &mut Pager, number: u32) -> Result<Node> {
    Ok(load_raw(pager, number)?.node())
}

/// The first index in `0..len` for which `at` is true, when it is false and then true all the way (a binary search).
fn first_where(len: usize, mut at: impl FnMut(usize) -> Result<bool>) -> Result<usize> {
    let (mut lo, mut hi) = (0, len);
    while lo < hi {
        let mid = usize::midpoint(lo, hi);
        if at(mid)? {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Ok(lo)
}

fn table_key(cell: &[u8]) -> i64 {
    varint::get(&cell[4..]).map_or(0, |(k, _)| k as i64)
}

const fn header_len(kind: Kind) -> usize {
    if kind.is_leaf() { 8 } else { 12 }
}

const fn base_of(number: u32) -> usize {
    if number == 1 { crate::header::SIZE } else { 0 }
}

fn fits(pager: &Pager, number: u32, node: &Node) -> bool {
    let used: usize = node.cells.iter().map(|c| c.len() + 2).sum();
    base_of(number) + header_len(node.kind) + used <= pager.usable_size()
}

fn store(pager: &mut Pager, number: u32, node: &Node) -> Result<()> {
    if !fits(pager, number, node) {
        return Err(internal(number, "a node does not fit its page"));
    }
    let base = base_of(number);
    let hl = header_len(node.kind);
    let usable = pager.usable_size();
    let mut page = vec![0_u8; pager.page_size()];
    if number == 1 {
        page[..base].copy_from_slice(&pager.page(1)?[..base]);
    }
    page[base] = node.kind.flag();
    page[base + 3..base + 5].copy_from_slice(&(node.cells.len() as u16).to_be_bytes());
    let mut top = usable;
    for (i, cell) in node.cells.iter().enumerate() {
        top -= cell.len();
        page[top..top + cell.len()].copy_from_slice(cell);
        page[base + hl + 2 * i..base + hl + 2 * i + 2].copy_from_slice(&(top as u16).to_be_bytes());
    }
    // 0 stands for 65536.
    page[base + 5..base + 7].copy_from_slice(&((top & 0xffff) as u16).to_be_bytes());
    if !node.kind.is_leaf() {
        page[base + 8..base + 12].copy_from_slice(&node.right.to_be_bytes());
    }
    pager.write_page(number, page)
}

/// A page for an empty tree of the given kind; its number is the tree's root.
pub fn new_tree(pager: &mut Pager, tree: Tree) -> Result<u32> {
    let number = pager.allocate()?;
    let kind = match tree {
        Tree::Table => Kind::TableLeaf,
        Tree::Index => Kind::IndexLeaf,
    };
    store(
        pager,
        number,
        &Node {
            kind,
            cells: Vec::new(),
            right: 0,
        },
    )?;
    Ok(number)
}

// ---- cells that spill onto overflow pages

fn write_overflow(pager: &mut Pager, rest: &[u8]) -> Result<u32> {
    let chunk = pager.usable_size() - 4;
    let chunks: Vec<&[u8]> = rest.chunks(chunk).collect();
    let mut numbers = Vec::with_capacity(chunks.len());
    for _ in &chunks {
        numbers.push(pager.allocate()?);
    }
    for (i, data) in chunks.iter().enumerate() {
        let mut page = vec![0_u8; pager.page_size()];
        page[..4].copy_from_slice(&numbers.get(i + 1).copied().unwrap_or(0).to_be_bytes());
        page[4..4 + data.len()].copy_from_slice(data);
        pager.write_page(numbers[i], page)?;
    }
    Ok(numbers[0])
}

/// A leaf cell for `payload` (and `rowid`, for a table).
fn make_cell(pager: &mut Pager, kind: Kind, rowid: i64, payload: &[u8]) -> Result<Vec<u8>> {
    let total = payload.len();
    let local = local_size(kind, pager.usable_size(), total);
    let mut cell = Vec::with_capacity(local + 24);
    varint::put(&mut cell, total as u64);
    if kind == Kind::TableLeaf {
        varint::put(&mut cell, rowid as u64);
    }
    cell.extend_from_slice(&payload[..local]);
    if local < total {
        let first = write_overflow(pager, &payload[local..])?;
        cell.extend_from_slice(&first.to_be_bytes());
    }
    Ok(cell)
}

/// Where a cell's local payload starts, the payload size, and the local size.
fn payload_parts(kind: Kind, usable: usize, cell: &[u8]) -> Result<(usize, usize, usize)> {
    let bad = || Error::corrupt(0, "a cell is malformed");
    let mut at = if kind == Kind::IndexInterior { 4 } else { 0 };
    let (total, n) = varint::get(cell.get(at..).ok_or_else(bad)?).ok_or_else(bad)?;
    at += n;
    if kind == Kind::TableLeaf {
        at += varint::get(cell.get(at..).ok_or_else(bad)?)
            .ok_or_else(bad)?
            .1;
    }
    let total = total as usize;
    Ok((at, total, local_size(kind, usable, total)))
}

/// The whole payload of a cell, overflow pages included.
fn cell_payload(pager: &mut Pager, kind: Kind, cell: &[u8]) -> Result<Vec<u8>> {
    let usable = pager.usable_size();
    let (at, total, local) = payload_parts(kind, usable, cell)?;
    let mut payload = cell
        .get(at..at + local)
        .ok_or_else(|| Error::corrupt(0, "a cell is malformed"))?
        .to_vec();
    if local < total {
        let mut next = be32(cell, at + local);
        let mut hops = 0;
        while payload.len() < total {
            hops += 1;
            if next == 0 || hops > pager.page_count() {
                return Err(Error::corrupt(next, "overflow chain ends early or loops"));
            }
            let page = pager.page(next)?;
            let take = (total - payload.len()).min(usable - 4);
            payload.extend_from_slice(&page[4..4 + take]);
            next = be32(&page, 0);
        }
    }
    Ok(payload)
}

/// Frees the overflow pages of a cell, if it has any.
fn free_overflow(pager: &mut Pager, kind: Kind, cell: &[u8]) -> Result<()> {
    if kind == Kind::TableInterior {
        return Ok(());
    }
    let usable = pager.usable_size();
    let (at, total, local) = payload_parts(kind, usable, cell)?;
    if local >= total {
        return Ok(());
    }
    let mut next = be32(cell, at + local);
    let mut hops = 0;
    while next != 0 {
        hops += 1;
        if hops > pager.page_count() {
            return Err(Error::corrupt(next, "overflow chain loops"));
        }
        let after = be32(&pager.page(next)?, 0);
        pager.free(next)?;
        next = after;
    }
    Ok(())
}

// ---- splitting

#[derive(Debug)]
struct Plan {
    parts: Vec<Node>,
    dividers: Vec<Vec<u8>>,
}

/// Where to cut a list of cells (their sizes with the pointer each takes) so that every part fits `cap`. With
/// `consume`, the cell at a cut moves up to the parent and belongs to neither part.
fn choose(sizes: &[usize], cap: usize, consume: bool, append: bool) -> Result<Vec<usize>> {
    let n = sizes.len();
    if append && n >= 2 {
        return Ok(vec![n - 1]);
    }
    let total: usize = sizes.iter().sum();
    let hi = if consume { n.saturating_sub(1) } else { n };
    let mut best: Option<(usize, usize)> = None;
    let mut left = sizes.first().copied().unwrap_or(0);
    for (s, &size) in sizes.iter().enumerate().take(hi).skip(1) {
        let right = total - left - if consume { size } else { 0 };
        if left <= cap && right <= cap {
            let diff = left.abs_diff(right);
            if best.is_none_or(|(d, _)| diff < d) {
                best = Some((diff, s));
            }
        }
        left += size;
    }
    if let Some((_, s)) = best {
        return Ok(vec![s]);
    }
    // No cut makes two parts: pack greedily.
    let mut cuts = Vec::new();
    let mut pos = 0;
    loop {
        let (mut end, mut sum) = (pos, 0);
        while end < n && sum + sizes[end] <= cap {
            sum += sizes[end];
            end += 1;
        }
        if end == n {
            return Ok(cuts);
        }
        if end == pos {
            return Err(internal(0, "a cell is larger than a page"));
        }
        let cut = if consume && end + 1 >= n {
            if end - 1 > pos {
                end - 1
            } else {
                return Err(internal(0, "cannot cut the cells"));
            }
        } else {
            end
        };
        cuts.push(cut);
        pos = if consume { cut + 1 } else { cut };
    }
}

fn table_leaf_rowid(cell: &[u8]) -> Result<u64> {
    let bad = || Error::corrupt(0, "a table cell is malformed");
    let (_, n) = varint::get(cell).ok_or_else(bad)?;
    Ok(varint::get(cell.get(n..).ok_or_else(bad)?)
        .ok_or_else(bad)?
        .0)
}

fn plan_split(pager: &Pager, node: &Node, append: bool) -> Result<Plan> {
    let kind = node.kind;
    let cap = pager.usable_size() - header_len(kind);
    let consume = kind != Kind::TableLeaf;
    let sizes: Vec<usize> = node.cells.iter().map(|c| c.len() + 2).collect();
    let cuts = choose(&sizes, cap, consume, append && kind == Kind::TableLeaf)?;
    let mut parts = Vec::new();
    let mut dividers = Vec::new();
    let mut start = 0;
    for cut in cuts {
        let cells = node.cells[start..cut].to_vec();
        if consume {
            let sep = &node.cells[cut];
            let (right, divider) = if kind == Kind::IndexLeaf {
                (0, sep.clone())
            } else {
                (child(sep), sep[4..].to_vec())
            };
            parts.push(Node { kind, cells, right });
            dividers.push(divider);
            start = cut + 1;
        } else {
            let last = cells.last().ok_or_else(|| internal(0, "an empty part"))?;
            let mut divider = Vec::new();
            varint::put(&mut divider, table_leaf_rowid(last)?);
            parts.push(Node {
                kind,
                cells,
                right: 0,
            });
            dividers.push(divider);
            start = cut;
        }
    }
    parts.push(Node {
        kind,
        cells: node.cells[start..].to_vec(),
        right: node.right,
    });
    Ok(Plan { parts, dividers })
}

const fn interior_of(kind: Kind) -> Kind {
    if kind.is_index() {
        Kind::IndexInterior
    } else {
        Kind::TableInterior
    }
}

fn divider_cell(page: u32, divider: &[u8]) -> Vec<u8> {
    let mut cell = page.to_be_bytes().to_vec();
    cell.extend_from_slice(divider);
    cell
}

/// Stores `node` as the last page of `path`, splitting it (and its parents) while it does not fit.
fn settle(pager: &mut Pager, mut path: Vec<Frame>, mut node: Node, mut append: bool) -> Result<()> {
    loop {
        let frame = *path.last().ok_or_else(|| internal(0, "empty path"))?;
        if fits(pager, frame.page, &node) {
            return store(pager, frame.page, &node);
        }
        let plan = plan_split(pager, &node, append)?;
        if path.len() == 1 {
            // The root keeps its number: the parts move to new targets and the root points at them.
            let mut targets = Vec::new();
            for part in &plan.parts {
                let page = pager.allocate()?;
                store(pager, page, part)?;
                targets.push(page);
            }
            let cells = plan
                .dividers
                .iter()
                .enumerate()
                .map(|(i, d)| divider_cell(targets[i], d))
                .collect();
            let root = Node {
                kind: interior_of(node.kind),
                cells,
                right: targets[targets.len() - 1],
            };
            return store(pager, frame.page, &root);
        }
        let mut targets = vec![frame.page];
        for _ in 1..plan.parts.len() {
            targets.push(pager.allocate()?);
        }
        for (part, page) in plan.parts.iter().zip(&targets) {
            store(pager, *page, part)?;
        }
        let parent_frame = path[path.len() - 2];
        let mut parent = load(pager, parent_frame.page)?;
        let slot = parent_frame.slot;
        let last = targets[targets.len() - 1];
        if slot < parent.cells.len() {
            set_child(&mut parent.cells[slot], last);
        } else {
            parent.right = last;
        }
        let dividers: Vec<Vec<u8>> = plan
            .dividers
            .iter()
            .enumerate()
            .map(|(i, d)| divider_cell(targets[i], d))
            .collect();
        parent.cells.splice(slot..slot, dividers);
        path.pop();
        node = parent;
        append = false;
    }
}

// ---- merging

fn combine(left: &Node, right: &Node, separator: &[u8]) -> Node {
    let mut cells = left.cells.clone();
    let kind = left.kind;
    match kind {
        Kind::TableLeaf => {}
        Kind::IndexLeaf => cells.push(separator[4..].to_vec()),
        Kind::TableInterior | Kind::IndexInterior => {
            cells.push(divider_cell(left.right, &separator[4..]));
        }
    }
    cells.extend(right.cells.iter().cloned());
    Node {
        kind,
        cells,
        right: right.right,
    }
}

/// `node` (the last page of `path`) has lost cells: an empty non-root node is merged with a neighbour, an empty
/// root with one child is replaced by that child.
fn rebalance(pager: &mut Pager, mut path: Vec<Frame>, mut node: Node) -> Result<()> {
    loop {
        let frame = *path.last().ok_or_else(|| internal(0, "empty path"))?;
        if path.len() == 1 {
            if !node.kind.is_leaf() && node.cells.is_empty() {
                let only = load(pager, node.right)?;
                if fits(pager, frame.page, &only) {
                    store(pager, frame.page, &only)?;
                    return pager.free(node.right);
                }
            }
            return store(pager, frame.page, &node);
        }
        if !node.cells.is_empty() {
            return store(pager, frame.page, &node);
        }
        let parent_frame = path[path.len() - 2];
        let mut parent = load(pager, parent_frame.page)?;
        let n = parent.cells.len();
        if n == 0 {
            return Err(internal(
                parent_frame.page,
                "an interior page with a single child",
            ));
        }
        let (j, left_page, left, right_page, right) = if parent_frame.slot < n {
            let j = parent_frame.slot;
            let right_page = if j + 1 < n {
                child(&parent.cells[j + 1])
            } else {
                parent.right
            };
            (
                j,
                frame.page,
                node.clone(),
                right_page,
                load(pager, right_page)?,
            )
        } else {
            let j = n - 1;
            let left_page = child(&parent.cells[j]);
            (
                j,
                left_page,
                load(pager, left_page)?,
                frame.page,
                node.clone(),
            )
        };
        let combined = combine(&left, &right, &parent.cells[j]);
        path.pop();
        if fits(pager, left_page, &combined) {
            store(pager, left_page, &combined)?;
            pager.free(right_page)?;
            parent.cells.remove(j);
            if j < parent.cells.len() {
                set_child(&mut parent.cells[j], left_page);
            } else {
                parent.right = left_page;
            }
            node = parent;
            continue;
        }
        // Too much for one page: share it out again between two (or, with huge cells, three).
        let plan = plan_split(pager, &combined, false)?;
        let mut targets = vec![left_page, right_page];
        while targets.len() < plan.parts.len() {
            targets.push(pager.allocate()?);
        }
        for (part, page) in plan.parts.iter().zip(&targets) {
            store(pager, *page, part)?;
        }
        let dividers: Vec<Vec<u8>> = plan
            .dividers
            .iter()
            .enumerate()
            .map(|(i, d)| divider_cell(targets[i], d))
            .collect();
        let last_page = targets[targets.len() - 1];
        parent.cells.remove(j);
        if j < parent.cells.len() {
            set_child(&mut parent.cells[j], last_page);
        } else {
            parent.right = last_page;
        }
        parent.cells.splice(j..j, dividers);
        return settle(pager, path, parent, false);
    }
}

// ---- tables

/// What `table_find` returns: the path, the leaf, the position (`Ok` found, `Err` where it would go), and
/// whether the path only took rightmost pointers.
type TableSearch = (Vec<Frame>, Node, std::result::Result<usize, usize>, bool);

/// Finds the leaf of the table tree for `rowid`: the path, the leaf, and the position of the row (`Ok`) or
/// where it would go (`Err`). The last flag says the path only ever took the rightmost pointer.
fn table_find(pager: &mut Pager, root: u32, rowid: i64) -> Result<TableSearch> {
    let mut path = Vec::new();
    let mut number = root;
    let mut rightmost = true;
    for _ in 0..=MAX_DEPTH {
        let raw = load_raw(pager, number)?;
        match raw.kind {
            Kind::TableInterior => {
                let slot = first_where(raw.len(), |i| Ok(table_key(raw.cell(i)) >= rowid))?;
                path.push(Frame { page: number, slot });
                rightmost &= slot == raw.len();
                number = if slot == raw.len() {
                    raw.right
                } else {
                    child(raw.cell(slot))
                };
            }
            Kind::TableLeaf => {
                let at = first_where(raw.len(), |i| {
                    Ok(table_leaf_rowid(raw.cell(i))? as i64 >= rowid)
                })?;
                let found = if at < raw.len() && table_leaf_rowid(raw.cell(at))? as i64 == rowid {
                    Ok(at)
                } else {
                    Err(at)
                };
                path.push(Frame {
                    page: number,
                    slot: 0,
                });
                return Ok((path, raw.node(), found, rightmost));
            }
            _ => return Err(Error::corrupt(number, "an index page in a table tree")),
        }
    }
    Err(Error::corrupt(number, "the tree is too deep (a loop?)"))
}

/// Adds the row `rowid` with `payload` (a record). With `replace`, a row with that rowid is replaced.
pub fn table_insert(
    pager: &mut Pager,
    root: u32,
    rowid: i64,
    payload: &[u8],
    replace: bool,
) -> Result<()> {
    let (path, mut node, found, rightmost) = table_find(pager, root, rowid)?;
    let cell = make_cell(pager, Kind::TableLeaf, rowid, payload)?;
    let at = match found {
        Ok(i) => {
            if !replace {
                free_overflow(pager, Kind::TableLeaf, &cell)?;
                return Err(Error::KeyExists);
            }
            free_overflow(pager, Kind::TableLeaf, &node.cells[i])?;
            node.cells[i] = cell;
            return settle(pager, path, node, false);
        }
        Err(at) => at,
    };
    let append = rightmost && at == node.cells.len();
    node.cells.insert(at, cell);
    settle(pager, path, node, append)
}

/// Removes the row `rowid`; false when there was none.
pub fn table_delete(pager: &mut Pager, root: u32, rowid: i64) -> Result<bool> {
    let (path, mut node, found, _) = table_find(pager, root, rowid)?;
    let Ok(i) = found else {
        return Ok(false);
    };
    let cell = node.cells.remove(i);
    free_overflow(pager, Kind::TableLeaf, &cell)?;
    rebalance(pager, path, node)?;
    Ok(true)
}

/// One more than the largest rowid, 1 for an empty table.
pub fn next_rowid(pager: &mut Pager, root: u32) -> Result<i64> {
    let mut number = root;
    for _ in 0..=MAX_DEPTH {
        let raw = load_raw(pager, number)?;
        match raw.kind {
            Kind::TableInterior => number = raw.right,
            Kind::TableLeaf => {
                if raw.len() == 0 {
                    return Ok(1);
                }
                return (table_leaf_rowid(raw.cell(raw.len() - 1))? as i64)
                    .checked_add(1)
                    .ok_or_else(|| Error::Unsupported("the rowids are used up".into()));
            }
            _ => return Err(Error::corrupt(number, "an index page in a table tree")),
        }
    }
    Err(Error::corrupt(number, "the tree is too deep (a loop?)"))
}

// ---- indexes

/// SQLite's order of storage classes and values (binary collation): NULL, then numbers, then text, then blobs.
#[must_use]
pub fn compare_values(a: &Value, b: &Value) -> Ordering {
    const fn class(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Integer(_) | Value::Real(_) => 1,
            Value::Text(_) => 2,
            Value::Blob(_) => 3,
        }
    }
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
        (Value::Integer(x), Value::Real(y)) => (*x as f64).total_cmp(y),
        (Value::Real(x), Value::Integer(y)) => x.total_cmp(&(*y as f64)),
        (Value::Real(x), Value::Real(y)) => x.total_cmp(y),
        (Value::Text(x), Value::Text(y)) => x.as_bytes().cmp(y.as_bytes()),
        (Value::Blob(x), Value::Blob(y)) => x.cmp(y),
        _ => class(a).cmp(&class(b)),
    }
}

/// Entries compare value by value; an entry that is a prefix of another is the smaller.
#[must_use]
pub fn compare_entries(a: &[Value], b: &[Value]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = compare_values(x, y);
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

fn entry_of(pager: &mut Pager, kind: Kind, cell: &[u8]) -> Result<Vec<Value>> {
    record::decode(&cell_payload(pager, kind, cell)?)
}

/// Where an index entry is: the path to the page, and the cell (`Ok`) or the place it would go (`Err`).
fn index_find(
    pager: &mut Pager,
    root: u32,
    key: &[Value],
) -> Result<(Vec<Frame>, Node, std::result::Result<usize, usize>)> {
    let mut path = Vec::new();
    let mut number = root;
    for _ in 0..=MAX_DEPTH {
        let raw = load_raw(pager, number)?;
        if !raw.kind.is_index() {
            return Err(Error::corrupt(number, "a table page in an index tree"));
        }
        let kind = raw.kind;
        let at = first_where(raw.len(), |i| {
            Ok(compare_entries(&entry_of(pager, kind, raw.cell(i))?, key) != Ordering::Less)
        })?;
        if at < raw.len()
            && compare_entries(&entry_of(pager, kind, raw.cell(at))?, key) == Ordering::Equal
        {
            path.push(Frame {
                page: number,
                slot: at,
            });
            return Ok((path, raw.node(), Ok(at)));
        }
        path.push(Frame {
            page: number,
            slot: at,
        });
        if kind == Kind::IndexLeaf {
            return Ok((path, raw.node(), Err(at)));
        }
        number = if at == raw.len() {
            raw.right
        } else {
            child(raw.cell(at))
        };
    }
    Err(Error::corrupt(number, "the tree is too deep (a loop?)"))
}

/// Adds an entry (the indexed values and then the rowid) to an index.
pub fn index_insert(pager: &mut Pager, root: u32, key: &[Value]) -> Result<()> {
    let (path, mut node, found) = index_find(pager, root, key)?;
    let Err(at) = found else {
        return Err(Error::KeyExists);
    };
    let cell = make_cell(pager, Kind::IndexLeaf, 0, &record::encode(key))?;
    node.cells.insert(at, cell);
    settle(pager, path, node, false)
}

/// Removes an entry; false when it was not there.
pub fn index_delete(pager: &mut Pager, root: u32, key: &[Value]) -> Result<bool> {
    let (path, mut node, found) = index_find(pager, root, key)?;
    let Ok(i) = found else {
        return Ok(false);
    };
    if node.kind == Kind::IndexLeaf {
        let cell = node.cells.remove(i);
        free_overflow(pager, Kind::IndexLeaf, &cell)?;
        rebalance(pager, path, node)?;
        return Ok(true);
    }
    // An entry of an interior page is replaced by the one before it, the last of the subtree to its left.
    let mut number = child(&node.cells[i]);
    let predecessor = loop {
        let below = load(pager, number)?;
        if below.kind == Kind::IndexLeaf {
            break below
                .cells
                .last()
                .cloned()
                .ok_or_else(|| internal(number, "an empty leaf"))?;
        }
        number = below.right;
    };
    let replacement = divider_cell(child(&node.cells[i]), &predecessor);
    let old = std::mem::replace(&mut node.cells[i], replacement);
    free_overflow(pager, Kind::IndexInterior, &old)?;
    settle(pager, path, node, false)?;
    // The entry now stands twice; the copy in the leaf goes, without its overflow pages (the other copy has them).
    let wanted = entry_of(pager, Kind::IndexLeaf, &predecessor)?;
    remove_duplicate(pager, root, &wanted)?;
    Ok(true)
}

fn remove_duplicate(pager: &mut Pager, root: u32, key: &[Value]) -> Result<()> {
    let mut path = Vec::new();
    let mut number = root;
    // Until the copy in an interior page is found the descent follows the key; after it, the other copy is the
    // last entry of the subtree to the left of it, so every step but the first takes the right pointer.
    let mut found = false;
    for _ in 0..=MAX_DEPTH {
        let mut node = load(pager, number)?;
        if node.kind == Kind::IndexLeaf {
            if !found || node.cells.pop().is_none() {
                return Err(internal(number, "the duplicate entry is missing"));
            }
            path.push(Frame {
                page: number,
                slot: 0,
            });
            return rebalance(pager, path, node);
        }
        let n = node.cells.len();
        let slot = if found {
            n
        } else {
            let (mut lo, mut hi) = (0, n);
            let mut equal = None;
            while lo < hi {
                let mid = usize::midpoint(lo, hi);
                let entry = entry_of(pager, node.kind, &node.cells[mid])?;
                match compare_entries(&entry, key) {
                    Ordering::Less => lo = mid + 1,
                    Ordering::Greater => hi = mid,
                    Ordering::Equal => {
                        equal = Some(mid);
                        break;
                    }
                }
            }
            equal.map_or(lo, |mid| {
                found = true;
                mid
            })
        };
        path.push(Frame { page: number, slot });
        number = if slot == n {
            node.right
        } else {
            child(&node.cells[slot])
        };
    }
    Err(Error::corrupt(number, "the tree is too deep (a loop?)"))
}

// ---- dropping a tree

/// Frees every page of the tree rooted at `root`, the root included, with their overflow pages.
pub fn free_tree(pager: &mut Pager, root: u32) -> Result<()> {
    free_below(pager, root, 0)
}

fn free_below(pager: &mut Pager, number: u32, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::corrupt(number, "the tree is too deep (a loop?)"));
    }
    let node = load(pager, number)?;
    for cell in &node.cells {
        free_overflow(pager, node.kind, cell)?;
    }
    if !node.kind.is_leaf() {
        for cell in &node.cells {
            free_below(pager, child(cell), depth + 1)?;
        }
        free_below(pager, node.right, depth + 1)?;
    }
    pager.free(number)
}

// ---- ranged scans (the reading side of the executor)

/// Calls `f(rowid, payload)` for the rows with `rowid >= from`, in order, until `f` returns false.
pub fn table_scan_from(
    pager: &mut Pager,
    root: u32,
    from: i64,
    f: &mut dyn FnMut(i64, Vec<u8>) -> Result<bool>,
) -> Result<()> {
    table_walk(pager, root, from, f, 0).map(|_| ())
}

fn table_walk(
    pager: &mut Pager,
    number: u32,
    from: i64,
    f: &mut dyn FnMut(i64, Vec<u8>) -> Result<bool>,
    depth: usize,
) -> Result<bool> {
    if depth > MAX_DEPTH {
        return Err(Error::corrupt(number, "the tree is too deep (a loop?)"));
    }
    let raw = load_raw(pager, number)?;
    match raw.kind {
        Kind::TableLeaf => {
            let start = first_where(raw.len(), |i| {
                Ok(table_leaf_rowid(raw.cell(i))? as i64 >= from)
            })?;
            for i in start..raw.len() {
                let rowid = table_leaf_rowid(raw.cell(i))? as i64;
                if !f(rowid, cell_payload(pager, Kind::TableLeaf, raw.cell(i))?)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Kind::TableInterior => {
            let start = first_where(raw.len(), |i| Ok(table_key(raw.cell(i)) >= from))?;
            for i in start..raw.len() {
                if !table_walk(pager, child(raw.cell(i)), from, f, depth + 1)? {
                    return Ok(false);
                }
            }
            table_walk(pager, raw.right, from, f, depth + 1)
        }
        _ => Err(Error::corrupt(number, "an index page in a table tree")),
    }
}

/// Calls `f(entry)` for the entries not less than `lower`, in order, until `f` returns false. `lower` may be a
/// prefix of an entry: it sorts before every entry that starts with it.
pub fn index_scan_from(
    pager: &mut Pager,
    root: u32,
    lower: &[Value],
    f: &mut dyn FnMut(Vec<Value>) -> Result<bool>,
) -> Result<()> {
    index_walk(pager, root, lower, f, 0).map(|_| ())
}

fn index_walk(
    pager: &mut Pager,
    number: u32,
    lower: &[Value],
    f: &mut dyn FnMut(Vec<Value>) -> Result<bool>,
    depth: usize,
) -> Result<bool> {
    if depth > MAX_DEPTH {
        return Err(Error::corrupt(number, "the tree is too deep (a loop?)"));
    }
    let raw = load_raw(pager, number)?;
    if !raw.kind.is_index() {
        return Err(Error::corrupt(number, "a table page in an index tree"));
    }
    let kind = raw.kind;
    // Entries before the first one not less than `lower` (and the subtrees left of them) are all less.
    let start = first_where(raw.len(), |i| {
        Ok(compare_entries(&entry_of(pager, kind, raw.cell(i))?, lower) != Ordering::Less)
    })?;
    for i in start..raw.len() {
        let entry = entry_of(pager, kind, raw.cell(i))?;
        if kind == Kind::IndexInterior
            && !index_walk(pager, child(raw.cell(i)), lower, f, depth + 1)?
        {
            return Ok(false);
        }
        if !f(entry)? {
            return Ok(false);
        }
    }
    if kind == Kind::IndexInterior {
        return index_walk(pager, raw.right, lower, f, depth + 1);
    }
    Ok(true)
}

// ---- integrity checking

/// Where the checker records what it finds.
pub struct Findings {
    /// Who owns each page (`owner[n - 1]`), to find pages used twice or not at all.
    pub owner: Vec<Option<String>>,
    pub problems: Vec<String>,
}

impl Findings {
    #[must_use]
    pub fn new(pages: u32) -> Self {
        Self {
            owner: vec![None; pages as usize],
            problems: Vec::new(),
        }
    }

    fn problem(&mut self, text: String) {
        if self.problems.len() < 100 {
            self.problems.push(text);
        }
    }

    pub fn claim(&mut self, page: u32, by: &str) {
        match self.owner.get_mut((page as usize).wrapping_sub(1)) {
            None => self.problem(format!("Page {page}: out of range (used by {by})")),
            Some(slot @ None) => *slot = Some(by.to_string()),
            Some(Some(other)) => {
                let other = other.clone();
                self.problem(format!("Page {page}: used by {by} and by {other}"));
            }
        }
    }
}

/// What a tree holds, in order.
#[derive(Debug, Default)]
pub struct TreeContents {
    /// A table's rows: `(rowid, payload)`.
    pub rows: Vec<(i64, Vec<u8>)>,
    /// An index's entries.
    pub entries: Vec<Vec<Value>>,
}

struct Checker<'a> {
    by: &'a str,
    leaf_depth: Option<usize>,
    out: TreeContents,
}

/// Walks the tree at `root`, claiming its pages and recording what is wrong with it.
pub fn check_tree(
    pager: &mut Pager,
    root: u32,
    tree: Tree,
    by: &str,
    found: &mut Findings,
) -> TreeContents {
    let mut c = Checker {
        by,
        leaf_depth: None,
        out: TreeContents::default(),
    };
    match tree {
        Tree::Table => c.table(pager, root, 0, (None, None), found, true),
        Tree::Index => c.index(pager, root, 0, (None, None), found, true),
    }
    c.out
}

type IndexBounds = (Option<Vec<Value>>, Option<Vec<Value>>);

impl Checker<'_> {
    /// Loads a page and checks its layout; `None` when it cannot be read (the problem is recorded).
    fn page(
        &self,
        pager: &mut Pager,
        number: u32,
        depth: usize,
        found: &mut Findings,
        is_root: bool,
    ) -> Option<Node> {
        found.claim(number, self.by);
        if depth > MAX_DEPTH {
            found.problem(format!("Page {number}: the tree is too deep (a loop?)"));
            return None;
        }
        let node = match load(pager, number) {
            Ok(n) => n,
            Err(e) => {
                found.problem(format!("Page {number}: {e}"));
                return None;
            }
        };
        if node.cells.is_empty() && !is_root {
            found.problem(format!("Page {number}: a non-root page without cells"));
        }
        // The cells must lie in the content area, without overlapping.
        if let Ok(bytes) = pager.page(number)
            && let Ok(header) = PageHeader::parse(number, &bytes)
        {
            let usable = pager.usable_size();
            let start = base_of(number) + header_len(header.kind) + 2 * header.cell_count;
            let mut spans: Vec<(usize, usize)> = Vec::new();
            for i in 0..header.cell_count {
                if let Ok(at) = header.cell_offset(number, &bytes, i)
                    && let Ok(len) = cell_len(header.kind, usable, &bytes, at, number)
                {
                    if at < start || at + len > usable {
                        found.problem(format!(
                            "Page {number}: cell {i} lies outside the content area"
                        ));
                    }
                    spans.push((at, at + len));
                }
            }
            spans.sort_unstable();
            if spans.windows(2).any(|w| w[0].1 > w[1].0) {
                found.problem(format!("Page {number}: cells overlap"));
            }
        }
        Some(node)
    }

    fn overflow(
        &self,
        pager: &mut Pager,
        kind: Kind,
        cell: &[u8],
        page: u32,
        found: &mut Findings,
    ) {
        let usable = pager.usable_size();
        let Ok((at, total, local)) = payload_parts(kind, usable, cell) else {
            return;
        };
        if local >= total {
            return;
        }
        let mut need = total - local;
        let mut next = be32(cell, at + local);
        let mut hops = 0;
        while need > 0 {
            hops += 1;
            if next == 0 {
                found.problem(format!(
                    "Page {page}: an overflow chain ends {need} bytes early"
                ));
                return;
            }
            if hops > pager.page_count() {
                found.problem(format!("Page {page}: an overflow chain loops"));
                return;
            }
            found.claim(next, self.by);
            let Ok(p) = pager.page(next) else {
                found.problem(format!("Page {next}: an overflow page that cannot be read"));
                return;
            };
            need = need.saturating_sub(usable - 4);
            next = be32(&p, 0);
        }
        if next != 0 {
            found.problem(format!(
                "Page {page}: an overflow chain is longer than the payload"
            ));
        }
    }

    fn leaf(&mut self, depth: usize, number: u32, found: &mut Findings) {
        match self.leaf_depth {
            None => self.leaf_depth = Some(depth),
            Some(d) if d != depth => found.problem(format!(
                "Page {number}: leaves are not all at the same depth"
            )),
            Some(_) => {}
        }
    }

    fn table(
        &mut self,
        pager: &mut Pager,
        number: u32,
        depth: usize,
        bounds: (Option<i64>, Option<i64>),
        found: &mut Findings,
        is_root: bool,
    ) {
        let Some(node) = self.page(pager, number, depth, found, is_root) else {
            return;
        };
        let (low, high) = bounds;
        match node.kind {
            Kind::TableLeaf => {
                self.leaf(depth, number, found);
                let mut prev = low;
                for cell in &node.cells {
                    let Ok(rowid) = table_leaf_rowid(cell).map(|r| r as i64) else {
                        found.problem(format!("Page {number}: a malformed cell"));
                        continue;
                    };
                    if prev.is_some_and(|p| rowid <= p) || high.is_some_and(|h| rowid > h) {
                        found.problem(format!(
                            "Page {number}: rowid {rowid} is out of order or out of its bounds"
                        ));
                    }
                    prev = Some(rowid);
                    self.overflow(pager, Kind::TableLeaf, cell, number, found);
                    match cell_payload(pager, Kind::TableLeaf, cell) {
                        Ok(payload) => self.out.rows.push((rowid, payload)),
                        Err(e) => found.problem(format!("Page {number}: row {rowid}: {e}")),
                    }
                }
            }
            Kind::TableInterior => {
                let mut prev = low;
                for cell in &node.cells {
                    let key = varint::get(&cell[4..]).map_or(0, |(k, _)| k as i64);
                    if prev.is_some_and(|p| key <= p) || high.is_some_and(|h| key > h) {
                        found.problem(format!(
                            "Page {number}: key {key} is out of order or out of its bounds"
                        ));
                    }
                    self.table(
                        pager,
                        child(cell),
                        depth + 1,
                        (prev, Some(key)),
                        found,
                        false,
                    );
                    prev = Some(key);
                }
                self.table(pager, node.right, depth + 1, (prev, high), found, false);
            }
            _ => found.problem(format!("Page {number}: an index page in a table tree")),
        }
    }

    fn index(
        &mut self,
        pager: &mut Pager,
        number: u32,
        depth: usize,
        bounds: IndexBounds,
        found: &mut Findings,
        is_root: bool,
    ) {
        let Some(node) = self.page(pager, number, depth, found, is_root) else {
            return;
        };
        if !node.kind.is_index() {
            found.problem(format!("Page {number}: a table page in an index tree"));
            return;
        }
        let (low, high) = bounds;
        let mut prev = low;
        for cell in &node.cells {
            self.overflow(pager, node.kind, cell, number, found);
            let entry = match entry_of(pager, node.kind, cell) {
                Ok(e) => e,
                Err(e) => {
                    found.problem(format!("Page {number}: {e}"));
                    continue;
                }
            };
            if prev
                .as_ref()
                .is_some_and(|p| compare_entries(&entry, p) != Ordering::Greater)
                || high
                    .as_ref()
                    .is_some_and(|h| compare_entries(&entry, h) != Ordering::Less)
            {
                found.problem(format!(
                    "Page {number}: an index entry is out of order or out of its bounds"
                ));
            }
            if node.kind == Kind::IndexInterior {
                self.index(
                    pager,
                    child(cell),
                    depth + 1,
                    (prev.clone(), Some(entry.clone())),
                    found,
                    false,
                );
            } else {
                self.leaf(depth, number, found);
            }
            self.out.entries.push(entry.clone());
            prev = Some(entry);
        }
        if node.kind == Kind::IndexInterior {
            self.index(pager, node.right, depth + 1, (prev, high), found, false);
        } else if node.cells.is_empty() {
            self.leaf(depth, number, found);
        }
    }
}
