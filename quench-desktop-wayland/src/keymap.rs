//! Keys: from the key codes the compositor sends to what was typed.
//!
//! A Wayland compositor describes the keyboard layout with an XKB keymap as text. A keyboard event carries only
//! a key code (the Linux input code) and the state of the modifiers. [`Keymap::parse`] reads the part of the
//! keymap that answers "what does this key give at this level": the key names and codes (`xkb_keycodes`) and the
//! symbols of each key (`xkb_symbols`, first group). A keymap this cannot read gives [`Keymap::us`].
//!
//! Levels are the usual four: plain, Shift, `AltGr` (the third-level shift, so `AltGr`+a is `ą` on a Polish
//! programmer's layout), `AltGr`+Shift. Caps Lock swaps the case of letters. Dead keys, compose sequences and
//! groups after the first are not handled.

use std::collections::HashMap;

/// The modifier keys held, as the compositor reports them (`wl_keyboard.modifiers`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the modifier keys
pub struct Modifiers {
    pub shift: bool,
    pub caps: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// `AltGr`: the third-level shift.
    pub altgr: bool,
    pub meta: bool,
}

impl Modifiers {
    /// From the masks of `wl_keyboard.modifiers` (depressed, latched and locked together), with the bit
    /// positions the standard keymaps use: Shift 0, Lock 1, Control 2, Mod1 (Alt) 3, Mod4 (Super) 6, Mod5 (level
    /// three) 7.
    #[must_use]
    pub const fn from_masks(depressed: u32, latched: u32, locked: u32) -> Self {
        let all = depressed | latched | locked;
        Self {
            shift: all & 1 != 0,
            caps: all & 2 != 0,
            ctrl: all & 4 != 0,
            alt: all & 8 != 0,
            meta: all & 64 != 0,
            altgr: all & 128 != 0,
        }
    }
}

/// What a key press is, in the terms of a DOM `KeyboardEvent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// The DOM `key` value: `"a"`, `"Ą"`, `"Enter"`, `"ArrowLeft"`, `"Shift"`.
    pub name: String,
    /// The text it types, if it types any.
    pub text: Option<String>,
    /// Whether it is a modifier key (Shift, Control, Alt, Super, `AltGr`, Caps Lock): such a key changes the
    /// modifier state and is not a press of its own.
    pub modifier: bool,
}

/// The symbols of every key of a layout.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    /// By Linux input code: the keysym names of its levels.
    keys: HashMap<u32, Vec<String>>,
}

impl Keymap {
    /// The US layout, for a compositor that sends no keymap or one that cannot be read.
    #[must_use]
    pub fn us() -> Self {
        const ROWS: &[(u32, &str)] = &[
            (1, "Escape"),
            (2, "1,exclam"),
            (3, "2,at"),
            (4, "3,numbersign"),
            (5, "4,dollar"),
            (6, "5,percent"),
            (7, "6,asciicircum"),
            (8, "7,ampersand"),
            (9, "8,asterisk"),
            (10, "9,parenleft"),
            (11, "0,parenright"),
            (12, "minus,underscore"),
            (13, "equal,plus"),
            (14, "BackSpace"),
            (15, "Tab,ISO_Left_Tab"),
            (16, "q,Q"),
            (17, "w,W"),
            (18, "e,E"),
            (19, "r,R"),
            (20, "t,T"),
            (21, "y,Y"),
            (22, "u,U"),
            (23, "i,I"),
            (24, "o,O"),
            (25, "p,P"),
            (26, "bracketleft,braceleft"),
            (27, "bracketright,braceright"),
            (28, "Return"),
            (29, "Control_L"),
            (30, "a,A"),
            (31, "s,S"),
            (32, "d,D"),
            (33, "f,F"),
            (34, "g,G"),
            (35, "h,H"),
            (36, "j,J"),
            (37, "k,K"),
            (38, "l,L"),
            (39, "semicolon,colon"),
            (40, "apostrophe,quotedbl"),
            (41, "grave,asciitilde"),
            (42, "Shift_L"),
            (43, "backslash,bar"),
            (44, "z,Z"),
            (45, "x,X"),
            (46, "c,C"),
            (47, "v,V"),
            (48, "b,B"),
            (49, "n,N"),
            (50, "m,M"),
            (51, "comma,less"),
            (52, "period,greater"),
            (53, "slash,question"),
            (54, "Shift_R"),
            (56, "Alt_L"),
            (57, "space"),
            (58, "Caps_Lock"),
            (59, "F1"),
            (60, "F2"),
            (61, "F3"),
            (62, "F4"),
            (63, "F5"),
            (64, "F6"),
            (65, "F7"),
            (66, "F8"),
            (67, "F9"),
            (68, "F10"),
            (87, "F11"),
            (88, "F12"),
            (96, "KP_Enter"),
            (97, "Control_R"),
            (100, "ISO_Level3_Shift"),
            (102, "Home"),
            (103, "Up"),
            (104, "Prior"),
            (105, "Left"),
            (106, "Right"),
            (107, "End"),
            (108, "Down"),
            (109, "Next"),
            (110, "Insert"),
            (111, "Delete"),
            (125, "Super_L"),
            (126, "Super_R"),
        ];
        Self {
            keys: ROWS
                .iter()
                .map(|(code, symbols)| (*code, symbols.split(',').map(String::from).collect()))
                .collect(),
        }
    }

