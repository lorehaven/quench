//! A real dependency-injection container: components declare their
//! dependencies (as `Arc<T>` constructor parameters via `#[injectable]`),
//! and [`ContainerBuilder::build`] resolves the whole graph with a
//! topological sort - detecting missing dependencies and cycles at startup
//! rather than leaving them to surface as a panic on first use.
pub use crate::error::DiError;
use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

/// A type-erased, resolved component.
pub type BoxedAny = Arc<dyn Any + Send + Sync>;
/// The future a generated `#[injectable]` constructor returns. Public only
/// so macro expansion can name it.
pub type ConstructFuture = Pin<Box<dyn Future<Output = Result<BoxedAny, DiError>> + Send>>;

/// Registered by `#[injectable]` for every discovered component. Not meant
/// to be constructed by hand.
///
/// `inventory::submit!` places its argument inside a `static` initializer
/// (see the `inventory` crate's expansion), so every field here has to be
/// const-constructible - that's why `type_id` and `dependencies` are
/// function pointers rather than a `TypeId`/`Vec` computed up front: a bare
/// `TypeId::of::<T>()` or `vec![...]` call isn't valid in that position, but
/// a reference to a plain `fn` is.
pub struct ComponentFactory {
    pub type_id: fn() -> TypeId,
    /// Also a function pointer, not a plain `&'static str`: like `type_id`,
    /// `std::any::type_name::<T>()` isn't const-stable, so it has to be
    /// deferred to a call at graph-resolution time rather than evaluated in
    /// `submit!`'s `static` initializer.
    pub type_name: fn() -> &'static str,
    /// The dependencies this component's constructor needs, as
    /// `(type_id, type_name)` pairs - the name is carried alongside so a
    /// "missing dependency" error can name it without a second lookup.
    pub dependencies: fn() -> Vec<(TypeId, &'static str)>,
    pub construct: fn(&Container) -> ConstructFuture,
}

inventory::collect!(ComponentFactory);

/// Shared, read-through store of resolved components. Cheap to clone (an
/// `Arc` around the map) and safe to hand to request handlers.
#[derive(Clone, Default)]
pub struct Container {
    // `resolved` intentionally has no `Debug` impl of its own (its values
    // are `dyn Any`) - `Container` gets one below that just reports how
    // many components are resolved, enough for `unwrap_err` et al.
    resolved: Arc<RwLock<HashMap<TypeId, BoxedAny>>>,
}

impl std::fmt::Debug for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.resolved.read().map(|r| r.len()).unwrap_or(0);
        f.debug_struct("Container")
            .field("resolved_components", &count)
            .finish()
    }
}

impl Container {
    fn insert<T: Any + Send + Sync + 'static>(&self, value: Arc<T>) {
        self.resolved
            .write()
            .expect("container lock poisoned")
            .insert(TypeId::of::<T>(), value as BoxedAny);
    }

    fn insert_dyn(&self, type_id: TypeId, value: BoxedAny) {
        self.resolved
            .write()
            .expect("container lock poisoned")
            .insert(type_id, value);
    }

    fn contains(&self, type_id: TypeId) -> bool {
        self.resolved
            .read()
            .expect("container lock poisoned")
            .contains_key(&type_id)
    }

    /// Looks up a previously-resolved (via `#[injectable]`) or provided (via
    /// `ContainerBuilder::provide`) component.
    pub fn get<T: Any + Send + Sync + 'static>(&self) -> Result<Arc<T>, DiError> {
        self.resolved
            .read()
            .expect("container lock poisoned")
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|v| v.downcast::<T>().ok())
            .ok_or_else(DiError::not_found::<T>)
    }
}

/// Builds a [`Container`]: seed it with externally-constructed values via
/// [`provide`](Self::provide) (config loaded from env, a DB pool, anything
/// that isn't itself `#[injectable]`), then [`build`](Self::build) resolves
/// every `#[injectable]` component discovered in the linked binary,
/// constructing each exactly once, in dependency order.
#[derive(Default)]
pub struct ContainerBuilder {
    container: Container,
}

impl ContainerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provide<T: Any + Send + Sync + 'static>(self, value: T) -> Self {
        self.container.insert(Arc::new(value));
        self
    }

    /// Like [`provide`](Self::provide), for a value that's already behind
    /// an `Arc` (e.g. a `SessionDb::init()` that hands one back directly,
    /// rather than a bare `T` this could wrap itself). Storing it via
    /// `provide(arc)` instead would key it on `TypeId::of::<Arc<T>>()`, not
    /// `TypeId::of::<T>()` - an `Inject<T>`/`#[injectable]` dependency on
    /// `Arc<T>` would then find nothing.
    pub fn provide_arc<T: Any + Send + Sync + 'static>(self, value: Arc<T>) -> Self {
        self.container.insert(value);
        self
    }

    pub async fn build(self) -> Result<Container, DiError> {
        let factories: Vec<&'static ComponentFactory> =
            inventory::iter::<ComponentFactory>().collect();
        let by_id: HashMap<TypeId, &'static ComponentFactory> =
            factories.iter().map(|f| ((f.type_id)(), *f)).collect();

        let mut in_degree: HashMap<TypeId, usize> = HashMap::new();
        let mut dependents: HashMap<TypeId, Vec<TypeId>> = HashMap::new();

        for factory in &factories {
            let id = (factory.type_id)();
            let mut degree = 0usize;
            for (dep_id, dep_name) in (factory.dependencies)() {
                if self.container.contains(dep_id) {
                    continue;
                }
                if !by_id.contains_key(&dep_id) {
                    return Err(DiError::MissingDependency {
                        component: (factory.type_name)(),
                        dependency: dep_name,
                    });
                }
                degree += 1;
                dependents.entry(dep_id).or_default().push(id);
            }
            in_degree.insert(id, degree);
        }

        let mut queue: VecDeque<TypeId> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| *id)
            .collect();

        let container = self.container;
        let mut constructed = 0usize;

        while let Some(id) = queue.pop_front() {
            let factory = by_id[&id];
            let value = (factory.construct)(&container).await?;
            container.insert_dyn(id, value);
            constructed += 1;

            if let Some(waiting) = dependents.get(&id) {
                for waiter in waiting {
                    let degree = in_degree
                        .get_mut(waiter)
                        .expect("waiter was registered above");
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(*waiter);
                    }
                }
            }
        }

        if constructed != factories.len() {
            let stuck: Vec<&'static str> = in_degree
                .into_iter()
                .filter(|(_, degree)| *degree > 0)
                .map(|(id, _)| (by_id[&id].type_name)())
                .collect();
            return Err(DiError::Cycle(stuck));
        }

        Ok(container)
    }
}
