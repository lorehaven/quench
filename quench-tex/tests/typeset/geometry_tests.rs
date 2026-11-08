//! The layout checked by its geometry: the positions the OpenType MATH constants fix, read from the display list.

use quench_tex::typeset::{Color, Error, Face, Item, Typeset, typeset};
use quench_text::{Font, MathConstants};

const TOLERANCE: f32 = 0.01;

fn set(tex: &str, display: bool) -> Typeset {
    typeset(&quench_tex::parse(tex).unwrap(), display).unwrap()
}

fn constants(set: &Typeset) -> MathConstants {
    *set.fonts.math.math().unwrap().constants().unwrap()
}

/// The math italic form of a Latin letter, as layout draws it.
fn italic(ch: char) -> char {
    char::from_u32(0x1D44E + u32::from(ch) - u32::from('a')).unwrap()
}

/// A glyph item: its id, origin, size.
#[derive(Debug, Clone, Copy)]
struct G {
    glyph: u16,
    x: f32,
    y: f32,
    size: f32,
}

fn glyphs(set: &Typeset) -> Vec<G> {
    set.items
        .iter()
        .filter_map(|item| match *item {
            Item::Glyph {
                font: Face::Math,
                glyph,
                x,
                y,
                size,
                ..
            } => Some(G { glyph, x, y, size }),
            Item::Glyph { .. } | Item::Rule { .. } => None,
        })
        .collect()
}

/// The rules: `(x, y, width, height)`.
fn rules(set: &Typeset) -> Vec<(f32, f32, f32, f32)> {
    set.items
        .iter()
        .filter_map(|item| match *item {
            Item::Rule {
                x,
                y,
                width,
                height,
                ..
            } => Some((x, y, width, height)),
            Item::Glyph { .. } => None,
        })
        .collect()
}

fn math(set: &Typeset) -> &Font {
    &set.fonts.math
}

fn math_glyph(set: &Typeset, ch: char) -> u16 {
    math(set).glyph(ch).unwrap()
}

/// The glyph items drawing `ch` (by its glyph in the math font).
fn find(set: &Typeset, ch: char) -> Vec<G> {
    let id = math_glyph(set, ch);
    glyphs(set).into_iter().filter(|g| g.glyph == id).collect()
}

fn one(set: &Typeset, ch: char) -> G {
    let found = find(set, ch);
    assert_eq!(found.len(), 1, "one `{ch}` expected, found {found:?}");
    found[0]
}

/// The ink box of a glyph item: `(x_min, y_min, x_max, y_max)`.
fn ink(set: &Typeset, g: G) -> (f32, f32, f32, f32) {
    let b = math(set).bounds(g.glyph).unwrap();
    let k = g.size / 1000.0;
    (
        g.x + f32::from(b.x_min) * k,
        g.y + f32::from(b.y_min) * k,
        g.x + f32::from(b.x_max) * k,
        g.y + f32::from(b.y_max) * k,
    )
}

/// The top and bottom of a glyph item's box as layout sees it: its ink, the baseline always inside.
fn top(set: &Typeset, g: G) -> f32 {
    ink(set, g).3.max(g.y)
}

fn bottom(set: &Typeset, g: G) -> f32 {
    ink(set, g).1.min(g.y)
}

fn advance(set: &Typeset, g: G) -> f32 {
    f32::from(math(set).advance(g.glyph)) * g.size / 1000.0
}

fn italic_correction(set: &Typeset, g: G) -> f32 {
    let math = math(set).math().unwrap();
    math.italics_correction(g.glyph).map_or(0.0, f32::from) * g.size / 1000.0
}

/// The union of the ink of glyph items.
fn union(set: &Typeset, items: &[G]) -> (f32, f32, f32, f32) {
    items
        .iter()
        .map(|&g| ink(set, g))
        .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |a, b| {
            (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
        })
}

/// The glyphs of the vertical construction of `ch`: its variants and assembly parts.
fn construction_glyphs(set: &Typeset, ch: char) -> Vec<u16> {
    let math = math(set).math().unwrap();
    let construction = math.vertical_construction(math_glyph(set, ch)).unwrap();
    let mut ids: Vec<u16> = construction.variants.iter().map(|v| v.glyph).collect();
    if let Some(a) = &construction.assembly {
        ids.extend(a.parts.iter().map(|p| p.glyph));
    }
    ids
}

