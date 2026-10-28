use crate::color::{Color, mul255};
use crate::path::Path;

/// A rectangle of whole pixels: `(x, y, width, height)`.
type PixelRect = (i32, i32, i32, i32);

/// A rectangle in pixels with fractions, `(x, y, width, height)`, and the radii of its corners.
pub type Rounded = ((f32, f32, f32, f32), [f32; 4]);

/// A pixel buffer you draw into: premultiplied BGRA, `width * 4` bytes per row.
///
/// It owns its bytes by default (`Canvas::new`); [`Canvas::over`] draws into bytes that live elsewhere (a
/// slice, a memory-mapped frame buffer) without a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas<S = Vec<u8>> {
    width: usize,
    height: usize,
    pixels: S,
    /// What drawing is limited to, in pixels: `(x0, y0, x1, y1)`, end excluded.
    clip: (i32, i32, i32, i32),
}

impl Canvas {
    /// A transparent canvas.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            clip: (0, 0, width as i32, height as i32),
        }
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> Canvas<S> {
    /// A canvas that draws into `pixels` as it is (`None` when it is too short for `width * height` pixels).
    /// The bytes are used as they are: clear them first for a transparent canvas.
    pub fn over(width: usize, height: usize, pixels: S) -> Option<Self> {
        (pixels.as_ref().len() >= width.checked_mul(height)?.checked_mul(4)?).then(|| Self {
            width,
            height,
            pixels,
            clip: (0, 0, width as i32, height as i32),
        })
    }

    fn px(&self) -> &[u8] {
        self.pixels.as_ref()
    }

    fn px_mut(&mut self) -> &mut [u8] {
        self.pixels.as_mut()
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// The bytes, `width * 4` per row: blue, green, red, alpha, premultiplied.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.px()[..self.width * self.height * 4]
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        let n = self.width * self.height * 4;
        &mut self.px_mut()[..n]
    }

    /// The pixel at `(x, y)` as straight-alpha colour; transparent outside the canvas.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }
        let i = (y * self.width + x) * 4;
        let (b, g, r, a) = (
            self.px()[i],
            self.px()[i + 1],
            self.px()[i + 2],
            self.px()[i + 3],
        );
        if a == 0 {
            return Color::TRANSPARENT;
        }
        let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
        Color {
            r: un(r),
            g: un(g),
            b: un(b),
            a,
        }
    }

    /// Makes every pixel `color` (replacing, not blending).
    pub fn clear(&mut self, color: Color) {
        let p = premultiplied(color, 255);
        let n = self.width * self.height * 4;
        for pixel in self.px_mut()[..n].as_chunks_mut::<4>().0 {
            *pixel = p;
        }
    }

    /// Limits drawing to the rectangle (clipped to the canvas); returns the previous clip to restore.
    pub fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
        let before = self.clip;
        self.clip = (
            x.max(0),
            y.max(0),
            (x.saturating_add(w)).min(self.width as i32),
            (y.saturating_add(h)).min(self.height as i32),
        );
        before
    }

    /// Limits drawing to the part of the rectangle that is already inside the clip; returns the previous clip.
    pub fn clip_to(&mut self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
        let before = self.clip;
        self.clip = (
            x.max(before.0),
            y.max(before.1),
            x.saturating_add(w).min(before.2),
            y.saturating_add(h).min(before.3),
        );
        before
    }

    /// Puts back a clip returned by [`set_clip`](Self::set_clip).
    pub const fn restore_clip(&mut self, clip: (i32, i32, i32, i32)) {
        self.clip = clip;
    }

    /// Blends `color` into the pixel at `(x, y)` with `coverage` (0 to 255). Outside the clip: nothing.
    #[inline]
    fn blend(&mut self, x: i32, y: i32, color: Color, coverage: u32) {
        if x < self.clip.0
            || y < self.clip.1
            || x >= self.clip.2
            || y >= self.clip.3
            || coverage == 0
        {
            return;
        }
        let alpha = mul255(u32::from(color.a), coverage);
        if alpha == 0 {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
        let inv = 255 - alpha;
        let src = [color.b, color.g, color.r];
        for (k, s) in src.iter().enumerate() {
            let premultiplied = mul255(u32::from(*s), alpha);
            let old = u32::from(self.px()[i + k]);
            self.px_mut()[i + k] = (premultiplied + mul255(old, inv)).min(255) as u8;
        }
        let old = u32::from(self.px()[i + 3]);
        self.px_mut()[i + 3] = (alpha + mul255(old, inv)).min(255) as u8;
    }

    /// Fills a rectangle of whole pixels (the coordinates are rounded to the nearest pixel).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let (x0, y0) = (x.round() as i32, y.round() as i32);
        let (x1, y1) = ((x + w).round() as i32, (y + h).round() as i32);
        let (cx0, cy0, cx1, cy1) = (
            x0.max(self.clip.0),
            y0.max(self.clip.1),
            x1.min(self.clip.2),
            y1.min(self.clip.3),
        );
        if color.a == 255 {
            let p = premultiplied(color, 255);
            for yy in cy0..cy1 {
                for xx in cx0..cx1 {
                    let i = (yy as usize * self.width + xx as usize) * 4;
                    self.px_mut()[i..i + 4].copy_from_slice(&p);
                }
            }
        } else {
            for yy in cy0..cy1 {
                for xx in cx0..cx1 {
                    self.blend(xx, yy, color, 255);
                }
            }
        }
    }

    /// Fills a rectangle with rounded corners of radius `radius`, anti-aliased, at fractional coordinates.
    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        self.paint_sdf(
            (x, y, w, h),
            [radius.clamp(0.0, w.min(h) / 2.0); 4],
            color,
            coverage_of,
        );
    }

    /// Draws the outline of a rounded rectangle, `width` thick and inside the given bounds.
    pub fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        width: f32,
        color: Color,
    ) {
        self.stroke_rrect(
            (x, y, w, h),
            [radius.clamp(0.0, w.min(h) / 2.0); 4],
            width,
            color,
        );
    }

    /// Fills a rectangle whose corners have their own radii: `[top-left, top-right, bottom-right, bottom-left]`.
    pub fn fill_rrect(&mut self, rect: (f32, f32, f32, f32), radii: [f32; 4], color: Color) {
        self.paint_sdf(rect, radii, color, coverage_of);
    }

    /// Draws the outline of a rectangle with per-corner radii, `width` thick and inside the given bounds.
    pub fn stroke_rrect(
        &mut self,
        rect: (f32, f32, f32, f32),
        radii: [f32; 4],
        width: f32,
        color: Color,
    ) {
        self.paint_sdf(rect, radii, color, move |sd| {
            (coverage_of(sd) * (1.0 - coverage_of(sd + width))).clamp(0.0, 1.0)
        });
    }

    /// A blurred shadow of a rounded rectangle, as `box-shadow` makes it: `blur` is the CSS blur radius (the
    /// edge fades over twice that, as a Gaussian of `blur / 2`). An `inset` shadow is the same shape seen
    /// from inside: it darkens the area of `clip` (the box it is inset into) that the shape does not cover.
    pub fn shadow_rrect(
        &mut self,
        rect: (f32, f32, f32, f32),
        radii: [f32; 4],
        blur: f32,
        color: Color,
        inset: Option<Rounded>,
    ) {
        let sigma = (blur / 2.0).max(0.0);
        let spread = if sigma > 0.0 { sigma * 3.0 } else { 0.0 };
        let fade = move |sd: f32| -> f32 {
            if sigma <= 0.0 {
                coverage_of(sd)
            } else {
                // The Gaussian-blurred edge: the integral of the bell curve across it.
                0.5 * (1.0 - erf(sd / (sigma * std::f32::consts::SQRT_2)))
            }
        };
        match inset {
            None => {
                let grown = (
                    rect.0 - spread,
                    rect.1 - spread,
                    rect.2 + 2.0 * spread,
                    rect.3 + 2.0 * spread,
                );
                self.paint_sdf_in(grown, rect, radii, color, fade);
            }
            Some((outer, outer_radii)) => {
                // Inside `outer`, the shadow is where the hole `rect` is not.
                let (x0, y0) = (outer.0.floor() as i32 - 1, outer.1.floor() as i32 - 1);
                let (x1, y1) = (
                    (outer.0 + outer.2).ceil() as i32 + 1,
                    (outer.1 + outer.3).ceil() as i32 + 1,
                );
                let hole = rect;
                for yy in y0.max(self.clip.1)..y1.min(self.clip.3) {
                    for xx in x0.max(self.clip.0)..x1.min(self.clip.2) {
                        let (px, py) = (xx as f32 + 0.5, yy as f32 + 0.5);
                        let inside_outer = coverage_of(rrect_sd(px, py, outer, outer_radii));
                        if inside_outer <= 0.0 {
                            continue;
                        }
                        let shadow = 1.0 - fade(rrect_sd(px, py, hole, radii));
                        self.blend(
                            xx,
                            yy,
                            color,
                            (inside_outer * shadow.clamp(0.0, 1.0) * 255.0).round() as u32,
                        );
                    }
                }
            }
        }
    }

    /// Paints a rounded rectangle through a function of the signed distance to its edge (negative inside).
    fn paint_sdf(
        &mut self,
        rect: (f32, f32, f32, f32),
        radii: [f32; 4],
        color: Color,
        coverage: impl Fn(f32) -> f32,
    ) {
        self.paint_sdf_in(rect, rect, radii, color, coverage);
    }

    /// Like `paint_sdf`, visiting the pixels of `area` and measuring the distance to the shape `rect`.
    fn paint_sdf_in(
        &mut self,
        area: (f32, f32, f32, f32),
        rect: (f32, f32, f32, f32),
        radii: [f32; 4],
        color: Color,
        coverage: impl Fn(f32) -> f32,
    ) {
        if rect.2 <= 0.0 || rect.3 <= 0.0 {
            return;
        }
        let bounds: PixelRect = (
            (area.0.floor() as i32 - 1).max(self.clip.0),
            (area.1.floor() as i32 - 1).max(self.clip.1),
            ((area.0 + area.2).ceil() as i32 + 1).min(self.clip.2),
            ((area.1 + area.3).ceil() as i32 + 1).min(self.clip.3),
        );
        for yy in bounds.1..bounds.3 {
            for xx in bounds.0..bounds.2 {
                let sd = rrect_sd(xx as f32 + 0.5, yy as f32 + 0.5, rect, radii);
                self.blend(xx, yy, color, (coverage(sd) * 255.0).round() as u32);
            }
        }
    }

    /// Draws `layer` with its top left corner at `(x, y)` over this canvas, scaled in opacity by `opacity`.
    /// Both are premultiplied, so this is the usual "source over".
    pub fn draw_layer<T: AsRef<[u8]> + AsMut<[u8]>>(
        &mut self,
        layer: &Canvas<T>,
        x: i32,
        y: i32,
        opacity: f32,
    ) {
        let opacity = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        for ly in 0..layer.height as i32 {
            let dy = y + ly;
            if dy < self.clip.1 || dy >= self.clip.3 {
                continue;
            }
            for lx in 0..layer.width as i32 {
                let dx = x + lx;
                if dx < self.clip.0 || dx >= self.clip.2 {
                    continue;
                }
                let si = (ly as usize * layer.width + lx as usize) * 4;
                let src = &layer.px()[si..si + 4];
                if src[3] == 0 {
                    continue;
                }
                let sa = mul255(u32::from(src[3]), opacity);
                let inv = 255 - sa;
                let di = (dy as usize * self.width + dx as usize) * 4;
                for (k, channel) in src.iter().enumerate() {
                    let s = mul255(u32::from(*channel), opacity);
                    let d = u32::from(self.px()[di + k]);
                    self.px_mut()[di + k] = (s + mul255(d, inv)).min(255) as u8;
                }
            }
        }
    }

    /// Fills `path`, moved by `(dx, dy)`, anti-aliased, with non-zero winding.
    pub fn fill_path(&mut self, path: &Path, dx: f32, dy: f32, color: Color) {
        let Some((bx0, by0, bx1, by1)) = path.bounds() else {
            return;
        };
        // Only the part of the path's box that can be seen is rasterised.
        let x0 = ((bx0 + dx).floor() as i32).max(self.clip.0);
        let y0 = ((by0 + dy).floor() as i32).max(self.clip.1);
        let x1 = ((bx1 + dx).ceil() as i32).min(self.clip.2);
        let y1 = ((by1 + dy).ceil() as i32).min(self.clip.3);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut area = Accumulator::new(w, h);
        for contour in path.contours() {
            let at = |p: (f32, f32)| (p.0 + dx - x0 as f32, p.1 + dy - y0 as f32);
            for pair in contour.windows(2) {
                area.line(at(pair[0]), at(pair[1]));
            }
            // A fill closes every contour.
            if let (Some(&last), Some(&first)) = (contour.last(), contour.first()) {
                area.line(at(last), at(first));
            }
        }
        for (row, covered) in area.rows().enumerate() {
            for (col, c) in covered.enumerate() {
                self.blend(x0 + col as i32, y0 + row as i32, color, c);
            }
        }
    }
}

