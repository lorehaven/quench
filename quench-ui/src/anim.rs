//! Animation helpers that lean on CSS transitions: the app says when something is shown, CSS says
//! how it moves. Everything honours the user's reduced-motion preference by skipping the wait.

use crate::event::EventKind;
use crate::platform::{self, Timer};
use crate::reactive::{create_effect, create_memo, create_signal, on_cleanup, untrack};
use crate::view::{IntoView, View, div};
use std::cell::Cell;
use std::rc::Rc;

/// How long the entering state is held before the shown state is applied, so the browser has
/// drawn the first one and has something to transition from.
const ENTER_DELAY_MS: u32 = 20;

/// A little longer than the transition itself, in case `transitionend` never comes (the element
/// was hidden, or the transition was cancelled).
const LEAVE_SLACK_MS: u32 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Gone,
    Entering,
    In,
    Leaving,
}

/// Content that fades (or slides, or scales) in when `visible` becomes true and stays in the page
/// until its exit has played when it becomes false.
///
/// The content sits in a `div.q-anim`. It starts in that state alone, gets `q-anim-in` a moment
/// after it is added, and loses it when it is to go; style the two states and a CSS `transition`
/// does the rest. `duration_ms` is how long the transition lasts, for the fallback that removes the
/// content if `transitionend` does not arrive. With reduced motion preferred the content appears
/// and disappears at once. A `visible` that starts true is shown with no animation, so a server
/// render and the first client render agree.
#[must_use]
pub fn presence(
    visible: impl Fn() -> bool + 'static,
    duration_ms: u32,
    content: impl Fn() -> View + 'static,
) -> View {
    View::lazy(move || build(visible, duration_ms, content))
}

fn build(
    visible: impl Fn() -> bool + 'static,
    duration_ms: u32,
    content: impl Fn() -> View + 'static,
) -> View {
    let initial = untrack(&visible);
    let phase = create_signal(if initial { Phase::In } else { Phase::Gone });
    let timer: Rc<Cell<Option<Timer>>> = Rc::new(Cell::new(None));
    let set_timer = move |next: Option<Timer>| {
        if let Some(old) = timer.replace(next) {
            old.cancel();
        }
    };
    {
        let (phase, set_timer) = (phase.clone(), set_timer.clone());
        create_effect(move || {
            let (want, now) = (visible(), phase.get_untracked());
            let reduced = platform::prefers_reduced_motion();
            match (want, now) {
                (true, Phase::Gone) if reduced => phase.set(Phase::In),
                (true, Phase::Gone) => {
                    phase.set(Phase::Entering);
                    let p = phase.clone();
                    set_timer(Some(platform::after(ENTER_DELAY_MS, move || {
                        if p.get_untracked() == Phase::Entering {
                            p.set(Phase::In);
                        }
                    })));
                }
                (true, Phase::Leaving) => {
                    set_timer(None);
                    phase.set(Phase::In);
                }
                (false, Phase::Entering | Phase::In) if reduced => {
                    set_timer(None);
                    phase.set(Phase::Gone);
                }
                (false, Phase::Entering | Phase::In) => {
                    phase.set(Phase::Leaving);
                    let p = phase.clone();
                    set_timer(Some(platform::after(
                        duration_ms + LEAVE_SLACK_MS,
                        move || {
                            if p.get_untracked() == Phase::Leaving {
                                p.set(Phase::Gone);
                            }
                        },
                    )));
                }
                _ => {}
            }
        });
    }
    on_cleanup(move || set_timer(None));
    // Rebuilt when the content comes or goes, not at each step of the animation in between.
    let present = {
        let phase = phase.clone();
        create_memo(move || phase.get() != Phase::Gone)
    };
    View::dynamic(move || {
        if !present.get() {
            return View::Empty;
        }
        let (class, ended) = (phase.clone(), phase.clone());
        // Gone is decided here, not by an effect, so the content leaves in the same update.
        div()
            .class("q-anim")
            .class_if("q-anim-in", move || class.get() == Phase::In)
            .on(EventKind::TransitionEnd, move |_| {
                if ended.get_untracked() == Phase::Leaving {
                    ended.set(Phase::Gone);
                }
            })
            .child(untrack(&content))
            .into_view()
    })
}
