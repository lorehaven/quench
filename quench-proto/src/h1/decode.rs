//! Incremental body decoders: fixed length, chunked (RFC 9112 section 7.1) and read-to-EOF.

use super::parse::{Framing, ProtoError};

const MAX_LINE: usize = 4096;
const MAX_TRAILER_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Reading the hex size line.
    Size,
    /// This many bytes of chunk data remain.
    Data(u64),
    /// Expecting the CRLF after chunk data.
    DataEnd,
    /// Reading trailer lines until the blank one.
    Trailers,
    Done,
}

/// Decodes a body as bytes arrive.
///
/// [`feed`](Self::feed) consumes what it can from the input and appends decoded
/// payload to `out`; call it again with more input until
/// [`is_done`](Self::is_done). Never reads past the end of the message, so
/// bytes of a pipelined next message stay unconsumed.
#[derive(Debug)]
pub struct BodyDecoder {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Length(u64),
    Chunked {
        state: State,
        line: Vec<u8>,
        trailer_bytes: usize,
    },
    Eof,
    Empty,
}

impl BodyDecoder {
    #[must_use]
    pub const fn new(framing: Framing) -> Self {
        Self {
            kind: match framing {
                Framing::None | Framing::Length(0) => Kind::Empty,
                Framing::Length(n) => Kind::Length(n),
                Framing::Chunked => Kind::Chunked {
                    state: State::Size,
                    line: Vec::new(),
                    trailer_bytes: 0,
                },
                Framing::UntilClose => Kind::Eof,
            },
        }
    }

    /// Whether the message is complete (never true for read-to-EOF; see [`finish_eof`](Self::finish_eof)).
    #[must_use]
    pub const fn is_done(&self) -> bool {
        match &self.kind {
            Kind::Empty => true,
            Kind::Length(n) => *n == 0,
            Kind::Chunked { state, .. } => matches!(state, State::Done),
            Kind::Eof => false,
        }
    }

    /// Whether the body is delimited by the connection closing.
    #[must_use]
    pub const fn reads_to_eof(&self) -> bool {
        matches!(self.kind, Kind::Eof)
    }

    /// Bytes still owed for a fixed-length body.
    #[must_use]
    pub const fn remaining(&self) -> Option<u64> {
        match &self.kind {
            Kind::Length(n) => Some(*n),
            _ => None,
        }
    }

    /// Consumes input, appending payload to `out`; returns how many input bytes were used.
    ///
    /// # Errors
    ///
    /// [`ProtoError::BadRequest`] for a malformed chunk size, missing CRLF or oversized line/trailer.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<usize, ProtoError> {
        match &mut self.kind {
            Kind::Empty => Ok(0),
            Kind::Eof => {
                out.extend_from_slice(input);
                Ok(input.len())
            }
            Kind::Length(remaining) => {
                let take = usize::try_from(*remaining).map_or(input.len(), |r| r.min(input.len()));
                out.extend_from_slice(&input[..take]);
                *remaining -= take as u64;
                Ok(take)
            }
            Kind::Chunked {
                state,
                line,
                trailer_bytes,
            } => feed_chunked(state, line, trailer_bytes, input, out),
        }
    }
}