#[quench_test::test]
fn fraction_rule_is_on_the_axis_with_the_font_thickness() {
    for display in [true, false] {
        let s = set(r"\frac{a}{b}", display);
        let k = constants(&s);
        assert_eq!(k.axis_height, 278);
        let rules = rules(&s);
        assert_eq!(rules.len(), 1);
        let (_, y, _, height) = rules[0];
        assert!((height - f32::from(k.fraction_rule_thickness)).abs() < TOLERANCE);
        assert!(
            (y + height / 2.0 - f32::from(k.axis_height)).abs() < TOLERANCE,
            "{y}"
        );

        let (num, den) = (one(&s, italic('a')), one(&s, italic('b')));
        let (shift_up, shift_down, gap_num, gap_den) = if display {
            (
                k.fraction_numerator_display_style_shift_up,
                k.fraction_denominator_display_style_shift_down,
                k.fraction_num_display_style_gap_min,
                k.fraction_denom_display_style_gap_min,
            )
        } else {
            (
                k.fraction_numerator_shift_up,
                k.fraction_denominator_shift_down,
                k.fraction_numerator_gap_min,
                k.fraction_denominator_gap_min,
            )
        };
        assert!(num.y >= f32::from(shift_up) - TOLERANCE);
        assert!(-den.y >= f32::from(shift_down) - TOLERANCE);
        assert!(bottom(&s, num) - (y + height) >= f32::from(gap_num) - TOLERANCE);
        assert!(y - top(&s, den) >= f32::from(gap_den) - TOLERANCE);
        // Numerator and denominator are a style smaller in text style, the same size in display.
        assert_eq!(num.size, if display { 1000.0 } else { 600.0 });
        let centre = |g: G| g.x + advance(&s, g) / 2.0;
        assert!((centre(num) - centre(den)).abs() < 1.0);
    }
}

#[quench_test::test]
fn a_tall_numerator_keeps_the_gap_to_the_rule() {
    let s = set(r"\frac{\frac{a}{b}}{c}", true);
    let k = constants(&s);
    let main = rules(&s)
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let inner_b = one(&s, italic('b'));
    assert!(
        bottom(&s, inner_b) - (main.1 + main.3)
            >= f32::from(k.fraction_num_display_style_gap_min) - TOLERANCE
    );
}

#[quench_test::test]
fn binom_has_no_rule_and_keeps_the_stack_gap() {
    let s = set(r"\binom{n}{k}", true);
    let k = constants(&s);
    assert_eq!(rules(&s), []);
    let (n, kk) = (one(&s, italic('n')), one(&s, italic('k')));
    assert!(bottom(&s, n) - top(&s, kk) >= f32::from(k.stack_display_style_gap_min) - TOLERANCE);
    assert!(n.y >= f32::from(k.stack_top_display_style_shift_up) - TOLERANCE);
    let open = construction_glyphs(&s, '(');
    let close = construction_glyphs(&s, ')');
    let delimiters = glyphs(&s)
        .iter()
        .filter(|g| open.contains(&g.glyph) || close.contains(&g.glyph))
        .count();
    assert_eq!(delimiters, 2);
}

#[quench_test::test]
fn superscripts_are_shifted_by_the_font_constants() {
    let s = set("x^2", false);
    let k = constants(&s);
    let two = one(&s, '2');
    assert_eq!(two.size, 600.0);
    assert!(two.y >= f32::from(k.superscript_shift_up) - TOLERANCE);
    assert!(bottom(&s, two) >= f32::from(k.superscript_bottom_min) - TOLERANCE);
    // A letter's superscript starts after its italic correction.
    let x = one(&s, italic('x'));
    assert!(italic_correction(&s, x) > 0.0);
    assert!((two.x - (x.x + advance(&s, x) + italic_correction(&s, x))).abs() < TOLERANCE);

    // Cramped, under a radical: the cramped shift.
    let s = set(r"\sqrt{x^2}", false);
    let two = one(&s, '2');
    assert!(two.y >= f32::from(k.superscript_shift_up_cramped) - TOLERANCE);
    assert!(two.y < f32::from(k.superscript_shift_up));

    // A superscript with a deep descender is raised by the bottom-min rule.
    let s = set(r"x^{\frac{a}{b}}", false);
    let b = one(&s, italic('b'));
    assert!(bottom(&s, b) >= f32::from(k.superscript_bottom_min) - TOLERANCE);
    assert!(one(&s, italic('a')).y > f32::from(k.superscript_shift_up));

    // A box base (a parenthesised group) drops its superscript from its top.
    let s = set(r"\left(x\right)^2", false);
    let two = one(&s, '2');
    let close = glyphs(&s)
        .into_iter()
        .rfind(|g| construction_glyphs(&s, ')').contains(&g.glyph))
        .unwrap();
    assert!(two.y >= top(&s, close) - f32::from(k.superscript_baseline_drop_max) - TOLERANCE);
}

