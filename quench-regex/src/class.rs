//! Sets of characters as sorted, non-overlapping ranges.

use crate::unicode;

const MAX: u32 = 0x10_FFFF;

/// A set of characters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClassSet {
    /// Sorted, non-overlapping, non-adjacent inclusive ranges of code points.
    ranges: Vec<(u32, u32)>,
}

impl ClassSet {
    #[must_use]
    pub const fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// A set from a table of ranges.
    #[must_use]
    pub fn from_table(table: &[(u32, u32)]) -> Self {
        Self {
            ranges: table.to_vec(),
        }
    }

    /// Adds `start..=end` and re-normalises.
    pub fn push(&mut self, start: u32, end: u32) {
        self.ranges.push((start.min(end), start.max(end)));
        self.normalise();
    }

    pub fn push_set(&mut self, other: &Self) {
        self.ranges.extend_from_slice(&other.ranges);
        self.normalise();
    }

    fn normalise(&mut self) {
        self.ranges.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(self.ranges.len());
        for &(a, b) in &self.ranges {
            match out.last_mut() {
                Some(last) if a <= last.1.saturating_add(1) => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        self.ranges = out;
    }

    /// Everything not in the set (surrogates are not characters and never matter).
    #[must_use]
    pub fn negated(&self) -> Self {
        let mut out = Vec::with_capacity(self.ranges.len() + 1);
        let mut next = 0u32;
        for &(a, b) in &self.ranges {
            if a > next {
                out.push((next, a - 1));
            }
            next = b.saturating_add(1);
        }
        if next <= MAX {
            out.push((next, MAX));
        }
        Self { ranges: out }
    }

    #[must_use]
    pub fn contains(&self, c: char) -> bool {
        let c = c as u32;
        self.ranges
            .binary_search_by(|&(a, b)| {
                if c < a {
                    std::cmp::Ordering::Greater
                } else if c > b {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// Adds, for every member, the other characters that match it case-insensitively.
    pub fn fold_case(&mut self) {
        let mut extra: Vec<u32> = Vec::new();
        for orbit in unicode::ORBITS {
            let hit = orbit
                .iter()
                .any(|&cp| char::from_u32(cp).is_some_and(|c| self.contains(c)));
            if hit {
                extra.extend_from_slice(orbit);
            }
        }
        for cp in extra {
            self.ranges.push((cp, cp));
        }
        self.normalise();
    }

    /// The only member, if there is exactly one.
    #[must_use]
    pub fn as_single(&self) -> Option<char> {
        match self.ranges.as_slice() {
            [(a, b)] if a == b => char::from_u32(*a),
            _ => None,
        }
    }
}

/// The characters that match `c` case-insensitively, including `c`.
pub fn orbit_of(c: char) -> Vec<char> {
    let cp = c as u32;
    for orbit in unicode::ORBITS {
        if orbit.contains(&cp) {
            return orbit.iter().filter_map(|&x| char::from_u32(x)).collect();
        }
    }
    vec![c]
}

#[must_use]
pub fn word() -> ClassSet {
    ClassSet::from_table(unicode::WORD)
}

#[must_use]
pub fn digit() -> ClassSet {
    ClassSet::from_table(unicode::DIGIT)
}

#[must_use]
pub fn space() -> ClassSet {
    ClassSet::from_table(unicode::SPACE)
}

/// Whether `c` is a word character (`\w`), for `\b`.
#[must_use]
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    let cp = c as u32;
    unicode::WORD
        .binary_search_by(|&(a, b)| {
            if cp < a {
                std::cmp::Ordering::Greater
            } else if cp > b {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn building_and_negating() {
        let mut set = ClassSet::new();
        set.push('a' as u32, 'c' as u32);
        set.push('d' as u32, 'f' as u32);
        set.push('x' as u32, 'x' as u32);
        assert_eq!(set.ranges, [(97, 102), (120, 120)], "adjacent ranges merge");
        assert!(set.contains('b') && set.contains('x') && !set.contains('g'));
        let negated = set.negated();
        assert!(!negated.contains('b') && negated.contains('g') && negated.contains('\u{10FFFF}'));
        assert_eq!(negated.negated(), set);
        assert_eq!(ClassSet::new().negated().ranges, [(0, 0x10_FFFF)]);
        let mut all = ClassSet::new();
        all.push(0, MAX);
        assert!(all.negated().ranges.is_empty());
        let mut union = ClassSet::new();
        union.push('q' as u32, 'q' as u32);
        union.push_set(&set);
        assert!(union.contains('q') && union.contains('a'));
        let mut z = ClassSet::new();
        z.push('z' as u32, 'z' as u32);
        assert_eq!(z.as_single(), Some('z'));
        assert_eq!(set.as_single(), None);
        assert_eq!(ClassSet::new().as_single(), None);
    }

    #[quench_test::test]
    fn case_folding_follows_the_unicode_orbits() {
        let mut set = ClassSet::new();
        set.push('k' as u32, 'k' as u32);
        set.fold_case();
        assert!(
            set.contains('K') && set.contains('\u{212A}'),
            "the Kelvin sign folds to k"
        );
        let mut greek = ClassSet::new();
        greek.push('σ' as u32, 'σ' as u32);
        greek.fold_case();
        assert!(greek.contains('Σ') && greek.contains('ς'));
        let mut digits = ClassSet::new();
        digits.push('5' as u32, '5' as u32);
        digits.fold_case();
        assert_eq!(digits.as_single(), Some('5'));
        assert_eq!(orbit_of('s').len(), 3, "s, S and the long s");
        assert_eq!(orbit_of('5'), ['5']);
    }

    #[quench_test::test]
    fn the_predefined_classes() {
        assert!(word().contains('é') && word().contains('_') && !word().contains('-'));
        assert!(digit().contains('٣') && !digit().contains('x'));
        assert!(space().contains('\u{00A0}') && space().contains('\n'));
        for c in ['a', 'Z', '_', '9', 'ł', '٣', '-', ' ', '😀'] {
            assert_eq!(is_word(c), word().contains(c), "{c}");
        }
    }
}
