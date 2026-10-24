//! A minimal `Stream` (async iterator) and the few adapters quench uses.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Values that arrive over time.
pub trait Stream {
    type Item;

    /// `Ready(Some(item))`, `Ready(None)` once finished, or `Pending`.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;

    /// `(lower bound, upper bound)` on the remaining items.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

/// A boxed, sendable stream.
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

impl<S: Stream + Unpin + ?Sized> Stream for &mut S {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut **self).poll_next(cx)
    }
}

impl<S: Stream + Unpin + ?Sized> Stream for Box<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut **self).poll_next(cx)
    }
}

impl<P> Stream for Pin<P>
where
    P: std::ops::DerefMut + Unpin,
    P::Target: Stream,
{
    type Item = <P::Target as Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().as_mut().poll_next(cx)
    }
}

/// Adapters, available on every [`Stream`].
pub trait StreamExt: Stream {
    /// The next item, or `None` when the stream has finished.
    fn next(&mut self) -> Next<'_, Self>
    where
        Self: Unpin,
    {
        Next { stream: self }
    }

    /// Transforms each item.
    fn map<T, F: FnMut(Self::Item) -> T>(self, f: F) -> Map<Self, F>
    where
        Self: Sized + Unpin,
    {
        Map { stream: self, f }
    }

    /// This stream's items, then `other`'s.
    fn chain<U>(self, other: U) -> Chain<Self, U>
    where
        Self: Sized + Unpin,
        U: Stream<Item = Self::Item> + Unpin,
    {
        Chain {
            first: Some(self),
            second: other,
        }
    }

    /// Keeps the items `f` maps to `Some`, transformed.
    fn filter_map<T, F: FnMut(Self::Item) -> Option<T>>(self, f: F) -> FilterMap<Self, F>
    where
        Self: Sized + Unpin,
    {
        FilterMap { stream: self, f }
    }

    /// Treats each item as a future and runs up to `limit` of them at once, yielding outputs as they finish
    /// (not in source order).
    fn buffer_unordered(self, limit: usize) -> BufferUnordered<Self>
    where
        Self: Sized + Unpin,
        Self::Item: Future,
    {
        BufferUnordered {
            stream: Some(self),
            in_flight: FuturesUnordered::new(),
            limit: limit.max(1),
        }
    }

    /// Collects every item. Never returns for an endless stream.
    fn collect_vec(self) -> CollectVec<Self>
    where
        Self: Sized + Unpin,
    {
        CollectVec {
            stream: Some(self),
            items: Vec::new(),
        }
    }
}

impl<S: Stream + ?Sized> StreamExt for S {}

/// Future for [`StreamExt::next`].
#[derive(Debug)]
pub struct Next<'a, S: ?Sized> {
    stream: &'a mut S,
}

impl<S: Stream + Unpin + ?Sized> Future for Next<'_, S> {
    type Output = Option<S::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.stream).poll_next(cx)
    }
}

/// Stream for [`StreamExt::map`].
#[derive(Debug)]
pub struct Map<S, F> {
    stream: S,
    f: F,
}

impl<S: Stream + Unpin, T, F: FnMut(S::Item) -> T + Unpin> Stream for Map<S, F> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        Pin::new(&mut this.stream)
            .poll_next(cx)
            .map(|item| item.map(&mut this.f))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

/// Stream for [`StreamExt::chain`].
#[derive(Debug)]
pub struct Chain<A, B> {
    first: Option<A>,
    second: B,
}

impl<A: Stream + Unpin, B: Stream<Item = A::Item> + Unpin> Stream for Chain<A, B> {
    type Item = A::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<A::Item>> {
        let this = &mut *self;
        if let Some(first) = this.first.as_mut() {
            match Pin::new(first).poll_next(cx) {
                Poll::Ready(None) => this.first = None,
                other => return other,
            }
        }
        Pin::new(&mut this.second).poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (second_low, second_high) = self.second.size_hint();
        self.first
            .as_ref()
            .map_or((second_low, second_high), |first| {
                let (low, high) = first.size_hint();
                (
                    low.saturating_add(second_low),
                    high.zip(second_high).and_then(|(a, b)| a.checked_add(b)),
                )
            })
    }
}

/// Stream for [`StreamExt::filter_map`].
#[derive(Debug)]
pub struct FilterMap<S, F> {
    stream: S,
    f: F,
}

