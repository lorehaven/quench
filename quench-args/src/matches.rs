//! What was found on the command line.

use std::collections::{HashMap, HashSet};

/// The values of one command's arguments, and the subcommand chosen (with its own matches).
#[derive(Debug, Clone, Default)]
pub struct Matches {
    pub(crate) values: HashMap<String, Vec<String>>,
    /// Arguments the user typed, as opposed to ones that only have their default.
    pub(crate) given: HashSet<String>,
    pub(crate) flags: HashSet<String>,
    pub(crate) sub: Option<(String, Box<Self>)>,
}

impl Matches {
    /// Whether the switch `id` was given.
    #[must_use]
    pub fn flag(&self, id: &str) -> bool {
        self.flags.contains(id)
    }

    /// Whether the user typed the argument `id` (a default does not count).
    #[must_use]
    pub fn is_present(&self, id: &str) -> bool {
        self.given.contains(id)
    }

    /// The value of `id`; when it was repeated, the last one.
    #[must_use]
    pub fn get_one(&self, id: &str) -> Option<&str> {
        self.values
            .get(id)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    /// Every value of `id`, in the order given (empty when there are none).
    #[must_use]
    pub fn get_many(&self, id: &str) -> &[String] {
        self.values.get(id).map_or(&[], Vec::as_slice)
    }

    /// The subcommand chosen and what it matched.
    #[must_use]
    pub fn subcommand(&self) -> Option<(&str, &Self)> {
        self.sub.as_ref().map(|(name, m)| (name.as_str(), &**m))
    }
}

impl Matches {
    /// The value of a required argument.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::MissingRequired`](crate::ErrorKind) when it is absent (the parser normally catches this
    /// first).
    pub fn get_required(&self, id: &str) -> Result<&str, crate::Error> {
        self.get_one(id).ok_or_else(|| {
            crate::Error::new(
                crate::ErrorKind::MissingRequired,
                format!("the argument '{id}' was not provided"),
            )
        })
    }
}