/// A straight-alpha colour at `opacity` (0 to 255), as premultiplied BGRA.
fn premultiplied(color: Color, opacity: u32) -> [u8; 4] {
    let a = mul255(u32::from(color.a), opacity);
    [
        mul255(u32::from(color.b), a) as u8,
        mul255(u32::from(color.g), a) as u8,
        mul255(u32::from(color.r), a) as u8,
        a as u8,
    ]
}

/// The coverage of a pixel whose centre is `sd` pixels from the edge (negative: inside).
/// The signed distance from a point to a rectangle with per-corner radii (negative inside).
fn rrect_sd(px: f32, py: f32, rect: (f32, f32, f32, f32), radii: [f32; 4]) -> f32 {
    let (cx, cy) = (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
    let (x, y) = (px - cx, py - cy);
    let r = match (x < 0.0, y < 0.0) {
        (true, true) => radii[0],
        (false, true) => radii[1],
        (false, false) => radii[2],
        (true, false) => radii[3],
    }
    .clamp(0.0, (rect.2.min(rect.3)) / 2.0);
    let (dx, dy) = (x.abs() - (rect.2 / 2.0 - r), y.abs() - (rect.3 / 2.0 - r));
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - r
}

/// The error function (Abramowitz and Stegun 7.1.26, error below 1.5e-7).
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_8 + t * (-1.453_152_1 + t * 1.061_405_4))));
    let y = 1.0 - poly * (-x * x).exp();
    if x >= 0.0 { y } else { -y }
}

