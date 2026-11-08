//! Unicode tables for the text output: superscript and subscript forms, the mathematical alphanumerics
//! (`\mathbb{R}` is `ℝ`) with the letters that live elsewhere in Unicode, and the width of text in terminal cells.

use crate::ast::MathFont;

/// The superscript form of a character, if Unicode has one.
#[must_use]
pub const fn superscript(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'A' => 'ᴬ',
        'B' => 'ᴮ',
        'D' => 'ᴰ',
        'E' => 'ᴱ',
        'G' => 'ᴳ',
        'H' => 'ᴴ',
        'I' => 'ᴵ',
        'J' => 'ᴶ',
        'K' => 'ᴷ',
        'L' => 'ᴸ',
        'M' => 'ᴹ',
        'N' => 'ᴺ',
        'O' => 'ᴼ',
        'P' => 'ᴾ',
        'R' => 'ᴿ',
        'T' => 'ᵀ',
        'U' => 'ᵁ',
        'V' => 'ⱽ',
        'W' => 'ᵂ',
        'α' => 'ᵅ',
        'β' => 'ᵝ',
        'γ' => 'ᵞ',
        'δ' => 'ᵟ',
        'ε' | 'ϵ' => 'ᵋ',
        'θ' => 'ᶿ',
        'ι' => 'ᶥ',
        'φ' | 'ϕ' => 'ᵠ',
        'χ' => 'ᵡ',
        // Primes are superscripts already.
        '′' | '″' | '‴' | '⁗' => ch,
        _ => return None,
    })
}

/// The subscript form of a character, if Unicode has one.
#[must_use]
pub const fn subscript(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' | '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        'β' => 'ᵦ',
        'γ' => 'ᵧ',
        'ρ' => 'ᵨ',
        'φ' | 'ϕ' => 'ᵩ',
        'χ' => 'ᵪ',
        _ => return None,
    })
}

/// `text` in superscript (or subscript) forms, when every character has one.
#[must_use]
pub fn scripted(text: &str, sup: bool) -> Option<String> {
    text.chars()
        .map(|c| if sup { superscript(c) } else { subscript(c) })
        .collect()
}

/// The first code point of each Latin alphabet in the Mathematical Alphanumeric Symbols block, and of its
/// digits (`None` where the alphabet has no digits).
const fn alphabet(font: MathFont) -> Option<(u32, Option<u32>)> {
    Some(match font {
        MathFont::Bold | MathFont::BoldSymbol => (0x1D400, Some(0x1D7CE)),
        MathFont::Calligraphic | MathFont::Script => (0x1D49C, None),
        MathFont::Fraktur => (0x1D504, None),
        MathFont::Blackboard => (0x1D538, Some(0x1D7D8)),
        MathFont::SansSerif => (0x1D5A0, Some(0x1D7E2)),
        MathFont::Monospace => (0x1D670, Some(0x1D7F6)),
        MathFont::Roman | MathFont::Italic => return None,
    })
}

/// The letters of an alphabet that Unicode encoded before the block, which leaves holes in it.
const fn hole(font: MathFont, ch: char) -> Option<char> {
    Some(match (font, ch) {
        (MathFont::Blackboard, 'C') => 'ℂ',
        (MathFont::Blackboard, 'H') => 'ℍ',
        (MathFont::Blackboard, 'N') => 'ℕ',
        (MathFont::Blackboard, 'P') => 'ℙ',
        (MathFont::Blackboard, 'Q') => 'ℚ',
        (MathFont::Blackboard, 'R') => 'ℝ',
        (MathFont::Blackboard, 'Z') => 'ℤ',
        (MathFont::Calligraphic | MathFont::Script, 'B') => 'ℬ',
        (MathFont::Calligraphic | MathFont::Script, 'E') => 'ℰ',
        (MathFont::Calligraphic | MathFont::Script, 'F') => 'ℱ',
        (MathFont::Calligraphic | MathFont::Script, 'H') => 'ℋ',
        (MathFont::Calligraphic | MathFont::Script, 'I') => 'ℐ',
        (MathFont::Calligraphic | MathFont::Script, 'L') => 'ℒ',
        (MathFont::Calligraphic | MathFont::Script, 'M') => 'ℳ',
        (MathFont::Calligraphic | MathFont::Script, 'R') => 'ℛ',
        (MathFont::Calligraphic | MathFont::Script, 'e') => 'ℯ',
        (MathFont::Calligraphic | MathFont::Script, 'g') => 'ℊ',
        (MathFont::Calligraphic | MathFont::Script, 'o') => 'ℴ',
        (MathFont::Fraktur, 'C') => 'ℭ',
        (MathFont::Fraktur, 'H') => 'ℌ',
        (MathFont::Fraktur, 'I') => 'ℑ',
        (MathFont::Fraktur, 'R') => 'ℜ',
        (MathFont::Fraktur, 'Z') => 'ℨ',
        _ => return None,
    })
}

