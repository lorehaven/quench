//! Boxed futures and a minimal `block_on`.

use std::future::Future;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
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

    #[test]
    fn parks_until_woken() {
        let future = WokenLater {
            done: Arc::new(AtomicBool::new(false)),
            started: false,
        };
        assert_eq!(block_on(future), "woken");
    }

    #[test]
    fn boxed_futures_run() {
        let boxed: BoxFuture<'_, u8> = Box::pin(async { 7 });
        assert_eq!(block_on(boxed), 7);
        let local: LocalBoxFuture<'_, u8> = Box::pin(async { 8 });
        assert_eq!(block_on(local), 8);
    }
}