#[quench_test::test]
fn scripts_scale_to_sixty_and_fifty_percent() {
    let s = set("x^{y^{z}}", false);
    assert_eq!(one(&s, italic('x')).size, 1000.0);
    assert_eq!(one(&s, italic('y')).size, 600.0);
    assert_eq!(one(&s, italic('z')).size, 500.0);
    let s = set(r"\frac{a^b}{c}", false);
    assert_eq!(one(&s, italic('a')).size, 600.0);
    assert_eq!(one(&s, italic('b')).size, 500.0);
    let s = set(r"\begin{smallmatrix} a \end{smallmatrix}", false);
    assert_eq!(one(&s, italic('a')).size, 600.0);
}

#[quench_test::test]
fn subscript_and_superscript_keep_their_gap() {
    for tex in [
        "x_i^2",
        r"x_{\frac{a}{b}}^{\frac{c}{d}}",
        "P_{n}^{k}",
        "x_{T}^{y}",
    ] {
        let s = set(tex, false);
        let k = constants(&s);
        let (sup, sub): (Vec<G>, Vec<G>) = glyphs(&s)
            .into_iter()
            .filter(|g| g.size < 1000.0)
            .partition(|g| g.y > 0.0);
        let mut sup_bottom = sup.iter().map(|&g| bottom(&s, g)).fold(f32::MAX, f32::min);
        let mut sub_top = sub.iter().map(|&g| top(&s, g)).fold(f32::MIN, f32::max);
        for r in rules(&s) {
            if r.1 > 0.0 {
                sup_bottom = sup_bottom.min(r.1);
            } else {
                sub_top = sub_top.max(r.1 + r.3);
            }
        }
        let gap = sup_bottom - sub_top;
        assert!(
            gap >= f32::from(k.sub_superscript_gap_min) - TOLERANCE,
            "{tex}: {gap}"
        );
    }
    let s = set("x_i", false);
    let k = constants(&s);
    assert!(-one(&s, italic('i')).y >= f32::from(k.subscript_shift_down) - TOLERANCE);
    // A letter's subscript is not moved by its italic correction.
    let s = set("f_i", false);
    let f = one(&s, italic('f'));
    assert!((one(&s, italic('i')).x - (f.x + advance(&s, f))).abs() < TOLERANCE);
}

#[quench_test::test]
fn display_sum_takes_the_display_variant_with_limits_above_and_below() {
    let s = set(r"\sum_{i=1}^{n} i", true);
    let k = constants(&s);
    let math = math(&s).math().unwrap();
    let variants = &math
        .vertical_construction(math_glyph(&s, '∑'))
        .unwrap()
        .variants;
    let least = f32::from(k.display_operator_min_height);
    // The first variant at least displayOperatorMinHeight tall, or the largest when none is (Noto Sans Math's
    // largest sum is 1336 units, its minimum 2300).
    let expected = variants
        .iter()
        .find(|v| f32::from(v.advance) >= least)
        .unwrap_or_else(|| variants.last().unwrap());
    assert_ne!(expected.glyph, math_glyph(&s, '∑'));
    let sum = glyphs(&s)
        .into_iter()
        .find(|g| g.glyph == expected.glyph)
        .expect("the display variant of the sum");
    let (sx0, sum_bottom, sx1, sum_top) = ink(&s, sum);
    assert!((f32::midpoint(sum_top, sum_bottom) - f32::from(k.axis_height)).abs() < 1.0);

    let n = one(&s, italic('n'));
    assert_eq!(n.size, 600.0);
    assert!(bottom(&s, n) - sum_top >= f32::from(k.upper_limit_gap_min) - TOLERANCE);
    assert!(n.y - sum_top >= f32::from(k.upper_limit_baseline_rise_min) - TOLERANCE);
    let lower: Vec<G> = glyphs(&s)
        .into_iter()
        .filter(|g| g.y < sum_bottom)
        .collect();
    assert_eq!(lower.len(), 3); // i = 1
    let lower_top = lower.iter().map(|&g| top(&s, g)).fold(f32::MIN, f32::max);
    assert!(sum_bottom - lower_top >= f32::from(k.lower_limit_gap_min) - TOLERANCE);
    assert!(sum_bottom - lower[0].y >= f32::from(k.lower_limit_baseline_drop_min) - TOLERANCE);
    // Limits centred on the operator.
    let centre = f32::midpoint(sx0, sx1);
    let (lx0, _, lx1, _) = union(&s, &lower);
    assert!((f32::midpoint(lx0, lx1) - centre).abs() < 60.0);
    let (nx0, _, nx1, _) = ink(&s, n);
    assert!((f32::midpoint(nx0, nx1) - centre).abs() < 60.0);
}

