// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use quench_svg::{Error, render, size};

/// The pixel at `(x, y)` as `[r, g, b, a]`.
fn at(image: &quench_image::Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[i],
        image.rgba[i + 1],
        image.rgba[i + 2],
        image.rgba[i + 3],
    ]
}

const RED: [u8; 4] = [255, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[quench_test::test]
fn shapes_are_filled_in_their_colours() {
    let svg = r##"<?xml version="1.0"?><!-- a comment --><svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
        <rect x="2" y="2" width="8" height="8" fill="#f00"/>
        <circle cx="25" cy="10" r="6" fill="rgb(0, 0, 255)"/>
        <polygon points="2,12 10,12 6,19" fill="lime"/>
        <ellipse cx="35" cy="5" rx="3" ry="2" style="fill: #0f0"/>
    </svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!((image.width, image.height), (40, 20));
    assert_eq!(at(&image, 5, 5), RED);
    assert_eq!(at(&image, 25, 10), [0, 0, 255, 255]);
    assert_eq!(at(&image, 6, 14), [0, 255, 0, 255]);
    assert_eq!(at(&image, 35, 5), [0, 255, 0, 255]);
    assert_eq!(at(&image, 20, 1), CLEAR, "the background is clear");
    assert_eq!(at(&image, 25, 0), CLEAR, "outside the circle");
}

#[quench_test::test]
fn a_view_box_scales_and_centres_and_the_density_multiplies() {
    // 10x10 units into 40x20: scaled by 2, centred horizontally.
    let svg = r##"<svg width="40" height="20" viewBox="0 0 10 10"><rect width="10" height="10" fill="#f00"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 5, 10), CLEAR, "left of the centred square");
    assert_eq!(at(&image, 20, 10), RED);
    assert_eq!(at(&image, 35, 10), CLEAR);
    let dense = render(svg, 2.0).unwrap();
    assert_eq!((dense.width, dense.height), (80, 40));
    assert_eq!(at(&dense, 40, 20), RED);
    // Without a size the box is the view box; without either, 300 x 150.
    assert_eq!(size(r#"<svg viewBox="0 0 24 12"/>"#).unwrap(), (24.0, 12.0));
    assert_eq!(size("<svg/>").unwrap(), (300.0, 150.0));
    assert_eq!(
        size(r#"<svg width="2in" height="12pt"/>"#).unwrap(),
        (192.0, 16.0)
    );
}

#[quench_test::test]
fn strokes_have_width_caps_and_joins() {
    let svg = r##"<svg width="40" height="40">
        <rect x="5" y="5" width="20" height="20" fill="none" stroke="#f00" stroke-width="4"/>
        <line x1="30" y1="5" x2="30" y2="30" stroke="#00f" stroke-width="6" stroke-linecap="round"/>
        <polyline points="2,38 8,32 14,38" fill="none" stroke="#0a0" stroke-width="2"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 15, 5), RED, "on the edge, centred on it");
    assert_eq!(at(&image, 15, 4), RED);
    assert_eq!(
        at(&image, 15, 15),
        CLEAR,
        "a stroke only: the inside is empty"
    );
    assert_eq!(
        at(&image, 3, 3),
        RED,
        "the miter join fills the outer corner"
    );
    assert_eq!(at(&image, 30, 17), [0, 0, 255, 255]);
    assert_eq!(
        at(&image, 30, 31)[3],
        255,
        "the round cap runs past the end"
    );
    assert_eq!(at(&image, 30, 36), CLEAR);
}

#[quench_test::test]
fn paths_have_lines_curves_and_arcs() {
    let svg = r##"<svg width="60" height="30">
        <path d="M2 2 h10 v10 h-10 z" fill="#f00"/>
        <path d="M20,2 L30,2 30,12 20,12Z" fill="#00f"/>
        <path d="M40 20 a8 8 0 1 1 0.01 0 z" fill="#0f0"/>
        <path d="M2 28 C 2 18, 12 18, 12 28 Z" fill="#f0f"/>
        <path d="M20 28 Q 25 16 30 28 T 40 28 Z" fill="#0ff"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 6, 6), RED, "relative lines");
    assert_eq!(
        at(&image, 25, 6),
        [0, 0, 255, 255],
        "implicit lineto and an uppercase Z"
    );
    assert!(
        at(&image, 40, 12)[3] == 255 || at(&image, 40, 28)[3] == 255,
        "an arc that is nearly a full circle"
    );
    assert_eq!(at(&image, 7, 26), [255, 0, 255, 255], "a cubic");
    assert_eq!(
        at(&image, 25, 26),
        [0, 255, 255, 255],
        "a quadratic and a smooth one"
    );
}