impl<S: Stream + Unpin, T, F: FnMut(S::Item) -> Option<T> + Unpin> Stream for FilterMap<S, F> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        loop {
            match Pin::new(&mut this.stream).poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    if let Some(mapped) = (this.f)(item) {
                        return Poll::Ready(Some(mapped));
                    }
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.stream.size_hint().1)
    }
}

/// Future for [`StreamExt::collect_vec`].
#[derive(Debug)]
pub struct CollectVec<S: Stream> {
    stream: Option<S>,
    items: Vec<S::Item>,
}

impl<S: Stream + Unpin> Unpin for CollectVec<S> {}

impl<S: Stream + Unpin> Future for CollectVec<S> {
    type Output = Vec<S::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let Some(stream) = this.stream.as_mut() else {
            return Poll::Ready(std::mem::take(&mut this.items));
        };
        loop {
            match Pin::new(&mut *stream).poll_next(cx) {
                Poll::Ready(Some(item)) => this.items.push(item),
                Poll::Ready(None) => {
                    this.stream = None;
                    return Poll::Ready(std::mem::take(&mut this.items));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// A set of futures that yields each output as soon as its future finishes.
///
/// Every future is polled whenever the set is, which suits the handful of tasks it is used for.
pub struct FuturesUnordered<Fut> {
    futures: Vec<Pin<Box<Fut>>>,
}

impl<Fut> FuturesUnordered<Fut> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            futures: Vec::new(),
        }
    }

    /// Adds a future; it first runs the next time the set is polled.
    pub fn push(&mut self, future: Fut) {
        self.futures.push(Box::pin(future));
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.futures.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.futures.is_empty()
    }
}

impl<Fut> Default for FuturesUnordered<Fut> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Fut> std::fmt::Debug for FuturesUnordered<Fut> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FuturesUnordered")
            .field("futures", &self.futures.len())
            .finish()
    }
}

impl<Fut: Future> Stream for FuturesUnordered<Fut> {
    type Item = Fut::Output;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Fut::Output>> {
        if self.futures.is_empty() {
            return Poll::Ready(None);
        }
        for index in 0..self.futures.len() {
            if let Poll::Ready(output) = self.futures[index].as_mut().poll(cx) {
                drop(self.futures.swap_remove(index));
                return Poll::Ready(Some(output));
            }
        }
        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.futures.len(), Some(self.futures.len()))
    }
}

impl<Fut> FromIterator<Fut> for FuturesUnordered<Fut> {
    fn from_iter<I: IntoIterator<Item = Fut>>(iter: I) -> Self {
        Self {
            futures: iter.into_iter().map(Box::pin).collect(),
        }
    }
}

/// Stream for [`StreamExt::buffer_unordered`].
pub struct BufferUnordered<S: Stream> {
    stream: Option<S>,
    in_flight: FuturesUnordered<S::Item>,
    limit: usize,
}

impl<S: Stream> std::fmt::Debug for BufferUnordered<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferUnordered")
            .field("in_flight", &self.in_flight.len())
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl<S: Stream + Unpin> Stream for BufferUnordered<S>
where
    S::Item: Future,
{
    type Item = <S::Item as Future>::Output;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        while this.in_flight.len() < this.limit {
            let Some(stream) = this.stream.as_mut() else {
                break;
            };
            match Pin::new(stream).poll_next(cx) {
                Poll::Ready(Some(future)) => this.in_flight.push(future),
                Poll::Ready(None) => this.stream = None,
                Poll::Pending => break,
            }
        }
        match Pin::new(&mut this.in_flight).poll_next(cx) {
            Poll::Ready(Some(output)) => Poll::Ready(Some(output)),
            Poll::Ready(None) if this.stream.is_none() => Poll::Ready(None),
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }
}

/// A stream over an iterator's items, all immediately ready.
pub fn iter<I: IntoIterator>(items: I) -> Iter<I::IntoIter> {
    Iter {
        items: items.into_iter(),
    }
}

/// Stream for [`iter`].
#[derive(Debug, Clone)]
pub struct Iter<I> {
    items: I,
}

