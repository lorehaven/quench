//! How many terminal cells text takes.
//!
//! Terminals give most characters one cell, East Asian Wide and Fullwidth characters and emoji two, and
//! combining marks none: they sit on the character before them. The tables below are compact ranges, not the
//! whole Unicode database: they cover the scripts and symbols text uses, which is what keeps every laid-out
//! line inside its width.
//!
//! Emoji sequences are counted as one picture: a character followed by U+FE0F (emoji presentation) takes two
//! cells, characters joined to an emoji by U+200D (ZWJ) add nothing, and two regional indicators (a flag) take
//! two cells together. Text is measured and broken by these clusters ([`clusters`]), never inside one.

/// Ranges of zero-width characters: combining marks, variation selectors, zero-width spaces and joiners.
const ZERO: &[(u32, u32)] = &[
    (0x0300, 0x036F),
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x0711, 0x0711),
    (0x0730, 0x074A),
    (0x0900, 0x0902),
    (0x093A, 0x093A),
    (0x093C, 0x093C),
    (0x0941, 0x0948),
    (0x094D, 0x094D),
    (0x0951, 0x0957),
    (0x0962, 0x0963),
    (0x0E31, 0x0E31),
    (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E),
    (0x1160, 0x11FF),
    (0x1AB0, 0x1AFF),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x20D0, 0x20FF),
    (0x302A, 0x302D),
    (0x3099, 0x309A),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F),
    (0xFEFF, 0xFEFF),
    (0x1F3FB, 0x1F3FF),
    (0xE0000, 0xE007F),
    (0xE0100, 0xE01EF),
];

/// Ranges of two-cell characters: East Asian Wide and Fullwidth, and emoji shown as emoji by default.
const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115F),
    (0x231A, 0x231B),
    (0x2329, 0x232A),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x25FD, 0x25FE),
    (0x2614, 0x2615),
    (0x2648, 0x2653),
    (0x267F, 0x267F),
    (0x2693, 0x2693),
    (0x26A1, 0x26A1),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D4, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26FA, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x270A, 0x270B),
    (0x2728, 0x2728),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x2E80, 0x3029),
    (0x302E, 0x303E),
    (0x3041, 0x3098),
    (0x309B, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA4CF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE6F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x16FE0, 0x16FE4),
    (0x17000, 0x18AFF),
    (0x1B000, 0x1B2FF),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F200, 0x1F202),
    (0x1F210, 0x1F23B),
    (0x1F240, 0x1F248),
    (0x1F250, 0x1F251),
    (0x1F260, 0x1F265),
    (0x1F300, 0x1F3FA),
    (0x1F400, 0x1F64F),
    (0x1F680, 0x1F6FF),
    (0x1F7E0, 0x1F7F0),
    (0x1F90C, 0x1F9FF),
    (0x1FA70, 0x1FAFF),
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
];

fn within(table: &[(u32, u32)], code: u32) -> bool {
    table
        .binary_search_by(|&(low, high)| {
            if high < code {
                std::cmp::Ordering::Less
            } else if low > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The cells `c` takes in a terminal on its own.
///
/// 0 for combining marks, variation selectors, joiners, skin-tone modifiers and control characters, 2 for
/// wide characters and emoji, 1 for the rest.
///
/// It sees one character, so it knows nothing of sequences: measure text with [`display_width`], which adds
/// the emoji rules (U+FE0F, ZWJ sequences, flags).
#[must_use]
pub fn char_width(c: char) -> usize {
    let code = u32::from(c);
    if code < 0x20 || (0x7F..0xA0).contains(&code) {
        0
    } else if code < 0x300 {
        1
    } else if within(ZERO, code) {
        0
    } else if within(WIDE, code) {
        2
    } else {
        1
    }
}

/// Emoji presentation selector: the character before it is drawn as a two-cell emoji.
const EMOJI_PRESENTATION: char = '\u{FE0F}';
/// Zero width joiner: in an emoji sequence, the next character is part of the same picture.
const ZWJ: char = '\u{200D}';

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

/// Whether `c` can follow a ZWJ in an emoji sequence: an emoji, or a symbol drawn as one (`♀`, `⚕`, `❤`...).
fn joins_emoji(c: char) -> bool {
    char_width(c) == 2 || ('\u{2000}'..'\u{2C00}').contains(&c)
}

/// `text` as the units a terminal draws, each with its cells: a character with the zero-width characters
/// after it, an emoji sequence joined by ZWJ, a flag (a pair of regional indicators). A line is never broken
/// inside one.
pub fn clusters(text: &str) -> impl Iterator<Item = (&str, usize)> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        let mut cells = char_width(first);
        let mut end = first.len_utf8();
        let mut flag = is_regional_indicator(first);
        let mut chars = chars.peekable();
        while let Some(&(at, c)) = chars.peek() {
            if flag && is_regional_indicator(c) {
                // The second half of a flag: the pair takes two cells.
                cells = 2;
                flag = false;
            } else if c == EMOJI_PRESENTATION {
                // A space stays a space; any other visible character becomes an emoji.
                if cells > 0 && !first.is_whitespace() {
                    cells = 2;
                }
            } else if c == ZWJ && cells == 2 {
                // An emoji joined to the next character: that one adds nothing.
                chars.next();
                end = at + c.len_utf8();
                if let Some(&(next_at, next)) = chars.peek()
                    && joins_emoji(next)
                {
                    end = next_at + next.len_utf8();
                    chars.next();
                }
                continue;
            } else if char_width(c) != 0 || c.is_control() {
                break;
            } else {
                flag = false;
            }
            end = at + c.len_utf8();
            chars.next();
        }
        let (cluster, after) = rest.split_at(end);
        rest = after;
        Some((cluster, cells))
    })
}

/// The cells `text` takes in a terminal.
///
/// Each character takes its [`char_width`], and emoji sequences count as one picture: a character other
/// than whitespace followed by U+FE0F takes two cells, characters joined to an emoji by U+200D (ZWJ) add
/// nothing, skin-tone modifiers and U+FE0E/U+FE0F take none, and a flag (two regional indicators) takes
/// two.
///
/// ```
/// use quench_markdown_term::display_width;
///
/// assert_eq!(display_width("abc"), 3);
/// assert_eq!(display_width("セン"), 4);
/// assert_eq!(display_width("e\u{301}"), 1);
/// assert_eq!(display_width("\u{2764}\u{FE0F}"), 2);
/// assert_eq!(display_width("👨\u{200D}👩\u{200D}👧"), 2);
/// assert_eq!(display_width("🇵🇱"), 2);
/// ```
#[must_use]
pub fn display_width(text: &str) -> usize {
    clusters(text).map(|(_, cells)| cells).sum()
}
