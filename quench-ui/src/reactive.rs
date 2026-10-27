//! Fine-grained reactivity: signals, memos, effects and the scopes that own them.
//!
//! Everything lives on the current thread (`Rc`/`RefCell`), so none of the types are `Send`.
//!
//! The graph is push-dirty, pull-check. Writing a [`Signal`] marks its observers dirty and the
//! observers behind them "check"; effects are queued once. Running the queue first refreshes the
//! memos an effect reads, and an effect re-runs only if one of them really changed, so diamond
//! shaped dependencies never run an effect twice or show it a half-updated state.
//!
//! Ownership is a tree of [`Scope`]s. Effects, memos, cleanups and contexts belong to the scope
//! that was current when they were created, and disposing a scope disposes all of it, unsubscribing
//! everything from the signals it read and dropping the closures.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::ptr;
use std::rc::{Rc, Weak};

/// Effects flushed in one go before the runtime decides two effects are feeding each other.
const FLUSH_LIMIT: usize = 100_000;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Clean,
    Check,
    Dirty,
}

struct Runtime {
    observer: RefCell<Option<Rc<Computation>>>,
    owner: RefCell<Option<Rc<ScopeInner>>>,
    batch_depth: Cell<usize>,
    queue: RefCell<VecDeque<Rc<Computation>>>,
    flushing: Cell<bool>,
}

thread_local! {
    static RUNTIME: Runtime = const {
        Runtime {
            observer: RefCell::new(None),
            owner: RefCell::new(None),
            batch_depth: Cell::new(0),
            queue: RefCell::new(VecDeque::new()),
            flushing: Cell::new(false),
        }
    };
}

fn with_runtime<T>(f: impl FnOnce(&Runtime) -> T) -> T {
    RUNTIME.with(f)
}

type Observers = RefCell<Vec<Weak<Computation>>>;

/// Something a computation can read: a signal or another computation.
trait Source {
    fn observers(&self) -> &Observers;

    /// Brings the value up to date. Only computations have anything to do.
    fn refresh(&self) {}
}

/// An effect or a memo.
struct Computation {
    state: Cell<State>,
    sources: RefCell<Vec<Rc<dyn Source>>>,
    observers: Observers,
    run: RefCell<Option<Box<dyn FnMut() -> bool>>>,
    effect: bool,
    scope: Rc<ScopeInner>,
    parent: Weak<ScopeInner>,
    disposed: Cell<bool>,
    weak_self: Weak<Self>,
}

impl Source for Computation {
    fn observers(&self) -> &Observers {
        &self.observers
    }

    fn refresh(&self) {
        if let Some(this) = self.weak_self.upgrade() {
            this.update_if_necessary();
        }
    }
}

impl Computation {
    /// Creates a computation owned by the current scope and runs it once.
    fn spawn(effect: bool, run: Box<dyn FnMut() -> bool>) -> Rc<Self> {
        let parent = current_scope();
        let scope = ScopeInner::new(parent.as_ref());
        let comp = Rc::new_cyclic(|weak| Self {
            state: Cell::new(State::Dirty),
            sources: RefCell::new(Vec::new()),
            observers: RefCell::new(Vec::new()),
            run: RefCell::new(Some(run)),
            effect,
            scope,
            parent: parent.as_ref().map_or_else(Weak::new, Rc::downgrade),
            disposed: Cell::new(false),
            weak_self: weak.clone(),
        });
        if let Some(parent) = &parent {
            parent.comps.borrow_mut().push(Rc::clone(&comp));
        }
        // In a batch, so a write the first run makes to its own sources is queued rather than
        // flushed into a computation that is still running.
        batch(|| comp.update());
        comp
    }

    /// Runs the closure under this computation. Returns whether its value changed.
    fn update(self: &Rc<Self>) -> bool {
        if self.disposed.get() {
            return false;
        }
        self.unsubscribe();
        self.scope.reset();
        self.state.set(State::Clean);
        let Some(mut run) = self.run.borrow_mut().take() else {
            return false;
        };
        let previous_observer = with_runtime(|rt| rt.observer.replace(Some(Rc::clone(self))));
        let previous_owner = with_runtime(|rt| rt.owner.replace(Some(Rc::clone(&self.scope))));
        let restore = RestoreOnDrop {
            observer: previous_observer,
            owner: previous_owner,
        };
        let changed = run();
        drop(restore);
        if self.disposed.get() {
            drop(run);
        } else {
            *self.run.borrow_mut() = Some(run);
        }
        if changed {
            for observer in self.live_observers() {
                if !observer.disposed.get() {
                    observer.state.set(State::Dirty);
                }
            }
        }
        changed
    }

