//! PNG snapshots of reference formulas, compared by a hash of their pixels. With `QUENCH_TEX_SNAPSHOTS=1` the
//! PNGs are written to `target/quench-tex-snapshots/` to be looked at, and the hashes printed, to regenerate the
//! table deliberately.

use quench_tex::typeset::{
    Color, Item, RenderOptions, SvgOptions, Typeset, tex_to_png, tex_to_svg, to_png, to_rgba,
    to_svg, typeset,
};

/// The reference set: name, display style, TeX.
pub const FORMULAS: &[(&str, bool, &str)] = &[
    ("quadratic", true, r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}"),
    ("sum", true, r"\sum_{i=1}^{n} i^2 = \frac{n(n+1)(2n+1)}{6}"),
    (
        "integral",
        true,
        r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
    ),
    ("inline", false, r"\sum_{k=0}^n \binom{n}{k} x^k = (1+x)^n"),
    (
        "pmatrix",
        true,
        r"A = \begin{pmatrix} a_{11} & a_{12} \\ a_{21} & a_{22} \end{pmatrix}",
    ),
    (
        "cases",
        true,
        r"|x| = \begin{cases} x & \text{if } x \ge 0 \\ -x & \text{otherwise} \end{cases}",
    ),
    (
        "radicals",
        true,
        r"\sqrt[3]{x^3 + y^3} \le \sqrt{\frac{a}{b}}",
    ),
    (
        "accents",
        true,
        r"\hat{x} + \bar{y} + \vec{v} + \tilde{n} + \dot{a} + \widehat{xyz}",
    ),
    (
        "greek",
        true,
        r"\alpha\beta\gamma\delta + \Gamma\Delta\Theta + \varepsilon\varphi\omega",
    ),
    (
        "blackboard",
        true,
        r"f\colon \mathbb{R}^n \to \mathbb{C}, \quad \mathcal{L}, \mathfrak{g}, \mathbf{v}",
    ),
    (
        "fenced",
        true,
        r"\left( \frac{a}{b} \right)^2 \left\{ x \middle| x > 0 \right\} \left\langle \psi \right\rangle",
    ),
    (
        "limits",
        true,
        r"\lim_{n \to \infty} \left(1 + \frac{1}{n}\right)^n = e",
    ),
    (
        "brace",
        true,
        r"\overbrace{a + b + c}^{3} = \underbrace{x_1 + \cdots + x_n}_{n}",
    ),
    (
        "aligned",
        true,
        r"\begin{aligned} f(x) &= (x+1)^2 \\ &= x^2 + 2x + 1 \end{aligned}",
    ),
    (
        "bigs",
        true,
        r"\big( \Big( \bigg( \Bigg( x \Bigg) \bigg) \Big) \big) \quad \left[ \int_a^b f'(t)\,dt \right]",
    ),
    ("colour", false, r"a + \color{red} b + \textcolor{blue}{c}"),
];

