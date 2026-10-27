//! Taps, long presses, drags and swipes from pointer events.
//!
//! [`Recognizer`] is a plain state machine: feed it pointer positions with times, get gestures
//! back, and test it without a browser. [`on_gesture`] wires one to an element.
//!
//! A touch that the browser takes over to scroll the page arrives as `pointercancel`; to keep
//! the gestures that belong to the app, tell the browser which directions it may take with
//! [`Axis`], which sets `touch-action` through a class.

use crate::event::{Event, EventKind, PointerEvent};
use crate::platform::{self, Timer};
use crate::view::Element;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A direction of a swipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// What was recognised.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// Pressed and released in place, quickly enough not to be a long press.
    Tap,
    /// Held in place for the long-press time.
    LongPress,
    /// The pointer moved far enough to count as a drag.
    DragStart,
    /// Where the drag is now, relative to where it started.
    Drag { dx: f64, dy: f64 },
    /// The drag ended; the velocity is in pixels per millisecond.
    DragEnd { dx: f64, dy: f64, vx: f64, vy: f64 },
    /// A drag that ended fast and far enough to be a flick.
    Swipe(Direction),
    /// The browser took the pointer over, or the gesture was abandoned.
    Cancel,
}

/// The directions the browser may use for its own scrolling and zooming (`touch-action`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// The app takes every direction: the browser does nothing here.
    Both,
    /// The app takes left and right; vertical scrolling stays with the browser.
    Horizontal,
    /// The app takes up and down; horizontal scrolling stays with the browser.
    Vertical,
}

impl Axis {
    const fn class(self) -> &'static str {
        match self {
            Self::Both => "q-touch-none",
            Self::Horizontal => "q-touch-pan-y",
            Self::Vertical => "q-touch-pan-x",
        }
    }
}

/// Thresholds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// How far a press may move and still be a tap or a long press, in pixels.
    pub slop: f64,
    pub long_press_ms: f64,
    /// The least distance of a swipe, in pixels.
    pub swipe_distance: f64,
    /// The least speed of a swipe, in pixels per millisecond.
    pub swipe_speed: f64,
    pub axis: Axis,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            slop: 10.0,
            long_press_ms: 500.0,
            swipe_distance: 40.0,
            swipe_speed: 0.3,
            axis: Axis::Both,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    x: f64,
    y: f64,
    t: f64,
}

#[derive(Clone, Copy, Debug)]
enum State {
    Idle,
    Down {
        start: Sample,
        long: bool,
    },
    Dragging {
        start: Sample,
        last: Sample,
        before: Sample,
    },
}

/// The state machine behind [`on_gesture`].
#[derive(Debug)]
pub struct Recognizer {
    config: Config,
    state: State,
}

