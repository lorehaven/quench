//! Picking files and offering downloads: the browser's side of `quench_ui::platform::{pick_files, download}`.
//!
//! The picker is the browser's own and may only open from a click or key press of the person, so
//! [`pick`] has to be called while an event handler is running. The files come back one at a time through
//! `q_file_picked`; `done` is called once all have arrived, or at once with none when the person cancels.

use quench_ui::platform::PickedFile;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::host;

struct Waiting {
    done: Box<dyn FnOnce(Vec<PickedFile>)>,
    files: Vec<PickedFile>,
}

#[derive(Default)]
struct Pending {
    next: u32,
    waiting: HashMap<u32, Waiting>,
}

thread_local! {
    static PICKS: RefCell<Pending> = RefCell::new(Pending::default());
}

/// Opens the picker; `done` is called with the files, or with none on cancel.
pub fn pick(accept: &str, multiple: bool, done: Box<dyn FnOnce(Vec<PickedFile>)>) {
    let id = PICKS.with(|p| {
        let mut p = p.borrow_mut();
        let id = p.next;
        p.next = p.next.wrapping_add(1);
        p.waiting.insert(
            id,
            Waiting {
                done,
                files: Vec::new(),
            },
        );
        id
    });
    host::file_pick(id, accept, multiple);
}

/// The id the browser gives to files dropped on the page (they were not asked for by any picker).
pub const DROP_ID: u32 = u32::MAX;

thread_local! {
    static DROPPED: RefCell<Vec<PickedFile>> = const { RefCell::new(Vec::new()) };
}

/// One picked file arrived (`index` of `total`); `total` 0 means the person cancelled.
pub fn picked(id: u32, index: u32, total: u32, file: PickedFile) {
    if id == DROP_ID {
        // Dropped files go to whoever asked to hear of them, once all of them are here.
        let all = DROPPED.with(|d| {
            let mut d = d.borrow_mut();
            d.push(file);
            (d.len() >= total as usize).then(|| std::mem::take(&mut *d))
        });
        if let Some(files) = all {
            quench_ui::platform::deliver_drop(files);
        }
        return;
    }
    let finished = PICKS.with(|p| {
        let mut p = p.borrow_mut();
        if total == 0 {
            return p.waiting.remove(&id).map(|w| (w.done, Vec::new()));
        }
        let waiting = p.waiting.get_mut(&id)?;
        let _ = index;
        waiting.files.push(file);
        if waiting.files.len() < total as usize {
            return None;
        }
        p.waiting.remove(&id).map(|w| (w.done, w.files))
    });
    if let Some((done, files)) = finished {
        done(files);
    }
}

/// Pickers still waiting for the person.
#[must_use]
pub fn pending() -> usize {
    PICKS.with(|p| p.borrow().waiting.len())
}

/// Forgets every waiting picker. For tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    PICKS.with(|p| *p.borrow_mut() = Pending::default());
}
