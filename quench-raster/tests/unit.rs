// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::suboptimal_flops
)]

use quench_raster::{Canvas, Color, Path};

fn alpha_at(c: &Canvas, x: usize, y: usize) -> u8 {
    c.bytes()[(y * c.width() + x) * 4 + 3]
}

/// The sum of coverage over the canvas, as pixels of area.
fn area(c: &Canvas) -> f32 {
    (0..c.height())
        .flat_map(|y| (0..c.width()).map(move |x| (x, y)))
        .map(|(x, y)| f32::from(alpha_at(c, x, y)) / 255.0)
        .sum()
}

#[quench_test::test]
fn a_new_canvas_is_transparent_and_clear_replaces_every_pixel() {
    let mut c = Canvas::new(4, 3);
    assert_eq!((c.width(), c.height(), c.bytes().len()), (4, 3, 48));
    assert!(c.bytes().iter().all(|b| *b == 0));
    c.clear(Color::rgb(10, 20, 30));
    assert_eq!(c.pixel(3, 2), Color::rgb(10, 20, 30));
    // The layout is premultiplied BGRA: blue first.
    assert_eq!(&c.bytes()[..4], &[30, 20, 10, 255]);
    assert_eq!(c.pixel(9, 9), Color::TRANSPARENT, "outside is transparent");
}

#[quench_test::test]
fn a_rectangle_fills_whole_pixels_and_is_clipped_to_the_canvas() {
    let mut c = Canvas::new(8, 8);
    c.fill_rect(2.0, 3.0, 3.0, 2.0, Color::rgb(255, 0, 0));
    for y in 0..8 {
        for x in 0..8 {
            let inside = (2..5).contains(&x) && (3..5).contains(&y);
            assert_eq!(c.pixel(x, y) == Color::rgb(255, 0, 0), inside, "({x},{y})");
        }
    }
    // Partly or wholly outside, and a degenerate one: nothing panics, only what is inside changes.
    let mut d = Canvas::new(4, 4);
    d.fill_rect(-10.0, -10.0, 12.0, 12.0, Color::WHITE);
    d.fill_rect(100.0, 100.0, 5.0, 5.0, Color::WHITE);
    d.fill_rect(1.0, 1.0, 0.0, 5.0, Color::WHITE);
    d.fill_rect(1.0, 1.0, -3.0, 5.0, Color::WHITE);
    assert_eq!(area(&d), 4.0, "only the 2x2 that overlaps");
}

#[quench_test::test]
fn blending_is_source_over_on_premultiplied_pixels() {
    let mut c = Canvas::new(1, 1);
    c.clear(Color::BLACK);
    c.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgba(255, 255, 255, 128));
    assert_eq!(
        c.pixel(0, 0),
        Color::rgb(128, 128, 128),
        "half white over black"
    );
    // Over a transparent pixel the alpha is the source's, and reading it back un-premultiplies.
    let mut t = Canvas::new(1, 1);
    t.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgba(200, 100, 50, 128));
    let p = t.pixel(0, 0);
    assert_eq!(p.a, 128);
    assert!(
        p.r.abs_diff(200) <= 1 && p.g.abs_diff(100) <= 1 && p.b.abs_diff(50) <= 1,
        "{p:?}"
    );
    // Two half layers: 1 - 0.5 * 0.5 = 0.75 opaque.
    t.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgba(200, 100, 50, 128));
    assert!(
        alpha_at(&t, 0, 0).abs_diff(192) <= 1,
        "{}",
        alpha_at(&t, 0, 0)
    );
    // A transparent colour changes nothing.
    let before = t.clone();
    t.fill_rect(0.0, 0.0, 1.0, 1.0, Color::TRANSPARENT);
    assert_eq!(t, before);
}