    /// Reads the key names and symbols of an XKB keymap in text form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let codes = parse_keycodes(section(text, "xkb_keycodes")?);
        let symbols = section(text, "xkb_symbols")?;
        let mut keys = HashMap::new();
        let mut rest = symbols;
        while let Some(at) = rest.find("key <") {
            rest = &rest[at + 4..];
            let name_end = rest.find('>')?;
            let name = &rest[1..name_end];
            let body_start = rest.find('{')?;
            let mut depth = 0;
            let mut body_end = body_start;
            for (i, c) in rest[body_start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            body_end = body_start + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &rest[body_start + 1..body_end];
            rest = &rest[body_end..];
            // The first symbol list of the key: `symbols[Group1]= [ a, A ]` or just `[ a, A ]`.
            let list = symbol_list(body);
            let (Some(list), Some(&xkb)) = (list, codes.get(name)) else {
                continue;
            };
            if xkb < 8 {
                continue;
            }
            let levels: Vec<String> = list.split(',').map(|s| symbol_name(s.trim())).collect();
            keys.insert(xkb - 8, levels);
        }
        (!keys.is_empty()).then_some(Self { keys })
    }

    /// What `code` (a Linux input code) is with `mods` held; `None` for a key the layout does not have.
    #[must_use]
    pub fn key(&self, code: u32, mods: Modifiers) -> Option<Key> {
        let levels = self.keys.get(&code)?;
        let at = |i: usize| levels.get(i).map(String::as_str);
        let mut level = usize::from(mods.shift) + if mods.altgr { 2 } else { 0 };
        let mut sym = at(level);
        if sym.is_none() || sym == Some("NoSymbol") {
            // No such level: without the third-level shift, then without shift.
            level &= 1;
            sym = at(level);
            if sym.is_none() || sym == Some("NoSymbol") {
                level = 0;
                sym = at(0);
            }
        }
        let mut sym = sym?;
        // Caps Lock swaps the case of a letter.
        if mods.caps && level < 2 {
            let other = at(level ^ 1);
            if let (Some(a), Some(b)) = (keysym_text(sym), other.and_then(keysym_text))
                && a.chars().all(char::is_lowercase)
                && b == a.to_uppercase()
                && a.chars().all(char::is_alphabetic)
            {
                sym = other.unwrap_or(sym);
            } else if let (Some(a), Some(b)) = (keysym_text(sym), other.and_then(keysym_text))
                && a.chars().all(char::is_uppercase)
                && b == a.to_lowercase()
                && a.chars().all(char::is_alphabetic)
            {
                sym = other.unwrap_or(sym);
            }
        }
        Some(describe(sym))
    }
}

/// The first list of symbols in a key's body: `[ a, A ]` or `symbols[Group1]= [ a, A ]` (the `[Group1]` is not it).
fn symbol_list(body: &str) -> Option<&str> {
    let mut previous = '{';
    for (i, c) in body.char_indices() {
        if c == '[' && matches!(previous, '=' | '{' | ',') {
            let end = body[i + 1..].find(']')?;
            return Some(&body[i + 1..i + 1 + end]);
        }
        if !c.is_whitespace() {
            previous = c;
        }
    }
    None
}

/// The text between the braces of the section that starts with `kind` (`xkb_keycodes`, `xkb_symbols`).
fn section<'a>(text: &'a str, kind: &str) -> Option<&'a str> {
    let start = text.find(kind)?;
    let open = start + text[start..].find('{')?;
    let mut depth = 0;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `<AC01> = 38;` and `alias <LatQ> = <AD01>;`.