fn coverage_of(sd: f32) -> f32 {
    (0.5 - sd).clamp(0.0, 1.0)
}

/// Signed-area coverage: each edge adds the area it sweeps to the pixels it crosses, and a running sum
/// along a row turns those into coverage. One extra column holds what is left of the right edge.
struct Accumulator {
    w: usize,
    h: usize,
    area: Vec<f32>,
}

impl Accumulator {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            area: vec![0.0; (w + 2) * h],
        }
    }

    const fn stride(&self) -> usize {
        self.w + 2
    }

    /// Adds an edge, split at the left and right sides so that what is outside still counts (as a vertical
    /// edge on the side) without bending the part that is inside.
    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        let right = self.w as f32;
        let (mut points, mut n) = ([a, a, a, a], 1);
        // The x positions at which the edge crosses a side, in order along the edge.
        let mut crossings: [Option<f32>; 2] = [None, None];
        for (k, side) in [0.0, right].into_iter().enumerate() {
            // Crossing a side means the two ends are on different sides of it, so they differ.
            if (a.0 < side) != (b.0 < side) {
                crossings[k] = Some((side - a.0) / (b.0 - a.0));
            }
        }
        let mut ts: Vec<f32> = crossings.iter().flatten().copied().collect();
        ts.sort_by(f32::total_cmp);
        for t in ts {
            points[n] = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            n += 1;
        }
        points[n] = b;
        n += 1;
        for pair in points[..n].windows(2) {
            let mid = (pair[0].0 + pair[1].0) / 2.0;
            let clamp = |x: f32| x.clamp(0.0, right);
            if mid < 0.0 || mid > right {
                // Outside: the winding still changes here, so keep it as a vertical edge on that side.
                let x = clamp(mid);
                self.segment((x, pair[0].1), (x, pair[1].1));
            } else {
                self.segment((clamp(pair[0].0), pair[0].1), (clamp(pair[1].0), pair[1].1));
            }
        }
    }

    /// An edge wholly inside `0..=w` horizontally.
    fn segment(&mut self, p0: (f32, f32), p1: (f32, f32)) {
        if p0.1 == p1.1 {
            return;
        }
        let (dir, p0, p1) = if p0.1 < p1.1 {
            (1.0, p0, p1)
        } else {
            (-1.0, p1, p0)
        };
        let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
        let mut x = p0.0;
        if p0.1 < 0.0 {
            x -= p0.1 * dxdy;
        }
        let first = p0.1.max(0.0) as usize;
        let last = self.h.min(p1.1.ceil().max(0.0) as usize);
        let stride = self.stride();
        for y in first..last {
            let line = y * stride;
            let dy = ((y + 1) as f32).min(p1.1) - (y as f32).max(p0.1);
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (x0, x1) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let x0floor = x0.floor();
            let x0i = x0floor as usize;
            let x1ceil = x1.ceil();
            let x1i = x1ceil as usize;
            if x1i <= x0i + 1 {
                let xmf = 0.5 * (x + xnext) - x0floor;
                self.area[line + x0i] += d - d * xmf;
                self.area[line + x0i + 1] += d * xmf;
            } else {
                let s = 1.0 / (x1 - x0);
                let x0f = x0 - x0floor;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = x1 - x1ceil + 1.0;
                let am = 0.5 * s * x1f * x1f;
                self.area[line + x0i] += d * a0;
                if x1i == x0i + 2 {
                    self.area[line + x0i + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    self.area[line + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        self.area[line + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    self.area[line + x1i - 1] += d * (1.0 - a2 - am);
                }
                self.area[line + x1i] += d * am;
            }
            x = xnext;
        }
    }

    /// The coverage of each pixel, row by row (0 to 255).
    fn rows(&self) -> impl Iterator<Item = impl Iterator<Item = u32> + '_> + '_ {
        let stride = self.stride();
        (0..self.h).map(move |y| {
            let mut sum = 0.0_f32;
            self.area[y * stride..y * stride + self.w]
                .iter()
                .map(move |a| {
                    sum += a;
                    (sum.abs().min(1.0) * 255.0 + 0.5) as u32
                })
        })
    }
}
