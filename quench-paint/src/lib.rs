//! Paints a laid out box tree into a [`quench_raster::Canvas`]: shadows, backgrounds, borders with radii and
//! dashes, inset shadows, overflow clipping, text, outlines, opacity (through a layer) and `translate()`.
//!
//! The order follows CSS in the parts that matter without full stacking contexts: a box paints its shadow,
//! background and border, then its negative-`z-index` children, in-flow blocks, inline backgrounds and text,
//! inline blocks, positioned children by `z-index`, and last its outline.

// Pixel arithmetic in `f32`.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::unused_self,
    clippy::while_float
)]

use quench_css::{BorderStyle, ComputedStyle, Display, Overflow, Position};
use quench_layout::{Fonts, Fragment, InlineFragment, LayoutBox, TextFragment};
use quench_raster::{Canvas, Color, Path};

/// How far each scroll container is scrolled, by the key of its box: `(x, y)` in pixels.
pub type Scroll<'a> = &'a dyn Fn(usize) -> (f32, f32);

/// Something drawn on top of a box that is not made of CSS: the mark in a checkbox, the caret in a text field.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoration {
    /// A check mark filling the content box.
    Check(quench_css::Color),
    /// An image (straight-alpha RGBA, `width * height * 4` bytes) stretched over the content box.
    Image {
        rgba: std::rc::Rc<Vec<u8>>,
        width: u32,
        height: u32,
    },
    /// A `background-image`: drawn over the padding box, after the background colour. `scale` is device pixels
    /// per CSS pixel (the image's own size is in CSS pixels).
    Background {
        rgba: std::rc::Rc<Vec<u8>>,
        width: u32,
        height: u32,
        size: quench_css::BackgroundSize,
        repeat: bool,
        position: (f32, f32),
        scale: f32,
    },
    /// A filled circle in the middle of the content box (a selected radio button).
    Dot(quench_css::Color),
    /// A bar `width` wide at `(x, y)` from the border box's top left, `height` tall (a caret).
    Bar {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: quench_css::Color,
    },
}

/// What to decorate a box with, by its key.
pub type Decorations<'a> = &'a dyn Fn(usize) -> Vec<Decoration>;

/// Paints `root` (laid out by `quench_layout::layout`) into `canvas`.
pub fn paint<S: AsRef<[u8]> + AsMut<[u8]>>(
    canvas: &mut Canvas<S>,
    root: &LayoutBox,
    fonts: &Fonts,
    scroll: Scroll<'_>,
    decorations: Decorations<'_>,
) {
    Painter {
        fonts,
        scroll,
        decorations,
        scrolled: std::cell::Cell::new((0.0, 0.0)),
    }
    .paint_box(canvas, root, root.x, root.y);
}

/// The children of a box in the order they paint (and, reversed, the order a pointer finds them).
#[derive(Debug, Default)]
pub struct Layers<'a> {
    /// Positioned, with a negative `z-index`.
    pub below: Vec<&'a LayoutBox>,
    /// In-flow blocks.
    pub blocks: Vec<&'a LayoutBox>,
    /// Inline blocks and inline flex containers, painted with the text.
    pub inline: Vec<&'a LayoutBox>,
    /// Positioned, by `z-index`.
    pub above: Vec<&'a LayoutBox>,
}

impl<'a> Layers<'a> {
    #[must_use]
    pub fn of(b: &'a LayoutBox) -> Self {
        let mut layers = Self::default();
        let z = |c: &LayoutBox| c.style.z_index.unwrap_or(0);
        for c in &b.children {
            if c.style.position != Position::Static {
                if z(c) < 0 {
                    layers.below.push(c);
                } else {
                    layers.above.push(c);
                }
            } else if matches!(c.style.display, Display::InlineBlock | Display::InlineFlex) {
                layers.inline.push(c);
            } else {
                layers.blocks.push(c);
            }
        }
        layers.above.sort_by_key(|c| z(c));
        layers
    }
}

/// How far outside the visible area a box may be and still be painted: its shadow or outline might reach in.
const CULL_MARGIN: f32 = 160.0;

