//! Forms: fields that hold a value and its error, rules that check them, and a form that checks
//! every field before it submits.
//!
//! ```
//! use quench_ui::form::{Field, Form, rules};
//!
//! let (_, _scope) = quench_ui::create_root(|| {
//!     let email = Field::new("email", "").rule(rules::required("Enter an email address"));
//!     let form = Form::new().field(&email);
//!     assert!(!form.validate());
//!     assert_eq!(email.error().as_deref(), Some("Enter an email address"));
//!     email.set("ola@example.com");
//!     assert!(form.validate());
//! });
//! ```

use crate::event::{Event, EventKind};
use crate::reactive::{Signal, batch, create_signal};
use crate::view::{Element, element, form as form_element, input as input_element};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Checks a value: `Some(message)` when it is not acceptable.
pub type Rule = Rc<dyn Fn(&str) -> Option<String>>;

/// Ready-made rules.
pub mod rules {
    use super::Rule;
    use std::rc::Rc;

    /// The value has something other than whitespace in it.
    #[must_use]
    pub fn required(message: &str) -> Rule {
        let message = message.to_string();
        Rc::new(move |v| v.trim().is_empty().then(|| message.clone()))
    }

    /// At least `min` characters (an empty value passes: combine with [`required`]).
    #[must_use]
    pub fn min_len(min: usize, message: &str) -> Rule {
        let message = message.to_string();
        Rc::new(move |v| (!v.is_empty() && v.chars().count() < min).then(|| message.clone()))
    }

    /// At most `max` characters.
    #[must_use]
    pub fn max_len(max: usize, message: &str) -> Rule {
        let message = message.to_string();
        Rc::new(move |v| (v.chars().count() > max).then(|| message.clone()))
    }

    /// A date as a date input holds it: `YYYY-MM-DD`, and a day the month has.
    #[must_use]
    pub fn date(message: &str) -> Rule {
        check(is_date, message)
    }

    /// A time as a time input holds it: `HH:MM`.
    #[must_use]
    pub fn time(message: &str) -> Rule {
        check(is_time, message)
    }

    fn is_date(v: &str) -> bool {
        let parts: Vec<&str> = v.split('-').collect();
        let [year, month, day] = parts[..] else {
            return false;
        };
        let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
        if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
            return false;
        }
        let (y, m, d): (u32, u32, u32) = (
            year.parse().unwrap_or(0),
            month.parse().unwrap_or(0),
            day.parse().unwrap_or(0),
        );
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let days = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return false,
        };
        (1..=days).contains(&d)
    }

    fn is_time(v: &str) -> bool {
        let Some((h, m)) = v.split_once(':') else {
            return false;
        };
        let two = |s: &str, max: u32| s.len() == 2 && s.parse::<u32>().is_ok_and(|n| n <= max);
        two(h, 23) && two(m, 59)
    }

    /// Whatever `ok` accepts (an empty value passes: combine with [`required`]).
    #[must_use]
    pub fn check(ok: impl Fn(&str) -> bool + 'static, message: &str) -> Rule {
        let message = message.to_string();
        Rc::new(move |v| (!v.is_empty() && !ok(v)).then(|| message.clone()))
    }
}

