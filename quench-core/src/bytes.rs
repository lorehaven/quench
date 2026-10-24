//! A cheaply clonable, immutable byte buffer (`bytes::Bytes`' role).
//!
//! Clones and slices share one allocation. Request and response bodies pass
//! through several layers; copying them at each one would be the cost.

use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, RangeBounds};
use std::sync::Arc;

#[derive(Clone)]
enum Repr {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

/// An immutable, shareable run of bytes.
#[derive(Clone)]
pub struct Bytes {
    repr: Repr,
    start: usize,
    end: usize,
}

impl Bytes {
    /// An empty buffer; does not allocate.
    #[must_use]
    pub const fn new() -> Self {
        Self::from_static(&[])
    }

    /// Wraps a `'static` slice without copying.
    #[must_use]
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            repr: Repr::Static(bytes),
            start: 0,
            end: bytes.len(),
        }
    }

    /// Copies `data` into a new buffer.
    #[must_use]
    pub fn copy_from_slice(data: &[u8]) -> Self {
        Self::from(data.to_vec())
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// A view of part of this buffer, sharing its storage.
    ///
    /// # Panics
    ///
    /// Panics when the range is out of bounds, like slice indexing.
    #[must_use]
    pub fn slice(&self, range: impl RangeBounds<usize>) -> Self {
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let begin = match range.start_bound() {
            Included(&n) => n,
            Excluded(&n) => n + 1,
            Unbounded => 0,
        };
        let end = match range.end_bound() {
            Included(&n) => n + 1,
            Excluded(&n) => n,
            Unbounded => self.len(),
        };
        assert!(
            begin <= end && end <= self.len(),
            "range {begin}..{end} out of bounds for {} bytes",
            self.len()
        );
        Self {
            repr: self.repr.clone(),
            start: self.start + begin,
            end: self.start + end,
        }
    }

    /// Removes and returns the first `at` bytes.
    ///
    /// # Panics
    ///
    /// Panics when `at > len`.
    #[must_use = "the removed bytes are dropped otherwise"]
    pub fn split_to(&mut self, at: usize) -> Self {
        let head = self.slice(..at);
        self.start += at;
        head
    }

    /// Removes the bytes from `at` on and returns them.
    ///
    /// # Panics
    ///
    /// Panics when `at > len`.
    #[must_use = "the removed bytes are dropped otherwise"]
    pub fn split_off(&mut self, at: usize) -> Self {
        let tail = self.slice(at..);
        self.end = self.start + at;
        tail
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        self.as_ref().to_vec()
    }
}

impl Default for Bytes {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match &self.repr {
            Repr::Static(bytes) => &bytes[self.start..self.end],
            Repr::Shared(bytes) => &bytes[self.start..self.end],
        }
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Borrow<[u8]> for Bytes {
    fn borrow(&self) -> &[u8] {
        self
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(data: Vec<u8>) -> Self {
        let end = data.len();
        Self {
            repr: Repr::Shared(data.into()),
            start: 0,
            end,
        }
    }
}

impl From<String> for Bytes {
    fn from(text: String) -> Self {
        text.into_bytes().into()
    }
}

impl From<&'static [u8]> for Bytes {
    fn from(data: &'static [u8]) -> Self {
        Self::from_static(data)
    }
}

impl From<&'static str> for Bytes {
    fn from(text: &'static str) -> Self {
        Self::from_static(text.as_bytes())
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(bytes: Bytes) -> Self {
        bytes.to_vec()
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for Bytes {}

impl PartialEq<[u8]> for Bytes {
    fn eq(&self, other: &[u8]) -> bool {
        **self == *other
    }
}

impl PartialEq<&[u8]> for Bytes {
    fn eq(&self, other: &&[u8]) -> bool {
        **self == **other
    }
}

impl PartialEq<Vec<u8>> for Bytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        **self == **other
    }
}

impl PartialEq<str> for Bytes {
    fn eq(&self, other: &str) -> bool {
        **self == *other.as_bytes()
    }
}

impl PartialEq<&str> for Bytes {
    fn eq(&self, other: &&str) -> bool {
        **self == *other.as_bytes()
    }
}

impl PartialOrd for Bytes {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Bytes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl Hash for Bytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl fmt::Debug for Bytes {
    /// `b"text\x00"`-style: printable ASCII as is, the rest escaped.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("b\"")?;
        for &byte in &**self {
            for escaped in std::ascii::escape_default(byte) {
                write!(f, "{}", char::from(escaped))?;
            }
        }
        f.write_str("\"")
    }
}

impl FromIterator<u8> for Bytes {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<u8>>().into()
    }
}