impl<I: Iterator + Unpin> Stream for Iter<I> {
    type Item = I::Item;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<I::Item>> {
        Poll::Ready(self.items.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

/// A stream of exactly one item.
pub fn once<T: Unpin>(item: T) -> Iter<std::option::IntoIter<T>> {
    iter(Some(item))
}

/// A stream with no items.
pub fn empty<T: Unpin>() -> Iter<std::option::IntoIter<T>> {
    iter(None)
}

/// A stream built from a state and an async step.
///
/// `f(state)` yields the next item and the state for the step after it, or `None` to finish. It wraps
/// anything with an async `recv`, e.g. a tokio channel:
/// `unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })`.
pub const fn unfold<S, T, F, Fut>(state: S, f: F) -> Unfold<S, F, Fut>
where
    F: FnMut(S) -> Fut,
    Fut: Future<Output = Option<(T, S)>>,
{
    Unfold {
        state: Some(state),
        f,
        pending: None,
    }
}

/// Stream for [`unfold`].
pub struct Unfold<S, F, Fut> {
    state: Option<S>,
    f: F,
    pending: Option<Pin<Box<Fut>>>,
}

impl<S, F, Fut> std::fmt::Debug for Unfold<S, F, Fut> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unfold").finish_non_exhaustive()
    }
}

impl<S: Unpin, T, F: FnMut(S) -> Fut + Unpin, Fut: Future<Output = Option<(T, S)>>> Stream
    for Unfold<S, F, Fut>
{
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        if this.pending.is_none() {
            let Some(state) = this.state.take() else {
                return Poll::Ready(None);
            };
            this.pending = Some(Box::pin((this.f)(state)));
        }
        let Some(pending) = this.pending.as_mut() else {
            return Poll::Ready(None);
        };
        match pending.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some((item, next))) => {
                this.pending = None;
                this.state = Some(next);
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                this.pending = None;
                Poll::Ready(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::future::{LocalBoxFuture, block_on};

    #[quench_test::test]
    fn next_walks_an_iterator_stream() {
        block_on(async {
            let mut s = iter(vec![1, 2, 3]);
            assert_eq!(s.size_hint(), (3, Some(3)));
            assert_eq!(s.next().await, Some(1));
            assert_eq!(s.next().await, Some(2));
            assert_eq!(s.next().await, Some(3));
            assert_eq!(s.next().await, None);
            assert_eq!(s.next().await, None);
        });
    }

    #[quench_test::test]
    fn once_and_empty() {
        block_on(async {
            assert_eq!(once("x").next().await, Some("x"));
            assert_eq!(empty::<u8>().next().await, None);
        });
    }

    #[quench_test::test]
    fn map_transforms_and_keeps_size_hint() {
        block_on(async {
            let mut s = iter(1..=3).map(|n| n * 10);
            assert_eq!(s.size_hint(), (3, Some(3)));
            assert_eq!(s.next().await, Some(10));
            assert_eq!(s.next().await, Some(20));
            assert_eq!(s.next().await, Some(30));
            assert_eq!(s.next().await, None);
        });
    }

    #[quench_test::test]
    fn futures_unordered_yields_in_completion_order() {
        block_on(async {
            let mut set: FuturesUnordered<LocalBoxFuture<'_, u8>> = FuturesUnordered::default();
            assert!(set.is_empty());
            assert_eq!(set.next().await, None);
            set.push(Box::pin(async { 1 }));
            set.push(Box::pin(async { 2 }));
            assert_eq!(set.len(), 2);
            assert_eq!(set.size_hint(), (2, Some(2)));
            let mut got = set.collect_vec().await;
            got.sort_unstable();
            assert_eq!(got, vec![1, 2]);
            assert!(
                format!("{:?}", FuturesUnordered::<std::future::Ready<u8>>::new()).contains('0')
            );
        });
    }

    #[quench_test::test]
    fn buffer_unordered_limits_concurrency_and_finishes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let running = std::sync::Arc::new(AtomicUsize::new(0));
        let peak = std::sync::Arc::new(AtomicUsize::new(0));
        let (r, p) = (running, peak.clone());
        let mut got = block_on(
            iter(0..10)
                .map(move |n| {
                    let (r, p) = (r.clone(), p.clone());
                    async move {
                        let now = r.fetch_add(1, Ordering::SeqCst) + 1;
                        p.fetch_max(now, Ordering::SeqCst);
                        Yield(false).await;
                        r.fetch_sub(1, Ordering::SeqCst);
                        n
                    }
                })
                .buffer_unordered(3)
                .collect_vec(),
        );
        got.sort_unstable();
        assert_eq!(got, (0..10).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::SeqCst), 3);

        let s = iter(0..2).map(|n| async move { n }).buffer_unordered(0);
        assert!(format!("{s:?}").contains("BufferUnordered"));
        assert_eq!(block_on(s.collect_vec()), vec![0, 1]);
    }