fn parse_keycodes(section: &str) -> HashMap<&str, u32> {
    let mut codes = HashMap::new();
    let mut aliases = Vec::new();
    for statement in section.split(';') {
        let statement = statement.trim();
        if let Some(alias) = statement.strip_prefix("alias") {
            if let Some((a, b)) = alias.split_once('=') {
                aliases.push((
                    a.trim().trim_matches(['<', '>']),
                    b.trim().trim_matches(['<', '>']),
                ));
            }
        } else if let Some((name, value)) = statement.split_once('=')
            && name.trim().starts_with('<')
            && let Ok(n) = value.trim().parse::<u32>()
        {
            codes.insert(name.trim().trim_matches(['<', '>']), n);
        }
    }
    for (alias, target) in aliases {
        if let Some(&n) = codes.get(target) {
            codes.insert(alias, n);
        }
    }
    codes
}

fn describe(sym: &str) -> Key {
    let key = |name: &str, modifier: bool| Key {
        name: name.to_string(),
        text: None,
        modifier,
    };
    match sym {
        "Return" | "KP_Enter" | "ISO_Enter" => key("Enter", false),
        "BackSpace" => key("Backspace", false),
        "Tab" | "ISO_Left_Tab" => key("Tab", false),
        "Escape" => key("Escape", false),
        "Delete" | "KP_Delete" => key("Delete", false),
        "Insert" => key("Insert", false),
        "Home" | "KP_Home" => key("Home", false),
        "End" | "KP_End" => key("End", false),
        "Left" | "KP_Left" => key("ArrowLeft", false),
        "Right" | "KP_Right" => key("ArrowRight", false),
        "Up" | "KP_Up" => key("ArrowUp", false),
        "Down" | "KP_Down" => key("ArrowDown", false),
        "Prior" | "Page_Up" | "KP_Prior" => key("PageUp", false),
        "Next" | "Page_Down" | "KP_Next" => key("PageDown", false),
        "Shift_L" | "Shift_R" => key("Shift", true),
        "Control_L" | "Control_R" => key("Control", true),
        "Alt_L" | "Alt_R" => key("Alt", true),
        "ISO_Level3_Shift" | "Mode_switch" => key("AltGraph", true),
        "Super_L" | "Super_R" | "Meta_L" | "Meta_R" => key("Meta", true),
        "Caps_Lock" => key("CapsLock", true),
        "Num_Lock" => key("NumLock", true),
        s if s.len() > 1 && s.starts_with('F') && s[1..].parse::<u32>().is_ok() => key(s, false),
        s => keysym_text(s).map_or_else(
            || key(s, false),
            |text| Key {
                name: text.clone(),
                text: Some(text),
                modifier: false,
            },
        ),
    }
}

/// A key symbol as this module names it.
///
/// A compositor may write the symbols of a keymap as numbers (`0x6c`, `0xff0d`: what a newer xkbcommon does when it
/// is not asked for names) instead of names (`l`, `Return`). The number is turned into the name or the character it
/// stands for, and anything else is left as it is.
fn symbol_name(symbol: &str) -> String {
    let Some(hex) = symbol
        .strip_prefix("0x")
        .or_else(|| symbol.strip_prefix("0X"))
    else {
        return symbol.to_string();
    };
    let Ok(value) = u32::from_str_radix(hex, 16) else {
        return symbol.to_string();
    };
    let named = |name: &str| name.to_string();
    match value {
        0 => named("NoSymbol"),
        // Latin-1: the number is the character.
        0x20..=0x7e | 0xa0..=0xff => {
            char::from_u32(value).map_or_else(|| symbol.to_string(), String::from)
        }
        // A Unicode character past Latin-1: the number is the character plus 0x1000000.
        0x0100_0100..=0x0110_ffff => {
            char::from_u32(value - 0x0100_0000).map_or_else(|| symbol.to_string(), String::from)
        }
        // The Latin-2 letters (Polish, Czech, ...) that are not Unicode keysyms.
        0x1a1 => "Ą".into(),
        0x1b1 => "ą".into(),
        0x1a3 => "Ł".into(),
        0x1b3 => "ł".into(),
        0x1a6 => "Ś".into(),
        0x1b6 => "ś".into(),
        0x1ac => "Ź".into(),
        0x1bc => "ź".into(),
        0x1af => "Ż".into(),
        0x1bf => "ż".into(),
        0x1c6 => "Ć".into(),
        0x1e6 => "ć".into(),
        0x1ca => "Ę".into(),
        0x1ea => "ę".into(),
        0x1d1 => "Ń".into(),
        0x1f1 => "ń".into(),
        0xff08 => named("BackSpace"),
        0xff09 => named("Tab"),
        0xfe20 => named("ISO_Left_Tab"),
        0xff0d => named("Return"),
        0xff8d => named("KP_Enter"),
        0xff1b => named("Escape"),
        0xffff => named("Delete"),
        0xff63 => named("Insert"),
        0xff50 => named("Home"),
        0xff57 => named("End"),
        0xff51 => named("Left"),
        0xff52 => named("Up"),
        0xff53 => named("Right"),
        0xff54 => named("Down"),
        0xff55 => named("Page_Up"),
        0xff56 => named("Page_Down"),
        0xffe1 => named("Shift_L"),
        0xffe2 => named("Shift_R"),
        0xffe3 => named("Control_L"),
        0xffe4 => named("Control_R"),
        0xffe9 => named("Alt_L"),
        0xffea => named("Alt_R"),
        0xfe03 => named("ISO_Level3_Shift"),
        0xffeb => named("Super_L"),
        0xffec => named("Super_R"),
        0xffe7 => named("Meta_L"),
        0xffe8 => named("Meta_R"),
        0xffe5 => named("Caps_Lock"),
        0xff7f => named("Num_Lock"),
        // The keypad's digits and operators type what they show.
        0xffb0..=0xffb9 => char::from_u32(u32::from(b'0') + (value - 0xffb0))
            .map_or_else(String::new, String::from),
        0xffaa => "*".into(),
        0xffab => "+".into(),
        0xffad => "-".into(),
        0xffae => ".".into(),
        0xffaf => "/".into(),
        0xffbe..=0xffc9 => format!("F{}", value - 0xffbd),
        _ => symbol.to_string(),
    }
}