/// One input's value, its error and whether the user has been through it. Cheap to clone; clones
/// share the state.
#[derive(Clone)]
pub struct Field {
    name: Rc<str>,
    value: Signal<String>,
    error: Signal<Option<String>>,
    touched: Signal<bool>,
    rules: Rc<RefCell<Vec<Rule>>>,
    composing: Rc<Cell<bool>>,
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Field {
    /// A field called `name` (its `id` and `name` attributes, and what a server's error refers to).
    #[must_use]
    pub fn new(name: &str, initial: &str) -> Self {
        Self {
            name: name.into(),
            value: create_signal(initial.to_string()),
            error: create_signal(None),
            touched: create_signal(false),
            rules: Rc::new(RefCell::new(Vec::new())),
            composing: Rc::new(Cell::new(false)),
        }
    }

    /// Adds a rule. Rules run in order; the first message wins.
    #[must_use]
    pub fn rule(self, rule: Rule) -> Self {
        self.rules.borrow_mut().push(rule);
        self
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value; reading it in a computation follows changes.
    #[must_use]
    pub fn value(&self) -> String {
        self.value.get()
    }

    #[must_use]
    pub fn value_untracked(&self) -> String {
        self.value.get_untracked()
    }

    /// Changes the value. Once the user has been through the field the new value is checked at once.
    pub fn set(&self, value: &str) {
        batch(|| {
            self.value.set(value.to_string());
            if self.touched.get_untracked() {
                let _ = self.validate();
            } else {
                self.error.set(None);
            }
        });
    }

    /// The current error, if there is one.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }

    /// Shows `message` as this field's error (a server's verdict on it).
    pub fn set_error(&self, message: &str) {
        self.error.set(Some(message.to_string()));
    }

    /// Checks the value against the rules and shows the first error. True when there is none.
    #[must_use]
    pub fn validate(&self) -> bool {
        let value = self.value.get_untracked();
        let error = self.rules.borrow().iter().find_map(|rule| rule(&value));
        let ok = error.is_none();
        self.error.set_if_changed(error);
        ok
    }

    /// Back to `initial`, with no error and not yet visited.
    pub fn reset(&self, initial: &str) {
        batch(|| {
            self.value.set(initial.to_string());
            self.error.set(None);
            self.touched.set(false);
        });
    }

    /// An `<input>` bound to this field: its value follows the field and typing changes it,
    /// except while an input method is composing (a half-typed syllable is not a value yet).
    /// Leaving the input checks it. It carries the `id` and `name` of the field, `aria-invalid`
    /// while there is an error, and `aria-describedby` pointing at [`Field::error_view`].
    #[must_use]
    pub fn input(&self) -> Element {
        self.bind(input_element())
    }

    /// As [`Field::input`] for any element with a `value` (a `textarea`, a `select`).
    #[must_use]
    pub fn bind(&self, control: Element) -> Element {
        let (shown, typed, start, end, left) = (
            self.clone(),
            self.clone(),
            self.composing.clone(),
            self.clone(),
            self.clone(),
        );
        let (invalid, described) = (self.clone(), self.clone());
        control
            .attr("id", &*self.name)
            .attr("name", &*self.name)
            .attr_fn("value", move || shown.value())
            .attr_opt("aria-invalid", move || {
                invalid.error().map(|_| "true".to_string())
            })
            .attr_opt("aria-describedby", move || {
                described
                    .error()
                    .map(|_| format!("{}-error", described.name))
            })
            .on_input(move |v| {
                if !typed.composing.get() {
                    typed.set(v);
                }
            })
            .on(EventKind::CompositionStart, move |_| start.set(true))
            .on(EventKind::CompositionEnd, move |e| {
                end.composing.set(false);
                if let Event::CompositionEnd(v) = e {
                    end.set(v);
                }
            })
            .on(EventKind::Blur, move |_| {
                left.touched.set(true);
                let _ = left.validate();
            })
    }

    /// The element that shows the error (empty when there is none). A screen reader announces
    /// it when it appears.
    #[must_use]
    pub fn error_view(&self) -> Element {
        let shown = self.clone();
        element("p")
            .attr("id", format!("{}-error", self.name))
            .attr("role", "alert")
            .class("q-field-error")
            .text_fn(move || shown.error().unwrap_or_default())
    }
}

/// Fields that are submitted together, and the state of the submission.
#[derive(Clone)]
pub struct Form {
    fields: Rc<RefCell<Vec<Field>>>,
    error: Signal<Option<String>>,
    submitting: Signal<bool>,
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form").finish_non_exhaustive()
    }
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: Rc::new(RefCell::new(Vec::new())),
            error: create_signal(None),
            submitting: create_signal(false),
        }
    }

    #[must_use]
    pub fn field(self, field: &Field) -> Self {
        self.fields.borrow_mut().push(field.clone());
        self
    }

    /// Checks every field (so every error shows, not only the first). True when all pass.
    #[must_use]
    pub fn validate(&self) -> bool {
        let fields = self.fields.borrow().clone();
        batch(|| {
            // Every field is checked, so every error shows; `all` would stop at the first.
            let results: Vec<bool> = fields.iter().map(Field::validate).collect();
            results.iter().all(|ok| *ok)
        })
    }

    /// The error that belongs to the form rather than a field ("wrong password", "offline").
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }

    /// Whether a submission is under way (disable the button while it is).
    #[must_use]
    pub fn submitting(&self) -> bool {
        self.submitting.get()
    }

    pub fn set_submitting(&self, on: bool) {
        self.submitting.set(on);
    }

    /// Shows a server's verdict: `fields` are `(field name, message)`, shown on those fields;
    /// `message` (and any field that is not in this form) is the form's own error.
    pub fn apply_errors(&self, message: &str, fields: &[(String, String)]) {
        let own = self.fields.borrow().clone();
        batch(|| {
            self.submitting.set(false);
            let mut unplaced = fields.is_empty();
            for (name, text) in fields {
                match own.iter().find(|f| f.name() == name) {
                    Some(field) => field.set_error(text),
                    None => unplaced = true,
                }
            }
            self.error
                .set((unplaced && !message.is_empty()).then(|| message.to_string()));
        });
    }

    /// Clears the form's error and every field's.
    pub fn clear_errors(&self) {
        let own = self.fields.borrow().clone();
        batch(|| {
            self.error.set(None);
            for field in &own {
                field.error.set(None);
            }
        });
    }

    /// A `<form>` that checks every field when submitted and calls `on_valid` only if they all
    /// pass. The browser's own validation bubbles are off: the errors are the fields'.
    #[must_use]
    pub fn element(&self, on_valid: impl Fn(&Self) + 'static) -> Element {
        let form = self.clone();
        form_element().attr("novalidate", "").on_submit(move || {
            if form.submitting.get_untracked() {
                return;
            }
            form.error.set(None);
            if form.validate() {
                on_valid(&form);
            }
        })
    }

    /// The form's own error as an alert (empty when there is none).
    #[must_use]
    pub fn error_view(&self) -> Element {
        let shown = self.clone();
        element("p")
            .attr("role", "alert")
            .class("q-form-error")
            .text_fn(move || shown.error().unwrap_or_default())
    }
}
