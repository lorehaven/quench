//! Boxed futures, `join_all`, `catch_unwind` and a minimal `block_on`.

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe, UnwindSafe};
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// A boxed, sendable future.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A boxed future that stays on its thread.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs a future to completion on the current thread, parking while it waits.
///
/// For tests and tiny tools. Anything doing real I/O belongs on the tokio
/// runtime, which this does not replace.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        thread::park();
    }
}

/// Runs every future at once and resolves to their outputs, in the order the futures were given.
pub fn join_all<I>(futures: I) -> JoinAll<I::Item>
where
    I: IntoIterator,
    I::Item: Future,
{
    JoinAll {
        slots: futures
            .into_iter()
            .map(|future| Slot::Pending(Box::pin(future)))
            .collect(),
    }
}

enum Slot<F: Future> {
    Pending(Pin<Box<F>>),
    Done(Option<F::Output>),
}

/// Future for [`join_all`].
pub struct JoinAll<F: Future> {
    slots: Vec<Slot<F>>,
}

impl<F: Future> fmt::Debug for JoinAll<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinAll")
            .field("futures", &self.slots.len())
            .finish()
    }
}

impl<F: Future> Unpin for JoinAll<F> {}

impl<F: Future> Future for JoinAll<F> {
    type Output = Vec<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut all_done = true;
        for slot in &mut self.slots {
            if let Slot::Pending(future) = slot {
                match future.as_mut().poll(cx) {
                    Poll::Ready(output) => *slot = Slot::Done(Some(output)),
                    Poll::Pending => all_done = false,
                }
            }
        }
        if !all_done {
            return Poll::Pending;
        }
        Poll::Ready(
            std::mem::take(&mut self.slots)
                .into_iter()
                .filter_map(|slot| match slot {
                    Slot::Done(output) => output,
                    Slot::Pending(_) => None,
                })
                .collect(),
        )
    }
}

/// Turns a panic inside the future into an `Err` carrying the panic payload.
///
/// Wrap a future that is not `UnwindSafe` in [`AssertUnwindSafe`] first.
pub fn catch_unwind<F: Future + UnwindSafe>(future: F) -> CatchUnwind<F> {
    CatchUnwind {
        future: Box::pin(future),
    }
}

/// Future for [`catch_unwind`].
pub struct CatchUnwind<F> {
    future: Pin<Box<F>>,
}

impl<F> fmt::Debug for CatchUnwind<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CatchUnwind").finish_non_exhaustive()
    }
}

impl<F: Future + UnwindSafe> Future for CatchUnwind<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let future = self.future.as_mut();
        match panic::catch_unwind(AssertUnwindSafe(|| future.poll(cx))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[quench_test::test]
    fn join_all_keeps_the_order_of_the_futures() {
        let futures = (0..5).map(|n| async move { n * 2 });
        assert_eq!(block_on(join_all(futures)), vec![0, 2, 4, 6, 8]);
        assert_eq!(
            block_on(join_all(Vec::<std::future::Ready<u8>>::new())),
            Vec::<u8>::new()
        );
    }

    #[quench_test::test]
    fn join_all_waits_for_the_slowest() {
        let slow = WokenLater {
            done: Arc::new(AtomicBool::new(false)),
            started: false,
        };
        let futures: Vec<LocalBoxFuture<'_, &str>> =
            vec![Box::pin(async { "quick" }), Box::pin(slow)];
        assert_eq!(block_on(join_all(futures)), vec!["quick", "woken"]);
        assert!(
            format!("{:?}", join_all(Vec::<std::future::Ready<u8>>::new())).contains("JoinAll")
        );
    }

    #[quench_test::test]
    fn catch_unwind_returns_the_panic_payload() {
        let outcome = block_on(catch_unwind(async { 5 }));
        assert_eq!(outcome.ok(), Some(5));

        let outcome = block_on(catch_unwind(async {
            assert!(std::hint::black_box(false), "boom");
        }));
        let payload = outcome.expect_err("the future panicked");
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"boom"));

        let woken = WokenLater {
            done: Arc::new(AtomicBool::new(false)),
            started: false,
        };
        assert_eq!(block_on(catch_unwind(woken)).ok(), Some("woken"));
        assert!(format!("{:?}", catch_unwind(async {})).contains("CatchUnwind"));
    }

    #[quench_test::test]
    fn ready_futures_complete_immediately() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    /// Pending on first poll, woken from another thread.
    struct WokenLater {
        done: Arc<AtomicBool>,
        started: bool,
    }

    impl Future for WokenLater {
        type Output = &'static str;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.done.load(Ordering::SeqCst) {
                return Poll::Ready("woken");
            }
            if !self.started {
                self.started = true;
                let (done, waker) = (Arc::clone(&self.done), cx.waker().clone());
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(20));
                    done.store(true, Ordering::SeqCst);
                    waker.wake();
                });
            }
            Poll::Pending
        }
    }

    #[quench_test::test]
    fn parks_until_woken() {
        let future = WokenLater {
            done: Arc::new(AtomicBool::new(false)),
            started: false,
        };
        assert_eq!(block_on(future), "woken");
    }

    #[quench_test::test]
    fn boxed_futures_run() {
        let boxed: BoxFuture<'_, u8> = Box::pin(async { 7 });
        assert_eq!(block_on(boxed), 7);
        let local: LocalBoxFuture<'_, u8> = Box::pin(async { 8 });
        assert_eq!(block_on(local), 8);
    }
}