    fn update_if_necessary(self: &Rc<Self>) {
        if self.state.get() == State::Check {
            let sources: Vec<Rc<dyn Source>> = self.sources.borrow().clone();
            for source in sources {
                source.refresh();
                if self.state.get() == State::Dirty {
                    break;
                }
            }
        }
        match self.state.get() {
            State::Dirty => {
                self.update();
            }
            State::Check => self.state.set(State::Clean),
            State::Clean => {}
        }
    }

    fn live_observers(&self) -> Vec<Rc<Self>> {
        self.observers
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect()
    }

    /// Removes this computation from every source it was subscribed to.
    fn unsubscribe(self: &Rc<Self>) {
        let sources = std::mem::take(&mut *self.sources.borrow_mut());
        for source in sources {
            source
                .observers()
                .borrow_mut()
                .retain(|w| !ptr::eq(w.as_ptr(), Rc::as_ptr(self)));
        }
    }

    fn dispose(self: &Rc<Self>) {
        if self.disposed.replace(true) {
            return;
        }
        self.unsubscribe();
        let run = self.run.borrow_mut().take();
        self.scope.dispose();
        if let Some(parent) = self.parent.upgrade() {
            parent.comps.borrow_mut().retain(|c| !Rc::ptr_eq(c, self));
        }
        drop(run);
    }
}

struct RestoreOnDrop {
    observer: Option<Rc<Computation>>,
    owner: Option<Rc<ScopeInner>>,
}

impl Drop for RestoreOnDrop {
    fn drop(&mut self) {
        let observer = self.observer.take();
        let owner = self.owner.take();
        with_runtime(|rt| {
            *rt.observer.borrow_mut() = observer;
            *rt.owner.borrow_mut() = owner;
        });
    }
}

/// Marks `comp` stale and, the first time, everything downstream of it.
fn stale(comp: &Rc<Computation>, state: State) {
    if comp.disposed.get() || comp.state.get() >= state {
        return;
    }
    if comp.state.get() == State::Clean {
        if comp.effect {
            with_runtime(|rt| rt.queue.borrow_mut().push_back(Rc::clone(comp)));
        } else {
            for observer in comp.live_observers() {
                stale(&observer, State::Check);
            }
        }
    }
    comp.state.set(state);
}

/// Subscribes the running computation, if any, to `source`.
fn track(source: &Rc<dyn Source>) {
    let Some(observer) = with_runtime(|rt| rt.observer.borrow().clone()) else {
        return;
    };
    let mut sources = observer.sources.borrow_mut();
    if sources
        .iter()
        .any(|s| ptr::addr_eq(Rc::as_ptr(s), Rc::as_ptr(source)))
    {
        return;
    }
    source
        .observers()
        .borrow_mut()
        .push(Rc::downgrade(&observer));
    sources.push(Rc::clone(source));
}

/// Marks the observers of a written source and runs the queue unless a batch is open.
fn notify(observers: &Observers) {
    let live: Vec<Rc<Computation>> = observers
        .borrow()
        .iter()
        .filter_map(Weak::upgrade)
        .collect();
    for observer in &live {
        stale(observer, State::Dirty);
    }
    flush_if_idle();
}

fn flush_if_idle() {
    let idle = with_runtime(|rt| rt.batch_depth.get() == 0 && !rt.flushing.get());
    if idle {
        flush();
    }
}

fn flush() {
    struct Flushing;
    impl Drop for Flushing {
        fn drop(&mut self) {
            with_runtime(|rt| rt.flushing.set(false));
        }
    }
    with_runtime(|rt| rt.flushing.set(true));
    let _guard = Flushing;
    let mut ran = 0;
    while let Some(effect) = with_runtime(|rt| rt.queue.borrow_mut().pop_front()) {
        ran += 1;
        assert!(
            ran < FLUSH_LIMIT,
            "reactive cycle: effects keep triggering each other"
        );
        effect.update_if_necessary();
    }
}

/// Runs `f`, deferring effects until it returns. Writes inside it are visible immediately to
/// reads; each effect that depends on any of them runs once afterwards.
pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    struct Batch;
    impl Drop for Batch {
        fn drop(&mut self) {
            let outermost = with_runtime(|rt| {
                rt.batch_depth.set(rt.batch_depth.get() - 1);
                rt.batch_depth.get() == 0
            });
            if outermost && !std::thread::panicking() {
                flush_if_idle();
            }
        }
    }
    with_runtime(|rt| rt.batch_depth.set(rt.batch_depth.get() + 1));
    let _batch = Batch;
    f()
}

/// Runs `f` without subscribing the running effect or memo to anything `f` reads.
pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    let previous = with_runtime(|rt| rt.observer.borrow_mut().take());
    let _restore = RestoreOnDrop {
        observer: previous,
        owner: with_runtime(|rt| rt.owner.borrow().clone()),
    };
    f()
}