#[quench_test::test]
fn text_sum_has_its_limits_beside() {
    let s = set(r"\sum_{i=1}^{n} i", false);
    let sum = one(&s, '∑');
    let n = one(&s, italic('n'));
    assert!(n.x >= sum.x + advance(&s, sum) - TOLERANCE);
    assert!(n.y > 0.0);
    let digit = one(&s, '1');
    assert!(digit.y < 0.0 && digit.x > sum.x + advance(&s, sum) / 2.0);
    // `\limits` puts them above and below in text style too.
    let s = set(r"\sum\limits_{i=1}^{n}", false);
    let sum = one(&s, '∑');
    let n = one(&s, italic('n'));
    assert!(n.x < sum.x + advance(&s, sum));
    assert!(bottom(&s, n) > ink(&s, sum).3);
}

#[quench_test::test]
fn integral_scripts_are_beside_with_the_superscript_shifted_by_the_italic_correction() {
    for display in [false, true] {
        let s = set(r"\int_0^1 f", display);
        let ids = construction_glyphs(&s, '∫');
        let integral = glyphs(&s)
            .into_iter()
            .find(|g| ids.contains(&g.glyph))
            .unwrap();
        let (zero, one_) = (one(&s, '0'), one(&s, '1'));
        let italic = italic_correction(&s, integral);
        assert!(italic > 0.0);
        assert!((one_.x - zero.x - italic).abs() < TOLERANCE, "{display}");
        assert!(one_.x > integral.x + advance(&s, integral) / 2.0);
        assert!(zero.y < 0.0 && one_.y > 0.0);
        if display {
            assert_ne!(integral.glyph, math_glyph(&s, '∫'));
        }
    }
}

#[quench_test::test]
fn left_right_delimiters_cover_their_content_around_the_axis() {
    for tex in [
        r"\left( \frac{\frac{a}{b}}{\frac{c}{d}} \right)",
        r"\left( \begin{matrix} a \\ b \\ c \\ d \\ e \\ f \\ g \end{matrix} \right)",
        r"\left( x \right)",
    ] {
        let s = set(tex, true);
        let k = constants(&s);
        let axis = f32::from(k.axis_height);
        let ids = construction_glyphs(&s, '(');
        let (parens, content): (Vec<G>, Vec<G>) =
            glyphs(&s).into_iter().partition(|g| ids.contains(&g.glyph));
        assert!(!parens.is_empty(), "{tex}");
        let (_, low, _, high) = union(&s, &parens);
        let mut covered = content
            .iter()
            .map(|&g| (top(&s, g), bottom(&s, g)))
            .fold((f32::MIN, f32::MAX), |(t, b), (gt, gb)| {
                (t.max(gt), b.min(gb))
            });
        for r in rules(&s) {
            covered = (covered.0.max(r.1 + r.3), covered.1.min(r.1));
        }
        let half = (covered.0 - axis).max(axis - covered.1);
        let wanted = (2.0 * half * 0.901).max(2.0 * half - 500.0);
        assert!(
            high - low >= wanted - 1.0,
            "{tex}: {} < {wanted}",
            high - low
        );
        assert!((f32::midpoint(high, low) - axis).abs() < 1.0, "{tex}");
        if tex.contains("matrix") {
            assert!(parens.len() > 1, "{tex}: an assembly");
        }
    }
}