#[quench_test::test]
fn the_clip_limits_every_kind_of_drawing_and_can_be_restored() {
    let mut c = Canvas::new(10, 10);
    let before = c.set_clip(2, 2, 4, 4);
    c.fill_rect(0.0, 0.0, 10.0, 10.0, Color::WHITE);
    assert_eq!(area(&c), 16.0);
    c.fill_rounded_rect(0.0, 0.0, 10.0, 10.0, 0.0, Color::BLACK);
    let mut p = Path::new();
    p.rect(0.0, 0.0, 10.0, 10.0);
    c.fill_path(&p, 0.0, 0.0, Color::WHITE);
    assert_eq!(area(&c), 16.0, "still only the clip");
    c.restore_clip(before);
    c.fill_rect(0.0, 0.0, 1.0, 1.0, Color::WHITE);
    assert_eq!(area(&c), 17.0);
    // A clip that starts outside the canvas is cut to it.
    c.set_clip(-5, -5, 100, 100);
    c.fill_rect(8.0, 8.0, 10.0, 10.0, Color::WHITE);
    assert_eq!(area(&c), 21.0);
}

#[quench_test::test]
fn a_path_rectangle_on_whole_pixels_is_exact_and_on_half_pixels_is_half_covered() {
    let mut c = Canvas::new(8, 8);
    let mut p = Path::new();
    p.rect(2.0, 2.0, 3.0, 2.0);
    c.fill_path(&p, 0.0, 0.0, Color::WHITE);
    assert_eq!(area(&c), 6.0);
    assert_eq!(
        (
            alpha_at(&c, 2, 2),
            alpha_at(&c, 4, 3),
            alpha_at(&c, 5, 2),
            alpha_at(&c, 1, 2)
        ),
        (255, 255, 0, 0)
    );
    // Moved by half a pixel each way: the edge pixels are half covered and the corner a quarter.
    let mut d = Canvas::new(8, 8);
    d.fill_path(&p, 0.5, 0.5, Color::WHITE);
    assert!((area(&d) - 6.0).abs() < 0.05, "{}", area(&d));
    assert!(
        alpha_at(&d, 2, 3).abs_diff(128) <= 1,
        "left edge: {}",
        alpha_at(&d, 2, 3)
    );
    assert!(
        alpha_at(&d, 2, 2).abs_diff(64) <= 1,
        "corner: {}",
        alpha_at(&d, 2, 2)
    );
    assert_eq!(alpha_at(&d, 3, 3), 255, "inside");
}

#[quench_test::test]
fn a_triangle_covers_its_area_and_a_circle_its_pi_r_squared() {
    let mut c = Canvas::new(64, 64);
    let mut t = Path::new();
    t.move_to(4.3, 5.1)
        .line_to(50.7, 12.9)
        .line_to(20.2, 58.4)
        .close();
    c.fill_path(&t, 0.0, 0.0, Color::WHITE);
    let exact = 0.5 * ((50.7_f32 - 4.3) * (58.4 - 5.1) - (20.2 - 4.3) * (12.9 - 5.1)).abs();
    assert!(
        (area(&c) - exact).abs() / exact < 0.005,
        "{} against {exact}",
        area(&c)
    );
    let mut d = Canvas::new(64, 64);
    let mut circle = Path::new();
    circle.ellipse(32.0, 32.0, 20.0, 20.0);
    d.fill_path(&circle, 0.0, 0.0, Color::WHITE);
    let pi_r2 = std::f32::consts::PI * 400.0;
    assert!(
        (area(&d) - pi_r2).abs() / pi_r2 < 0.005,
        "{} against {pi_r2}",
        area(&d)
    );
    assert_eq!(alpha_at(&d, 32, 32), 255);
    assert_eq!(alpha_at(&d, 2, 2), 0);
}

#[quench_test::test]
fn curves_flatten_to_the_right_shape() {
    // A quadratic from (0,0) with control (8,8) to (16,0), closed: the area under a parabola is 2/3 of its box.
    let mut c = Canvas::new(16, 8);
    let mut p = Path::new();
    p.move_to(0.0, 8.0).quad_to(8.0, -8.0, 16.0, 8.0).close();
    c.fill_path(&p, 0.0, 0.0, Color::WHITE);
    let exact = 16.0 * 8.0 * 2.0 / 3.0;
    assert!(
        (area(&c) - exact).abs() / exact < 0.01,
        "{} against {exact}",
        area(&c)
    );
    // A cubic that is really a straight line fills nothing, and a path with one point nothing at all.
    let mut flat = Canvas::new(8, 8);
    let mut l = Path::new();
    l.move_to(1.0, 1.0)
        .cubic_to(2.0, 2.0, 3.0, 3.0, 4.0, 4.0)
        .close();
    flat.fill_path(&l, 0.0, 0.0, Color::WHITE);
    assert!(area(&flat) < 0.01);
    let mut single = Path::new();
    single.move_to(3.0, 3.0).close();
    assert_eq!(single.bounds(), None);
    // `quad_to` and `cubic_to` with no current point start one.
    let mut start = Path::new();
    start
        .quad_to(1.0, 1.0, 2.0, 2.0)
        .cubic_to(3.0, 3.0, 4.0, 4.0, 5.0, 5.0);
    assert_eq!(start.bounds(), Some((2.0, 2.0, 5.0, 5.0)));
}