/// A wider set, TeXbook-style: each must typeset, with its ink inside its box vertically; written next to the
/// reference PNGs for looking at, not compared.
pub const TORTURE: &[(&str, bool, &str)] = &[
    (
        "t-nested-fractions",
        true,
        r"\frac{1}{1 + \frac{1}{1 + \frac{1}{1 + \frac{1}{x}}}}",
    ),
    (
        "t-tall-radical",
        true,
        r"\sqrt{\frac{\frac{a}{b}}{\frac{\frac{c}{d}}{\frac{e}{f}}}} + \sqrt[n]{\frac{1}{2}}",
    ),
    (
        "t-assembly",
        true,
        r"\left[ \begin{matrix} 1 \\ 2 \\ 3 \\ 4 \\ 5 \\ 6 \end{matrix} \right\} \left\langle \frac{\frac{a}{b}}{\frac{c}{d}} \right\rangle \left\| \begin{matrix} a \\ b \\ c \\ d \\ e \end{matrix} \right|",
    ),
    ("t-primes", false, r"f'(x) + f''_1(x) + g'^2 + h'''"),
    (
        "t-operators",
        true,
        r"\lim_{x \to 0} \frac{\sin x}{x} = 1, \quad \max_{i} a_i, \quad \det A, \quad \operatorname{rank}(M) \prod_{p} \bigcup_{n=1}^{\infty} A_n",
    ),
    (
        "t-integrals",
        true,
        r"\iint_D f \, dA + \oint_C \mathbf{F} \cdot d\mathbf{r} + \int\limits_0^1 x\,dx",
    ),
    (
        "t-arrows",
        true,
        r"\overrightarrow{AB} + \overleftarrow{xy} + \vec{a} \to a \Rightarrow b \iff c \mapsto d",
    ),
    (
        "t-stacks",
        true,
        r"a \stackrel{\text{def}}{=} b, \quad \overset{!}{=}, \quad \underset{x}{\arg\min}\, f(x)",
    ),
    (
        "t-matrices",
        true,
        r"\begin{vmatrix} a & b \\ c & d \end{vmatrix} \begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix} \begin{Bmatrix} x \end{Bmatrix} \begin{smallmatrix} a & b \\ c & d \end{smallmatrix}",
    ),
    (
        "t-array",
        true,
        r"\begin{array}{|c|l|} \hline x & y \\ \hline 1 & 22 \\ 333 & 4 \\ \hline \end{array}",
    ),
    (
        "t-text",
        false,
        r"\text{area } A = \pi r^2 \textbf{ bold } \textit{italic } \texttt{mono}",
    ),
    (
        "t-dots",
        false,
        r"a_1, a_2, \ldots, a_n \quad 1 + 2 + \cdots + n \quad \vdots \quad \ddots",
    ),
    (
        "t-styles",
        false,
        r"\displaystyle \sum_{n} \textstyle \sum_{n} \scriptstyle x \scriptscriptstyle y \quad \tfrac{1}{2} \dfrac{1}{2}",
    ),
    (
        "t-gathered",
        true,
        r"\begin{gathered} a = b \\ c + d = e + f \end{gathered}",
    ),
    (
        "t-script-heavy",
        true,
        r"e^{-\frac{(x-\mu)^2}{2\sigma^2}} \quad x_{i_{j_k}}^{2^{2^2}} \quad {}^{14}_{6}\mathrm{C}",
    ),
    (
        "t-accents-wide",
        true,
        r"\widetilde{abc} \widehat{AB} \overline{\overline{x}} \underline{y} \ddot{x} \breve{u} \check{c} \acute{e} \grave{a} \mathring{A}",
    ),
];

/// FNV-1a of each reference formula's RGBA at 40 pixels to the em, black on white.
const HASHES: &[(&str, u64)] = &[
    ("quadratic", 0x18323a5f623b5234),
    ("sum", 0x691be9084b8e057a),
    ("integral", 0xaf013803fa08fc06),
    ("inline", 0x3a60f30004ebd83e),
    ("pmatrix", 0xac1842c649eeadb1),
    ("cases", 0xf172524daef908fb),
    ("radicals", 0x116c0e95b151f50d),
    ("accents", 0x1214e5caa3244a98),
    ("greek", 0xe3688bfd803f22d5),
    ("blackboard", 0xadb1c7dfb0cfd6cf),
    ("fenced", 0xa1819ee7919eec83),
    ("limits", 0x7569e536db767cd1),
    ("brace", 0x1b58813bd31162b6),
    ("aligned", 0xee992b6b9f5d6909),
    ("bigs", 0xc153bf9cc7d4879f),
    ("colour", 0x1bb2a353f15a4f11),
];

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn options() -> RenderOptions {
    RenderOptions {
        pixels_per_em: 40.0,
        background: Some(Color::WHITE),
        ..RenderOptions::default()
    }
}

