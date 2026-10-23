//! The RESP wire format, versions 2 and 3: values, an incremental decoder and a command encoder.

use crate::error::{Error, Result, ServerError};

/// Largest bulk string accepted (Redis' own limit).
const MAX_BULK: usize = 512 * 1024 * 1024;
/// Largest aggregate (array, map, set) element count accepted.
const MAX_ELEMENTS: usize = 16 * 1024 * 1024;
/// Deepest nesting accepted.
const MAX_DEPTH: usize = 32;

/// A reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Int(i64),
    Bulk(Vec<u8>),
    Simple(String),
    Array(Vec<Self>),
    /// RESP3 map, in the order sent.
    Map(Vec<(Self, Self)>),
    /// RESP3 set.
    Set(Vec<Self>),
    Double(f64),
    Bool(bool),
    BigNumber(String),
    /// RESP3 verbatim string (the three-letter format is dropped).
    Verbatim(String),
    /// RESP3 out-of-band push.
    Push(Vec<Self>),
    /// An error inside an aggregate; a top-level error reply becomes [`Error::Server`].
    Error(ServerError),
}

impl Value {
    /// The text of a bulk, simple or verbatim string, if it is UTF-8.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Bulk(bytes) => std::str::from_utf8(bytes).ok(),
            Self::Simple(text) | Self::Verbatim(text) => Some(text),
            _ => None,
        }
    }
}

/// Appends a command in the multi-bulk request format.
pub fn encode_command(out: &mut Vec<u8>, args: &[Vec<u8>]) {
    out.push(b'*');
    out.extend_from_slice(args.len().to_string().as_bytes());
    out.extend_from_slice(b"\r\n");
    for arg in args {
        out.push(b'$');
        out.extend_from_slice(arg.len().to_string().as_bytes());
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
}

/// Decodes one value from the front of `input`.
///
/// Returns the value and the bytes it used, or `None` when `input` holds only part of one.
///
/// # Errors
///
/// [`Error::Protocol`] for input that cannot be RESP, or exceeds the size limits.
pub fn decode(input: &[u8]) -> Result<Option<(Value, usize)>> {
    parse(input, 0, 0)
}

fn protocol(why: impl Into<String>) -> Error {
    Error::Protocol(why.into())
}

/// The line starting at `at` (without CRLF) and the offset after it.
fn line(input: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let rest = input.get(at..)?;
    let end = rest.windows(2).position(|w| w == b"\r\n")?;
    Some((&rest[..end], at + end + 2))
}

fn number(bytes: &[u8]) -> Result<i64> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| protocol(format!("bad number {:?}", String::from_utf8_lossy(bytes))))
}

