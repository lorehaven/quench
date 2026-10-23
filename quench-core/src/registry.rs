//! Registrations collected before `main` - what the `inventory` crate did.
//!
//! A [`Registry`] is a `static` that code anywhere in the final binary adds
//! items to with [`submit!`](crate::submit). The items are added by a
//! constructor the linker places in the platform's init-array section, so
//! they are in place before `main` runs and no registration list has to be
//! kept in sync by hand.
//!
//! The usual caveat applies: a crate whose object code the linker never pulls
//! in (nothing in the binary references it) contributes no registrations.
//! Iteration is newest first, i.e. reverse of the order constructors ran,
//! which is the order `inventory` used.

use std::sync::{Mutex, PoisonError};

/// A set of `'static` items that grows as constructors run.
#[derive(Debug)]
pub struct Registry<T: 'static> {
    items: Mutex<Vec<&'static T>>,
}

impl<T: 'static> Registry<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
        }
    }

    /// Adds an item. Called by [`submit!`](crate::submit), not usually by hand.
    pub fn register(&self, item: &'static T) {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(item);
    }

    /// A snapshot of the items registered so far, newest first.
    #[must_use]
    pub fn iter(&self) -> std::vec::IntoIter<&'static T> {
        let mut snapshot = self
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        snapshot.reverse();
        snapshot.into_iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: 'static> IntoIterator for &Registry<T> {
    type Item = &'static T;
    type IntoIter = std::vec::IntoIter<&'static T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: 'static> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers `$value` (of type `$ty`, evaluated in a `static`) in `$registry` before `main`.
///
/// ```ignore
/// static HANDLERS: quench_core::registry::Registry<Handler> = quench_core::registry::Registry::new();
/// quench_core::submit!(HANDLERS, Handler, Handler { name: "ping" });
/// ```
#[macro_export]
macro_rules! submit {
    ($registry:expr, $ty:ty, $value:expr) => {
        const _: () = {
            static ITEM: $ty = $value;

            extern "C" fn register() {
                $registry.register(&ITEM);
            }

            #[used]
            #[cfg_attr(
                any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "dragonfly",
                    target_os = "illumos",
                    target_os = "solaris"
                ),
                unsafe(link_section = ".init_array")
            )]
            #[cfg_attr(
                any(
                    target_os = "macos",
                    target_os = "ios",
                    target_os = "tvos",
                    target_os = "watchos"
                ),
                unsafe(link_section = "__DATA,__mod_init_func")
            )]
            #[cfg_attr(target_os = "windows", unsafe(link_section = ".CRT$XCU"))]
            static CONSTRUCTOR: extern "C" fn() = register;
        };
    };
}