impl<'a> IntoIterator for &'a Bytes {
    type Item = &'a u8;
    type IntoIter = std::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[quench_test::test]
    fn construction_forms() {
        assert!(Bytes::new().is_empty());
        assert_eq!(Bytes::default().len(), 0);
        assert_eq!(Bytes::from_static(b"abc"), b"abc".as_slice());
        assert_eq!(Bytes::from("abc"), "abc");
        assert_eq!(Bytes::from(b"abc".as_slice()), *"abc");
        assert_eq!(Bytes::from(String::from("abc")), "abc");
        assert_eq!(Bytes::from(vec![1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(Bytes::copy_from_slice(&[9, 8]), [9u8, 8][..]);
        assert_eq!(Bytes::from_iter(1..=3u8), vec![1, 2, 3]);
        assert_eq!(Vec::from(Bytes::from("hi")), b"hi");
    }

    #[quench_test::test]
    fn behaves_like_a_slice() {
        let bytes = Bytes::from("hello");
        assert_eq!(bytes.len(), 5);
        assert_eq!(&bytes[1..3], b"el");
        assert_eq!(bytes.first(), Some(&b'h'));
        assert_eq!(bytes.as_ref(), b"hello");
        let borrowed: &[u8] = std::borrow::Borrow::borrow(&bytes);
        assert_eq!(borrowed, b"hello");
        assert_eq!((&bytes).into_iter().count(), 5);
        assert_eq!(bytes.to_vec(), b"hello");
    }

    #[quench_test::test]
    fn slices_share_and_nest() {
        let whole = Bytes::from(String::from("hello world"));
        let world = whole.slice(6..);
        assert_eq!(world, "world");
        assert_eq!(world.slice(1..=2), "or");
        assert_eq!(world.slice(..), "world");
        assert_eq!(whole.slice(..5), "hello");
        assert_eq!(whole.slice(3..3), "");
        let stat = Bytes::from_static(b"static");
        assert_eq!(stat.slice(1..3), "ta");
    }

    #[quench_test::test]
    #[should_panic(expected = "out of bounds")]
    fn slice_out_of_bounds_panics() {
        let _ = Bytes::from("abc").slice(2..9);
    }

    #[quench_test::test]
    fn splitting() {
        let mut bytes = Bytes::from("headerbody");
        let head = bytes.split_to(6);
        assert_eq!(
            (head, bytes.clone()),
            (Bytes::from("header"), Bytes::from("body"))
        );
        let tail = bytes.split_off(2);
        assert_eq!(bytes, "bo");
        assert_eq!(tail, "dy");
        let mut empty = Bytes::new();
        assert!(empty.split_to(0).is_empty());
    }

    #[quench_test::test]
    fn equality_ordering_hashing() {
        let a = Bytes::from("abc");
        let b = Bytes::from(vec![b'a', b'b', b'c']);
        assert_eq!(a, b);
        assert_eq!(a.cmp(&Bytes::from("abd")), std::cmp::Ordering::Less);
        assert_eq!(a.partial_cmp(&b), Some(std::cmp::Ordering::Equal));
        assert!(
            a == *"abc"
                && a == "abc"
                && a == b"abc".to_vec()
                && a == b"abc".as_slice()
                && a == *b"abc".as_slice()
        );
        assert_ne!(a, "abcd");
        let set: HashSet<Bytes> = [a.clone(), b].into_iter().collect();
        assert_eq!(set.len(), 1);
        // a slice equals a fresh buffer with the same content
        assert_eq!(Bytes::from("xabcx").slice(1..4), a);
    }

    #[quench_test::test]
    fn debug_escapes_non_printables() {
        assert_eq!(
            format!("{:?}", Bytes::from(vec![b'a', 0, b'"', 0xff, b'\n'])),
            r#"b"a\x00\"\xff\n""#
        );
        assert_eq!(format!("{:?}", Bytes::new()), "b\"\"");
    }

    #[quench_test::test]
    fn clones_are_independent_views() {
        let mut a = Bytes::from("abcdef");
        let b = a.clone();
        let _ = a.split_to(3);
        assert_eq!(a, "def");
        assert_eq!(b, "abcdef");
    }
}
