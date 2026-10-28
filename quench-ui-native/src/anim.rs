//! CSS transitions: when the style of an element changes in `opacity`, `color`, `background-color` or
//! `border-color` and its `transition` asks for it, the old value gives way to the new one over time.
//!
//! The state lives with the document and is consulted while styles are computed: it remembers the style each
//! element had (what the cascade said, and what was shown), starts a transition when they differ, and puts the
//! value in between into the style. The host's [`tick`](crate::NativeRenderer::tick) keeps frames coming while
//! one runs. Keyframe animations are not played.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use quench_css::{
    Animation, Cascade, Color, ColorValue, ComputedStyle, Direction, FillMode, Length, Timing,
    Transition,
};

/// The values a transition can move.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    opacity: f32,
    color: Color,
    background: Color,
    border: [Color; 4],
}

impl Look {
    fn of(style: &ComputedStyle) -> Self {
        Self {
            opacity: style.opacity,
            color: style.color,
            background: style.resolve(style.background),
            border: std::array::from_fn(|i| style.resolve(style.border_color[i])),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Prop {
    Opacity,
    Color,
    Background,
    Border,
}

impl Prop {
    const ALL: [Self; 4] = [Self::Opacity, Self::Color, Self::Background, Self::Border];

    const fn name(self) -> &'static str {
        match self {
            Self::Opacity => "opacity",
            Self::Color => "color",
            Self::Background => "background-color",
            Self::Border => "border-color",
        }
    }
}

/// A transition under way.
#[derive(Debug, Clone, Copy)]
struct Run {
    from: Look,
    to: Look,
    start: Instant,
    duration: f32,
    delay: f32,
    timing: Timing,
}

#[derive(Debug, Default)]
pub(crate) struct Anim {
    /// What each element's style said last time, and what was shown for it.
    last: HashMap<usize, (Look, Look)>,
    running: HashMap<(usize, Prop), Run>,
    now: Option<Instant>,
    /// Added to the clock (for tests and for stills at a chosen moment).
    skew: Duration,
    reduced_motion: bool,
    /// When each keyframe animation of an element began, by element and name.
    started: HashMap<(usize, String), Instant>,
    /// Whether any animation was still going in the last frame.
    animating: bool,
}

/// The progress of a curve at time `t` (0 to 1): the cubic Bézier of the named timing function.
fn eased(timing: Timing, t: f32) -> f32 {
    let (x1, y1, x2, y2) = match timing {
        Timing::Linear => return t,
        Timing::Ease => (0.25, 0.1, 0.25, 1.0),
        Timing::EaseIn => (0.42, 0.0, 1.0, 1.0),
        Timing::EaseOut => (0.0, 0.0, 0.58, 1.0),
        Timing::EaseInOut => (0.42, 0.0, 0.58, 1.0),
    };
    let bezier = |a: f32, b: f32, s: f32| {
        3.0 * (1.0 - s).powi(2) * s * a + 3.0 * (1.0 - s) * s * s * b + s.powi(3)
    };
    // Find the curve parameter whose x is `t`, then read y there.
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = lo.midpoint(hi);
        if bezier(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier(y1, y2, lo.midpoint(hi))
}

fn mix_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Colours blend with their alpha in the weights (so a clear colour does not darken the other).
fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let (aa, ab) = (f32::from(a.a) / 255.0, f32::from(b.a) / 255.0);
    let alpha = mix_f(aa, ab, t);
    if alpha <= 0.0 {
        return Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }
    let channel = |x: u8, y: u8| {
        let (px, py) = (f32::from(x) * aa, f32::from(y) * ab);
        (mix_f(px, py, t) / alpha).round().clamp(0.0, 255.0) as u8
    };
    Color {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

fn mix(a: &Look, b: &Look, t: Look, prop: Prop, progress: f32) -> Look {
    let mut out = t;
    match prop {
        Prop::Opacity => out.opacity = mix_f(a.opacity, b.opacity, progress),
        Prop::Color => out.color = mix_color(a.color, b.color, progress),
        Prop::Background => out.background = mix_color(a.background, b.background, progress),
        Prop::Border => {
            out.border = std::array::from_fn(|i| mix_color(a.border[i], b.border[i], progress));
        }
    }
    out
}

fn differs(a: &Look, b: &Look, prop: Prop) -> bool {
    match prop {
        Prop::Opacity => (a.opacity - b.opacity).abs() > f32::EPSILON,
        Prop::Color => a.color != b.color,
        Prop::Background => a.background != b.background,
        Prop::Border => a.border != b.border,
    }
}

/// The entry of `transition` that covers a property (the last one that names it or `all`).
fn entry_for(transitions: &[Transition], prop: Prop) -> Option<&Transition> {
    transitions.iter().rev().find(|t| {
        t.property == prop.name()
            || t.property == "all"
            || (prop == Prop::Background && t.property == "background")
    })
}

impl Anim {
    /// Starts a frame at the current time.
    pub(crate) fn begin(&mut self, reduced_motion: bool) {
        self.now = Some(Instant::now() + self.skew);
        self.reduced_motion = reduced_motion;
        self.animating = false;
    }

    pub(crate) fn advance(&mut self, by: Duration) {
        self.skew += by;
    }

    /// Whether a transition is still under way (frames must keep coming).
    pub(crate) fn active(&self) -> bool {
        self.animating || !self.running.is_empty()
    }

    /// Looks at the style the cascade gave `id` and puts in it what should be shown now: transitions first, then
    /// the keyframe animations on top of them.
    pub(crate) fn apply(
        &mut self,
        id: usize,
        style: &mut ComputedStyle,
        cascade: &Cascade,
        parent: &ComputedStyle,
        root_font: f32,
    ) {
        self.transition(id, style);
        self.play(id, style, cascade, parent, root_font);
    }

    fn transition(&mut self, id: usize, style: &mut ComputedStyle) {
        let now = self.now.unwrap_or_else(Instant::now);
        let target = Look::of(style);
        let Some((was_target, was_shown)) = self.last.get(&id).copied() else {
            self.last.insert(id, (target, target));
            return;
        };
        for prop in Prop::ALL {
            if !differs(&was_target, &target, prop) {
                continue;
            }
            self.running.remove(&(id, prop));
            let Some(t) = entry_for(&style.transitions, prop) else {
                continue;
            };
            if self.reduced_motion || t.duration + t.delay <= 0.0 {
                continue;
            }
            self.running.insert(
                (id, prop),
                Run {
                    from: was_shown,
                    to: target,
                    start: now,
                    duration: t.duration,
                    delay: t.delay,
                    timing: t.timing,
                },
            );
        }
        let mut shown = target;
        let mut finished = Vec::new();
        for prop in Prop::ALL {
            let Some(run) = self.running.get(&(id, prop)) else {
                continue;
            };
            let elapsed = now.saturating_duration_since(run.start).as_secs_f32() - run.delay;
            let t = if run.duration > 0.0 {
                (elapsed / run.duration).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if elapsed >= run.duration {
                finished.push(prop);
                continue;
            }
            let progress = eased(run.timing, t);
            shown = mix(&run.from, &run.to, shown, prop, progress);
        }
        for prop in finished {
            self.running.remove(&(id, prop));
        }
        style.opacity = shown.opacity;
        style.color = shown.color;
        style.background = ColorValue::Color(shown.background);
        style.border_color = shown.border.map(ColorValue::Color);
        self.last.insert(id, (target, shown));
    }
}

fn mix_length(a: &Length, b: &Length, t: f32) -> Length {
    match (a, b) {
        (Length::Px(x), Length::Px(y)) => Length::Px(mix_f(*x, *y, t)),
        (Length::Percent(x), Length::Percent(y)) => Length::Percent(mix_f(*x, *y, t)),
        _ if t < 0.5 => a.clone(),
        _ => b.clone(),
    }
}

/// The groups of properties an animation moves; each is interpolated between the keyframes that set it.
#[derive(Debug, Clone, Copy)]
enum Group {
    Opacity,
    Color,
    Background,
    Border,
    Transform,
}

impl Group {
    const ALL: [Self; 5] = [
        Self::Opacity,
        Self::Color,
        Self::Background,
        Self::Border,
        Self::Transform,
    ];

    /// Whether a declaration of this name sets a property of the group.
    fn sets(self, name: &str) -> bool {
        match self {
            Self::Opacity => name == "opacity",
            Self::Color => name == "color",
            Self::Background => name == "background" || name == "background-color",
            Self::Border => {
                name == "border" || (name.starts_with("border") && name.ends_with("color"))
            }
            Self::Transform => matches!(name, "transform" | "rotate"),
        }
    }
}

/// Puts into `out` the values of `group` that lie `t` of the way from those of `a` to those of `b`.
fn blend(group: Group, out: &mut ComputedStyle, a: &ComputedStyle, b: &ComputedStyle, t: f32) {
    let (la, lb) = (Look::of(a), Look::of(b));
    match group {
        Group::Opacity => out.opacity = mix_f(la.opacity, lb.opacity, t),
        Group::Color => out.color = mix_color(la.color, lb.color, t),
        Group::Background => {
            out.background = ColorValue::Color(mix_color(la.background, lb.background, t));
        }
        Group::Border => {
            out.border_color = std::array::from_fn(|i| {
                ColorValue::Color(mix_color(la.border[i], lb.border[i], t))
            });
        }
        Group::Transform => {
            out.transform.translate = (
                mix_length(&a.transform.translate.0, &b.transform.translate.0, t),
                mix_length(&a.transform.translate.1, &b.transform.translate.1, t),
            );
            out.transform.scale = (
                mix_f(a.transform.scale.0, b.transform.scale.0, t),
                mix_f(a.transform.scale.1, b.transform.scale.1, t),
            );
            out.transform.rotate = mix_f(a.transform.rotate, b.transform.rotate, t);
        }
    }
}

impl Anim {
    /// Plays the element's keyframe animations: each one that runs puts its current values in `style`.
    fn play(
        &mut self,
        id: usize,
        style: &mut ComputedStyle,
        cascade: &Cascade,
        parent: &ComputedStyle,
        root_font: f32,
    ) {
        let names: Vec<&str> = style.animations.iter().map(|a| a.name.as_str()).collect();
        // An animation that is no longer asked for is forgotten, so it starts again when it is.
        self.started
            .retain(|(element, name), _| *element != id || names.contains(&name.as_str()));
        if self.reduced_motion || style.animations.is_empty() {
            return;
        }
        let now = self.now.unwrap_or_else(Instant::now);
        let base = style.clone();
        for animation in &base.animations {
            let Some(keyframes) = cascade.keyframes(&animation.name) else {
                continue;
            };
            let start = *self
                .started
                .entry((id, animation.name.clone()))
                .or_insert(now);
            let elapsed = now.saturating_duration_since(start).as_secs_f32() - animation.delay;
            let Some(progress) = animation_progress(animation, elapsed) else {
                continue;
            };
            if progress.running {
                self.animating = true;
            }
            // Each group moves between the keyframes that set it, from the element's own value at an end the
            // keyframes leave out.
            let p = progress.position.clamp(0.0, 1.0);
            let styled: Vec<(f32, ComputedStyle, &Vec<quench_css::Declaration>)> = keyframes
                .frames
                .iter()
                .map(|(offset, declarations)| {
                    (
                        *offset,
                        cascade.style_with(&base, parent, root_font, declarations),
                        declarations,
                    )
                })
                .collect();
            for group in Group::ALL {
                let mut frames: Vec<(f32, &ComputedStyle)> = styled
                    .iter()
                    .filter(|(_, _, d)| d.iter().any(|d| group.sets(&d.name)))
                    .map(|(o, s, _)| (*o, s))
                    .collect();
                if frames.is_empty() {
                    continue;
                }
                frames.sort_by(|a, b| a.0.total_cmp(&b.0));
                if frames.first().is_some_and(|f| f.0 > 0.0) {
                    frames.insert(0, (0.0, &base));
                }
                if frames.last().is_some_and(|f| f.0 < 1.0) {
                    frames.push((1.0, &base));
                }
                let next = frames
                    .iter()
                    .position(|f| f.0 > p)
                    .unwrap_or(frames.len() - 1)
                    .max(1);
                let (from, to) = (frames[next - 1], frames[next]);
                let span = to.0 - from.0;
                let t = if span > 0.0 {
                    eased(animation.timing, ((p - from.0) / span).clamp(0.0, 1.0))
                } else {
                    1.0
                };
                blend(group, style, from.1, to.1, t);
            }
        }
    }
}

/// Where an animation is in its keyframes (0 to 1) `elapsed` seconds after its delay, and whether it still runs.
struct Progress {
    position: f32,
    running: bool,
}

fn animation_progress(a: &Animation, elapsed: f32) -> Option<Progress> {
    let at = |iteration: f32, fraction: f32| {
        let forward = match a.direction {
            Direction::Normal => true,
            Direction::Reverse => false,
            Direction::Alternate => iteration % 2.0 == 0.0,
            Direction::AlternateReverse => iteration % 2.0 != 0.0,
        };
        if forward { fraction } else { 1.0 - fraction }
    };
    if elapsed < 0.0 {
        // Waiting for the delay: shown at the start only when the fill says so.
        return matches!(a.fill, FillMode::Backwards | FillMode::Both).then(|| Progress {
            position: at(0.0, 0.0),
            running: true,
        });
    }
    if a.duration <= 0.0 || elapsed >= a.duration * a.iterations {
        return matches!(a.fill, FillMode::Forwards | FillMode::Both).then(|| {
            // After the last iteration: its end (a half iteration ends half way).
            let total = if a.duration > 0.0 { a.iterations } else { 0.0 };
            let (iteration, fraction) = if total.fract() == 0.0 && total > 0.0 {
                (total - 1.0, 1.0)
            } else {
                (total.floor(), total.fract())
            };
            Progress {
                position: at(iteration, fraction),
                running: false,
            }
        });
    }
    let done = elapsed / a.duration;
    Some(Progress {
        position: at(done.floor(), done.fract()),
        running: true,
    })
}