#[quench_test::test]
fn transforms_groups_use_and_inheritance() {
    let svg = r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink" width="40" height="40">
        <defs><rect id="box" width="5" height="5"/></defs>
        <g fill="#f00" transform="translate(10 10) scale(2)">
            <rect width="5" height="5"/>
            <rect x="5" width="5" height="5" fill="#00f"/>
        </g>
        <use xlink:href="#box" x="30" y="30" fill="#0f0"/>
        <g transform="rotate(90 5 5)"><rect x="0" y="5" width="2" height="2" fill="#000"/></g>
        <g display="none"><rect width="40" height="40" fill="#fff"/></g>
        <rect x="30" y="0" width="5" height="5" fill="#f00" opacity="0.5"/>
        </svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 14, 14), RED, "scaled and moved by the group");
    assert_eq!(at(&image, 24, 14), [0, 0, 255, 255], "overridden fill");
    assert_eq!(
        at(&image, 32, 32),
        [0, 255, 0, 255],
        "use takes the fill it sits under"
    );
    assert_eq!(at(&image, 36, 36), CLEAR);
    assert_eq!(at(&image, 3, 0)[3], 255, "rotated about a point");
    assert_eq!(at(&image, 1, 39), CLEAR, "display none draws nothing");
    let half = at(&image, 32, 2);
    assert!((126..=129).contains(&half[3]) && half[0] == 255, "{half:?}");
}

#[quench_test::test]
fn colours_come_in_every_notation_and_gradients_as_their_first_stop() {
    let svg = r##"<svg width="30" height="10" color="#0000ff">
        <linearGradient id="g"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#00ff00"/></linearGradient>
        <rect width="10" height="10" fill="currentColor"/>
        <rect x="10" width="10" height="10" fill="url(#g)"/>
        <rect x="20" width="10" height="10" fill="#0f08"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 5, 5), [0, 0, 255, 255]);
    assert_eq!(at(&image, 15, 5), RED);
    let translucent = at(&image, 25, 5);
    assert_eq!(
        (translucent[0], translucent[1], translucent[2]),
        (0, 255, 0),
        "straight alpha"
    );
    assert!((135..=137).contains(&translucent[3]), "{translucent:?}");
}