impl Recognizer {
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            state: State::Idle,
        }
    }

    /// A pointer went down at `(x, y)` at time `t` (ms).
    pub const fn down(&mut self, x: f64, y: f64, t: f64) {
        self.state = State::Down {
            start: Sample { x, y, t },
            long: false,
        };
    }

    /// The pointer moved.
    pub fn moved(&mut self, x: f64, y: f64, t: f64) -> Vec<Gesture> {
        let here = Sample { x, y, t };
        match self.state {
            State::Idle => Vec::new(),
            State::Down { start, .. } => {
                if (x - start.x).hypot(y - start.y) <= self.config.slop {
                    return Vec::new();
                }
                self.state = State::Dragging {
                    start,
                    last: here,
                    before: start,
                };
                vec![
                    Gesture::DragStart,
                    Gesture::Drag {
                        dx: x - start.x,
                        dy: y - start.y,
                    },
                ]
            }
            State::Dragging { start, last, .. } => {
                // Keep the previous sample for the speed at the end, unless it is the same instant.
                let before = if t > last.t { last } else { self.before() };
                self.state = State::Dragging {
                    start,
                    last: here,
                    before,
                };
                vec![Gesture::Drag {
                    dx: x - start.x,
                    dy: y - start.y,
                }]
            }
        }
    }

    const fn before(&self) -> Sample {
        match self.state {
            State::Dragging { before, .. } => before,
            State::Down { start, .. } => start,
            State::Idle => Sample {
                x: 0.0,
                y: 0.0,
                t: 0.0,
            },
        }
    }

    /// The pointer went up.
    pub fn up(&mut self, x: f64, y: f64, t: f64) -> Vec<Gesture> {
        let state = std::mem::replace(&mut self.state, State::Idle);
        match state {
            State::Idle => Vec::new(),
            State::Down { long, .. } => {
                if long {
                    Vec::new()
                } else {
                    vec![Gesture::Tap]
                }
            }
            State::Dragging {
                start,
                last,
                before,
            } => {
                let (dx, dy) = (x - start.x, y - start.y);
                let span = (t - before.t).max(t - last.t).max(1.0);
                let (vx, vy) = ((x - before.x) / span, (y - before.y) / span);
                let mut out = vec![Gesture::DragEnd { dx, dy, vx, vy }];
                if let Some(direction) = self.swipe(dx, dy, vx, vy) {
                    out.push(Gesture::Swipe(direction));
                }
                out
            }
        }
    }

    fn swipe(&self, dx: f64, dy: f64, vx: f64, vy: f64) -> Option<Direction> {
        let horizontal = dx.abs() >= dy.abs();
        let (distance, speed) = if horizontal {
            (dx.abs(), vx.abs())
        } else {
            (dy.abs(), vy.abs())
        };
        if distance < self.config.swipe_distance || speed < self.config.swipe_speed {
            return None;
        }
        Some(match (horizontal, if horizontal { dx } else { dy } > 0.0) {
            (true, true) => Direction::Right,
            (true, false) => Direction::Left,
            (false, true) => Direction::Down,
            (false, false) => Direction::Up,
        })
    }

    /// Time `t` has come: a press held in place long enough becomes a long press.
    pub fn long_press_due(&mut self, t: f64) -> Option<Gesture> {
        if let State::Down { start, long: false } = self.state
            && t - start.t >= self.config.long_press_ms
        {
            self.state = State::Down { start, long: true };
            return Some(Gesture::LongPress);
        }
        None
    }

    /// The gesture is off (the browser took the pointer, or the app gave up on it).
    pub fn cancel(&mut self) -> Vec<Gesture> {
        let was = std::mem::replace(&mut self.state, State::Idle);
        match was {
            State::Dragging { .. } => vec![Gesture::Cancel],
            State::Down { .. } | State::Idle => Vec::new(),
        }
    }
}

/// Attaches gesture recognition to `element`: `handler` receives each [`Gesture`] as it happens.
#[must_use]
pub fn on_gesture(
    element: Element,
    config: Config,
    handler: impl Fn(&Gesture) + 'static,
) -> Element {
    let recognizer = Rc::new(RefCell::new(Recognizer::new(config)));
    let handler: Rc<dyn Fn(&Gesture)> = Rc::new(handler);
    let timer: Rc<Cell<Option<Timer>>> = Rc::new(Cell::new(None));

    // Handlers run with no borrow held, so they may do anything, including start another gesture.
    let emit = {
        let handler = handler.clone();
        move |gestures: Vec<Gesture>| {
            for g in &gestures {
                handler(g);
            }
        }
    };
    let stop_timer = {
        let timer = timer.clone();
        move || {
            if let Some(t) = timer.take() {
                t.cancel();
            }
        }
    };

    let (r_down, t_down, s_down) = (recognizer.clone(), timer, stop_timer.clone());
    let (r_long, e_long) = (recognizer.clone(), emit.clone());
    let (r_move, s_move, e_move) = (recognizer.clone(), stop_timer.clone(), emit.clone());
    let (r_up, s_up, e_up) = (recognizer.clone(), stop_timer.clone(), emit.clone());
    let (r_cancel, e_cancel) = (recognizer, emit);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // a small, positive number of ms
    let long_press_ms = config.long_press_ms.clamp(0.0, f64::from(u32::MAX)) as u32;
    element
        .class(config.axis.class())
        .on(EventKind::PointerDown, move |e| {
            let Event::PointerDown(p) = e else { return };
            s_down();
            r_down.borrow_mut().down(p.x, p.y, platform::now());
            let (r, e) = (r_long.clone(), e_long.clone());
            t_down.set(Some(platform::after(long_press_ms, move || {
                let due = r.borrow_mut().long_press_due(platform::now());
                e(due.into_iter().collect());
            })));
        })
        .on(EventKind::PointerMove, move |e| {
            let Event::PointerMove(PointerEvent { x, y, .. }) = e else {
                return;
            };
            let out = r_move.borrow_mut().moved(*x, *y, platform::now());
            if !out.is_empty() {
                s_move();
            }
            e_move(out);
        })
        .on(EventKind::PointerUp, move |e| {
            let Event::PointerUp(PointerEvent { x, y, .. }) = e else {
                return;
            };
            s_up();
            let out = r_up.borrow_mut().up(*x, *y, platform::now());
            e_up(out);
        })
        .on(EventKind::PointerCancel, move |_| {
            stop_timer();
            let out = r_cancel.borrow_mut().cancel();
            e_cancel(out);
        })
}