// ---------------------------------------------------------------------------------------------
// Signals

struct SignalInner<T> {
    value: RefCell<T>,
    observers: Observers,
}

impl<T> Source for SignalInner<T> {
    fn observers(&self) -> &Observers {
        &self.observers
    }
}

/// A value that notifies whatever read it when it is written.
pub struct Signal<T> {
    inner: Rc<SignalInner<T>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Signal")
            .field(&*self.inner.value.borrow())
            .finish()
    }
}

/// Creates a signal holding `value`.
#[must_use]
pub fn create_signal<T: 'static>(value: T) -> Signal<T> {
    Signal {
        inner: Rc::new(SignalInner {
            value: RefCell::new(value),
            observers: RefCell::new(Vec::new()),
        }),
    }
}

impl<T: 'static> Signal<T> {
    fn track(&self) {
        let source: Rc<dyn Source> = Rc::clone(&self.inner) as Rc<dyn Source>;
        track(&source);
    }

    /// Reads the value and subscribes the running effect or memo.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }

    /// Reads the value without subscribing.
    #[must_use]
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.inner.value.borrow().clone()
    }

    /// Borrows the value and subscribes. `f` must not write this signal.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&self.inner.value.borrow())
    }

    /// Borrows the value without subscribing.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }

    /// Replaces the value and notifies every observer.
    pub fn set(&self, value: T) {
        *self.inner.value.borrow_mut() = value;
        notify(&self.inner.observers);
    }

    /// Changes the value in place and notifies every observer.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let out = f(&mut self.inner.value.borrow_mut());
        notify(&self.inner.observers);
        out
    }

    /// Replaces the value, notifying only if it differs from the current one.
    pub fn set_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        if *self.inner.value.borrow() != value {
            self.set(value);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Memos

/// A derived value, recomputed only when what it read changed, and observed only if it differs.
pub struct Memo<T> {
    value: Rc<RefCell<Option<T>>>,
    comp: Rc<Computation>,
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            value: Rc::clone(&self.value),
            comp: Rc::clone(&self.comp),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Memo").field(&*self.value.borrow()).finish()
    }
}

/// Creates a memo. Its observers are notified only when the new value is not equal to the old.
#[must_use]
pub fn create_memo<T: PartialEq + 'static>(mut f: impl FnMut() -> T + 'static) -> Memo<T> {
    let value: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&value);
    let comp = Computation::spawn(
        false,
        Box::new(move || {
            let new = f();
            let mut slot = slot.borrow_mut();
            let changed = slot.as_ref() != Some(&new);
            if changed {
                *slot = Some(new);
            }
            changed
        }),
    );
    Memo { value, comp }
}

impl<T: 'static> Memo<T> {
    fn refresh_and_track(&self) {
        self.comp.update_if_necessary();
        let source: Rc<dyn Source> = Rc::clone(&self.comp) as Rc<dyn Source>;
        track(&source);
    }

    /// Reads the value, bringing it up to date first, and subscribes.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }

    /// Borrows the value, bringing it up to date first, and subscribes.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.refresh_and_track();
        f(self
            .value
            .borrow()
            .as_ref()
            .expect("memo computed on creation"))
    }

    /// Reads the value without subscribing.
    #[must_use]
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.comp.update_if_necessary();
        self.value
            .borrow()
            .clone()
            .expect("memo computed on creation")
    }
}

// ---------------------------------------------------------------------------------------------
// Effects

/// A side effect that re-runs when what it read changes.
#[derive(Clone)]
pub struct Effect {
    comp: Rc<Computation>,
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Effect")
            .field("disposed", &self.comp.disposed.get())
            .finish()
    }
}

/// Runs `f` now and again whenever a signal or memo it read changes. It belongs to the current
/// scope and stops when that scope is disposed, or on [`Effect::dispose`].
pub fn create_effect(mut f: impl FnMut() + 'static) -> Effect {
    let comp = Computation::spawn(
        true,
        Box::new(move || {
            f();
            false
        }),
    );
    Effect { comp }
}

impl Effect {
    /// Stops the effect and releases what it captured.
    pub fn dispose(&self) {
        self.comp.dispose();
    }

    #[must_use]
    pub fn is_disposed(&self) -> bool {
        self.comp.disposed.get()
    }
}

// ---------------------------------------------------------------------------------------------
// Scopes

struct ScopeInner {
    comps: RefCell<Vec<Rc<Computation>>>,
    children: RefCell<Vec<Rc<Self>>>,
    cleanups: RefCell<Vec<Box<dyn FnOnce()>>>,
    contexts: RefCell<Vec<(TypeId, Rc<dyn Any>)>>,
    parent: Weak<Self>,
    disposed: Cell<bool>,
}