#[quench_test::test]
fn reference_formulas_render() {
    let write = std::env::var_os("QUENCH_TEX_SNAPSHOTS").is_some();
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/quench-tex-snapshots");
    if write {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut mismatched = Vec::new();
    for (name, display, tex) in FORMULAS {
        let set = typeset(&quench_tex::parse(tex).unwrap(), *display).unwrap();
        let raster = to_rgba(&set, &options());
        let hash = fnv(&raster.rgba);
        if write {
            let png = tex_to_png(tex, *display, &options()).unwrap();
            std::fs::write(dir.join(format!("{name}.png")), png).unwrap();
            std::fs::write(
                dir.join(format!("{name}.svg")),
                to_svg(&set, &SvgOptions::default()),
            )
            .unwrap();
            println!("    (\"{name}\", 0x{hash:016x}),");
        }
        let expected = HASHES.iter().find(|(n, _)| n == name).map(|(_, h)| *h);
        if expected != Some(hash) {
            mismatched.push(format!("(\"{name}\", 0x{hash:016x})"));
        }
    }
    assert!(
        mismatched.is_empty(),
        "snapshots changed (look at them with QUENCH_TEX_SNAPSHOTS=1, then update HASHES): {mismatched:?}"
    );
}

#[quench_test::test]
fn torture_set_typesets() {
    let write = std::env::var_os("QUENCH_TEX_SNAPSHOTS").is_some();
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/quench-tex-snapshots");
    for (name, display, tex) in TORTURE {
        let set = typeset(&quench_tex::parse(tex).unwrap(), *display)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let (_, y0, _, y1) = set.ink().unwrap();
        assert!(y1 <= set.ascent + 0.5 && -y0 <= set.descent + 0.5, "{name}");
        if write {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{name}.png")), to_png(&set, &options())).unwrap();
        }
    }
}

/// Whether a glyph item has an outline (a path in the SVG).
fn drawn(set: &Typeset, item: &Item) -> bool {
    match *item {
        Item::Glyph { font, glyph, .. } => set.fonts.face(font).outline(glyph).is_some(),
        Item::Rule { .. } => false,
    }
}

#[quench_test::test]
fn svg_is_well_formed_with_one_path_per_glyph() {
    for (name, display, tex) in FORMULAS {
        let set = typeset(&quench_tex::parse(tex).unwrap(), *display).unwrap();
        let svg = to_svg(&set, &SvgOptions::default());
        assert!(
            svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
            "{name}"
        );
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("fill=\"currentColor\""));
        assert!(
            svg.contains("style=\"vertical-align: -"),
            "{name}: the baseline"
        );
        let glyphs = set.items.iter().filter(|i| drawn(&set, i)).count();
        let rules = set
            .items
            .iter()
            .filter(|i| matches!(i, Item::Rule { .. }))
            .count();
        assert_eq!(svg.matches("<path ").count(), glyphs, "{name}");
        assert_eq!(svg.matches("<rect ").count(), rules, "{name}");
        // quench-svg reads it back and draws ink.
        let image = quench_svg::render(&svg, 1.0).unwrap();
        assert!(image.rgba.chunks(4).any(|p| p[3] > 0), "{name}");
    }
    // A colour instead of currentColor, and `\color` parts with their own.
    let svg = tex_to_svg(
        r"a \color{red} b",
        false,
        &SvgOptions {
            colour: Some(Color::rgb(0x20, 0x40, 0x60)),
            padding: 0.1,
        },
    )
    .unwrap();
    assert!(svg.contains("fill=\"#204060\">"));
    assert!(svg.contains("<path fill=\"#ff0000\""));
}

#[quench_test::test]
fn png_is_transparent_by_default_with_the_baseline_known() {
    let set = typeset(&quench_tex::parse(r"x^2").unwrap(), false).unwrap();
    let options = RenderOptions {
        pixels_per_em: 50.0,
        colour: Color::rgb(10, 20, 30),
        ..RenderOptions::default()
    };
    let raster = to_rgba(&set, &options);
    assert_eq!(
        raster.rgba.len(),
        (raster.width * raster.height * 4) as usize
    );
    assert_eq!(&raster.rgba[..4], &[0, 0, 0, 0]);
    // Straight alpha: an opaque pixel has the colour as it is.
    let solid = raster.rgba.chunks(4).find(|p| p[3] == 255).unwrap();
    assert_eq!(&solid[..3], &[10, 20, 30]);
    // The baseline row: the ascent above it (plus padding), the descent below.
    let ascent_px = set.ascent * 50.0 / 1000.0;
    assert!((raster.baseline as f32 - ascent_px - 2.0).abs() <= 1.0);
    assert!(raster.height >= raster.baseline);
    let png = to_png(&set, &options);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let decoded = quench_image::decode(&png).unwrap();
    assert_eq!(
        (decoded.width, decoded.height),
        (raster.width, raster.height)
    );
    assert_eq!(decoded.rgba, raster.rgba);
}

#[quench_test::test]
fn empty_and_degenerate_formulas_draw() {
    for tex in [
        "",
        "{}",
        r"\left.\right.",
        r"\begin{matrix}\end{matrix}",
        r"\begin{pmatrix}\end{pmatrix}",
        "x^{}_{}",
        r"\sqrt{}",
        r"\sqrt[]{x}",
        r"\frac{}{}",
        r"\binom{}{}",
        r"\overbrace{}^{}",
        r"\underbrace{}",
        r"\widehat{}",
        r"\overrightarrow{}",
        r"\text{}",
        r"\begin{array}{|c|} \hline \end{array}",
        r"\left( \middle| \right)",
        r"\big. \Bigg|",
        r"\quad \! \!",
        r"\mathbb{}",
        r"\color{red}",
        r"\displaystyle",
        "{}^2",
        r"\sum",
        r"\int\limits",
        "''",
    ] {
        for display in [true, false] {
            let set = typeset(&quench_tex::parse(tex).unwrap(), display)
                .unwrap_or_else(|e| panic!("{tex}: {e}"));
            let raster = to_rgba(&set, &RenderOptions::default());
            assert!(raster.width >= 1 && raster.height >= 1, "{tex}");
            let svg = to_svg(&set, &SvgOptions::default());
            assert!(svg.ends_with("</svg>"), "{tex}");
        }
    }
}