#[quench_test::test]
fn middle_grows_with_its_fence() {
    let s = set(r"\left\{ \frac{a}{b} \middle| c \right\}", true);
    let ids = construction_glyphs(&s, '|');
    let bar = glyphs(&s)
        .into_iter()
        .find(|g| ids.contains(&g.glyph))
        .unwrap();
    assert_ne!(bar.glyph, math_glyph(&s, '|'));
}

#[quench_test::test]
fn radical_sign_covers_the_radicand_and_the_gap() {
    for (tex, display) in [
        (r"\sqrt{x^2}", false),
        (r"\sqrt{\frac{a}{b}}", true),
        (
            r"\sqrt{\frac{\frac{a}{b}}{\frac{\frac{c}{d}}{\frac{e}{f}}}}",
            true,
        ),
    ] {
        let s = set(tex, display);
        let k = constants(&s);
        let ids = construction_glyphs(&s, '√');
        let (sign, radicand): (Vec<G>, Vec<G>) =
            glyphs(&s).into_iter().partition(|g| ids.contains(&g.glyph));
        let (_, sign_bottom, _, sign_top) = union(&s, &sign);
        let rule = rules(&s)
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let mut radicand_top = radicand
            .iter()
            .map(|&g| top(&s, g))
            .fold(f32::MIN, f32::max);
        let radicand_bottom = radicand
            .iter()
            .map(|&g| bottom(&s, g))
            .fold(f32::MAX, f32::min);
        for r in rules(&s).iter().filter(|r| r.1 < rule.1) {
            radicand_top = radicand_top.max(r.1 + r.3);
        }
        let gap = f32::from(if display {
            k.radical_display_style_vertical_gap
        } else {
            k.radical_vertical_gap
        });
        assert!((rule.3 - f32::from(k.radical_rule_thickness)).abs() < TOLERANCE);
        assert!(rule.1 - radicand_top >= gap - TOLERANCE, "{tex}");
        assert!(
            (sign_top - (rule.1 + rule.3)).abs() < 1.0,
            "{tex}: the sign's top meets the bar's"
        );
        assert!(sign_bottom <= radicand_bottom + TOLERANCE, "{tex}");
        assert!(s.ascent >= rule.1 + rule.3 + f32::from(k.radical_extra_ascender) - TOLERANCE);
        if tex.len() > 30 {
            assert!(sign.len() > 1, "{tex}: an assembly");
        }
    }
}

#[quench_test::test]
fn radical_degree_is_raised_and_kerned() {
    let s = set(r"\sqrt[3]{x}", false);
    let k = constants(&s);
    let three = one(&s, '3');
    assert_eq!(three.size, 500.0);
    let sign = one(&s, '√');
    let (_, sign_bottom, _, sign_top) = ink(&s, sign);
    let raise = f32::from(k.radical_degree_bottom_raise_percent) / 100.0;
    assert!((bottom(&s, three) - (sign_bottom + raise * (sign_top - sign_bottom))).abs() < 1.0);
    assert!((three.x - f32::from(k.radical_kern_before_degree)).abs() < TOLERANCE);
    // The kern after the degree (negative) tucks it over the sign.
    assert!(three.x + advance(&s, three) > sign.x);
}

/// The space from the end of one glyph (its advance and italic correction) to the start of the next.
fn space_between(set: &Typeset, a: char, b: char) -> f32 {
    let (a, b) = (one(set, a), one(set, b));
    b.x - (a.x + advance(set, a) + italic_correction(set, a))
}