/// `ch` in the alphabet of `font`: `math_alphanumeric('R', MathFont::Blackboard)` is `ℝ`.
///
/// Characters the alphabet does not have (and every character for `Roman` and `Italic`, which stay plain letters) come back
/// unchanged. Bold also covers Greek.
#[must_use]
pub fn math_alphanumeric(ch: char, font: MathFont) -> char {
    if let Some(c) = hole(font, ch) {
        return c;
    }
    let Some((letters, digits)) = alphabet(font) else {
        return ch;
    };
    let code = u32::from(ch);
    let mapped = match ch {
        'A'..='Z' => Some(letters + code - u32::from('A')),
        'a'..='z' => Some(letters + 26 + code - u32::from('a')),
        '0'..='9' => digits.map(|d| d + code - u32::from('0')),
        _ if matches!(font, MathFont::Bold | MathFont::BoldSymbol) => bold_greek(ch),
        _ => None,
    };
    mapped.and_then(char::from_u32).unwrap_or(ch)
}

fn bold_greek(ch: char) -> Option<u32> {
    let code = u32::from(ch);
    match ch {
        'Α'..='Ω' if code != 0x3A2 => Some(0x1D6A8 + code - 0x391),
        'α'..='ω' => Some(0x1D6C2 + code - 0x3B1),
        '∇' => Some(0x1D6C1),
        '∂' => Some(0x1D6DB),
        'ϵ' => Some(0x1D6DC),
        'ϑ' => Some(0x1D6DD),
        'ϰ' => Some(0x1D6DE),
        'ϕ' => Some(0x1D6DF),
        'ϱ' => Some(0x1D6E0),
        'ϖ' => Some(0x1D6E1),
        _ => None,
    }
}

/// Whether a character takes no cell of its own: combining marks, zero-width spaces and joiners, variation
/// selectors, emoji skin-tone modifiers.
#[must_use]
pub const fn is_zero_width(ch: char) -> bool {
    matches!(ch,
        '\u{300}'..='\u{36F}'
        | '\u{483}'..='\u{489}'
        | '\u{591}'..='\u{5BD}'
        | '\u{610}'..='\u{61A}'
        | '\u{64B}'..='\u{65F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{200B}'..='\u{200F}'
        | '\u{2060}'..='\u{2064}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{FE20}'..='\u{FE2F}'
        | '\u{1F3FB}'..='\u{1F3FF}'
        | '\u{E0100}'..='\u{E01EF}')
}

/// Whether a character is East Asian wide, two terminal cells.
#[must_use]
pub const fn is_wide(ch: char) -> bool {
    matches!(ch,
        '\u{1100}'..='\u{115F}'
        | '\u{2E80}'..='\u{303E}'
        | '\u{3041}'..='\u{33FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{A000}'..='\u{A4CF}'
        | '\u{AC00}'..='\u{D7A3}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FE30}'..='\u{FE4F}'
        | '\u{FF00}'..='\u{FF60}'
        | '\u{FFE0}'..='\u{FFE6}'
        | '\u{1F300}'..='\u{1F64F}'
        | '\u{1F900}'..='\u{1F9FF}'
        | '\u{20000}'..='\u{3FFFD}')
}

/// The cells of one character on its own: none for a zero-width one, two for a wide one, one otherwise.
const fn char_cells(c: char) -> usize {
    if is_zero_width(c) {
        0
    } else if is_wide(c) {
        2
    } else {
        1
    }
}

const fn is_regional_indicator(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}

/// Whether `c` can follow a ZWJ in an emoji sequence: an emoji, or a symbol drawn as one (`♀`, `⚕`, `❤`...).
const fn joins_emoji(c: char) -> bool {
    char_cells(c) == 2 || matches!(c, '\u{2000}'..='\u{2BFF}')
}

/// The width of `text` in terminal cells.
///
/// Combining marks take none, East Asian wide characters and emoji two, every other character one. Emoji
/// sequences count as one picture: a character other than whitespace followed by U+FE0F (emoji
/// presentation) takes two cells, characters joined to an emoji by U+200D (ZWJ) add nothing, skin-tone
/// modifiers and U+FE0E/U+FE0F take none, and a flag (two regional indicators) takes two.
///
/// These are the rules of `quench-markdown-term`'s `display_width`, so a formula measured here fits there.
#[must_use]
pub fn display_width(text: &str) -> usize {
    let mut total = 0;
    let mut chars = text.chars().peekable();
    while let Some(first) = chars.next() {
        let mut cells = char_cells(first);
        let mut flag = is_regional_indicator(first);
        while let Some(&c) = chars.peek() {
            if flag && is_regional_indicator(c) {
                cells = 2;
                flag = false;
            } else if c == '\u{FE0F}' {
                // A space stays a space; any other visible character becomes an emoji.
                if cells > 0 && !first.is_whitespace() {
                    cells = 2;
                }
            } else if c == '\u{200D}' && cells == 2 {
                chars.next();
                if chars.peek().is_some_and(|&next| joins_emoji(next)) {
                    chars.next();
                }
                continue;
            } else if char_cells(c) != 0 || c.is_control() {
                break;
            } else {
                flag = false;
            }
            chars.next();
        }
        total += cells;
    }
    total
}