fn feed_chunked(
    state: &mut State,
    line: &mut Vec<u8>,
    trailer_bytes: &mut usize,
    input: &[u8],
    out: &mut Vec<u8>,
) -> Result<usize, ProtoError> {
    let mut used = 0;
    while used < input.len() {
        match state {
            State::Done => break,
            State::Data(remaining) => {
                let take = usize::try_from(*remaining)
                    .map_or(input.len() - used, |r| r.min(input.len() - used));
                out.extend_from_slice(&input[used..used + take]);
                used += take;
                *remaining -= take as u64;
                if *remaining == 0 {
                    *state = State::DataEnd;
                }
            }
            State::DataEnd => {
                let byte = input[used];
                used += 1;
                line.push(byte);
                if line.len() == 2 {
                    if line.as_slice() != b"\r\n" {
                        return Err(ProtoError::BadRequest("chunk data not followed by CRLF"));
                    }
                    line.clear();
                    *state = State::Size;
                } else if byte != b'\r' {
                    return Err(ProtoError::BadRequest("chunk data not followed by CRLF"));
                }
            }
            State::Size | State::Trailers => {
                let byte = input[used];
                used += 1;
                line.push(byte);
                if matches!(state, State::Trailers) {
                    *trailer_bytes += 1;
                    if *trailer_bytes > MAX_TRAILER_BYTES {
                        return Err(ProtoError::BadRequest("trailers too large"));
                    }
                }
                if line.len() > MAX_LINE {
                    return Err(ProtoError::BadRequest("chunk line too long"));
                }
                if byte == b'\n' {
                    let complete = std::mem::take(line);
                    let text = complete
                        .strip_suffix(b"\r\n")
                        .ok_or(ProtoError::BadRequest("bare line feed in chunked body"))?;
                    if matches!(state, State::Size) {
                        *state = match parse_chunk_size(text)? {
                            0 => State::Trailers,
                            n => State::Data(n),
                        };
                    } else if text.is_empty() {
                        *state = State::Done;
                    } else if text.iter().any(|&b| b == b'\r' || b == b'\n' || b == 0) {
                        return Err(ProtoError::BadRequest("bad trailer line"));
                    }
                }
            }
        }
    }
    Ok(used)
}

/// `1a3f` or `1a3f;ext=value`: hex digits only, at most 16 of them.
fn parse_chunk_size(line: &[u8]) -> Result<u64, ProtoError> {
    let digits = line.split(|&b| b == b';').next().unwrap_or(&[]);
    if digits.is_empty() || digits.len() > 16 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err(ProtoError::BadRequest("bad chunk size"));
    }
    let text = std::str::from_utf8(digits).map_err(|_| ProtoError::BadRequest("bad chunk size"))?;
    u64::from_str_radix(text, 16).map_err(|_| ProtoError::BadRequest("bad chunk size"))
}

/// Appends one chunk (`<hex size>\r\n<data>\r\n`). Empty data is skipped, since a
/// zero-size chunk would end the body.
pub fn encode_chunk(out: &mut Vec<u8>, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    out.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
}