#[quench_test::test]
fn opposite_windings_make_a_hole_and_the_same_direction_does_not_double() {
    let mut c = Canvas::new(16, 16);
    let mut ring = Path::new();
    ring.rect(2.0, 2.0, 12.0, 12.0);
    // The inner contour runs the other way.
    ring.move_to(5.0, 5.0)
        .line_to(5.0, 11.0)
        .line_to(11.0, 11.0)
        .line_to(11.0, 5.0)
        .close();
    c.fill_path(&ring, 0.0, 0.0, Color::WHITE);
    assert_eq!(area(&c), 144.0 - 36.0);
    assert_eq!((alpha_at(&c, 8, 8), alpha_at(&c, 3, 3)), (0, 255));
    // Two overlapping squares in the same direction are one shape, not twice as opaque.
    let mut d = Canvas::new(16, 16);
    let mut two = Path::new();
    two.rect(2.0, 2.0, 8.0, 8.0).rect(6.0, 6.0, 8.0, 8.0);
    d.fill_path(&two, 0.0, 0.0, Color::WHITE);
    assert_eq!(alpha_at(&d, 7, 7), 255);
    assert_eq!(area(&d), 64.0 + 64.0 - 16.0);
}

#[quench_test::test]
fn a_path_partly_outside_the_canvas_keeps_the_part_that_is_inside_exact() {
    // A big triangle whose corners are far outside: what shows is a slice of it, with the right slope.
    let mut c = Canvas::new(10, 10);
    let mut p = Path::new();
    p.move_to(-100.0, -100.0)
        .line_to(100.0, -100.0)
        .line_to(100.0, 100.0)
        .close();
    c.fill_path(&p, 0.0, 0.0, Color::WHITE);
    // The line y = x splits the canvas: above-right is covered, below-left is not.
    assert_eq!(alpha_at(&c, 8, 1), 255);
    assert_eq!(alpha_at(&c, 1, 8), 0);
    assert!(
        alpha_at(&c, 5, 5).abs_diff(128) <= 1,
        "the diagonal is half covered: {}",
        alpha_at(&c, 5, 5)
    );
    assert!(
        (area(&c) - 50.0).abs() < 0.05,
        "half of the 10x10: {}",
        area(&c)
    );
    // A shape wholly left of, above, or right of the canvas draws nothing and does not panic.
    for (x, y) in [(-50.0, 2.0), (2.0, -50.0), (50.0, 2.0), (2.0, 50.0)] {
        let mut q = Path::new();
        q.rect(x, y, 5.0, 5.0);
        let mut d = Canvas::new(10, 10);
        d.fill_path(&q, 0.0, 0.0, Color::WHITE);
        assert_eq!(area(&d), 0.0, "{x},{y}");
    }
    // An edge that starts above the canvas.
    let mut r = Path::new();
    r.move_to(3.0, -5.0)
        .line_to(6.0, -5.0)
        .line_to(6.0, 4.0)
        .line_to(3.0, 4.0)
        .close();
    let mut e = Canvas::new(10, 10);
    e.fill_path(&r, 0.0, 0.0, Color::WHITE);
    assert_eq!(area(&e), 12.0);
}