fn length(bytes: &[u8]) -> Result<Option<usize>> {
    let n = number(bytes)?;
    if n == -1 {
        return Ok(None);
    }
    usize::try_from(n)
        .map(Some)
        .map_err(|_| protocol(format!("negative length {n}")))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse(input: &[u8], at: usize, depth: usize) -> Result<Option<(Value, usize)>> {
    if depth > MAX_DEPTH {
        return Err(protocol("replies nested too deeply"));
    }
    let Some(&kind) = input.get(at) else {
        return Ok(None);
    };
    let Some((head, after)) = line(input, at + 1) else {
        return Ok(None);
    };
    Ok(match kind {
        b'+' => Some((Value::Simple(text(head)), after)),
        b'-' => Some((Value::Error(ServerError::parse(&text(head))), after)),
        b':' => Some((Value::Int(number(head)?), after)),
        b'_' => Some((Value::Nil, after)),
        b'#' => match head {
            b"t" => Some((Value::Bool(true), after)),
            b"f" => Some((Value::Bool(false), after)),
            _ => return Err(protocol("bad boolean")),
        },
        b',' => {
            let double = match head {
                b"inf" => f64::INFINITY,
                b"-inf" => f64::NEG_INFINITY,
                b"nan" => f64::NAN,
                other => std::str::from_utf8(other)
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| protocol("bad double"))?,
            };
            Some((Value::Double(double), after))
        }
        b'(' => Some((Value::BigNumber(text(head)), after)),
        b'$' | b'!' | b'=' => {
            let Some(len) = length(head)? else {
                return Ok(Some((Value::Nil, after)));
            };
            if len > MAX_BULK {
                return Err(protocol(format!("bulk of {len} bytes is over the limit")));
            }
            if input.len() < after + len + 2 {
                return Ok(None);
            }
            if &input[after + len..after + len + 2] != b"\r\n" {
                return Err(protocol("bulk not terminated by CRLF"));
            }
            let body = &input[after..after + len];
            let used = after + len + 2;
            Some(match kind {
                b'$' => (Value::Bulk(body.to_vec()), used),
                b'!' => (Value::Error(ServerError::parse(&text(body))), used),
                _ => (Value::Verbatim(text(body.get(4..).unwrap_or(body))), used),
            })
        }
        b'*' | b'~' | b'>' | b'%' | b'|' => {
            let Some(count) = length(head)? else {
                return Ok(Some((Value::Nil, after)));
            };
            let pairs = matches!(kind, b'%' | b'|');
            let total = if pairs {
                count.checked_mul(2)
            } else {
                Some(count)
            };
            let total = total.filter(|t| *t <= MAX_ELEMENTS).ok_or_else(|| {
                protocol(format!("aggregate of {count} elements is over the limit"))
            })?;
            let mut items = Vec::with_capacity(total.min(1024));
            let mut pos = after;
            for _ in 0..total {
                let Some((item, next)) = parse(input, pos, depth + 1)? else {
                    return Ok(None);
                };
                items.push(item);
                pos = next;
            }
            let value = match kind {
                b'*' => Value::Array(items),
                b'~' => Value::Set(items),
                b'>' => Value::Push(items),
                _ => {
                    let mut entries = Vec::with_capacity(count);
                    let mut iter = items.into_iter();
                    while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
                        entries.push((k, v));
                    }
                    Value::Map(entries)
                }
            };
            if kind == b'|' {
                // Attributes annotate the value that follows; skip them and return that value.
                return parse(input, pos, depth);
            }
            Some((value, pos))
        }
        other => {
            return Err(protocol(format!(
                "unknown type byte {:?}",
                char::from(other)
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &[u8]) -> (Value, usize) {
        decode(input).unwrap().unwrap()
    }

    #[test]
    fn encodes_commands() {
        let mut out = Vec::new();
        encode_command(
            &mut out,
            &[b"SET".to_vec(), b"k".to_vec(), b"va\r\nlue".to_vec()],
        );
        assert_eq!(out, b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$7\r\nva\r\nlue\r\n");
        out.clear();
        encode_command(&mut out, &[b"PING".to_vec(), Vec::new()]);
        assert_eq!(out, b"*2\r\n$4\r\nPING\r\n$0\r\n\r\n");
    }

    #[test]
    fn resp2_scalars() {
        assert_eq!(ok(b"+OK\r\n"), (Value::Simple("OK".into()), 5));
        assert_eq!(ok(b":-42\r\n"), (Value::Int(-42), 6));
        assert_eq!(ok(b"$5\r\nhello\r\n"), (Value::Bulk(b"hello".to_vec()), 11));
        assert_eq!(ok(b"$0\r\n\r\n").0, Value::Bulk(Vec::new()));
        assert_eq!(ok(b"$-1\r\n").0, Value::Nil);
        assert_eq!(ok(b"*-1\r\n").0, Value::Nil);
        assert_eq!(
            ok(b"-ERR unknown command\r\n").0,
            Value::Error(ServerError {
                kind: "ERR".into(),
                message: "unknown command".into()
            })
        );
        assert_eq!(ok(b"+OK\r\nextra").1, 5, "only the first value is consumed");
    }

    #[test]
    fn resp2_aggregates() {
        let (value, used) = ok(b"*3\r\n:1\r\n$2\r\nhi\r\n*1\r\n+x\r\n");
        assert_eq!(used, 24);
        assert_eq!(
            value,
            Value::Array(vec![
                Value::Int(1),
                Value::Bulk(b"hi".to_vec()),
                Value::Array(vec![Value::Simple("x".into())])
            ])
        );
        assert_eq!(ok(b"*0\r\n").0, Value::Array(vec![]));
        assert_eq!(
            ok(b"*2\r\n$-1\r\n-ERR no\r\n").0,
            Value::Array(vec![Value::Nil, Value::Error(ServerError::parse("ERR no"))])
        );
    }

    #[test]
    fn resp3_types() {
        assert_eq!(ok(b"_\r\n").0, Value::Nil);
        assert_eq!(ok(b"#t\r\n").0, Value::Bool(true));
        assert_eq!(ok(b"#f\r\n").0, Value::Bool(false));
        assert_eq!(ok(b",1.5\r\n").0, Value::Double(1.5));
        assert_eq!(ok(b",inf\r\n").0, Value::Double(f64::INFINITY));
        assert_eq!(ok(b",-inf\r\n").0, Value::Double(f64::NEG_INFINITY));
        assert!(matches!(ok(b",nan\r\n").0, Value::Double(d) if d.is_nan()));
        assert_eq!(
            ok(b"(3492890328409238509324850943850943825024385\r\n").0,
            Value::BigNumber("3492890328409238509324850943850943825024385".into())
        );
        assert_eq!(
            ok(b"!21\r\nSYNTAX invalid syntax\r\n").0,
            Value::Error(ServerError::parse("SYNTAX invalid syntax"))
        );
        assert_eq!(
            ok(b"=15\r\ntxt:Some string\r\n").0,
            Value::Verbatim("Some string".into())
        );
        assert_eq!(
            ok(b"~2\r\n:1\r\n:2\r\n").0,
            Value::Set(vec![Value::Int(1), Value::Int(2)])
        );
        assert_eq!(
            ok(b">2\r\n+pubsub\r\n:1\r\n").0,
            Value::Push(vec![Value::Simple("pubsub".into()), Value::Int(1)])
        );
        assert_eq!(
            ok(b"%2\r\n+a\r\n:1\r\n+b\r\n:2\r\n").0,
            Value::Map(vec![
                (Value::Simple("a".into()), Value::Int(1)),
                (Value::Simple("b".into()), Value::Int(2))
            ])
        );
        assert_eq!(
            ok(b"|1\r\n+ttl\r\n:3\r\n$3\r\nfoo\r\n"),
            (Value::Bulk(b"foo".to_vec()), 23)
        );
    }

    #[test]
    fn partial_input_asks_for_more() {
        let full = b"*2\r\n$3\r\nfoo\r\n:5\r\n";
        for cut in 0..full.len() {
            assert_eq!(decode(&full[..cut]).unwrap(), None, "cut {cut}");
        }
        assert!(decode(full).unwrap().is_some());
        assert_eq!(decode(b"$5\r\nhel").unwrap(), None);
        assert_eq!(
            decode(b"|1\r\n+a\r\n:1\r\n").unwrap(),
            None,
            "an attribute without its value"
        );
    }

    #[test]
    fn malformed_input_is_rejected() {
        for bad in [
            &b"?what\r\n"[..],
            b":abc\r\n",
            b"$x\r\n",
            b"$3\r\nabcXY",
            b"#maybe\r\n",
            b",wat\r\n",
            b"*-5\r\n",
            b"$-2\r\n",
        ] {
            assert!(
                matches!(decode(bad), Err(Error::Protocol(_))),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn size_and_depth_limits() {
        assert!(decode(b"$999999999999\r\n").is_err());
        assert!(decode(format!("${}\r\n", MAX_BULK + 1).as_bytes()).is_err());
        assert!(decode(format!("*{}\r\n", MAX_ELEMENTS + 1).as_bytes()).is_err());
        assert!(decode(format!("%{MAX_ELEMENTS}\r\n").as_bytes()).is_err());
        let mut deep = Vec::new();
        for _ in 0..40 {
            deep.extend_from_slice(b"*1\r\n");
        }
        deep.extend_from_slice(b":1\r\n");
        assert!(matches!(decode(&deep), Err(Error::Protocol(m)) if m.contains("deeply")));
        let mut shallow = Vec::new();
        for _ in 0..10 {
            shallow.extend_from_slice(b"*1\r\n");
        }
        shallow.extend_from_slice(b":1\r\n");
        assert!(decode(&shallow).unwrap().is_some());
    }

    #[test]
    fn value_text_access() {
        assert_eq!(Value::Bulk(b"hi".to_vec()).as_str(), Some("hi"));
        assert_eq!(Value::Simple("s".into()).as_str(), Some("s"));
        assert_eq!(Value::Verbatim("v".into()).as_str(), Some("v"));
        assert_eq!(Value::Bulk(vec![0xff]).as_str(), None);
        assert_eq!(Value::Int(1).as_str(), None);
    }
}