/// The thickness of the scroll bar a scroll container shows.
pub const SCROLLBAR: f32 = 6.0;

/// Converts a CSS colour to a canvas colour.
#[must_use]
pub const fn to_raster(c: quench_css::Color) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

struct Painter<'a> {
    fonts: &'a Fonts,
    scroll: Scroll<'a>,
    decorations: Decorations<'a>,
    /// The scrolling of every container above the box being painted, which fixed boxes do not follow.
    scrolled: std::cell::Cell<(f32, f32)>,
}

/// How far italic text leans (about 11 degrees).
const ITALIC_SLANT: f32 = 0.2;

/// A box edge snapped to whole pixels, as browsers do for backgrounds and borders.
fn snap(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let (x0, y0) = (x.round(), y.round());
    let (x1, y1) = ((x + w).round(), (y + h).round());
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// The corner radii of a box, scaled down together when two of them would not fit on a side.
fn radii(style: &ComputedStyle, w: f32, h: f32) -> [f32; 4] {
    let basis = w.min(h);
    let mut r: [f32; 4] = std::array::from_fn(|i| style.radius[i].resolve(basis).max(0.0));
    let sums = [
        (r[0] + r[1], w),
        (r[3] + r[2], w),
        (r[0] + r[3], h),
        (r[1] + r[2], h),
    ];
    let factor = sums
        .iter()
        .filter(|(sum, _)| *sum > 0.0)
        .map(|(sum, side)| side / sum)
        .fold(1.0_f32, f32::min);
    if factor < 1.0 {
        for v in &mut r {
            *v *= factor;
        }
    }
    r
}

impl Painter<'_> {
    fn paint_box<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        b: &LayoutBox,
        x: f32,
        y: f32,
    ) {
        let style = &b.style;
        if style.display == Display::None {
            return;
        }
        let (mut x, mut y) = (x, y);
        if !style.transform.is_identity() {
            x += style.transform.translate.0.resolve(b.width);
            y += style.transform.translate.1.resolve(b.height);
        }
        // A box that is nowhere near what can be seen is not painted, nor anything inside it: a long list in a
        // scroll container costs the rows on screen, not all of them. The margin covers what can spill out of a
        // box (shadows, outlines, glyphs that lean over their edge).
        let (cx0, cy0, cx1, cy1) = canvas.clip();
        let reach = (b.width.max(b.overflow.0), b.height.max(b.overflow.1));
        if x > cx1 as f32 + CULL_MARGIN
            || y > cy1 as f32 + CULL_MARGIN
            || x + reach.0 < cx0 as f32 - CULL_MARGIN
            || y + reach.1 < cy0 as f32 - CULL_MARGIN
        {
            return;
        }
        let turned = style.transform.rotate != 0.0 || style.transform.scale != (1.0, 1.0);
        if style.opacity >= 1.0 && !turned {
            self.paint_content(canvas, b, x, y);
            return;
        }
        if style.opacity <= 0.0 {
            return;
        }
        // The box and everything that spills out of it go to a layer of their own, drawn with the opacity.
        let margin = 64.0;
        let (lx, ly) = (
            (x - margin).floor().max(0.0) as i32,
            (y - margin).floor().max(0.0) as i32,
        );
        let right = (x + b.overflow.0.max(b.width) + margin)
            .ceil()
            .min(canvas.width() as f32) as i32;
        let bottom = (y + b.overflow.1.max(b.height) + margin)
            .ceil()
            .min(canvas.height() as f32) as i32;
        if right <= lx || bottom <= ly {
            return;
        }
        let mut layer = Canvas::new((right - lx) as usize, (bottom - ly) as usize);
        self.paint_content(&mut layer, b, x - lx as f32, y - ly as f32);
        if turned {
            // Turned and scaled about the middle of the box, as `transform-origin: 50% 50%` says.
            let pivot = (x + b.width / 2.0, y + b.height / 2.0);
            canvas.draw_layer_transformed(
                &layer,
                (lx, ly),
                style.opacity,
                pivot,
                style.transform.rotate,
                style.transform.scale,
            );
        } else {
            canvas.draw_layer(&layer, lx, ly, style.opacity);
        }
    }

    fn paint_content<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        b: &LayoutBox,
        x: f32,
        y: f32,
    ) {
        let style = &b.style;
        let visible = style.visible;
        let rect = snap(x, y, b.width, b.height);
        let r = radii(style, b.width, b.height);
        if visible {
            // Outer shadows, the first one on top.
            for s in style.shadows.iter().rev().filter(|s| !s.inset) {
                let grown = (
                    rect.0 + s.x - s.spread,
                    rect.1 + s.y - s.spread,
                    (rect.2 + 2.0 * s.spread).max(0.0),
                    (rect.3 + 2.0 * s.spread).max(0.0),
                );
                let rr = r.map(|v| (v + s.spread).max(0.0));
                canvas.shadow_rrect(grown, rr, s.blur, to_raster(style.resolve(s.color)), None);
            }
            let bg = style.resolve(style.background);
            if bg.a > 0 {
                canvas.fill_rrect(rect, r, to_raster(bg));
            }
            let padding = (
                rect.0 + b.border[3],
                rect.1 + b.border[0],
                (rect.2 - b.border[1] - b.border[3]).max(0.0),
                (rect.3 - b.border[0] - b.border[2]).max(0.0),
            );
            for d in (self.decorations)(b.key) {
                if let Decoration::Background { .. } = d {
                    Self::background_image(canvas, padding, &d);
                }
            }
            let inner_r: [f32; 4] = [
                (r[0] - b.border[0].max(b.border[3])).max(0.0),
                (r[1] - b.border[0].max(b.border[1])).max(0.0),
                (r[2] - b.border[2].max(b.border[1])).max(0.0),
                (r[3] - b.border[2].max(b.border[3])).max(0.0),
            ];
            for s in style.shadows.iter().rev().filter(|s| s.inset) {
                let hole = (
                    padding.0 + s.x + s.spread,
                    padding.1 + s.y + s.spread,
                    (padding.2 - 2.0 * s.spread).max(0.0),
                    (padding.3 - 2.0 * s.spread).max(0.0),
                );
                let hole_r = inner_r.map(|v| (v - s.spread).max(0.0));
                canvas.shadow_rrect(
                    hole,
                    hole_r,
                    s.blur,
                    to_raster(style.resolve(s.color)),
                    Some((padding, inner_r)),
                );
            }
            self.border(canvas, style, rect, b.border, r);
        }

        let clips = style.overflow != (Overflow::Visible, Overflow::Visible);
        let saved = if clips {
            let p = b.padding_rect();
            let inner = snap(x + p.0, y + p.1, p.2, p.3);
            Some(canvas.clip_to(
                inner.0 as i32,
                inner.1 as i32,
                inner.2 as i32,
                inner.3 as i32,
            ))
        } else {
            None
        };
        // The document scrolls through its root box, which does not clip.
        let (sx, sy) = if clips || b.key == 0 {
            (self.scroll)(b.key)
        } else {
            (0.0, 0.0)
        };
        let (ox, oy) = (x - sx, y - sy);
        let outer = self.scrolled.get();
        self.scrolled.set((outer.0 + sx, outer.1 + sy));
        // A fixed box stays where the window is: it undoes the scrolling of what it is inside.
        let origin = |c: &LayoutBox| {
            let (tx, ty) = if c.style.position == Position::Fixed {
                self.scrolled.get()
            } else {
                (0.0, 0.0)
            };
            (ox + c.x + tx, oy + c.y + ty)
        };

        let layers = Layers::of(b);
        for c in &layers.below {
            let (cx, cy) = origin(c);
            self.paint_box(canvas, c, cx, cy);
        }
        for c in &layers.blocks {
            let (cx, cy) = origin(c);
            self.paint_box(canvas, c, cx, cy);
        }
        for f in &b.fragments {
            if let Fragment::Inline(i) = f {
                self.inline_box(canvas, i, ox, oy);
            }
        }
        for f in &b.fragments {
            if let Fragment::Text(t) = f {
                self.text(canvas, t, ox, oy);
            }
        }
        for c in &layers.inline {
            let (cx, cy) = origin(c);
            self.paint_box(canvas, c, cx, cy);
        }
        for c in &layers.above {
            let (cx, cy) = origin(c);
            self.paint_box(canvas, c, cx, cy);
        }
        self.scrolled.set(outer);
        if visible && clips {
            Self::scrollbar(canvas, b, rect, (sx, sy));
        } else if visible && b.key == 0 {
            Self::page_scrollbar(
                canvas,
                b,
                (canvas.width() as f32, canvas.height() as f32),
                sy,
            );
        }
        if visible {
            for d in (self.decorations)(b.key) {
                Self::decorate(canvas, b, rect, d);
            }
        }
        if let Some(clip) = saved {
            canvas.restore_clip(clip);
        }

        if visible && style.outline_width > 0.0 && style.outline_style != BorderStyle::None {
            let grow = style.outline_offset + style.outline_width;
            let outer = (
                rect.0 - grow,
                rect.1 - grow,
                rect.2 + 2.0 * grow,
                rect.3 + 2.0 * grow,
            );
            let rr = r.map(|v| if v > 0.0 { v + grow } else { 0.0 });
            canvas.stroke_rrect(
                outer,
                rr,
                style.outline_width,
                to_raster(style.resolve(style.outline_color)),
            );
        }
    }

    /// A thin bar at the right edge of a scroll container whose contents are taller than it.
    /// The bar of the document's scroll, along the right edge of the window.
    fn page_scrollbar<S: AsRef<[u8]> + AsMut<[u8]>>(
        canvas: &mut Canvas<S>,
        b: &LayoutBox,
        window: (f32, f32),
        scrolled: f32,
    ) {
        let content = b.overflow.1.max(b.height);
        if content <= window.1 + 0.5 {
            return;
        }
        let thumb = (window.1 * window.1 / content).max(24.0).min(window.1);
        let top = (window.1 - thumb) * (scrolled / (content - window.1)).clamp(0.0, 1.0);
        canvas.fill_rrect(
            (window.0 - SCROLLBAR - 2.0, top, SCROLLBAR, thumb),
            [SCROLLBAR / 2.0; 4],
            Color::rgba(160, 160, 160, 150),
        );
    }

    fn scrollbar<S: AsRef<[u8]> + AsMut<[u8]>>(
        canvas: &mut Canvas<S>,
        b: &LayoutBox,
        rect: (f32, f32, f32, f32),
        scrolled: (f32, f32),
    ) {
        let (_, py, _, ph) = b.padding_rect();
        let overflows = matches!(b.style.overflow.1, Overflow::Auto | Overflow::Scroll)
            && b.overflow.1 > ph + 0.5;
        if !overflows || ph < 12.0 {
            return;
        }
        let track = ph;
        let thumb = (track * ph / b.overflow.1).max(24.0).min(track);
        let range = (b.overflow.1 - ph).max(1.0);
        let top = py + (track - thumb) * (scrolled.1 / range).clamp(0.0, 1.0);
        let x = rect.0 + rect.2 - b.border[1] - SCROLLBAR - 2.0;
        canvas.fill_rrect(
            (x, rect.1 + top, SCROLLBAR, thumb),
            [SCROLLBAR / 2.0; 4],
            Color::rgba(160, 160, 160, 150),
        );
    }

    /// Draws a `background-image` into the padding box: sized, placed and tiled as the style says.
    fn background_image<S: AsRef<[u8]> + AsMut<[u8]>>(
        canvas: &mut Canvas<S>,
        padding: (f32, f32, f32, f32),
        d: &Decoration,
    ) {
        let Decoration::Background {
            rgba,
            width,
            height,
            size,
            repeat,
            position,
            scale,
        } = d
        else {
            return;
        };
        let (iw, ih) = (*width as f32, *height as f32);
        if padding.2 <= 0.0 || padding.3 <= 0.0 || iw <= 0.0 || ih <= 0.0 {
            return;
        }
        let factor = match size {
            quench_css::BackgroundSize::Auto => *scale,
            quench_css::BackgroundSize::Cover => (padding.2 / iw).max(padding.3 / ih),
            quench_css::BackgroundSize::Contain => (padding.2 / iw).min(padding.3 / ih),
        };
        let (tw, th) = (iw * factor, ih * factor);
        let left = padding.0 + (padding.2 - tw) * position.0;
        let top = padding.1 + (padding.3 - th) * position.1;
        let before = canvas.clip_to(
            padding.0 as i32,
            padding.1 as i32,
            padding.2 as i32,
            padding.3 as i32,
        );
        // The first tile that can show: tiles go back from the placed one to the box's edge.
        let (x0, y0, columns, rows) = if *repeat {
            let back =
                |from: f32, edge: f32, step: f32| from - ((from - edge) / step).ceil() * step;
            let (x0, y0) = (back(left, padding.0, tw), back(top, padding.1, th));
            let columns = ((padding.0 + padding.2 - x0) / tw).ceil().clamp(1.0, 256.0) as u32;
            let rows = ((padding.1 + padding.3 - y0) / th).ceil().clamp(1.0, 256.0) as u32;
            (x0, y0, columns, rows)
        } else {
            (left, top, 1, 1)
        };
        for row in 0..rows {
            for column in 0..columns {
                let at = (x0 + column as f32 * tw, y0 + row as f32 * th, tw, th);
                canvas.draw_image(rgba, *width as usize, *height as usize, at, 1.0);
            }
        }
        canvas.restore_clip(before);
    }

    fn decorate<S: AsRef<[u8]> + AsMut<[u8]>>(
        canvas: &mut Canvas<S>,
        b: &LayoutBox,
        rect: (f32, f32, f32, f32),
        d: Decoration,
    ) {
        let (cx, cy, cw, ch) = b.content_rect();
        let (x, y) = (rect.0 + cx, rect.1 + cy);
        match d {
            Decoration::Check(color) => {
                let size = cw.min(ch);
                let thick = size * 0.16;
                let at = |u: f32, v: f32| (x + u * size, y + v * size);
                let mut path = Path::new();
                // Two strokes, each a quadrilateral around its centre line.
                for (a, b) in [((0.2, 0.52), (0.42, 0.74)), ((0.42, 0.74), (0.8, 0.28))] {
                    let (p, q) = (at(a.0, a.1), at(b.0, b.1));
                    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                    let len = dx.hypot(dy).max(0.001);
                    let (nx, ny) = (-dy / len * thick / 2.0, dx / len * thick / 2.0);
                    path.move_to(p.0 + nx, p.1 + ny);
                    path.line_to(q.0 + nx, q.1 + ny);
                    path.line_to(q.0 - nx, q.1 - ny);
                    path.line_to(p.0 - nx, p.1 - ny);
                    path.close();
                }
                canvas.fill_path(&path, 0.0, 0.0, to_raster(color));
            }
            Decoration::Image {
                rgba,
                width,
                height,
            } => {
                canvas.draw_image(&rgba, width as usize, height as usize, (x, y, cw, ch), 1.0);
            }
            Decoration::Background { .. } => {}
            Decoration::Dot(color) => {
                let size = cw.min(ch) * 0.5;
                let mut path = Path::new();
                path.ellipse(x + cw / 2.0, y + ch / 2.0, size / 2.0, size / 2.0);
                canvas.fill_path(&path, 0.0, 0.0, to_raster(color));
            }
            Decoration::Bar {
                x,
                y,
                width,
                height,
                color,
            } => {
                canvas.fill_rect(
                    (rect.0 + x).round(),
                    (rect.1 + y).round(),
                    width,
                    height,
                    to_raster(color),
                );
            }
        }
    }

    fn text<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        t: &TextFragment,
        ox: f32,
        oy: f32,
    ) {
        if !t.style.visible || t.text.is_empty() {
            return;
        }
        let set = self.fonts.set(&t.style);
        let layout = set.layout(&t.text, t.style.font_size, None);
        set.draw_slanted(
            canvas,
            &layout,
            (ox + t.x).round(),
            (oy + t.baseline).round() - layout.ascent,
            to_raster(t.style.color),
            // No italic face is embedded: the regular one is leaned.
            if t.style.italic { ITALIC_SLANT } else { 0.0 },
        );
    }

    fn inline_box<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        i: &InlineFragment,
        ox: f32,
        oy: f32,
    ) {
        let style = &i.style;
        if !style.visible {
            return;
        }
        let rect = snap(ox + i.x, oy + i.y, i.width, i.height);
        let full = radii(style, i.width, i.height);
        let r = [
            if i.first { full[0] } else { 0.0 },
            if i.last { full[1] } else { 0.0 },
            if i.last { full[2] } else { 0.0 },
            if i.first { full[3] } else { 0.0 },
        ];
        let bg = style.resolve(style.background);
        if bg.a > 0 {
            canvas.fill_rrect(rect, r, to_raster(bg));
        }
        let mut widths = style.border_width;
        if !i.first {
            widths[3] = 0.0;
        }
        if !i.last {
            widths[1] = 0.0;
        }
        self.border(canvas, style, rect, widths, r);
    }

    /// The border of a box: one ring when it is the same all round, otherwise side by side.
    fn border<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        style: &ComputedStyle,
        rect: (f32, f32, f32, f32),
        widths: [f32; 4],
        r: [f32; 4],
    ) {
        if widths.iter().all(|w| *w <= 0.0) {
            return;
        }
        let colors: [Color; 4] =
            std::array::from_fn(|i| to_raster(style.resolve(style.border_color[i])));
        let solid = style.border_style.iter().all(|s| *s == BorderStyle::Solid);
        let (w0, c0) = (widths[0], colors[0]);
        if solid && widths.iter().all(|w| *w == w0) && colors.iter().all(|c| *c == c0) {
            canvas.stroke_rrect(rect, r, w0, c0);
            return;
        }
        let (x, y, w, h) = rect;
        let round = r.iter().any(|v| *v > 0.0);
        for side in 0..4 {
            let t = widths[side];
            if t <= 0.0 {
                continue;
            }
            let color = colors[side];
            // Where this side's strip is: the corners go to the side that comes first (top, then bottom).
            let strip = match side {
                0 => (x, y, w, t.max(r[0]).max(r[1])),
                2 => (x, y + h - t.max(r[2]).max(r[3]), w, t.max(r[2]).max(r[3])),
                1 => (
                    x + w - t,
                    y + widths[0],
                    t,
                    (h - widths[0] - widths[2]).max(0.0),
                ),
                _ => (x, y + widths[0], t, (h - widths[0] - widths[2]).max(0.0)),
            };
            match style.border_style[side] {
                BorderStyle::None => {}
                BorderStyle::Solid if round => {
                    let saved = canvas.clip_to(
                        strip.0 as i32,
                        strip.1 as i32,
                        strip.2.ceil() as i32,
                        strip.3.ceil() as i32,
                    );
                    canvas.stroke_rrect(rect, r, t, color);
                    canvas.restore_clip(saved);
                }
                BorderStyle::Solid => canvas.fill_rect(strip.0, strip.1, strip.2, strip.3, color),
                BorderStyle::Dashed | BorderStyle::Dotted => {
                    let (dash, gap) = if style.border_style[side] == BorderStyle::Dashed {
                        (t * 3.0, t * 3.0)
                    } else {
                        (t, t)
                    };
                    let horizontal = side == 0 || side == 2;
                    let length = if horizontal { strip.2 } else { strip.3 };
                    let mut at = 0.0;
                    while at < length {
                        let seg = dash.min(length - at);
                        if horizontal {
                            canvas.fill_rect(strip.0 + at, strip.1, seg, t, color);
                        } else {
                            canvas.fill_rect(strip.0, strip.1 + at, t, seg, color);
                        }
                        at += dash + gap;
                    }
                }
            }
        }
    }
}