/// The terminating chunk with no trailers.
pub const LAST_CHUNK: &[u8] = b"0\r\n\r\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(framing: Framing, input: &[u8]) -> Result<(Vec<u8>, usize, bool), ProtoError> {
        let mut decoder = BodyDecoder::new(framing);
        let mut out = Vec::new();
        let used = decoder.feed(input, &mut out)?;
        Ok((out, used, decoder.is_done()))
    }

    /// Feeds one byte at a time, which exercises every resumption point.
    fn decode_bytewise(
        framing: Framing,
        input: &[u8],
    ) -> Result<(Vec<u8>, usize, bool), ProtoError> {
        let mut decoder = BodyDecoder::new(framing);
        let mut out = Vec::new();
        let mut used = 0;
        for byte in input {
            if decoder.is_done() {
                break;
            }
            used += decoder.feed(std::slice::from_ref(byte), &mut out)?;
        }
        Ok((out, used, decoder.is_done()))
    }

    #[test]
    fn fixed_length_stops_at_the_boundary() {
        let (out, used, done) = decode_all(Framing::Length(5), b"helloGET /next").unwrap();
        assert_eq!((out.as_slice(), used, done), (&b"hello"[..], 5, true));
        let (out, used, done) = decode_all(Framing::Length(10), b"hello").unwrap();
        assert_eq!((out.as_slice(), used, done), (&b"hello"[..], 5, false));
    }

    #[test]
    fn empty_framings_are_done_immediately() {
        for framing in [Framing::None, Framing::Length(0)] {
            let mut decoder = BodyDecoder::new(framing);
            assert!(decoder.is_done());
            assert_eq!(decoder.feed(b"next", &mut Vec::new()).unwrap(), 0);
        }
    }

    #[test]
    fn until_close_takes_everything_and_is_never_done() {
        let mut decoder = BodyDecoder::new(Framing::UntilClose);
        let mut out = Vec::new();
        assert_eq!(decoder.feed(b"abc", &mut out).unwrap(), 3);
        assert!(!decoder.is_done());
        assert!(decoder.reads_to_eof());
        assert_eq!(out, b"abc");
        assert_eq!(decoder.remaining(), None);
    }

    #[test]
    fn remaining_counts_down() {
        let mut decoder = BodyDecoder::new(Framing::Length(4));
        assert_eq!(decoder.remaining(), Some(4));
        decoder.feed(b"ab", &mut Vec::new()).unwrap();
        assert_eq!(decoder.remaining(), Some(2));
        assert!(!decoder.reads_to_eof());
    }

    #[test]
    fn chunked_decodes_whole_and_bytewise() {
        let wire = b"5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\n\r\nGET /next";
        for result in [
            decode_all(Framing::Chunked, wire),
            decode_bytewise(Framing::Chunked, wire),
        ] {
            let (out, used, done) = result.unwrap();
            assert_eq!(out, b"hello world");
            assert!(done);
            assert_eq!(&wire[used..], b"GET /next");
        }
    }

    #[test]
    fn chunked_accepts_uppercase_hex_and_trailers() {
        let wire = b"A\r\n0123456789\r\n0\r\nX-Trailer: yes\r\nOther: 1\r\n\r\n";
        for result in [
            decode_all(Framing::Chunked, wire),
            decode_bytewise(Framing::Chunked, wire),
        ] {
            let (out, used, done) = result.unwrap();
            assert_eq!(out, b"0123456789");
            assert!(done);
            assert_eq!(used, wire.len());
        }
    }

    #[test]
    fn chunked_rejects_malformed_input() {
        for bad in [
            &b"zz\r\nhello\r\n"[..],
            b"\r\nhello\r\n",
            b" 5\r\nhello\r\n",
            b"5 \r\nhello\r\n",
            b"-5\r\nhello\r\n",
            b"5\nhello\r\n",
            b"5\r\nhelloXX",
            b"5\r\nhello\rX",
            b"11111111111111111\r\n",
            b"0\r\nbad\ntrailer\r\n\r\n",
            b"0\r\nbad\0trailer\r\n\r\n",
        ] {
            assert!(
                decode_all(Framing::Chunked, bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
            assert!(
                decode_bytewise(Framing::Chunked, bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn chunked_limits_line_and_trailer_sizes() {
        let mut long_line = vec![b'1'; MAX_LINE + 1];
        long_line.extend_from_slice(b"\r\n");
        assert!(decode_all(Framing::Chunked, &long_line).is_err());

        let mut wire = b"0\r\n".to_vec();
        for _ in 0..(MAX_TRAILER_BYTES / 16 + 8) {
            wire.extend_from_slice(b"X-Pad: 12345678\r\n");
        }
        assert!(decode_all(Framing::Chunked, &wire).is_err());
    }

    #[test]
    fn chunked_is_incomplete_without_the_final_blank_line() {
        let (_, _, done) = decode_all(Framing::Chunked, b"3\r\nabc\r\n0\r\n").unwrap();
        assert!(!done);
        let (out, _, done) = decode_all(Framing::Chunked, b"3\r\nabc\r\n").unwrap();
        assert_eq!(out, b"abc");
        assert!(!done);
    }

    #[test]
    fn encode_chunk_formats_and_skips_empty_data() {
        let mut out = Vec::new();
        encode_chunk(&mut out, b"");
        assert!(out.is_empty());
        encode_chunk(&mut out, b"hello world!");
        assert_eq!(out, b"c\r\nhello world!\r\n");
        out.extend_from_slice(LAST_CHUNK);
        let (decoded, _, done) = decode_all(Framing::Chunked, &out).unwrap();
        assert_eq!(decoded, b"hello world!");
        assert!(done);
    }
}