#[quench_test::test]
fn files_that_are_not_svg_or_are_hostile_are_errors() {
    assert_eq!(render("<html/>", 1.0).unwrap_err(), Error::NotSvg);
    assert_eq!(size("<div/>").unwrap_err(), Error::NotSvg);
    for bad in [
        "",
        "<svg",
        "<svg></g>",
        "<svg><g></svg>",
        "<svg a=b/>",
        "<svg a=\"b/>",
        "<!-- <svg/>",
        "no markup",
    ] {
        assert!(matches!(render(bad, 1.0), Err(Error::Invalid(_))), "{bad}");
    }
    assert_eq!(
        render(r#"<svg width="99999" height="10"/>"#, 1.0).unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(
        render(r#"<svg width="10" height="10"/>"#, 4000.0).unwrap_err(),
        Error::TooLarge
    );
    // Nesting too deep, a use that points at itself, and wild numbers are survived.
    let deep = format!("<svg>{}{}</svg>", "<g>".repeat(200), "</g>".repeat(200));
    assert!(render(&deep, 1.0).is_err());
    let looping = r##"<svg width="10" height="10"><g id="a"><use href="#a"/></g></svg>"##;
    assert!(render(looping, 1.0).is_ok());
    let wild = r#"<svg width="10" height="10"><path d="M 1e999 -1e999 L nan 5 A 0 0 0 0 0 5 5 C 1 Z q" fill="red"/>
        <rect width="-5" height="5"/><circle r="0"/><path d="M0 0 L 1e30 1e30 L 5 5" stroke="red"/>
        <polygon points="1,2,3"/><g transform="matrix(0 0 0 0 0 0)"><rect width="5" height="5"/></g></svg>"#;
    assert!(render(wild, 1.0).is_ok());
    // Entities in attribute values.
    let entity = r#"<svg width="4" height="4"><rect width="&#52;" height="&#x34;" fill="&#35;f00" data-x="&amp;&lt;&bogus;"/></svg>"#;
    assert_eq!(at(&render(entity, 1.0).unwrap(), 2, 2), RED);
}

#[quench_test::test]
fn lengths_in_every_unit_and_transforms_in_every_form() {
    // 1in = 96px, 1pc = 16px, 25.4mm = 96px, 2.54cm = 96px, 1pt = 4/3 px, 1em = 16px.
    for (w, want) in [
        ("1in", 96.0),
        ("6pc", 96.0),
        ("25.4mm", 96.0),
        ("2.54cm", 96.0),
        ("72pt", 96.0),
        ("6em", 96.0),
        ("96px", 96.0),
    ] {
        let (got, _) = size(&format!(r#"<svg width="{w}" height="1"/>"#)).unwrap();
        assert!((got - want).abs() < 0.01, "{w}: {got}");
    }
    // A percentage or an unknown unit is not a size: the view box (or the default) is.
    assert_eq!(
        size(r#"<svg width="50%" height="7furlongs" viewBox="0 0 30 20"/>"#).unwrap(),
        (30.0, 20.0)
    );
    let svg = r##"<svg width="60" height="30">
        <rect width="4" height="4" fill="#f00" transform="translate(2)"/>
        <rect width="4" height="4" fill="#0f0" transform="scale(2) translate(5 0)"/>
        <rect width="4" height="4" fill="#00f" transform="skewX(45) translate(0 10)"/>
        <rect width="4" height="4" fill="#ff0" transform="skewY(45) translate(40 0)"/>
        <rect width="4" height="4" fill="#0ff" transform="matrix(1 0 0 1 50 20)"/>
        <rect x="52" y="2" width="4" height="4" fill="#f0f" transform="rotate(180 55 5)"/>
        <rect width="4" height="4" fill="#000" transform="bogus(1) translate(30 20)"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    assert_eq!(at(&image, 3, 2), RED);
    assert_eq!(at(&image, 12, 2), [0, 255, 0, 255]);
    assert_eq!(at(&image, 52, 22), [0, 255, 255, 255]);
    assert_eq!(
        at(&image, 55, 5),
        [255, 0, 255, 255],
        "rotated half a turn about (55, 5)"
    );
    assert_eq!(
        at(&image, 32, 22)[3],
        255,
        "an unknown function leaves the rest of the list"
    );
    // The skews moved their squares away from the origin.
    assert_eq!(
        at(&image, 13, 12),
        [0, 0, 255, 255],
        "skewed along x by its height"
    );
}

#[quench_test::test]
fn paths_with_smooth_curves_flags_without_separators_and_every_cap_and_join() {
    let svg = r##"<svg width="80" height="60">
        <path d="M2 20 C 2 2, 18 2, 18 20 S 34 38, 34 20" fill="none" stroke="#f00" stroke-width="2"/>
        <path d="M40 20 Q 45 5 50 20 T 60 20" fill="none" stroke="#00f" stroke-width="2"/>
        <path d="M2 40a4 4 0 1 0 8 0a4 4 0 1 0 -8 0" fill="#0f0"/>
        <path d="M20 40 l 10 0 l 0 10" fill="none" stroke="#000" stroke-width="6" stroke-linejoin="round" stroke-linecap="square"/>
        <path d="M40 40 l 10 0 l -9 5" fill="none" stroke="#000" stroke-width="4" stroke-linejoin="bevel"/>
        <path d="M60 40 l 10 0 l -9.9 1" fill="none" stroke="#000" stroke-width="4" stroke-miterlimit="2"/>
        <path d="M75 10 l 0 0" stroke="#f0f" stroke-width="6" stroke-linecap="round"/>
        <path d="M75 30 l 0 0" stroke="#0ff" stroke-width="6" stroke-linecap="square"/>
        <path d="M75 50 l 0 0" stroke="#ff0" stroke-width="6"/>
        <path d="M 5 5 L 5 5 L 5 5 Z" stroke="#000"/>
        <path d="M1 1 h" stroke="#000"/><path d="X"/><path d=""/><path/>
        <rect x="0" y="50" width="10" height="8" rx="3" fill="#888"/><rect x="12" y="50" width="4" height="8" ry="2" fill="#888"/>
        <svg x="20" y="52" width="10" height="6"><rect width="4" height="4" fill="#123456"/></svg>
        <polyline points="62,50 66,56 70,50" stroke="#000" stroke-width="1" fill="none" stroke-linecap="round"/>
        <g visibility="hidden"><rect width="80" height="60"/></g><rect width="80" height="60" fill="none"/></svg>"##;
    let image = render(svg, 1.0).unwrap();
    let covered = |x: u32, y: u32| at(&image, x, y)[3] > 0;
    assert!(covered(3, 11) && covered(16, 11), "the cubic stroke");
    assert!(covered(33, 25), "the smooth cubic continues from the first");
    assert!(
        covered(45, 12) && covered(55, 27),
        "the quadratic, and the smooth one after it"
    );
    assert!(covered(6, 40), "an arc that comes back round");
    assert!(
        covered(32, 40) && covered(20, 38) && covered(30, 52),
        "a round join and a square cap"
    );
    assert!(
        covered(75, 10) && covered(75, 30) && !covered(75, 50),
        "round and square dots, no butt dot"
    );
    assert!(
        at(&image, 0, 50)[3] < 128
            && at(&image, 3, 53)[3] == 255
            && at(&image, 12, 50)[3] < 128
            && at(&image, 14, 54)[3] == 255,
        "rounded corners take less of the corner pixel, rx alone gives ry its value, ry alone gives rx"
    );
    assert!(
        covered(22, 54) && at(&image, 22, 54) == [0x12, 0x34, 0x56, 255],
        "a nested svg"
    );
}

#[quench_test::test]
fn the_xml_reader_takes_what_svg_files_contain_and_refuses_the_rest() {
    let svg = "<?xml version=\"1.0\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" [ <!ENTITY a \"b\"> ]>\n<!-- c --><svg:svg xmlns:svg='http://www.w3.org/2000/svg' width='6' height='6'>\n  <![CDATA[ <ignored/> ]]>text <svg:rect width='6' height='6' fill='#f00' /></svg:svg>\n";
    assert_eq!(at(&render(svg, 1.0).unwrap(), 3, 3), RED);
    for bad in [
        "<svg width=\"1\"",
        "<svg><!-- never closed",
        "<svg><![CDATA[ never",
        "<svg><? never",
        "<!DOCTYPE svg [ never",
        "<svg a='1' a",
        "< >",
        "<svg a=\"1></svg>",
        "<svg/ >",
    ] {
        assert!(render(bad, 1.0).is_err(), "{bad}");
    }
    let nested = format!("<svg>{}</svg>", "<a>".repeat(70) + &"</a>".repeat(70));
    assert!(matches!(render(&nested, 1.0), Err(Error::Invalid(_))));
    assert!(
        Error::Invalid("x").to_string().contains('x')
            && Error::NotSvg.to_string().contains("SVG")
            && Error::TooLarge.to_string().contains("large")
    );
    assert!(
        render("<svg width='4' height='4'/>", f32::NAN).is_ok(),
        "a density that is not a number is 1"
    );
}

quench_test::main!();