#[quench_test::test]
fn rounded_rectangles_have_soft_round_corners_and_solid_middles() {
    let mut c = Canvas::new(40, 40);
    c.fill_rounded_rect(4.0, 4.0, 32.0, 32.0, 10.0, Color::WHITE);
    assert_eq!(alpha_at(&c, 20, 20), 255);
    assert_eq!(alpha_at(&c, 20, 5), 255, "the straight edge");
    assert_eq!(alpha_at(&c, 4, 4), 0, "the very corner is cut away");
    assert_eq!(alpha_at(&c, 6, 6), 0, "outside the arc");
    assert!(
        alpha_at(&c, 5, 9) > 0 && alpha_at(&c, 5, 9) < 255,
        "on the arc it is anti-aliased: {}",
        alpha_at(&c, 5, 9)
    );
    // Symmetric on all four sides (pixel x mirrors to 39 - x).
    for (x, y) in [
        (5, 9),
        (34, 9),
        (5, 30),
        (34, 30),
        (9, 5),
        (30, 5),
        (9, 34),
        (30, 34),
    ] {
        assert_eq!(alpha_at(&c, x, y), alpha_at(&c, 5, 9), "({x},{y})");
    }
    // The area is that of a rectangle less four corner pieces: (4 - pi) r^2.
    let exact = 32.0 * 32.0 - (4.0 - std::f32::consts::PI) * 100.0;
    assert!(
        (area(&c) - exact).abs() / exact < 0.01,
        "{} against {exact}",
        area(&c)
    );
    // Radius 0 is a plain rectangle; a radius beyond half the side is a pill, not a crash.
    let mut r = Canvas::new(20, 20);
    r.fill_rounded_rect(2.0, 2.0, 16.0, 8.0, 0.0, Color::WHITE);
    assert_eq!(area(&r), 128.0);
    let mut pill = Canvas::new(20, 20);
    pill.fill_rounded_rect(2.0, 2.0, 16.0, 8.0, 100.0, Color::WHITE);
    assert!(
        area(&pill) < 128.0 && area(&pill) > 100.0,
        "{}",
        area(&pill)
    );
    pill.fill_rounded_rect(2.0, 2.0, 0.0, 8.0, 3.0, Color::WHITE); // nothing to draw
}

#[quench_test::test]
fn a_stroke_is_a_ring_of_the_given_width_inside_the_bounds() {
    let mut c = Canvas::new(40, 40);
    c.stroke_rounded_rect(4.0, 4.0, 32.0, 32.0, 6.0, 2.0, Color::WHITE);
    assert_eq!(alpha_at(&c, 20, 20), 0, "the middle is empty");
    assert_eq!(alpha_at(&c, 20, 4), 255, "the top edge row");
    assert_eq!(alpha_at(&c, 20, 5), 255);
    assert_eq!(alpha_at(&c, 20, 6), 0, "two pixels in");
    // The ring is the outer rounded rectangle less the inner one (28 wide, radius 4).
    let corner = |r: f32| (4.0 - std::f32::consts::PI) * r * r;
    let ring = (32.0 * 32.0 - corner(6.0)) - (28.0 * 28.0 - corner(4.0));
    assert!(
        (area(&c) - ring).abs() / ring < 0.01,
        "{} against {ring}",
        area(&c)
    );
}

#[quench_test::test]
fn colours_come_from_hex_and_scale_their_opacity() {
    assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
    assert_eq!(
        Color::from_hex("#102030"),
        Some(Color::rgb(0x10, 0x20, 0x30))
    );
    assert_eq!(
        Color::from_hex("10203040"),
        Some(Color::rgba(0x10, 0x20, 0x30, 0x40))
    );
    assert_eq!(Color::from_hex("#abc"), Some(Color::rgb(0xaa, 0xbb, 0xcc)));
    for bad in [
        "", "#", "#ff", "#ffff", "#ggg", "#12345", "é12345", "#1234567",
    ] {
        assert_eq!(Color::from_hex(bad), None, "{bad:?}");
    }
    assert_eq!(Color::rgb(1, 2, 3).with_opacity(0.5).a, 128);
    assert_eq!(Color::rgb(1, 2, 3).with_opacity(2.0).a, 255);
    assert_eq!(Color::rgb(1, 2, 3).with_opacity(-1.0).a, 0);
}

#[quench_test::test]
fn the_bytes_can_be_written_directly_and_a_path_can_be_measured() {
    let mut c = Canvas::new(2, 1);
    c.bytes_mut()[..4].copy_from_slice(&[0, 0, 255, 255]);
    assert_eq!(c.pixel(0, 0), Color::rgb(255, 0, 0));
    let mut p = Path::new();
    p.rect(1.0, 2.0, 3.0, 4.0);
    assert_eq!(p.bounds(), Some((1.0, 2.0, 4.0, 6.0)));
    assert_eq!(Path::new().bounds(), None);
    // The contour being drawn counts before it is closed.
    let mut open = Path::new();
    open.move_to(0.0, 0.0).line_to(5.0, 5.0);
    assert_eq!(open.bounds(), Some((0.0, 0.0, 5.0, 5.0)));
}