    /// Pending once, then ready - lets other futures in a set run in between.
    struct Yield(bool);

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                return Poll::Ready(());
            }
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    #[quench_test::test]
    fn works_through_references_boxes_and_pins() {
        block_on(async {
            let mut inner = iter(vec!['a', 'b']);
            let mut by_ref = &mut inner;
            assert_eq!(StreamExt::next(&mut by_ref).await, Some('a'));
            assert_eq!(inner.next().await, Some('b'));

            let mut boxed: Box<dyn Stream<Item = u8> + Unpin> = Box::new(iter(vec![1u8]));
            assert_eq!(boxed.next().await, Some(1));

            let mut pinned: BoxStream<'_, u8> = Box::pin(iter(vec![5u8, 6]));
            assert_eq!(pinned.next().await, Some(5));
            assert_eq!(pinned.next().await, Some(6));
            assert_eq!(pinned.next().await, None);
        });
    }

    #[quench_test::test]
    fn default_size_hint_is_unbounded() {
        struct Never;
        impl Stream for Never {
            type Item = ();

            fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<()>> {
                Poll::Pending
            }
        }
        assert_eq!(Never.size_hint(), (0, None));
    }

    #[quench_test::test]
    fn collect_vec_gathers_everything() {
        assert_eq!(
            block_on(iter(1..=4).map(|n| n * n).collect_vec()),
            vec![1, 4, 9, 16]
        );
        assert_eq!(block_on(empty::<u8>().collect_vec()), Vec::<u8>::new());
    }

    #[quench_test::test]
    fn filter_map_drops_and_transforms() {
        block_on(async {
            let mut s = iter(1..=6).filter_map(|n| (n % 2 == 0).then_some(n * 10));
            assert_eq!(s.size_hint(), (0, Some(6)));
            assert_eq!(s.next().await, Some(20));
            assert_eq!(s.next().await, Some(40));
            assert_eq!(s.next().await, Some(60));
            assert_eq!(s.next().await, None);
        });
    }

    #[quench_test::test]
    fn unfold_runs_the_step_until_it_says_stop() {
        let counting = unfold(0u32, |n| async move { (n < 3).then_some((n * 2, n + 1)) });
        assert_eq!(block_on(counting.collect_vec()), vec![0, 2, 4]);
        assert!(format!("{:?}", unfold((), |()| async { None::<((), ())> })).contains("Unfold"));
    }

    #[quench_test::test]
    fn unfold_stays_finished_and_waits_for_pending_steps() {
        let mut s = unfold((), |()| async { None::<((), ())> });
        block_on(async {
            assert_eq!(s.next().await, None);
            assert_eq!(s.next().await, None);
        });
        // A step that is not ready yet: the stream is pending, then yields.
        let mut yielded = false;
        let mut s = unfold(false, move |done| {
            let first = !yielded;
            yielded = true;
            async move {
                if first {
                    std::future::pending::<()>().await;
                }
                (!done).then_some((7u8, true))
            }
        });
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        assert!(Pin::new(&mut s).poll_next(&mut cx).is_pending());
    }

    #[quench_test::test]
    fn chain_runs_one_stream_after_the_other() {
        block_on(async {
            let mut s = iter(vec![1, 2]).chain(iter(vec![3]));
            assert_eq!(s.size_hint(), (3, Some(3)));
            assert_eq!(s.next().await, Some(1));
            assert_eq!(s.next().await, Some(2));
            assert_eq!(s.size_hint(), (1, Some(1)));
            assert_eq!(s.next().await, Some(3));
            assert_eq!(s.size_hint(), (0, Some(0)));
            assert_eq!(s.next().await, None);
            assert_eq!(s.next().await, None);
        });
        let unbounded = iter(vec![1]).chain(unfold((), |()| async { None::<(i32, ())> }));
        assert_eq!(unbounded.size_hint(), (1, None));
    }
}