#[quench_test::test]
fn atoms_are_spaced_by_their_classes() {
    let mu = 1000.0 / 18.0;
    let s = set("a+b", false);
    assert!((space_between(&s, italic('a'), '+') - 4.0 * mu).abs() < TOLERANCE);
    assert!((space_between(&s, '+', italic('b')) - 4.0 * mu).abs() < TOLERANCE);
    let s = set("a=b", false);
    assert!((space_between(&s, italic('a'), '=') - 5.0 * mu).abs() < TOLERANCE);
    assert!((space_between(&s, '=', italic('b')) - 5.0 * mu).abs() < TOLERANCE);
    // A binary operator with nothing before it is ordinary: no space.
    let s = set("-x", false);
    assert!(space_between(&s, '−', italic('x')).abs() < TOLERANCE);
    let s = set("(-x)", false);
    assert!(space_between(&s, '−', italic('x')).abs() < TOLERANCE);
    let s = set("f(x)", false);
    assert!(space_between(&s, italic('f'), '(').abs() < TOLERANCE);
    assert!(space_between(&s, italic('x'), ')').abs() < TOLERANCE);
    // Punctuation: a thin space after.
    let s = set("a,b", false);
    assert!((space_between(&s, ',', italic('b')) - 3.0 * mu).abs() < TOLERANCE);
    // An operator name: a thin space before a letter.
    let s = set(r"\sin x", false);
    assert!((space_between(&s, 'n', italic('x')) - 3.0 * mu).abs() < TOLERANCE);
    // In a script, medium and thick spaces are dropped.
    let s = set("x^{a+b}", false);
    assert!(space_between(&s, italic('a'), '+').abs() < TOLERANCE);
    // Explicit spaces: 18 mu to the em, scaled in scripts.
    let s = set(r"a\quad b", false);
    assert!((space_between(&s, italic('a'), italic('b')) - 1000.0).abs() < TOLERANCE);
    let s = set(r"x^{a\,b}", false);
    assert!((space_between(&s, italic('a'), italic('b')) - 3.0 * mu * 0.6).abs() < TOLERANCE);
}

#[quench_test::test]
fn accents_attach_at_the_top_accent_attachment() {
    let s = set(r"\hat x", false);
    let k = constants(&s);
    let math = math(&s).math().unwrap();
    let x = one(&s, italic('x'));
    let hat = one(&s, '\u{302}');
    let x_attach = f32::from(math.top_accent_attachment(x.glyph).unwrap());
    let hat_attach = f32::from(math.top_accent_attachment(hat.glyph).unwrap());
    assert!((hat.x + hat_attach - (x.x + x_attach)).abs() < TOLERANCE);
    // x is lower than the accent base height: the accent stays where the font put it.
    assert!(top(&s, x) < f32::from(k.accent_base_height));
    assert!(hat.y.abs() < TOLERANCE);
    // A taller base raises it by the difference.
    let s = set(r"\hat A", false);
    let a = one(&s, char::from_u32(0x1D434).unwrap());
    let hat = one(&s, '\u{302}');
    assert!((hat.y - (top(&s, a) - f32::from(k.accent_base_height))).abs() < TOLERANCE);
    // A wide accent takes the widest variant no wider than its base (TeX's rule 12).
    let s = set(r"\widehat{xyz}", false);
    let letters = [
        one(&s, italic('x')),
        one(&s, italic('y')),
        one(&s, italic('z')),
    ];
    let accent = glyphs(&s)
        .into_iter()
        .find(|g| !letters.iter().any(|l| l.glyph == g.glyph))
        .unwrap();
    let variants = &math
        .horizontal_construction(math_glyph(&s, '\u{302}'))
        .unwrap()
        .variants;
    let at = variants
        .iter()
        .position(|v| v.glyph == accent.glyph)
        .unwrap();
    assert!(at > 0);
    assert!(f32::from(variants[at].advance) <= s.width);
    assert!(f32::from(variants[at + 1].advance) > s.width);
    // An arrow covers it.
    let s = set(r"\overrightarrow{xyz}", false);
    let letters = [
        one(&s, italic('x')),
        one(&s, italic('y')),
        one(&s, italic('z')),
    ];
    let base = union(&s, &letters);
    let arrow: Vec<G> = glyphs(&s)
        .into_iter()
        .filter(|g| !letters.iter().any(|l| l.glyph == g.glyph))
        .collect();
    let (ax0, _, ax1, _) = union(&s, &arrow);
    assert!(ax1 - ax0 >= base.2 - base.0 - 1.0);
}

