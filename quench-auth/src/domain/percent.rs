//! Percent-encoding for query-string values (RFC 3986 unreserved characters
//! pass through, everything else becomes `%XX`).

use std::fmt::Write;

pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// `None` when the decoded bytes aren't valid UTF-8. A `%` not followed by
/// two hex digits is kept literally, as `urlencoding` does.
pub fn decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(&h), Some(&l)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(h), Some(l)) = (hex(h), hex(l))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_reserved_and_multibyte_text() {
        let raw = "a b&c=d/é~-._";
        assert_eq!(encode(raw), "a%20b%26c%3Dd%2F%C3%A9~-._");
        assert_eq!(decode(&encode(raw)).as_deref(), Some(raw));
    }

    #[test]
    fn decode_keeps_malformed_escapes_and_rejects_bad_utf8() {
        assert_eq!(decode("100%").as_deref(), Some("100%"));
        assert_eq!(decode("%zz").as_deref(), Some("%zz"));
        assert_eq!(decode("%FF"), None);
    }
}
