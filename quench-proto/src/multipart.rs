//! A streaming `multipart/form-data` reader (RFC 7578 on top of RFC 2046).
//!
//! The reader pulls chunks from any byte stream and hands out one field at a
//! time. Field bodies are streamed, not buffered, unless the caller asks for
//! [`Field::bytes`] or [`Field::text`]; only a delimiter-length tail of the
//! stream is held back while looking for the next boundary.

use crate::body::BoxError;
use crate::header::{HeaderMap, HeaderName, HeaderValue};
use quench_core::bytes::Bytes;
use quench_core::stream::{Stream, StreamExt};
use std::fmt;

/// Size and count limits.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes read from the stream in total, boundaries and headers included.
    pub whole_stream: Option<u64>,
    /// Body bytes of a single field.
    pub per_field: Option<u64>,
    /// Header block of one part.
    pub max_header_bytes: usize,
    pub max_fields: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            whole_stream: None,
            per_field: None,
            max_header_bytes: 16 * 1024,
            max_fields: 1024,
        }
    }
}

/// Why reading a multipart body failed.
#[derive(Debug)]
pub enum Error {
    /// The content type is not `multipart/form-data` with a boundary.
    MissingBoundary,
    InvalidBoundary,
    /// The stream ended before the closing delimiter.
    Truncated,
    /// Bytes where a delimiter line or part header was expected did not fit the grammar.
    Malformed(&'static str),
    HeadersTooLarge,
    TooManyFields,
    StreamTooLarge(u64),
    FieldTooLarge(u64),
    /// A text field was not UTF-8.
    NotUtf8,
    /// The underlying stream failed.
    Stream(BoxError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBoundary => f.write_str("missing or invalid multipart boundary"),
            Self::InvalidBoundary => f.write_str("invalid multipart boundary"),
            Self::Truncated => f.write_str("multipart body ended before its closing boundary"),
            Self::Malformed(what) => write!(f, "malformed multipart body: {what}"),
            Self::HeadersTooLarge => f.write_str("multipart part headers too large"),
            Self::TooManyFields => f.write_str("too many multipart fields"),
            Self::StreamTooLarge(limit) => write!(f, "multipart body exceeds {limit} bytes"),
            Self::FieldTooLarge(limit) => write!(f, "multipart field exceeds {limit} bytes"),
            Self::NotUtf8 => f.write_str("multipart text field is not valid UTF-8"),
            Self::Stream(err) => write!(f, "failed to read multipart body: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Stream(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// Extracts the boundary from a `Content-Type` value.
///
/// # Errors
///
/// [`Error::MissingBoundary`] unless the type is `multipart/form-data` with a
/// `boundary` parameter; [`Error::InvalidBoundary`] when that is empty, longer
/// than 70 bytes or holds characters RFC 2046 does not allow.
pub fn parse_boundary(content_type: &str) -> Result<String, Error> {
    let mut parts = split_params(content_type);
    let essence = parts.next().ok_or(Error::MissingBoundary)?.0;
    if !essence.trim().eq_ignore_ascii_case("multipart/form-data") {
        return Err(Error::MissingBoundary);
    }
    let boundary = parts
        .find(|(name, _)| name.eq_ignore_ascii_case("boundary"))
        .ok_or(Error::MissingBoundary)?
        .1;
    let valid = !boundary.is_empty()
        && boundary.len() <= 70
        && boundary
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&b));
    if valid {
        Ok(boundary)
    } else {
        Err(Error::InvalidBoundary)
    }
}

/// Splits `a/b; name=value; other="quoted \" value"` into `(first, "")` then `(name, value)` pairs.
fn split_params(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    let mut chars = text.chars();
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' => quoted = !quoted,
            ';' if !quoted => segments.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    segments.push(current);
    segments.into_iter().enumerate().map(|(i, segment)| {
        if i == 0 {
            return (segment.trim().to_string(), String::new());
        }
        match segment.split_once('=') {
            Some((name, value)) => (name.trim().to_string(), value.trim().to_string()),
            None => (segment.trim().to_string(), String::new()),
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Looking for the first delimiter, skipping any preamble.
    Preamble,
    /// A delimiter was just consumed.
    AfterDelimiter,
    /// Between a part's header block and its body: inside a field.
    Body,
    Done,
}

/// The reader.
pub struct Multipart<S> {
    stream: S,
    delimiter: Vec<u8>,
    buf: Vec<u8>,
    pos: usize,
    state: State,
    limits: Limits,
    consumed: u64,
    fields_seen: usize,
    stream_ended: bool,
}

impl<S> fmt::Debug for Multipart<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Multipart")
            .field("state", &self.state)
            .field("fields", &self.fields_seen)
            .finish_non_exhaustive()
    }
}

impl<S, E> Multipart<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: Into<BoxError>,
{
    /// A reader for `stream`, whose parts are separated by `boundary` (see [`parse_boundary`]).
    pub fn new(stream: S, boundary: &str, limits: Limits) -> Self {
        let mut delimiter = b"\r\n--".to_vec();
        delimiter.extend_from_slice(boundary.as_bytes());
        // The first delimiter has no CRLF before it; pretend one is there.
        Self {
            stream,
            delimiter,
            buf: b"\r\n".to_vec(),
            pos: 0,
            state: State::Preamble,
            limits,
            consumed: 0,
            fields_seen: 0,
            stream_ended: false,
        }
    }

    fn available(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
        if self.pos > 64 * 1024 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Pulls one more chunk from the stream into the buffer.
    async fn fill(&mut self) -> Result<(), Error> {
        if self.stream_ended {
            return Err(Error::Truncated);
        }
        match self.stream.next().await {
            Some(Ok(chunk)) => {
                self.consumed += chunk.len() as u64;
                if let Some(limit) = self.limits.whole_stream
                    && self.consumed > limit
                {
                    return Err(Error::StreamTooLarge(limit));
                }
                self.buf.extend_from_slice(&chunk);
                Ok(())
            }
            Some(Err(err)) => Err(Error::Stream(err.into())),
            None => {
                self.stream_ended = true;
                Err(Error::Truncated)
            }
        }
    }

    /// Body bytes up to the next delimiter, or `None` once the delimiter was consumed.
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, Error> {
        loop {
            if let Some(at) = find(self.available(), &self.delimiter) {
                if at > 0 {
                    let chunk = Bytes::copy_from_slice(&self.available()[..at]);
                    self.advance(at);
                    return Ok(Some(chunk));
                }
                self.advance(self.delimiter.len());
                self.state = State::AfterDelimiter;
                return Ok(None);
            }
            // Hold back a delimiter-sized tail: it may be the start of one.
            let safe = self
                .available()
                .len()
                .saturating_sub(self.delimiter.len() - 1);
            if safe > 0 {
                let chunk = Bytes::copy_from_slice(&self.available()[..safe]);
                self.advance(safe);
                return Ok(Some(chunk));
            }
            self.fill().await?;
        }
    }

    /// The next field, or `None` after the closing delimiter. A field that
    /// was not read to its end is skipped.
    ///
    /// # Errors
    ///
    /// Any [`Error`]; after one, the reader should be dropped.
    pub async fn next_field(&mut self) -> Result<Option<Field<'_, S>>, Error> {
        loop {
            match self.state {
                State::Done => return Ok(None),
                State::Preamble | State::Body => {
                    // Discard the preamble or the rest of an unread field.
                    while self.next_chunk().await?.is_some() {}
                }
                State::AfterDelimiter => break,
            }
        }
        // After a delimiter: `--` closes the body, anything else must be padding and a CRLF.
        while self.available().len() < 2 {
            self.fill().await?;
        }
        if self.available().starts_with(b"--") {
            self.state = State::Done;
            return Ok(None);
        }
        loop {
            let line_end = find(self.available(), b"\r\n");
            let Some(end) = line_end else {
                if self.available().len() > 1024 {
                    return Err(Error::Malformed("delimiter line too long"));
                }
                self.fill().await?;
                continue;
            };
            if !self.available()[..end]
                .iter()
                .all(|&b| b == b' ' || b == b'\t')
            {
                return Err(Error::Malformed("unexpected bytes after the boundary"));
            }
            self.advance(end + 2);
            break;
        }
        self.fields_seen += 1;
        if self.fields_seen > self.limits.max_fields {
            return Err(Error::TooManyFields);
        }
        let headers = self.read_part_headers().await?;
        self.state = State::Body;
        let (name, file_name) = disposition(&headers);
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        Ok(Some(Field {
            mp: self,
            name,
            file_name,
            content_type,
            headers,
            read: 0,
        }))
    }

    async fn read_part_headers(&mut self) -> Result<HeaderMap, Error> {
        loop {
            if self.available().starts_with(b"\r\n") {
                self.advance(2);
                return Ok(HeaderMap::new());
            }
            if let Some(end) = find(self.available(), b"\r\n\r\n") {
                if end > self.limits.max_header_bytes {
                    return Err(Error::HeadersTooLarge);
                }
                let block = self.available()[..end].to_vec();
                self.advance(end + 4);
                return parse_part_headers(&block);
            }
            if self.available().len() > self.limits.max_header_bytes {
                return Err(Error::HeadersTooLarge);
            }
            self.fill().await?;
        }
    }
}

fn parse_part_headers(block: &[u8]) -> Result<HeaderMap, Error> {
    let mut headers = HeaderMap::new();
    for line in block.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let colon = line
            .iter()
            .position(|&b| b == b':')
            .ok_or(Error::Malformed("part header without a colon"))?;
        let name = HeaderName::from_bytes(&line[..colon])
            .map_err(|_| Error::Malformed("bad part header name"))?;
        let value = HeaderValue::from_bytes(line[colon + 1..].trim_ascii())
            .map_err(|_| Error::Malformed("bad part header value"))?;
        headers.append(name, value);
    }
    Ok(headers)
}

/// `name` and `filename` from a part's `Content-Disposition`.
fn disposition(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let Some(value) = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
    else {
        return (None, None);
    };
    let mut name = None;
    let mut file_name = None;
    let mut extended = None;
    for (key, value) in split_params(value).skip(1) {
        match key.to_ascii_lowercase().as_str() {
            "name" => name = Some(value),
            "filename" => file_name = Some(value),
            "filename*" => extended = decode_ext_value(&value),
            _ => {}
        }
    }
    (name, extended.or(file_name))
}

/// RFC 5987 `charset'lang'percent-encoded` values (UTF-8 only).
fn decode_ext_value(value: &str) -> Option<String> {
    let mut parts = value.splitn(3, '\'');
    let charset = parts.next()?;
    let _language = parts.next()?;
    let encoded = parts.next()?;
    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// One part of the body.
pub struct Field<'a, S> {
    mp: &'a mut Multipart<S>,
    name: Option<String>,
    file_name: Option<String>,
    content_type: Option<String>,
    headers: HeaderMap,
    read: u64,
}

impl<S> fmt::Debug for Field<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("name", &self.name)
            .field("file_name", &self.file_name)
            .finish_non_exhaustive()
    }
}

