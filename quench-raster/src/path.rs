//! Outlines: lines and curves, flattened to lines for rasterising.

/// A point, in pixels.
pub type Point = (f32, f32);

/// A sequence of closed contours made of lines and quadratic or cubic curves.
///
/// A contour is closed implicitly when the next one starts and when the path ends (a fill closes every
/// contour). Opposite winding directions leave a hole: draw the outer contour one way and the inner one the
/// other.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    /// Each contour as flattened points; curves are subdivided as they are added.
    contours: Vec<Vec<Point>>,
    current: Vec<Point>,
}

/// How far a flattened curve may stray from the true one, in pixels.
const TOLERANCE: f32 = 0.02;
/// The most segments a curve is flattened into.
const MAX_SEGMENTS: usize = 128;

/// How many line segments keep a curve within [`TOLERANCE`], given the size of its second difference
/// (`|p0 - 2 c + p1|` for a quadratic, the larger of the two for a cubic) and the factor of its form.
fn segments(second_difference: f32, factor: f32) -> usize {
    ((factor * second_difference / (8.0 * TOLERANCE))
        .sqrt()
        .ceil() as usize)
        .clamp(2, MAX_SEGMENTS)
}

fn second_difference(a: Point, b: Point, c: Point) -> f32 {
    (a.0 - 2.0 * b.0 + c.0).hypot(a.1 - 2.0 * b.1 + c.1)
}

impl Path {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new contour at `(x, y)`.
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.finish();
        self.current.push((x, y));
        self
    }

    /// A straight line to `(x, y)`.
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.current.push((x, y));
        self
    }

    /// A quadratic curve to `(x, y)` with the control point `(cx, cy)` (TrueType's curve).
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self {
        let Some(&p0) = self.current.last() else {
            return self.move_to(x, y);
        };
        let n = segments(second_difference(p0, (cx, cy), (x, y)), 2.0);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            self.current.push((
                u * u * p0.0 + 2.0 * u * t * cx + t * t * x,
                u * u * p0.1 + 2.0 * u * t * cy + t * t * y,
            ));
        }
        self
    }

    /// A cubic curve to `(x, y)` with control points `(c1x, c1y)` and `(c2x, c2y)`.
    pub fn cubic_to(
        &mut self,
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    ) -> &mut Self {
        let Some(&p0) = self.current.last() else {
            return self.move_to(x, y);
        };
        let m = second_difference(p0, (c1x, c1y), (c2x, c2y)).max(second_difference(
            (c1x, c1y),
            (c2x, c2y),
            (x, y),
        ));
        let n = segments(m, 6.0);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.current.push((
                a * p0.0 + b * c1x + c * c2x + d * x,
                a * p0.1 + b * c1y + c * c2y + d * y,
            ));
        }
        self
    }

    /// Ends the current contour.
    pub fn close(&mut self) -> &mut Self {
        self.finish();
        self
    }

    /// A rectangle contour, clockwise on screen.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.move_to(x, y)
            .line_to(x + w, y)
            .line_to(x + w, y + h)
            .line_to(x, y + h)
            .close()
    }

    /// An ellipse contour, clockwise on screen.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> &mut Self {
        const K: f32 = 0.552_284_8; // the cubic approximation of a quarter circle
        self.move_to(cx + rx, cy)
            .cubic_to(cx + rx, cy + ry * K, cx + rx * K, cy + ry, cx, cy + ry)
            .cubic_to(cx - rx * K, cy + ry, cx - rx, cy + ry * K, cx - rx, cy)
            .cubic_to(cx - rx, cy - ry * K, cx - rx * K, cy - ry, cx, cy - ry)
            .cubic_to(cx + rx * K, cy - ry, cx + rx, cy - ry * K, cx + rx, cy)
            .close()
    }

    fn finish(&mut self) {
        if self.current.len() >= 2 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }

    /// Every contour, the current one included.
    pub fn contours(&self) -> impl Iterator<Item = &[Point]> {
        self.contours
            .iter()
            .map(Vec::as_slice)
            .chain((self.current.len() >= 2).then_some(self.current.as_slice()))
    }

    /// The smallest box around the path: `(min_x, min_y, max_x, max_y)`; `None` when it is empty.
    #[must_use]
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut points = self.contours().flatten();
        let first = *points.next()?;
        Some(points.fold(
            (first.0, first.1, first.0, first.1),
            |(x0, y0, x1, y1), p| (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1)),
        ))
    }
}
