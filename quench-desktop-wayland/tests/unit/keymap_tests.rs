use quench_desktop_wayland::keymap::{Keymap, Modifiers, keysym_text};

fn text(map: &Keymap, code: u32, mods: Modifiers) -> Option<String> {
    map.key(code, mods).and_then(|k| k.text)
}

#[quench_test::test]
fn the_us_layout_has_letters_digits_symbols_and_named_keys() {
    let us = Keymap::us();
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..none
    };
    assert_eq!(text(&us, 30, none).as_deref(), Some("a"));
    assert_eq!(text(&us, 30, shift).as_deref(), Some("A"));
    assert_eq!(text(&us, 2, shift).as_deref(), Some("!"));
    assert_eq!(text(&us, 53, none).as_deref(), Some("/"));
    assert_eq!(text(&us, 57, none).as_deref(), Some(" "));
    let enter = us.key(28, none).unwrap();
    assert_eq!(
        (enter.name.as_str(), enter.text, enter.modifier),
        ("Enter", None, false)
    );
    assert_eq!(us.key(105, none).unwrap().name, "ArrowLeft");
    assert_eq!(us.key(104, none).unwrap().name, "PageUp");
    assert_eq!(us.key(15, shift).unwrap().name, "Tab");
    assert_eq!(us.key(59, none).unwrap().name, "F1");
    assert_eq!(us.key(14, none).unwrap().name, "Backspace");
    assert!(us.key(42, none).unwrap().modifier, "shift is a modifier");
    assert!(us.key(58, none).unwrap().modifier);
    assert!(us.key(0x2ff, none).is_none());
}

#[quench_test::test]
fn caps_lock_swaps_the_case_of_letters_only() {
    let us = Keymap::us();
    let caps = Modifiers {
        caps: true,
        ..Modifiers::default()
    };
    assert_eq!(text(&us, 30, caps).as_deref(), Some("A"));
    assert_eq!(
        text(
            &us,
            30,
            Modifiers {
                shift: true,
                caps: true,
                ..Modifiers::default()
            }
        )
        .as_deref(),
        Some("a")
    );
    assert_eq!(
        text(&us, 2, caps).as_deref(),
        Some("1"),
        "digits do not change"
    );
}

#[quench_test::test]
fn modifier_masks_use_the_standard_bits() {
    let m = Modifiers::from_masks(1 | 4, 8, 2 | 64 | 128);
    assert!(m.shift && m.ctrl && m.alt && m.caps && m.meta && m.altgr);
    assert_eq!(Modifiers::from_masks(0, 0, 0), Modifiers::default());
}

#[quench_test::test]
fn a_keymap_in_text_form_is_read_with_its_levels_and_aliases() {
    let text_map = r#"xkb_keymap { xkb_keycodes "x" { <AC01> = 38; <AE02> = 11; <SPCE> = 65; alias <LatA> = <AC01>; <BOGUS> = nine; };
    xkb_symbols "x" { key <LatA> { type="FOUR_LEVEL", symbols[Group1]=[ a, A, aogonek, Aogonek ] };
    key <AE02> { [ 2, at, NoSymbol, EuroSign ] }; key <SPCE> { [ space ] }; key <NOPE> { [ x ] };
    modifier_map Shift { <LFSH> }; }; }"#;
    let map = Keymap::parse(text_map).unwrap();
    let none = Modifiers::default();
    let altgr = Modifiers {
        altgr: true,
        ..none
    };
    assert_eq!(text(&map, 30, altgr).as_deref(), Some("ą"));
    assert_eq!(
        text(
            &map,
            30,
            Modifiers {
                shift: true,
                altgr: true,
                ..none
            }
        )
        .as_deref(),
        Some("Ą")
    );
    assert_eq!(
        text(&map, 30, none).as_deref(),
        Some("a"),
        "an alias names the same code"
    );
    assert_eq!(
        text(&map, 3, altgr).as_deref(),
        Some("2"),
        "no third level: the first"
    );
    assert_eq!(
        text(
            &map,
            3,
            Modifiers {
                shift: true,
                altgr: true,
                ..none
            }
        )
        .as_deref(),
        Some("€")
    );
    assert!(Keymap::parse("nothing here").is_none());
    assert!(
        Keymap::parse("xkb_keymap { xkb_keycodes \"x\" { <A> = 9; }; xkb_symbols \"y\" { }; }")
            .is_none()
    );
}

#[quench_test::test]
fn key_symbol_names_become_text() {
    for (name, want) in [
        ("a", "a"),
        ("7", "7"),
        ("space", " "),
        ("EuroSign", "€"),
        ("zabovedot", "ż"),
        ("U20AC", "€"),
        ("U0041", "A"),
        ("KP_5", "5"),
        ("plus", "+"),
    ] {
        assert_eq!(keysym_text(name).as_deref(), Some(want), "{name}");
    }
    assert_eq!(keysym_text("Frobnicate"), None);
    assert_eq!(keysym_text("Uzzzz"), None);
}

/// How a newer xkbcommon writes a keymap when it is not asked for key symbol names: the symbols are numbers.
const NUMERIC: &str = r#"xkb_keymap {
xkb_keycodes "evdev" {
	<TAB> = 23;
	<AC01> = 38;
	<AC09> = 46;
	<RTRN> = 36;
	<BKSP> = 22;
	<LFSH> = 50;
	<AE02> = 11;
	<SPCE> = 65;
	<KP1> = 87;
	<FK01> = 67;
};
xkb_symbols "pl" {
	key <TAB> {	[ 0xff09, 0xfe20 ] };
	key <AC01> {	[ 0x61, 0x41, 0x1b1, 0x1a1 ] };
	key <AC09> {	[ 0x6c, 0x4c, 0x1b3, 0x1a3 ] };
	key <RTRN> {	[ 0xff0d ] };
	key <BKSP> {	[ 0xff08 ] };
	key <LFSH> {	[ 0xffe1 ] };
	key <AE02> {	[ 0x32, 0x40 ] };
	key <SPCE> {	[ 0x20 ] };
	key <KP1> {	[ 0xffb1 ] };
	key <FK01> {	[ 0xffbe ] };
};
};"#;

#[quench_test::test]
fn a_keymap_written_with_numbers_for_its_symbols_is_read_like_one_with_names() {
    let map = Keymap::parse(NUMERIC).expect("it parses");
    let plain = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let altgr = Modifiers {
        altgr: true,
        ..Modifiers::default()
    };
    let text = |code: u32, m: Modifiers| map.key(code, m).and_then(|k| k.text);
    // Codes are the keycodes minus 8: <AC01> 38 is 30, <AC09> 46 is 38.
    assert_eq!(text(30, plain).as_deref(), Some("a"));
    assert_eq!(text(30, shift).as_deref(), Some("A"));
    assert_eq!(text(38, plain).as_deref(), Some("l"));
    assert_eq!(text(38, altgr).as_deref(), Some("ł"));
    assert_eq!(text(3, shift).as_deref(), Some("@"));
    assert_eq!(text(57, plain).as_deref(), Some(" "));
    assert_eq!(text(79, plain).as_deref(), Some("1"), "a keypad digit");
    // The keys that type nothing carry their names.
    let name = |code: u32| map.key(code, plain).map(|k| k.name);
    assert_eq!(name(28).as_deref(), Some("Enter"));
    assert_eq!(name(14).as_deref(), Some("Backspace"));
    assert_eq!(name(15).as_deref(), Some("Tab"));
    assert_eq!(name(59).as_deref(), Some("F1"));
    assert!(map.key(42, plain).unwrap().modifier, "shift is a modifier");
}
