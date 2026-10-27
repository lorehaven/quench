//! Unit tests for `theme/palette.rs`: every theme defines every colour token.

use quench_style::theme::{Color, GRAY_STEPS, Palette, SUCCESS_STEPS, Theme};

#[quench_test::test]
fn color_displays_as_lowercase_hex_and_exposes_its_channels() {
    let color = Color::rgb(0x0a_b1_ff);
    assert_eq!(color.to_string(), "#0ab1ff");
    assert_eq!(
        (color.red(), color.green(), color.blue()),
        (0x0a, 0xb1, 0xff)
    );
    assert_eq!(Color::rgb(0).to_string(), "#000000");
    // Bits above the 24 colour bits are dropped.
    assert_eq!(Color::rgb(0xff_12_34_56).to_string(), "#123456");
}

#[quench_test::test]
fn every_theme_defines_the_same_tokens_in_the_same_order() {
    let names = |theme: Theme| {
        theme
            .palette()
            .tokens()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    let expected = names(Theme::DefaultDark);
    assert_eq!(
        expected.len(),
        1 + SUCCESS_STEPS.len() + 2 + GRAY_STEPS.len()
    );
    for theme in Theme::iter() {
        assert_eq!(names(theme), expected, "{theme}");
    }
}

#[quench_test::test]
fn tokens_cover_the_named_colours_success_and_gray_steps() {
    let tokens = Theme::DefaultDark.palette().tokens();
    let has = |name: &str| tokens.iter().any(|(n, _)| n == name);
    assert!(has("--bs-warning") && has("--bs-danger") && has("--bs-code-bg"));
    for step in SUCCESS_STEPS {
        assert!(has(&format!("--bs-success-{step}")));
    }
    for step in GRAY_STEPS {
        assert!(has(&format!("--bs-gray-{step}")));
    }
}

#[quench_test::test]
fn rules_emit_one_root_rule_then_the_status_helpers() {
    let css = Palette::new(
        "#ffffff",
        ["#000001", "#000002", "#000003", "#000004", "#000005"],
        "#ff0000",
        "#00ff00",
        ["#000000"; 11],
    )
    .rules()
    .iter()
    .map(quench_style::css::CssRule::render)
    .collect::<String>();
    assert!(css.starts_with(":root {\n    --bs-warning: #ffffff;\n    --bs-success-500: #000001;"));
    assert!(css.contains("--bs-danger: #ff0000;"));
    assert!(css.contains(".color-green {\n    color: var(--bs-success-700);"));
    assert!(css.contains(".color-red"));
}

#[quench_test::test]
fn dark_ramps_run_light_to_dark_and_light_ramps_the_other_way() {
    let brightness = |c: Color| u32::from(c.red()) + u32::from(c.green()) + u32::from(c.blue());
    for theme in Theme::iter() {
        let ramp: Vec<u32> = theme
            .palette()
            .gray
            .iter()
            .map(|c| brightness(*c))
            .collect();
        let dark = theme.to_string().ends_with("dark");
        let ordered = if dark {
            ramp.windows(2).all(|w| w[0] >= w[1])
        } else {
            ramp.windows(2).all(|w| w[0] <= w[1])
        };
        assert!(ordered, "{theme}: {ramp:?}");
    }
}

#[quench_test::test]
fn hex_parses_either_case_and_rejects_malformed_input() {
    assert_eq!(Color::hex("#0AbC9f"), Color::rgb(0x0a_bc_9f));
    for bad in ["0abc9f", "#0abc9", "#0abc9fa", "#0abg9f", ""] {
        let result = std::panic::catch_unwind(|| Color::hex(bad));
        assert!(result.is_err(), "{bad:?} was accepted");
    }
}