#[quench_test::test]
fn matrix_columns_are_aligned() {
    let s = set(r"\begin{matrix} 1 & 22 \\ 333 & 4 \end{matrix}", false);
    let left = |s: &Typeset, ch: char| find(s, ch).iter().map(|g| g.x).fold(f32::MAX, f32::min);
    let right = |s: &Typeset, ch: char| {
        find(s, ch)
            .iter()
            .map(|g| g.x + advance(s, *g))
            .fold(f32::MIN, f32::max)
    };
    let centre = |s: &Typeset, ch: char| f32::midpoint(left(s, ch), right(s, ch));
    assert!((centre(&s, '1') - centre(&s, '3')).abs() < TOLERANCE);
    assert!((centre(&s, '2') - centre(&s, '4')).abs() < TOLERANCE);
    assert!(find(&s, '3')[0].y < find(&s, '1')[0].y);
    // Centred on the axis.
    let k = constants(&s);
    assert!(((s.ascent - s.descent) / 2.0 - f32::from(k.axis_height)).abs() < 1.0);
    // Columns a quad apart (twice \arraycolsep).
    assert!((left(&s, '2').min(left(&s, '4')) - right(&s, '3') - 1000.0).abs() < 1.0);

    // `array` with alignments and a vertical rule.
    let s = set(r"\begin{array}{l|r} 1 & 22 \\ 333 & 4 \end{array}", false);
    assert!((left(&s, '1') - left(&s, '3')).abs() < TOLERANCE);
    assert!((right(&s, '2') - right(&s, '4')).abs() < TOLERANCE);
    let rules = rules(&s);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].0 > right(&s, '3') && rules[0].0 < left(&s, '2'));

    // `aligned`: the relations line up.
    let s = set(r"\begin{aligned} a &= b \\ cd &= e \end{aligned}", true);
    let equals = find(&s, '=');
    assert_eq!(equals.len(), 2);
    assert!((equals[0].x - equals[1].x).abs() < TOLERANCE);
    // And are spaced as relations after the empty atom.
    let b = one(&s, italic('b'));
    assert!(b.x - (equals[0].x + advance(&s, equals[0])) > 200.0);
}

#[quench_test::test]
fn big_delimiters_grow_with_their_size() {
    let s = set(r"\big( \Big( \bigg( \Bigg(", false);
    let ids = construction_glyphs(&s, '(');
    let heights: Vec<f32> = glyphs(&s)
        .into_iter()
        .filter(|g| ids.contains(&g.glyph))
        .map(|g| {
            let (_, y0, _, y1) = ink(&s, g);
            y1 - y0
        })
        .collect();
    assert_eq!(heights.len(), 4);
    assert!(heights.windows(2).all(|w| w[1] > w[0]), "{heights:?}");
    assert!(heights[0] >= 1200.0 * 0.9);
}

#[quench_test::test]
fn braces_stretch_over_their_body_with_the_label_beyond() {
    let s = set(r"\overbrace{a+b+c}^{n}", false);
    let k = constants(&s);
    let body = union(&s, &[one(&s, italic('a')), one(&s, italic('c'))]);
    let n = one(&s, italic('n'));
    let brace: Vec<G> = glyphs(&s)
        .into_iter()
        .filter(|g| g.size == 1000.0 && ink(&s, *g).1 > body.3)
        .collect();
    assert!(!brace.is_empty());
    let (bx0, by0, bx1, by1) = union(&s, &brace);
    assert!(bx1 - bx0 >= (body.2 - body.0) * 0.9);
    assert!(by0 - body.3 >= f32::from(k.stretch_stack_gap_below_min) - 1.0);
    assert!(bottom(&s, n) - by1 >= f32::from(k.stretch_stack_gap_above_min) - 1.0);

    let s = set(r"\underbrace{a+b+c}_{n}", false);
    let n = one(&s, italic('n'));
    assert!(n.y < -500.0);
}

#[quench_test::test]
fn overline_and_underline_use_the_bar_constants() {
    let s = set(r"\overline{x}", false);
    let k = constants(&s);
    let x = one(&s, italic('x'));
    let rule = rules(&s)[0];
    assert!((rule.1 - (top(&s, x) + f32::from(k.overbar_vertical_gap))).abs() < TOLERANCE);
    assert!((rule.3 - f32::from(k.overbar_rule_thickness)).abs() < TOLERANCE);
    assert!((s.ascent - (rule.1 + rule.3 + f32::from(k.overbar_extra_ascender))).abs() < TOLERANCE);
    let s = set(r"\underline{x}", false);
    let rule = rules(&s)[0];
    assert!((rule.1 + rule.3 + f32::from(k.underbar_vertical_gap)).abs() < 1.0);
}

#[quench_test::test]
fn letters_are_math_italic_and_alphabets_use_the_math_alphanumerics() {
    let s = set(r"h \Gamma \alpha 1 \mathrm{d} \mathbb{R} \mathbf{v}", false);
    for ch in ['\u{210E}', 'Γ', '\u{1D6FC}', '1', 'd', 'ℝ', '\u{1D42F}'] {
        assert_eq!(find(&s, ch).len(), 1, "{ch}");
    }
}

