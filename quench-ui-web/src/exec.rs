//! A single-threaded executor, timers, and `sleep`. There is no thread and no runtime in the
//! browser: a task that becomes ready asks the host for a tick (`q_schedule_tick`), and `q_tick`
//! polls whatever is ready.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use crate::host;

struct Task {
    fut: Option<Pin<Box<dyn Future<Output = ()>>>>,
    queued: bool,
}

#[derive(Default)]
struct Executor {
    next: u64,
    tasks: HashMap<u64, Task>,
    ready: VecDeque<u64>,
    tick_scheduled: bool,
    ticking: bool,
}

#[derive(Default)]
struct TimerState {
    fired: bool,
    waker: Option<Waker>,
}

#[derive(Default)]
struct Timers {
    next: u32,
    pending: HashMap<u32, TimerState>,
}

thread_local! {
    static EXEC: RefCell<Executor> = RefCell::new(Executor::default());
    static TIMERS: RefCell<Timers> = RefCell::new(Timers::default());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    // Dropped after the borrow ends: dropping a task can wake another (closing a stream that a
    // task waits on does), and waking needs the executor.
    let tasks = EXEC.with(|e| std::mem::take(&mut *e.borrow_mut()));
    let timers = TIMERS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    drop(tasks);
    drop(timers);
}

/// Wakes a task by id. It is only an id, so the waker is `Send + Sync` without any `unsafe`.
struct TaskWaker(u64);

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        wake_task(self.0);
    }
}

fn request_tick(e: &mut Executor) {
    if !e.tick_scheduled && !e.ticking {
        e.tick_scheduled = true;
        host::schedule_tick();
    }
}

fn wake_task(id: u64) {
    EXEC.with(|e| {
        let mut e = e.borrow_mut();
        if let Some(task) = e.tasks.get_mut(&id)
            && !task.queued
        {
            task.queued = true;
            e.ready.push_back(id);
            request_tick(&mut e);
        }
    });
}

/// Runs `future` to completion in the background, polling it whenever it is woken.
pub fn spawn(future: impl Future<Output = ()> + 'static) {
    EXEC.with(|e| {
        let mut e = e.borrow_mut();
        let id = e.next;
        e.next += 1;
        e.tasks.insert(
            id,
            Task {
                fut: Some(Box::pin(future)),
                queued: true,
            },
        );
        e.ready.push_back(id);
        request_tick(&mut e);
    });
}

/// Polls every task that is ready, including those a poll makes ready.
pub fn tick() {
    struct Ticking;
    impl Drop for Ticking {
        fn drop(&mut self) {
            EXEC.with(|e| e.borrow_mut().ticking = false);
        }
    }
    EXEC.with(|e| {
        let mut e = e.borrow_mut();
        e.tick_scheduled = false;
        e.ticking = true;
    });
    let _guard = Ticking;
    while let Some(id) = EXEC.with(|e| e.borrow_mut().ready.pop_front()) {
        let fut = EXEC.with(|e| {
            e.borrow_mut().tasks.get_mut(&id).and_then(|t| {
                t.queued = false;
                t.fut.take()
            })
        });
        let Some(mut fut) = fut else { continue };
        let waker = Waker::from(Arc::new(TaskWaker(id)));
        let mut cx = Context::from_waker(&waker);
        let poll = fut.as_mut().poll(&mut cx);
        let finished = EXEC.with(|e| {
            let mut e = e.borrow_mut();
            match poll {
                Poll::Ready(()) => e.tasks.remove(&id),
                Poll::Pending => {
                    if let Some(task) = e.tasks.get_mut(&id) {
                        task.fut = Some(fut);
                    }
                    None
                }
            }
        });
        drop(finished); // outside the borrow, for the same reason as in `reset`
    }
}

/// Tasks that have been spawned and have not finished.
#[must_use]
pub fn pending_tasks() -> usize {
    EXEC.with(|e| e.borrow().tasks.len())
}

/// Called from `q_timer_fire`.
pub fn timer_fired(id: u32) {
    let waker = TIMERS.with(|t| {
        t.borrow_mut().pending.get_mut(&id).and_then(|state| {
            state.fired = true;
            state.waker.take()
        })
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// A future that completes after a delay. Dropping it before then cancels the timer.
#[derive(Debug)]
pub struct Sleep {
    ms: u32,
    id: Option<u32>,
    done: bool,
}

/// Completes after `ms` milliseconds.
#[must_use]
pub const fn sleep(ms: u32) -> Sleep {
    Sleep {
        ms,
        id: None,
        done: false,
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let Some(id) = self.id else {
            let id = TIMERS.with(|t| {
                let mut t = t.borrow_mut();
                let id = t.next;
                t.next = t.next.wrapping_add(1);
                t.pending.insert(
                    id,
                    TimerState {
                        fired: false,
                        waker: Some(cx.waker().clone()),
                    },
                );
                id
            });
            host::set_timeout(id, self.ms);
            self.id = Some(id);
            return Poll::Pending;
        };
        let fired = TIMERS.with(|t| {
            let mut t = t.borrow_mut();
            match t.pending.get_mut(&id) {
                Some(state) if state.fired => {
                    t.pending.remove(&id);
                    true
                }
                Some(state) => {
                    state.waker = Some(cx.waker().clone());
                    false
                }
                None => true,
            }
        });
        if fired {
            self.done = true;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let (Some(id), false) = (self.id, self.done) {
            let removed = TIMERS
                .try_with(|t| t.borrow_mut().pending.remove(&id))
                .ok()
                .flatten();
            if removed.is_some_and(|state| !state.fired) {
                host::clear_timeout(id);
            }
        }
    }
}