/// The text a key symbol types.
#[must_use]
pub fn keysym_text(name: &str) -> Option<String> {
    const NAMED: &[(&str, &str)] = &[
        ("space", " "),
        ("exclam", "!"),
        ("quotedbl", "\""),
        ("numbersign", "#"),
        ("dollar", "$"),
        ("percent", "%"),
        ("ampersand", "&"),
        ("apostrophe", "'"),
        ("parenleft", "("),
        ("parenright", ")"),
        ("asterisk", "*"),
        ("plus", "+"),
        ("comma", ","),
        ("minus", "-"),
        ("period", "."),
        ("slash", "/"),
        ("colon", ":"),
        ("semicolon", ";"),
        ("less", "<"),
        ("equal", "="),
        ("greater", ">"),
        ("question", "?"),
        ("at", "@"),
        ("bracketleft", "["),
        ("backslash", "\\"),
        ("bracketright", "]"),
        ("asciicircum", "^"),
        ("underscore", "_"),
        ("grave", "`"),
        ("braceleft", "{"),
        ("bar", "|"),
        ("braceright", "}"),
        ("asciitilde", "~"),
        ("EuroSign", "€"),
        ("sterling", "£"),
        ("section", "§"),
        ("degree", "°"),
        ("aogonek", "ą"),
        ("Aogonek", "Ą"),
        ("cacute", "ć"),
        ("Cacute", "Ć"),
        ("eogonek", "ę"),
        ("Eogonek", "Ę"),
        ("lstroke", "ł"),
        ("Lstroke", "Ł"),
        ("nacute", "ń"),
        ("Nacute", "Ń"),
        ("oacute", "ó"),
        ("Oacute", "Ó"),
        ("sacute", "ś"),
        ("Sacute", "Ś"),
        ("zacute", "ź"),
        ("Zacute", "Ź"),
        ("zabovedot", "ż"),
        ("Zabovedot", "Ż"),
        ("adiaeresis", "ä"),
        ("Adiaeresis", "Ä"),
        ("odiaeresis", "ö"),
        ("Odiaeresis", "Ö"),
        ("udiaeresis", "ü"),
        ("Udiaeresis", "Ü"),
        ("ssharp", "ß"),
        ("eacute", "é"),
        ("Eacute", "É"),
        ("egrave", "è"),
        ("agrave", "à"),
        ("ccedilla", "ç"),
        ("ntilde", "ñ"),
        ("Ntilde", "Ñ"),
        ("KP_Space", " "),
    ];
    if let Some((_, text)) = NAMED.iter().find(|(n, _)| *n == name) {
        return Some((*text).to_string());
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(c.to_string());
    }
    if let Some(digit) = name.strip_prefix("KP_")
        && digit.len() == 1
        && digit.chars().all(|c| c.is_ascii_digit())
    {
        return Some(digit.to_string());
    }
    let hex = name.strip_prefix('U')?;
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .map(String::from)
}