#[quench_test::test]
fn text_is_set_in_the_text_font() {
    let s = set(r"\text{if } x", false);
    let text = s
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Glyph {
                    font: Face::Text,
                    ..
                }
            )
        })
        .count();
    assert_eq!(text, 2); // `i`, `f`: spaces are not drawn
    assert_eq!(find(&s, italic('x')).len(), 1);
}

#[quench_test::test]
fn box_metrics_hold_the_items() {
    for (_, display, tex) in super::snapshot_tests::FORMULAS {
        let s = set(tex, *display);
        let (_, y0, _, y1) = s.ink().unwrap();
        assert!(y1 <= s.ascent + 0.5, "{tex}: ink above the box");
        assert!(-y0 <= s.descent + 0.5, "{tex}: ink below the box");
        let last = glyphs(&s).iter().map(|g| g.x).fold(f32::MIN, f32::max);
        assert!(last < s.width, "{tex}");
    }
}

#[quench_test::test]
fn a_character_the_font_lacks_is_an_error_naming_it() {
    let error = typeset(&quench_tex::parse(r"\text{漢}").unwrap(), false).unwrap_err();
    assert_eq!(error, Error::MissingGlyph('漢'));
    let message = error.to_string();
    assert!(
        message.contains('漢') && message.contains("U+6F22"),
        "{message}"
    );
    let error = typeset(
        &quench_tex::parse(r"x + \color{nocolour} y").unwrap(),
        false,
    )
    .unwrap_err();
    assert_eq!(error, Error::UnknownColour("nocolour".into()));
    assert!(matches!(
        quench_tex::typeset::tex_to_svg(
            r"\nosuchcommand",
            false,
            &quench_tex::typeset::SvgOptions::default()
        ),
        Err(Error::Parse(_))
    ));
}

#[quench_test::test]
fn colours_are_carried_to_the_items() {
    let s = set(
        r"a \color{red} b \textcolor{#0000ff}{c} \color[rgb]{0,1,0} d",
        false,
    );
    let colour_of = |ch: char| {
        let id = math_glyph(&s, italic(ch));
        s.items.iter().find_map(|i| match *i {
            Item::Glyph { glyph, colour, .. } if glyph == id => Some(colour),
            _ => None,
        })
    };
    assert_eq!(colour_of('a'), Some(None));
    assert_eq!(colour_of('b'), Some(Some(Color::rgb(255, 0, 0))));
    assert_eq!(colour_of('c'), Some(Some(Color::rgb(0, 0, 255))));
    assert_eq!(colour_of('d'), Some(Some(Color::rgb(0, 255, 0))));
}

#[quench_test::test]
fn mathop_takes_limits_and_nolimits() {
    // `\mathop{\Omega}`: limits below in display style, beside in text style, and `\limits`/`\nolimits` override
    // both, as for `\sum`.
    let below = |tex: &str, display: bool| {
        let s = set(tex, display);
        let omega = one(&s, 'Ω');
        let n = one(&s, italic('n'));
        n.x < omega.x + advance(&s, omega) && n.y < -100.0
    };
    assert!(below(r"\mathop{\Omega}_n", true));
    assert!(!below(r"\mathop{\Omega}_n", false));
    assert!(below(r"\mathop{\Omega}\limits_n", false));
    assert!(!below(r"\mathop{\Omega}\nolimits_n", true));
    assert!(below(r"\operatorname*{\Omega}_n", true));
    assert!(!below(r"\operatorname*{\Omega}\nolimits_n", true));
}

#[quench_test::test]
fn hex_colours_reach_the_items_whatever_digit_they_start_with() {
    let s = set(
        r"\color{#1a2b3c} a \textcolor{#123}{b} \color{#ff8000} c",
        false,
    );
    let colour_of = |ch: char| {
        let id = math_glyph(&s, italic(ch));
        s.items.iter().find_map(|i| match *i {
            Item::Glyph { glyph, colour, .. } if glyph == id => colour,
            _ => None,
        })
    };
    assert_eq!(colour_of('a'), Some(Color::rgb(0x1a, 0x2b, 0x3c)));
    assert_eq!(colour_of('b'), Some(Color::rgb(0x11, 0x22, 0x33)));
    assert_eq!(colour_of('c'), Some(Color::rgb(0xff, 0x80, 0x00)));
}
