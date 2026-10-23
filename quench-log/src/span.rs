//! Spans: a named stretch of work whose fields ride along on every event
//! logged inside it.
//!
//! A thread keeps a stack of the spans it has entered. Async code does not
//! stay on a thread, so a future is wrapped with [`Instrument::instrument`],
//! which enters the span for exactly the duration of each poll.

use std::cell::RefCell;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use crate::field::{FieldValue, IntoFieldValue};
use crate::level::Level;
use crate::record::SpanSnapshot;

#[derive(Debug)]
struct Inner {
    name: &'static str,
    fields: Mutex<Vec<(String, FieldValue)>>,
}

thread_local! {
    static STACK: RefCell<Vec<Arc<Inner>>> = const { RefCell::new(Vec::new()) };
}

/// A handle to a span; cheap to clone. A disabled span (its level filtered
/// out) is a no-op everywhere.
#[derive(Debug, Clone, Default)]
pub struct Span {
    inner: Option<Arc<Inner>>,
}

impl Span {
    /// A span that records nothing.
    #[must_use]
    pub const fn none() -> Self {
        Self { inner: None }
    }

    /// Creates a span if `level` is enabled for `target`. Prefer the
    /// `span!` family of macros.
    #[must_use]
    pub fn new(
        level: Level,
        target: &str,
        name: &'static str,
        fields: Vec<(&'static str, FieldValue)>,
    ) -> Self {
        if !crate::enabled(level, target) {
            return Self::none();
        }
        let fields = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        Self {
            inner: Some(Arc::new(Inner {
                name,
                fields: Mutex::new(fields),
            })),
        }
    }

    /// The innermost span this thread is in, or [`Span::none`].
    #[must_use]
    pub fn current() -> Self {
        STACK.with(|stack| Self {
            inner: stack.borrow().last().cloned(),
        })
    }

    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    /// Makes this the current span until the guard is dropped.
    #[must_use = "the span is only current while the guard lives"]
    pub fn enter(&self) -> Entered<'_> {
        if let Some(inner) = &self.inner {
            STACK.with(|stack| stack.borrow_mut().push(Arc::clone(inner)));
        }
        Entered { span: self }
    }

    /// Runs `f` with this span entered.
    pub fn in_scope<R>(&self, f: impl FnOnce() -> R) -> R {
        let _entered = self.enter();
        f()
    }

    /// Sets a field on this span, adding it if it was not declared.
    pub fn record(&self, name: &str, value: impl IntoFieldValue) -> &Self {
        self.set(name, value.field_value());
        self
    }

    /// Like [`record`](Self::record) for a `Display` value.
    pub fn record_display(&self, name: &str, value: &dyn Display) -> &Self {
        self.set(name, crate::field::display(value));
        self
    }

    fn set(&self, name: &str, value: FieldValue) {
        let Some(inner) = &self.inner else { return };
        let mut fields = inner.fields.lock().unwrap_or_else(PoisonError::into_inner);
        match fields.iter_mut().find(|(k, _)| k == name) {
            Some((_, slot)) => *slot = value,
            None => fields.push((name.to_string(), value)),
        }
    }

    /// The enclosing spans of the current thread, outermost first.
    pub(crate) fn snapshot() -> Vec<SpanSnapshot> {
        STACK.with(|stack| {
            stack
                .borrow()
                .iter()
                .map(|inner| SpanSnapshot {
                    name: inner.name,
                    fields: inner
                        .fields
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .iter()
                        .filter(|(_, v)| !v.is_empty())
                        .cloned()
                        .collect(),
                })
                .collect()
        })
    }
}

/// Leaves the span when dropped.
#[derive(Debug)]
pub struct Entered<'a> {
    span: &'a Span,
}

impl Drop for Entered<'_> {
    fn drop(&mut self) {
        if self.span.inner.is_some() {
            STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
}

/// Attaches a span to a future.
pub trait Instrument: Sized {
    /// The returned future enters `span` whenever it is polled.
    fn instrument(self, span: Span) -> Instrumented<Self> {
        Instrumented {
            span,
            inner: Box::pin(self),
        }
    }

    /// Instruments with the span that is current now.
    fn in_current_span(self) -> Instrumented<Self> {
        self.instrument(Span::current())
    }
}

impl<F: Future> Instrument for F {}

/// A future that runs inside a span.
pub struct Instrumented<F> {
    span: Span,
    // Boxed so that `Instrumented` is `Unpin` and can be polled without
    // pin-projection.
    inner: Pin<Box<F>>,
}

impl<F> std::fmt::Debug for Instrumented<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instrumented")
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

impl<F: Future> Future for Instrumented<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let _entered = this.span.enter();
        this.inner.as_mut().poll(cx)
    }
}
