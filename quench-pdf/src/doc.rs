//! The file structure: cross-reference data, objects, streams and the page tree.

use crate::error::{Error, Result};
use crate::filter;
use crate::object::{Dict, Object, Parser, Stream};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Clone, Copy)]
enum Entry {
    Offset(usize),
    InStream { stream: u32, index: u32 },
}

/// A page: its dictionary, with the inheritable entries filled in from its ancestors.
#[derive(Debug, Clone)]
pub struct Page {
    pub dict: Dict,
}

/// A parsed PDF file.
#[derive(Debug)]
pub struct Document {
    data: Vec<u8>,
    xref: HashMap<u32, Entry>,
    trailer: Dict,
    cache: RefCell<HashMap<u32, Object>>,
    object_streams: RefCell<HashMap<u32, Rc<Vec<(u32, Object)>>>>,
}

const MAX_RESOLVE: usize = 32;

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn find_from(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

impl Document {
    /// Reads the structure of a PDF.
    ///
    /// # Errors
    ///
    /// [`Error::Malformed`] when the data is not a PDF or no catalog can be found,
    /// [`Error::Unsupported`] for encrypted files.
    pub fn load(data: &[u8]) -> Result<Self> {
        let header = find_from(&data[..data.len().min(4096)], 0, b"%PDF-")
            .ok_or_else(|| Error::Malformed("not a PDF: no %PDF- header".into()))?;
        let mut doc = Self {
            data: data[header..].to_vec(),
            xref: HashMap::new(),
            trailer: Dict::new(),
            cache: RefCell::new(HashMap::new()),
            object_streams: RefCell::new(HashMap::new()),
        };
        if doc.read_xref().is_err() || doc.catalog().is_none() {
            doc.rebuild()?;
        }
        if doc.trailer.contains_key("Encrypt") {
            return Err(Error::Unsupported("encrypted PDF".into()));
        }
        if doc.catalog().is_none() {
            return Err(Error::Malformed("no document catalog".into()));
        }
        Ok(doc)
    }

    fn catalog(&self) -> Option<Dict> {
        match self.get(&self.trailer, "Root") {
            Some(Object::Dict(d)) if d.contains_key("Pages") => Some(d),
            _ => None,
        }
    }

    // ------------------------------------------------------------ xref

    fn read_xref(&mut self) -> Result<()> {
        let tail_start = self.data.len().saturating_sub(4096);
        let at = find_last(&self.data[tail_start..], b"startxref")
            .map(|i| i + tail_start)
            .ok_or_else(|| Error::Malformed("no startxref".into()))?;
        let mut parser = Parser::new(&self.data, at + 9, true);
        let Some(Object::Int(first)) = parser.parse()? else {
            return Err(Error::Malformed("bad startxref".into()));
        };
        let mut next =
            Some(usize::try_from(first).map_err(|_| Error::Malformed("bad startxref".into()))?);
        let mut seen = HashSet::new();
        let mut first_trailer = true;
        while let Some(offset) = next.take() {
            if !seen.insert(offset) || offset >= self.data.len() {
                break;
            }
            let (trailer, entries) = self.xref_section(offset)?;
            for (num, entry) in entries {
                self.xref.entry(num).or_insert(entry);
            }
            if let Some(Object::Int(stm)) = trailer.get("XRefStm")
                && let Ok(stm) = usize::try_from(*stm)
                && seen.insert(stm)
                && let Ok((_, extra)) = self.xref_section(stm)
            {
                for (num, entry) in extra {
                    self.xref.entry(num).or_insert(entry);
                }
            }
            next = trailer
                .get("Prev")
                .and_then(Object::as_int)
                .and_then(|p| usize::try_from(p).ok());
            if first_trailer {
                self.trailer = trailer;
                first_trailer = false;
            } else {
                for (k, v) in trailer {
                    self.trailer.entry(k).or_insert(v);
                }
            }
        }
        if self.trailer.is_empty() {
            return Err(Error::Malformed("no trailer".into()));
        }
        Ok(())
    }

    fn xref_section(&self, offset: usize) -> Result<(Dict, Vec<(u32, Entry)>)> {
        let mut parser = Parser::new(&self.data, offset, true);
        parser.skip_space();
        if self.data[parser.pos..].starts_with(b"xref") {
            parser.pos += 4;
            let mut entries = Vec::new();
            loop {
                parser.skip_space();
                if self.data[parser.pos..].starts_with(b"trailer") {
                    parser.pos += 7;
                    break;
                }
                let (Some(Object::Int(start)), Some(Object::Int(count))) =
                    (parser.parse()?, parser.parse()?)
                else {
                    return Err(Error::Malformed("bad xref subsection".into()));
                };
                for i in 0..count {
                    let (
                        Some(Object::Int(position)),
                        Some(Object::Int(_generation)),
                        Some(Object::Op(kind)),
                    ) = (parser.parse()?, parser.parse()?, parser.parse()?)
                    else {
                        return Err(Error::Malformed("bad xref entry".into()));
                    };
                    if kind == "n" && position > 0 {
                        entries.push((
                            u32::try_from(start + i).unwrap_or(0),
                            Entry::Offset(usize::try_from(position).unwrap_or(0)),
                        ));
                    }
                }
            }
            let Some(Object::Dict(trailer)) = parser.parse()? else {
                return Err(Error::Malformed("bad trailer".into()));
            };
            return Ok((trailer, entries));
        }

        // A cross-reference stream is an indirect object.
        let (_, object) = self.object_at(offset, None)?;
        let Object::Stream(stream) = object else {
            return Err(Error::Malformed(
                "xref is neither a table nor a stream".into(),
            ));
        };
        let data = self.decode_stream(&stream)?;
        let widths: Vec<usize> = stream
            .dict
            .get("W")
            .and_then(Object::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Object::as_int)
                    .map(|n| usize::try_from(n).unwrap_or(0))
                    .collect()
            })
            .unwrap_or_default();
        if widths.len() != 3 {
            return Err(Error::Malformed("bad xref stream /W".into()));
        }
        let size = stream
            .dict
            .get("Size")
            .and_then(Object::as_int)
            .unwrap_or(0);
        let index: Vec<i64> = match stream.dict.get("Index") {
            Some(Object::Array(a)) => a.iter().filter_map(Object::as_int).collect(),
            _ => vec![0, size],
        };
        let row = widths.iter().sum::<usize>();
        let mut entries = Vec::new();
        let mut at = 0;
        for pair in index.as_chunks::<2>().0 {
            for i in 0..pair[1] {
                let Some(bytes) = data.get(at..at + row) else {
                    break;
                };
                at += row;
                let field = |from: usize, len: usize, default: u64| {
                    if len == 0 {
                        default
                    } else {
                        bytes[from..from + len]
                            .iter()
                            .fold(0u64, |acc, &b| acc << 8 | u64::from(b))
                    }
                };
                let kind = field(0, widths[0], 1);
                let second = field(widths[0], widths[1], 0);
                let third = field(widths[0] + widths[1], widths[2], 0);
                let num = u32::try_from(pair[0] + i).unwrap_or(0);
                match kind {
                    1 => entries.push((num, Entry::Offset(second as usize))),
                    2 => entries.push((
                        num,
                        Entry::InStream {
                            stream: second as u32,
                            index: third as u32,
                        },
                    )),
                    _ => {}
                }
            }
        }
        Ok((stream.dict, entries))
    }

    /// Finds every `n g obj` in the file, for files whose cross-reference data is missing or wrong.
    fn rebuild(&mut self) -> Result<()> {
        self.xref.clear();
        self.cache.borrow_mut().clear();
        self.object_streams.borrow_mut().clear();
        let mut trailer = Dict::new();
        let mut at = 0;
        while let Some(i) = find_from(&self.data, at, b"obj") {
            at = i + 3;
            if i >= 3 && self.data[i - 3..i] == *b"end" {
                continue;
            }
            // Walk back over `n g `.
            let mut j = i;
            let skip_back = |mut j: usize, f: &dyn Fn(u8) -> bool| {
                while j > 0 && f(self.data[j - 1]) {
                    j -= 1;
                }
                j
            };
            j = skip_back(j, &|b| b.is_ascii_whitespace());
            let gen_end = j;
            j = skip_back(j, &|b| b.is_ascii_digit());
            if j == gen_end {
                continue;
            }
            j = skip_back(j, &|b| b.is_ascii_whitespace());
            let num_end = j;
            j = skip_back(j, &|b| b.is_ascii_digit());
            if j == num_end {
                continue;
            }
            if let Some(num) = std::str::from_utf8(&self.data[j..num_end])
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
            {
                self.xref.insert(num, Entry::Offset(j));
            }
        }
        // Trailer dictionaries, and cross-reference streams (which carry the same keys).
        let mut at = 0;
        while let Some(i) = find_from(&self.data, at, b"trailer") {
            at = i + 7;
            let mut parser = Parser::new(&self.data, at, true);
            if let Ok(Some(Object::Dict(d))) = parser.parse() {
                for (k, v) in d {
                    trailer.insert(k, v);
                }
            }
        }
        let nums: Vec<u32> = self.xref.keys().copied().collect();
        for num in nums {
            let Ok(Object::Stream(s)) = self.get_object(num) else {
                continue;
            };
            match s.dict.get("Type").and_then(Object::as_name) {
                Some("XRef") => {
                    for (k, v) in &s.dict {
                        trailer.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                Some("ObjStm") => {
                    if let Ok(contents) = self.object_stream(num) {
                        for (index, (inner, _)) in contents.iter().enumerate() {
                            self.xref.entry(*inner).or_insert(Entry::InStream {
                                stream: num,
                                index: index as u32,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        if !trailer.contains_key("Root") {
            let catalog = self.xref.keys().copied().find(|&n| {
                matches!(self.get_object(n), Ok(Object::Dict(d)) if d.get("Type").and_then(Object::as_name) == Some("Catalog"))
            });
            if let Some(n) = catalog {
                trailer.insert("Root".into(), Object::Ref(n, 0));
            }
        }
        self.trailer = trailer;
        if self.xref.is_empty() {
            return Err(Error::Malformed("no objects found".into()));
        }
        Ok(())
    }

    // ------------------------------------------------------------ objects

    /// Parses the indirect object at `offset`; `expect` checks its number.
    fn object_at(&self, offset: usize, expect: Option<u32>) -> Result<(u32, Object)> {
        let mut parser = Parser::new(&self.data, offset, false);
        let (Some(Object::Int(num)), Some(Object::Int(_)), Some(Object::Op(word))) =
            (parser.parse()?, parser.parse()?, parser.parse()?)
        else {
            return Err(Error::Malformed(format!("no object at offset {offset}")));
        };
        if word != "obj" {
            return Err(Error::Malformed(format!("no object at offset {offset}")));
        }
        let num = u32::try_from(num).map_err(|_| Error::Malformed("bad object number".into()))?;
        if expect.is_some_and(|e| e != num) {
            return Err(Error::Malformed(format!(
                "object {num} found where {expect:?} was expected"
            )));
        }
        parser.refs_on();
        let Some(object) = parser.parse()? else {
            return Ok((num, Object::Null));
        };
        let Object::Dict(dict) = object else {
            return Ok((num, object));
        };
        parser.skip_space();
        if !self.data[parser.pos..].starts_with(b"stream") {
            return Ok((num, Object::Dict(dict)));
        }
        let mut start = parser.pos + 6;
        if self.data.get(start) == Some(&b'\r') {
            start += 1;
        }
        if self.data.get(start) == Some(&b'\n') {
            start += 1;
        }
        let declared = match dict.get("Length") {
            Some(Object::Int(n)) => usize::try_from(*n).ok(),
            Some(Object::Ref(n, _)) if expect != Some(*n) => self
                .get_object(*n)
                .ok()
                .and_then(|o| o.as_int())
                .and_then(|n| usize::try_from(n).ok()),
            _ => None,
        };
        let end = match declared {
            Some(len)
                if start + len <= self.data.len()
                    && self.data[start + len..]
                        .iter()
                        .skip_while(|b| b.is_ascii_whitespace())
                        .take(9)
                        .copied()
                        .eq(*b"endstream") =>
            {
                start + len
            }
            _ => {
                // The length is missing or wrong: the stream runs to the next `endstream`.
                let found = find_from(&self.data, start, b"endstream")
                    .ok_or_else(|| Error::Malformed("unterminated stream".into()))?;
                let mut end = found;
                if end > start && self.data[end - 1] == b'\n' {
                    end -= 1;
                }
                if end > start && self.data[end - 1] == b'\r' {
                    end -= 1;
                }
                end
            }
        };
        Ok((
            num,
            Object::Stream(Box::new(Stream {
                dict,
                raw: self.data[start..end].to_vec(),
            })),
        ))
    }

    /// Object `num`, parsed (references inside it are left as they are).
    pub fn get_object(&self, num: u32) -> Result<Object> {
        if let Some(o) = self.cache.borrow().get(&num) {
            return Ok(o.clone());
        }
        let object = match self.xref.get(&num).copied() {
            Some(Entry::Offset(offset)) => self.object_at(offset, Some(num))?.1,
            Some(Entry::InStream { stream, index }) => {
                let contents = self.object_stream(stream)?;
                contents
                    .get(index as usize)
                    .filter(|(n, _)| *n == num)
                    .or_else(|| contents.iter().find(|(n, _)| *n == num))
                    .map(|(_, o)| o.clone())
                    .ok_or_else(|| {
                        Error::Malformed(format!("object {num} missing from its stream"))
                    })?
            }
            None => Object::Null,
        };
        self.cache.borrow_mut().insert(num, object.clone());
        Ok(object)
    }

    fn object_stream(&self, num: u32) -> Result<Rc<Vec<(u32, Object)>>> {
        if let Some(c) = self.object_streams.borrow().get(&num) {
            return Ok(Rc::clone(c));
        }
        let Object::Stream(stream) = self.get_object(num)? else {
            return Err(Error::Malformed("object stream is not a stream".into()));
        };
        let data = self.decode_stream(&stream)?;
        let count = stream.dict.get("N").and_then(Object::as_int).unwrap_or(0);
        let first = stream
            .dict
            .get("First")
            .and_then(Object::as_int)
            .unwrap_or(0);
        let first = usize::try_from(first).map_err(|_| Error::Malformed("bad /First".into()))?;
        let mut header = Parser::new(&data, 0, false);
        let mut offsets = Vec::new();
        for _ in 0..count {
            let (Some(Object::Int(n)), Some(Object::Int(o))) = (header.parse()?, header.parse()?)
            else {
                break;
            };
            offsets.push((
                u32::try_from(n).unwrap_or(0),
                usize::try_from(o).unwrap_or(0),
            ));
        }
        let mut out = Vec::new();
        for (n, offset) in offsets {
            let mut parser = Parser::new(&data, first + offset, true);
            out.push((n, parser.parse()?.unwrap_or(Object::Null)));
        }
        let out = Rc::new(out);
        self.object_streams
            .borrow_mut()
            .insert(num, Rc::clone(&out));
        Ok(out)
    }

    /// Follows references until a direct object (a missing one is `Null`).
    #[must_use]
    pub fn resolve(&self, object: &Object) -> Object {
        let mut current = object.clone();
        for _ in 0..MAX_RESOLVE {
            match current {
                Object::Ref(n, _) => current = self.get_object(n).unwrap_or(Object::Null),
                other => return other,
            }
        }
        Object::Null
    }

    /// `dict[key]`, resolved.
    #[must_use]
    pub fn get(&self, dict: &Dict, key: &str) -> Option<Object> {
        match self.resolve(dict.get(key)?) {
            Object::Null => None,
            other => Some(other),
        }
    }

    fn decode_stream(&self, stream: &Stream) -> Result<Vec<u8>> {
        let filters: Vec<String> = match self
            .get(&stream.dict, "Filter")
            .or_else(|| self.get(&stream.dict, "F"))
        {
            Some(Object::Name(n)) => vec![n],
            Some(Object::Array(a)) => a
                .iter()
                .filter_map(|o| self.resolve(o).as_name().map(String::from))
                .collect(),
            _ => Vec::new(),
        };
        let params: Vec<Option<Dict>> = match self
            .get(&stream.dict, "DecodeParms")
            .or_else(|| self.get(&stream.dict, "DP"))
        {
            Some(Object::Dict(d)) => vec![Some(d)],
            Some(Object::Array(a)) => a
                .iter()
                .map(|o| match self.resolve(o) {
                    Object::Dict(d) => Some(d),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut data = stream.raw.clone();
        for (i, name) in filters.iter().enumerate() {
            data = filter::apply(name, &data, params.get(i).and_then(Option::as_ref))?;
        }
        Ok(data)
    }

    /// The decoded bytes of a stream object (or a reference to one).
    #[must_use]
    pub fn stream_data(&self, object: &Object) -> Option<Vec<u8>> {
        match self.resolve(object) {
            Object::Stream(s) => self.decode_stream(&s).ok(),
            _ => None,
        }
    }

    // ------------------------------------------------------------ pages

    /// The pages in order, with inheritable entries (`Resources`, ...) copied down.
    #[must_use]
    pub fn pages(&self) -> Vec<Page> {
        let mut out = Vec::new();
        let Some(catalog) = self.catalog() else {
            return out;
        };
        let Some(root) = catalog.get("Pages") else {
            return out;
        };
        let mut visited = HashSet::new();
        self.collect_pages(root, &Dict::new(), &mut visited, &mut out, 0);
        out
    }

    fn collect_pages(
        &self,
        node: &Object,
        inherited: &Dict,
        visited: &mut HashSet<u32>,
        out: &mut Vec<Page>,
        depth: usize,
    ) {
        if depth > 64 {
            return;
        }
        if let Object::Ref(n, _) = node
            && !visited.insert(*n)
        {
            return;
        }
        let Object::Dict(dict) = self.resolve(node) else {
            return;
        };
        let mut inherit = inherited.clone();
        for key in ["Resources", "MediaBox", "Rotate"] {
            if let Some(v) = dict.get(key) {
                inherit.insert(key.to_string(), v.clone());
            }
        }
        if let Some(Object::Array(kids)) = self.get(&dict, "Kids") {
            for kid in &kids {
                self.collect_pages(kid, &inherit, visited, out, depth + 1);
            }
        } else {
            let mut page = dict;
            for (k, v) in inherit {
                page.entry(k).or_insert(v);
            }
            out.push(Page { dict: page });
        }
    }

    /// A page's content streams, joined.
    #[must_use]
    pub fn page_contents(&self, page: &Page) -> Vec<u8> {
        let mut out = Vec::new();
        let parts: Vec<Object> = match page.dict.get("Contents") {
            Some(o) => match self.resolve(o) {
                Object::Array(a) => a,
                other => vec![other],
            },
            None => Vec::new(),
        };
        for part in &parts {
            if let Some(data) = self.stream_data(part) {
                out.extend_from_slice(&data);
                out.push(b'\n');
            }
        }
        out
    }
}