impl ScopeInner {
    fn new(parent: Option<&Rc<Self>>) -> Rc<Self> {
        let scope = Rc::new(Self {
            comps: RefCell::new(Vec::new()),
            children: RefCell::new(Vec::new()),
            cleanups: RefCell::new(Vec::new()),
            contexts: RefCell::new(Vec::new()),
            parent: parent.map_or_else(Weak::new, Rc::downgrade),
            disposed: Cell::new(false),
        });
        if let Some(parent) = parent {
            parent.children.borrow_mut().push(Rc::clone(&scope));
        }
        scope
    }

    /// Disposes everything the scope owns but keeps the scope itself usable.
    fn reset(&self) {
        let children = std::mem::take(&mut *self.children.borrow_mut());
        for child in children {
            child.dispose();
        }
        let comps = std::mem::take(&mut *self.comps.borrow_mut());
        for comp in comps {
            comp.dispose();
        }
        let cleanups = std::mem::take(&mut *self.cleanups.borrow_mut());
        for cleanup in cleanups.into_iter().rev() {
            cleanup();
        }
        self.contexts.borrow_mut().clear();
    }

    fn dispose(self: &Rc<Self>) {
        if self.disposed.replace(true) {
            return;
        }
        self.reset();
        if let Some(parent) = self.parent.upgrade() {
            parent
                .children
                .borrow_mut()
                .retain(|c| !Rc::ptr_eq(c, self));
        }
    }
}

fn current_scope() -> Option<Rc<ScopeInner>> {
    with_runtime(|rt| rt.owner.borrow().clone())
}

/// An owner of effects, memos, cleanups and contexts.
#[derive(Clone)]
pub struct Scope {
    inner: Rc<ScopeInner>,
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("disposed", &self.inner.disposed.get())
            .finish_non_exhaustive()
    }
}

impl Scope {
    /// A new scope nested in the current one, or a root if there is none.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: ScopeInner::new(current_scope().as_ref()),
        }
    }

    /// A new scope nested in `parent`.
    #[must_use]
    pub fn new_in(parent: &Self) -> Self {
        Self {
            inner: ScopeInner::new(Some(&parent.inner)),
        }
    }

    /// The scope that is current, if any.
    #[must_use]
    pub fn current() -> Option<Self> {
        current_scope().map(|inner| Self { inner })
    }

    /// Runs `f` with this scope as the owner of what it creates.
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> T {
        let previous = with_runtime(|rt| rt.owner.replace(Some(Rc::clone(&self.inner))));
        let _restore = RestoreOnDrop {
            observer: with_runtime(|rt| rt.observer.borrow().clone()),
            owner: previous,
        };
        f()
    }

    /// Disposes everything this scope owns, now and for good.
    pub fn dispose(&self) {
        self.inner.dispose();
    }

    /// Disposes what the scope owns but leaves it usable.
    pub fn reset(&self) {
        self.inner.reset();
    }

    #[must_use]
    pub fn is_disposed(&self) -> bool {
        self.inner.disposed.get()
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs `f` in a fresh root scope and returns its result with the scope, to dispose later.
pub fn create_root<T>(f: impl FnOnce() -> T) -> (T, Scope) {
    let scope = Scope {
        inner: ScopeInner::new(None),
    };
    let out = scope.run(f);
    (out, scope)
}

/// Registers `f` to run when the current scope is disposed or re-run. Without a scope it is
/// dropped unrun.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    if let Some(scope) = current_scope() {
        scope.cleanups.borrow_mut().push(Box::new(f));
    }
}

/// Makes `value` available to this scope and everything under it through [`use_context`].
///
/// # Panics
/// Without a current scope.
pub fn provide_context<T: 'static>(value: T) {
    let scope =
        current_scope().expect("provide_context needs a scope; wrap the call in create_root");
    let mut contexts = scope.contexts.borrow_mut();
    let id = TypeId::of::<T>();
    contexts.retain(|(t, _)| *t != id);
    contexts.push((id, Rc::new(value)));
}

/// The nearest value of type `T` provided by this scope or one above it.
#[must_use]
pub fn use_context<T: Clone + 'static>() -> Option<T> {
    let id = TypeId::of::<T>();
    let mut scope = current_scope();
    while let Some(s) = scope {
        let found = s
            .contexts
            .borrow()
            .iter()
            .find(|(t, _)| *t == id)
            .and_then(|(_, v)| v.downcast_ref::<T>().cloned());
        if found.is_some() {
            return found;
        }
        scope = s.parent.upgrade();
    }
    None
}