impl<S, E> Field<'_, S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: Into<BoxError>,
{
    /// The `name` from `Content-Disposition`.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The `filename` (or RFC 5987 `filename*`) from `Content-Disposition`.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The next piece of the body, `None` at its end.
    ///
    /// # Errors
    ///
    /// Any [`Error`], including [`Error::FieldTooLarge`].
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error> {
        if self.mp.state != State::Body {
            return Ok(None);
        }
        let Some(chunk) = self.mp.next_chunk().await? else {
            return Ok(None);
        };
        self.read += chunk.len() as u64;
        if let Some(limit) = self.mp.limits.per_field
            && self.read > limit
        {
            return Err(Error::FieldTooLarge(limit));
        }
        Ok(Some(chunk))
    }

    /// The whole body in memory.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn bytes(mut self) -> Result<Bytes, Error> {
        let mut out = Vec::new();
        while let Some(chunk) = self.chunk().await? {
            out.extend_from_slice(&chunk);
        }
        Ok(Bytes::from(out))
    }

    /// The whole body as UTF-8 text.
    ///
    /// # Errors
    ///
    /// [`Error::NotUtf8`] or any other [`Error`].
    pub async fn text(self) -> Result<String, Error> {
        String::from_utf8(self.bytes().await?.to_vec()).map_err(|_| Error::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::stream;
    use std::error::Error as _;
    use std::fmt::Write as _;

    type Chunks = stream::Iter<std::vec::IntoIter<Result<Bytes, std::io::Error>>>;

    fn chunks(parts: Vec<Vec<u8>>) -> Chunks {
        let mut items = Vec::with_capacity(parts.len());
        for part in parts {
            items.push(Ok(Bytes::from(part)));
        }
        stream::iter(items)
    }

    fn reader(body: &str, chunk: usize) -> Multipart<Chunks> {
        let parts = body
            .as_bytes()
            .chunks(chunk.max(1))
            .map(<[u8]>::to_vec)
            .collect();
        Multipart::new(chunks(parts), "XyZ", Limits::default())
    }

    fn form(parts: &[(&str, Option<&str>, &str)]) -> String {
        let mut out = String::new();
        for (name, file, value) in parts {
            out.push_str("--XyZ\r\n");
            match file {
                Some(file) => {
                    let _ = write!(
                        out,
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                    );
                }
                None => {
                    let _ = write!(
                        out,
                        "Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"
                    );
                }
            }
            out.push_str(value);
            out.push_str("\r\n");
        }
        out.push_str("--XyZ--\r\n");
        out
    }

    async fn collect(
        mp: &mut Multipart<Chunks>,
    ) -> Result<Vec<(Option<String>, Option<String>, String)>, Error> {
        let mut out = Vec::new();
        while let Some(field) = mp.next_field().await? {
            let name = field.name().map(str::to_string);
            let file = field.file_name().map(str::to_string);
            out.push((name, file, field.text().await?));
        }
        Ok(out)
    }

    #[test]
    fn boundaries_come_from_the_content_type() {
        assert_eq!(
            parse_boundary("multipart/form-data; boundary=abc").unwrap(),
            "abc"
        );
        assert_eq!(
            parse_boundary("Multipart/Form-Data;BOUNDARY=\"a b:c\"").unwrap(),
            "a b:c"
        );
        assert_eq!(
            parse_boundary("multipart/form-data; charset=utf-8; boundary=zz").unwrap(),
            "zz"
        );
        for bad in [
            "application/json",
            "multipart/form-data",
            "multipart/mixed; boundary=x",
            "",
        ] {
            assert!(
                matches!(parse_boundary(bad), Err(Error::MissingBoundary)),
                "{bad:?}"
            );
        }
        for bad in [
            "multipart/form-data; boundary",
            "multipart/form-data; boundary=\"\"",
            "multipart/form-data; boundary=a\\\"b",
        ] {
            assert!(parse_boundary(bad).is_err(), "{bad:?}");
        }
        let long = format!("multipart/form-data; boundary={}", "a".repeat(71));
        assert!(matches!(parse_boundary(&long), Err(Error::InvalidBoundary)));
        assert!(
            parse_boundary(&format!("multipart/form-data; boundary={}", "a".repeat(70))).is_ok()
        );
    }

    #[tokio::test]
    async fn reads_text_and_file_fields() {
        let body = form(&[
            ("title", None, "hello"),
            ("upload", Some("a.bin"), "line1\r\nline2"),
        ]);
        let mut mp = reader(&body, 1 << 16);
        let fields = collect(&mut mp).await.unwrap();
        assert_eq!(
            fields,
            vec![
                (Some("title".into()), None, "hello".into()),
                (
                    Some("upload".into()),
                    Some("a.bin".into()),
                    "line1\r\nline2".into()
                )
            ]
        );
        assert!(mp.next_field().await.unwrap().is_none());
        assert!(format!("{mp:?}").contains("Done"));
    }

    #[tokio::test]
    async fn every_chunk_size_gives_the_same_result() {
        let body = form(&[
            ("a", None, "x--XyZ-y\r\n--Xy"),
            ("b", Some("f"), "\r\n\r\n--"),
            ("c", None, ""),
        ]);
        let expected = collect(&mut reader(&body, 1 << 16)).await.unwrap();
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].2, "x--XyZ-y\r\n--Xy");
        assert_eq!(expected[2].2, "");
        for size in 1..=40 {
            assert_eq!(
                collect(&mut reader(&body, size)).await.unwrap(),
                expected,
                "chunk size {size}"
            );
        }
    }

    #[tokio::test]
    async fn preamble_epilogue_and_padding_are_ignored() {
        let body = String::from(
            "this is a preamble\r\nwith lines\r\n--XyZ \t\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nv\r\n--XyZ--  trailing epilogue\r\nmore",
        );
        let fields = collect(&mut reader(&body, 7)).await.unwrap();
        assert_eq!(fields, vec![(Some("a".into()), None, "v".into())]);
    }

    #[tokio::test]
    async fn parts_without_headers_and_empty_bodies() {
        let body = "--XyZ\r\n\r\nbare\r\n--XyZ--";
        let fields = collect(&mut reader(body, 3)).await.unwrap();
        assert_eq!(fields, vec![(None, None, "bare".into())]);
        assert!(collect(&mut reader("--XyZ--", 2)).await.unwrap().is_empty());
        assert!(
            collect(&mut reader("preamble only\r\n--XyZ--\r\n", 5))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn skipped_fields_are_drained() {
        let body = form(&[("skip", None, &"x".repeat(5000)), ("keep", None, "me")]);
        let mut mp = reader(&body, 100);
        let first = mp.next_field().await.unwrap().unwrap();
        assert_eq!(first.name(), Some("skip"));
        drop(first);
        let second = mp.next_field().await.unwrap().unwrap();
        assert_eq!(second.text().await.unwrap(), "me");
        assert!(mp.next_field().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fields_can_be_streamed_in_pieces() {
        let value = "0123456789".repeat(100);
        let body = form(&[("big", Some("f.txt"), &value)]);
        let mut mp = reader(&body, 64);
        let mut field = mp.next_field().await.unwrap().unwrap();
        let mut got = Vec::new();
        let mut pieces = 0;
        while let Some(chunk) = field.chunk().await.unwrap() {
            got.extend_from_slice(&chunk);
            pieces += 1;
        }
        assert!(pieces > 1);
        assert_eq!(got, value.as_bytes());
        assert!(
            field.chunk().await.unwrap().is_none(),
            "a finished field stays finished"
        );
        assert_eq!(field.content_type(), Some("application/octet-stream"));
        assert!(field.headers().contains_key("content-disposition"));
        assert!(format!("{field:?}").contains("f.txt"));
        let bytes = reader(&body, 1000)
            .next_field()
            .await
            .unwrap()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(bytes, value.as_str());
    }

    #[tokio::test]
    async fn truncated_bodies_are_errors() {
        for body in [
            "--XyZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nno end",
            "--XyZ\r\nContent-Disposition: form-data; name=\"a\"",
            "--XyZ",
            "",
            "--XyZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nv\r\n--XyZ",
        ] {
            let result = collect(&mut reader(body, 4)).await;
            assert!(
                matches!(result, Err(Error::Truncated)),
                "{body:?}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn malformed_structure_is_rejected() {
        let junk_after_boundary = "--XyZ junk\r\n\r\nx\r\n--XyZ--";
        assert!(matches!(
            collect(&mut reader(junk_after_boundary, 50)).await,
            Err(Error::Malformed(_))
        ));
        let bad_header = "--XyZ\r\nno colon here\r\n\r\nx\r\n--XyZ--";
        assert!(matches!(
            collect(&mut reader(bad_header, 50)).await,
            Err(Error::Malformed(_))
        ));
        let bad_name = "--XyZ\r\nbad name: v\r\n\r\nx\r\n--XyZ--";
        assert!(matches!(
            collect(&mut reader(bad_name, 50)).await,
            Err(Error::Malformed(_))
        ));
        let long_line = format!("--XyZ{}\r\n", " ".repeat(2000));
        assert!(matches!(
            collect(&mut reader(&long_line, 100)).await,
            Err(Error::Malformed(_))
        ));
    }

    #[tokio::test]
    async fn limits_are_enforced() {
        let body = form(&[("a", None, &"x".repeat(100))]);
        let limits = Limits {
            whole_stream: Some(50),
            ..Limits::default()
        };
        let mut mp = Multipart::new(
            chunks(body.as_bytes().chunks(10).map(<[u8]>::to_vec).collect()),
            "XyZ",
            limits,
        );
        assert!(matches!(
            collect(&mut mp).await,
            Err(Error::StreamTooLarge(50))
        ));

        let limits = Limits {
            per_field: Some(10),
            ..Limits::default()
        };
        let mut mp = Multipart::new(chunks(vec![body.clone().into_bytes()]), "XyZ", limits);
        assert!(matches!(
            collect(&mut mp).await,
            Err(Error::FieldTooLarge(10))
        ));

        let many = form(&[("a", None, "1"), ("b", None, "2"), ("c", None, "3")]);
        let limits = Limits {
            max_fields: 2,
            ..Limits::default()
        };
        let mut mp = Multipart::new(chunks(vec![many.into_bytes()]), "XyZ", limits);
        assert!(matches!(collect(&mut mp).await, Err(Error::TooManyFields)));

        let fat_headers = format!("--XyZ\r\nX-Pad: {}\r\n\r\nv\r\n--XyZ--", "p".repeat(500));
        let limits = Limits {
            max_header_bytes: 100,
            ..Limits::default()
        };
        let mut mp = Multipart::new(
            chunks(vec![fat_headers.clone().into_bytes()]),
            "XyZ",
            limits,
        );
        assert!(matches!(
            collect(&mut mp).await,
            Err(Error::HeadersTooLarge)
        ));
        // Same limit, header block arriving without its terminator yet.
        let mut mp = Multipart::new(
            chunks(vec![fat_headers.as_bytes()[..300].to_vec()]),
            "XyZ",
            limits,
        );
        assert!(matches!(
            collect(&mut mp).await,
            Err(Error::HeadersTooLarge)
        ));
    }

    #[tokio::test]
    async fn text_fields_must_be_utf8() {
        let mut body = b"--XyZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n".to_vec();
        body.extend_from_slice(&[0xff, 0xfe]);
        body.extend_from_slice(b"\r\n--XyZ--");
        let mut mp = Multipart::new(chunks(vec![body]), "XyZ", Limits::default());
        assert!(matches!(collect(&mut mp).await, Err(Error::NotUtf8)));
    }

    #[tokio::test]
    async fn stream_errors_surface() {
        let items: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"--XyZ\r\n")),
            Err(std::io::Error::other("socket died")),
        ];
        let mut mp = Multipart::new(stream::iter(items), "XyZ", Limits::default());
        let err = mp.next_field().await.unwrap_err();
        assert!(matches!(err, Error::Stream(_)));
        assert!(err.to_string().contains("socket died"));
        assert!(err.source().is_some());
    }

    #[test]
    fn dispositions_handle_quotes_escapes_and_extended_names() {
        let mut headers = HeaderMap::new();
        headers.append(
            "content-disposition",
            HeaderValue::from_static("form-data; name=\"a;b\"; filename=\"we\\\"ird.txt\""),
        );
        assert_eq!(
            disposition(&headers),
            (Some("a;b".into()), Some("we\"ird.txt".into()))
        );
        let mut headers = HeaderMap::new();
        headers.append(
            "content-disposition",
            HeaderValue::from_static(
                "form-data; name=f; filename=\"plain.txt\"; filename*=UTF-8''na%C3%AFve%20file.txt",
            ),
        );
        assert_eq!(
            disposition(&headers),
            (Some("f".into()), Some("naïve file.txt".into()))
        );
        let mut headers = HeaderMap::new();
        headers.append(
            "content-disposition",
            HeaderValue::from_static(
                "form-data; name=f; filename=\"fallback.txt\"; filename*=latin1''x",
            ),
        );
        assert_eq!(disposition(&headers).1.as_deref(), Some("fallback.txt"));
        let mut headers = HeaderMap::new();
        headers.append(
            "content-disposition",
            HeaderValue::from_static("form-data; filename*=UTF-8''bad%zz; flag"),
        );
        assert_eq!(disposition(&headers), (None, None));
        assert_eq!(disposition(&HeaderMap::new()), (None, None));
    }

    #[test]
    fn errors_display() {
        for err in [
            Error::MissingBoundary,
            Error::InvalidBoundary,
            Error::Truncated,
            Error::Malformed("x"),
            Error::HeadersTooLarge,
            Error::TooManyFields,
            Error::StreamTooLarge(1),
            Error::FieldTooLarge(2),
            Error::NotUtf8,
            Error::Stream("s".into()),
        ] {
            assert!(!err.to_string().is_empty());
        }
    }

    #[tokio::test]
    async fn compaction_keeps_long_bodies_intact() {
        let value = "abcdefghij".repeat(30_000);
        let body = form(&[("big", None, &value), ("tail", None, "end")]);
        let fields = collect(&mut reader(&body, 9000)).await.unwrap();
        assert_eq!(fields[0].2.len(), value.len());
        assert_eq!(fields[1].2, "end");
    }
}