#[quench_test::test]
fn a_canvas_can_draw_into_bytes_that_live_elsewhere() {
    let mut memory = vec![0u8; 4 * 3 * 4 + 8]; // a little more than 4x3 pixels: the rest is left alone
    memory[48..].fill(0xAA);
    {
        let mut c = Canvas::over(4, 3, memory.as_mut_slice()).unwrap();
        c.clear(Color::rgb(1, 2, 3));
        c.fill_rect(1.0, 1.0, 2.0, 1.0, Color::rgb(200, 0, 0));
        assert_eq!(c.pixel(0, 0), Color::rgb(1, 2, 3));
        assert_eq!(c.pixel(1, 1), Color::rgb(200, 0, 0));
        assert_eq!(c.bytes().len(), 48);
        c.bytes_mut()[0] = 9;
    }
    assert_eq!(&memory[..4], &[9, 2, 1, 255], "written in place");
    assert!(
        memory[48..].iter().all(|b| *b == 0xAA),
        "beyond the canvas is untouched"
    );
    // Too short, and a size that overflows, are refused rather than panicking later.
    assert!(Canvas::over(4, 3, vec![0u8; 47]).is_none());
    assert!(Canvas::over(usize::MAX, 2, vec![0u8; 8]).is_none());
    assert!(Canvas::over(0, 0, Vec::<u8>::new()).is_some());
}

#[quench_test::test]
fn each_corner_has_its_own_radius() {
    let mut c = Canvas::new(40, 40);
    c.fill_rrect((0.0, 0.0, 40.0, 40.0), [20.0, 0.0, 10.0, 0.0], Color::WHITE);
    assert_eq!(alpha_at(&c, 0, 0), 0, "the top left is a quarter circle");
    assert_eq!(alpha_at(&c, 39, 0), 255, "the top right is square");
    assert_eq!(alpha_at(&c, 0, 39), 255);
    assert_eq!(
        alpha_at(&c, 39, 39),
        0,
        "the bottom right is rounder than a pixel"
    );
    let expected = 40.0 * 40.0
        - (400.0 - std::f32::consts::PI * 400.0 / 4.0)
        - (100.0 - std::f32::consts::PI * 100.0 / 4.0);
    assert!(
        (area(&c) - expected).abs() < 1.5,
        "{} vs {expected}",
        area(&c)
    );
    let mut ring = Canvas::new(40, 40);
    ring.stroke_rrect((0.0, 0.0, 40.0, 40.0), [0.0; 4], 3.0, Color::WHITE);
    assert_eq!(area(&ring), 40.0 * 40.0 - 34.0 * 34.0);
}

#[quench_test::test]
fn a_shadow_fades_across_the_edge() {
    let mut c = Canvas::new(100, 100);
    c.shadow_rrect(
        (30.0, 30.0, 40.0, 40.0),
        [0.0; 4],
        10.0,
        Color::rgba(0, 0, 0, 255),
        None,
    );
    // Half way across the edge the shadow is half strong; well inside, full; well outside, none.
    let edge = alpha_at(&c, 50, 30);
    assert!((120..=145).contains(&edge), "{edge}");
    assert!(alpha_at(&c, 50, 50) > 250);
    assert_eq!(alpha_at(&c, 50, 10), 0);
    assert!(
        alpha_at(&c, 50, 25) > 0 && alpha_at(&c, 50, 25) < edge,
        "it spreads outside"
    );
    let mut sharp = Canvas::new(20, 20);
    sharp.shadow_rrect((5.0, 5.0, 10.0, 10.0), [0.0; 4], 0.0, Color::WHITE, None);
    assert_eq!(area(&sharp), 100.0, "no blur is a plain rectangle");
}

