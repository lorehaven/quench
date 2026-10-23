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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::future::block_on;

    #[test]
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

    #[test]
    fn once_and_empty() {
        block_on(async {
            assert_eq!(once("x").next().await, Some("x"));
            assert_eq!(empty::<u8>().next().await, None);
        });
    }

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
    fn collect_vec_gathers_everything() {
        assert_eq!(
            block_on(iter(1..=4).map(|n| n * n).collect_vec()),
            vec![1, 4, 9, 16]
        );
        assert_eq!(block_on(empty::<u8>().collect_vec()), Vec::<u8>::new());
    }
}