#[quench_test::test]
fn an_inset_shadow_darkens_the_inside_edge() {
    let mut c = Canvas::new(60, 60);
    let outer = (10.0, 10.0, 40.0, 40.0);
    // A hole the size of the box, moved down by 4 pixels: the top edge is shadowed, the bottom is not.
    c.shadow_rrect(
        (10.0, 14.0, 40.0, 40.0),
        [0.0; 4],
        0.0,
        Color::BLACK,
        Some((outer, [0.0; 4])),
    );
    assert_eq!(alpha_at(&c, 30, 11), 255);
    assert_eq!(alpha_at(&c, 30, 30), 0);
    assert_eq!(alpha_at(&c, 30, 48), 0);
    assert_eq!(alpha_at(&c, 30, 5), 0, "nothing outside the box");
}

#[quench_test::test]
fn a_layer_is_drawn_over_with_an_opacity() {
    let mut base = Canvas::new(4, 4);
    base.clear(Color::rgb(0, 0, 255));
    let mut layer = Canvas::new(2, 2);
    layer.clear(Color::rgb(255, 0, 0));
    base.draw_layer(&layer, 1, 1, 0.5);
    assert_eq!(base.pixel(0, 0), Color::rgb(0, 0, 255));
    let mixed = base.pixel(1, 1);
    assert!(
        (125..=130).contains(&mixed.r) && (125..=130).contains(&mixed.b),
        "{mixed:?}"
    );
    assert_eq!(mixed.a, 255);
    base.draw_layer(&layer, 3, 3, 1.0);
    assert_eq!(
        base.pixel(3, 3),
        Color::rgb(255, 0, 0),
        "partly outside is clipped"
    );
}

#[quench_test::test]
fn clip_to_narrows_the_clip_it_never_widens_it() {
    let mut c = Canvas::new(10, 10);
    let outer = c.set_clip(2, 2, 6, 6);
    let before = c.clip_to(0, 0, 5, 5);
    assert_eq!(before, (2, 2, 8, 8));
    c.fill_rect(0.0, 0.0, 10.0, 10.0, Color::WHITE);
    assert_eq!(alpha_at(&c, 1, 1), 0);
    assert_eq!(alpha_at(&c, 3, 3), 255);
    assert_eq!(alpha_at(&c, 6, 6), 0, "outside the narrower clip");
    c.restore_clip(before);
    c.restore_clip(outer);
}

#[quench_test::test]
fn an_image_is_scaled_filtered_blended_and_clipped() {
    // A 2x1 image, red then blue, stretched over 4 pixels: the middle pixels are mixes.
    let image = [255, 0, 0, 255, 0, 0, 255, 255];
    let mut c = Canvas::new(8, 4);
    c.clear(Color::WHITE);
    c.draw_image(&image, 2, 1, (2.0, 1.0, 4.0, 2.0), 1.0);
    assert_eq!(c.pixel(2, 1), Color::rgb(255, 0, 0));
    assert_eq!(c.pixel(5, 2), Color::rgb(0, 0, 255));
    let middle = c.pixel(3, 1);
    assert!(
        middle.r > 100 && middle.b > 30 && middle.g == 0,
        "{middle:?}"
    );
    assert_eq!(c.pixel(1, 1), Color::WHITE, "outside the rectangle");
    assert_eq!(c.pixel(2, 3), Color::WHITE);
    // Transparent pixels show what is under them, and opacity scales the rest.
    let mut c = Canvas::new(2, 1);
    c.clear(Color::WHITE);
    c.draw_image(&[0, 0, 0, 0, 0, 0, 0, 255], 2, 1, (0.0, 0.0, 2.0, 1.0), 0.5);
    assert_eq!(c.pixel(0, 0), Color::WHITE);
    let half = c.pixel(1, 0);
    assert!((120..=135).contains(&half.r), "{half:?}");
    // The clip applies, and bad input draws nothing.
    let mut c = Canvas::new(4, 4);
    c.set_clip(0, 0, 2, 2);
    c.draw_image(&[9, 9, 9, 255], 1, 1, (0.0, 0.0, 4.0, 4.0), 1.0);
    assert_eq!(c.pixel(1, 1), Color::rgb(9, 9, 9));
    assert_eq!(c.pixel(3, 3), Color::TRANSPARENT);
    c.draw_image(&[1, 2, 3], 1, 1, (0.0, 0.0, 1.0, 1.0), 1.0);
    c.draw_image(&[], 0, 0, (0.0, 0.0, 1.0, 1.0), 1.0);
    c.draw_image(&[9, 9, 9, 255], 1, 1, (0.0, 0.0, 0.0, 4.0), 1.0);
}

quench_test::main!();

#[quench_test::test]
fn a_rounded_rectangle_on_whole_pixels_is_the_same_by_the_fast_path_as_by_the_curve() {
    let color = Color::rgb(30, 90, 160);
    // `fill_rounded_rect` always takes the curve; `fill_rrect` takes the rows where it can.
    for (radius, rect) in [
        (0.0, (3.0, 2.0, 30.0, 20.0)),
        (8.0, (3.0, 2.0, 30.0, 20.0)),
        (10.0, (0.0, 0.0, 40.0, 20.0)),
        (6.0, (5.0, 4.0, 12.0, 12.0)),
    ] {
        let mut slow = Canvas::new(48, 32);
        slow.clear(Color::rgb(200, 200, 200));
        slow.fill_rounded_rect(rect.0, rect.1, rect.2, rect.3, radius, color);
        let mut fast = Canvas::new(48, 32);
        fast.clear(Color::rgb(200, 200, 200));
        fast.fill_rrect(rect, [radius; 4], color);
        assert_eq!(slow.bytes(), fast.bytes(), "radius {radius} rect {rect:?}");
    }
}

#[quench_test::test]
fn a_box_with_different_corners_and_an_odd_size_keeps_its_shape_on_the_fast_path() {
    let color = Color::rgb(255, 0, 0);
    let mut c = Canvas::new(40, 30);
    c.fill_rrect((4.0, 4.0, 30.0, 20.0), [10.0, 0.0, 6.0, 2.0], color);
    // The square top right corner is filled to its very pixel; the round top left one is not.
    assert_eq!(alpha_at(&c, 33, 4), 255);
    assert!(alpha_at(&c, 4, 4) < 128);
    // The straight middle is solid edge to edge.
    assert_eq!((alpha_at(&c, 4, 14), alpha_at(&c, 33, 14)), (255, 255));
    // Nothing leaks outside the box.
    assert_eq!(
        (
            alpha_at(&c, 3, 14),
            alpha_at(&c, 34, 14),
            alpha_at(&c, 20, 3)
        ),
        (0, 0, 0)
    );
}

#[quench_test::test]
fn a_filled_rectangle_is_clipped_and_fast_filled_without_leaving_its_row() {
    let mut c = Canvas::new(10, 6);
    c.fill_rect(2.0, 1.0, 5.0, 3.0, Color::rgb(0, 255, 0));
    assert_eq!(c.pixel(2, 1), Color::rgb(0, 255, 0));
    assert_eq!(c.pixel(6, 3), Color::rgb(0, 255, 0));
    assert_eq!(c.pixel(1, 1), Color::TRANSPARENT);
    assert_eq!(c.pixel(7, 3), Color::TRANSPARENT);
    assert_eq!(c.pixel(4, 4), Color::TRANSPARENT);
    // A rectangle that runs off the canvas on every side stops at its edges.
    c.fill_rect(-5.0, -5.0, 30.0, 30.0, Color::rgb(1, 2, 3));
    assert_eq!(c.pixel(9, 5), Color::rgb(1, 2, 3));
}

#[quench_test::test]
fn a_translucent_rectangle_blends_the_same_by_rows_as_by_the_curve() {
    for color in [
        Color::rgba(11, 11, 11, 153),
        Color::rgba(255, 0, 0, 1),
        Color::rgba(40, 200, 90, 254),
    ] {
        let mut by_rows = Canvas::new(20, 12);
        by_rows.clear(Color::rgb(180, 120, 60));
        by_rows.fill_rect(2.0, 3.0, 14.0, 6.0, color);
        let mut by_curve = Canvas::new(20, 12);
        by_curve.clear(Color::rgb(180, 120, 60));
        by_curve.fill_rounded_rect(2.0, 3.0, 14.0, 6.0, 0.0, color);
        assert_eq!(by_rows.bytes(), by_curve.bytes(), "{color:?}");
    }
    // Nothing for a fully transparent colour.
    let mut c = Canvas::new(4, 4);
    c.clear(Color::rgb(1, 2, 3));
    c.fill_rect(0.0, 0.0, 4.0, 4.0, Color::rgba(255, 255, 255, 0));
    assert_eq!(c.pixel(2, 2), Color::rgb(1, 2, 3));
}
