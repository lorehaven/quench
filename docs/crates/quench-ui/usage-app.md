---
title: Usage - app features
parent: quench-ui
nav_order: 2
---

Worked examples for the application-level modules of `quench-ui`: routing, forms and translations. The signal, view and mounting basics are in [usage](usage.md). All snippets use `use quench_ui::prelude::*;` plus the imports shown, run on the in-memory `TestRenderer` (in an app the renderer is the browser or native backend), and were compiled and run (with their assertions) against this version (0.1.3) in a scratch project. Reference: [api-router](api-router.md), [api-form](api-form.md), [api-i18n](api-i18n.md), [api-kit](api-kit.md).

## 1. Routes, parameters and navigation

Goal: show a page per URL, read `:id` parameters and the query, show a custom not-found page, and move around with `navigate` and `back`.

```rust
use quench_ui::prelude::*;
use quench_ui::router::{MemoryHistory, Router, Routes, link};
use std::rc::Rc;

let r = Rc::new(TestRenderer::new());
let history = Rc::new(MemoryHistory::new("/users/7"));
let router = Router::new(history.clone());
let m = mount(&r, &r.root(), {
    let router = router.clone();
    // Routes are built when the view is mounted, under the mount's scope.
    View::dynamic(move || {
        Routes::new()
            .route("/", |_| {
                div().child(p().text("home")).child(link("/users/7")).into_view()
            })
            .route("/users/:id", |ctx| {
                let ctx = ctx.clone();
                p().text_fn(move || format!("user {}", ctx.param("id").unwrap_or_default()))
                    .into_view()
            })
            .not_found(|ctx| {
                let ctx = ctx.clone();
                p().text_fn(move || format!("no page at {}", ctx.location().path)).into_view()
            })
            .view(&router)
    })
});
assert_eq!(r.text(), "user 7");

router.navigate("/users/8?tab=posts");
assert_eq!(r.text(), "user 8");                        // same page, patched in place
assert_eq!(router.location().query_get("tab").as_deref(), Some("posts"));

router.navigate("/nope");
assert_eq!(r.text(), "no page at /nope");
assert!(router.is_not_found());                          // a server would answer 404

assert!(router.back());                                  // history: / users/7, users/8, nope
assert_eq!(r.text(), "user 8");
assert_eq!(history.entries().len(), 3);
m.unmount();
```

Effect: `Routes::view` makes the router available to `link` and `use_router` for everything inside. Reading `ctx.param(..)` inside the `text_fn` closure subscribes to the parameter memo, so `/users/7` to `/users/8` does not rebuild the page.

Pitfalls:

* Read `ctx.param` **inside** a view closure. Read once while building the page, it is a snapshot (the page function runs once per matched route).
* Patterns: `:name` is one segment, `*name` is the rest joined with `/`; earlier routes win, so put `"/users/new"` before `"/users/:id"`. A trailing slash in the URL does not matter. Segments are percent-decoded before matching.
* `link("/path")` needs a router in scope (it panics otherwise) but only for app paths starting with a single `/`; `link("https://...")` is a plain anchor.
* `Router::navigate` to the URL you are already at replaces the entry instead of pushing, so a repeated click does not pile up history.
* Build `Routes` inside the mounted view (as above) or inside a `create_root` closure; the router's signal and the route memos need a reactive scope.

## 2. Layouts, guards and redirects

Goal: an `/app` area with a shared frame that stays mounted, a sign-in guard that redirects, and an admin page that denies.

```rust
use quench_ui::prelude::*;
use quench_ui::router::{Guard, MemoryHistory, Router, Routes, layout};
use std::rc::Rc;

let r = Rc::new(TestRenderer::new());
let signed_in = create_signal(false);
let router = Router::new(Rc::new(MemoryHistory::new("/app/settings")));
let m = mount(&r, &r.root(), {
    let (router, signed_in) = (router.clone(), signed_in.clone());
    View::dynamic(move || {
        let gate = signed_in.clone();
        Routes::new()
            .route("/login", |_| p().text("login").into_view())
            .layout(
                layout("/app", |_, inner| {
                    div().child(h1().text("App")).child(inner).into_view()
                })
                .guard(move |_| {
                    if gate.get() { Guard::Allow } else { Guard::Redirect("/login".into()) }
                })
                .route("", |_| p().text("dashboard").into_view())
                .guarded_route(
                    "admin",
                    |visit| {
                        if visit.location.query_get("root").is_some() { Guard::Allow } else { Guard::Deny }
                    },
                    |_| p().text("admin").into_view(),
                )
                .route("settings", |_| p().text("settings").into_view()),
            )
            .forbidden(|_| p().text("forbidden").into_view())
            .view(&router)
    })
});
assert_eq!(r.text(), "login");                 // not signed in: redirected
assert_eq!(router.location().path, "/login");   // the history entry was replaced

signed_in.set(true);
router.navigate("/app");
assert_eq!(r.text(), "Appdashboard");           // "" is the layout's index page
router.navigate("/app/admin");
assert_eq!(r.text(), "forbidden");              // Deny: the whole table is replaced
router.navigate("/app/admin?root=1");
assert_eq!(r.text(), "Appadmin");

signed_in.set(false);                           // the guard reads a signal ...
assert_eq!(router.location().path, "/login");  // ... so signing out redirects at once
m.unmount();
```

Effect: guards run during route resolution, outermost first (the layout's guard, then the page's). The first non-`Allow` verdict wins. `Redirect` replaces the current history entry with the target; up to 8 redirects are followed. `Deny` shows the `forbidden` page (text `Forbidden` if you set none).

Pitfalls:

* The layout's `wrap` function receives the child as a `View` and should place it exactly once. The layout is not rebuilt when navigation moves between its children (`/app/settings` to `/app/admin`).
* `Layout::guard` consumes the layout; call it before handing the layout to its parent.
* A redirect to a URL that matches nothing ends in the not-found page, after the history entry has been replaced.
* A guard is a plain closure: it must be cheap and must not navigate. Make it read the signals it depends on with `.get()` (not `get_untracked`) if it should react to them.

## 3. Back handling for hosts with a back button

Goal: on Android the hardware back button should first close an open sheet, then go back in the history, and only then leave the app.

```rust
use quench_ui::prelude::*;
use quench_ui::router::{MemoryHistory, Router};
use std::rc::Rc;

let (_, _scope) = create_root(|| {
    let router = Router::new(Rc::new(MemoryHistory::new("/")));
    let sheet_open = create_signal(true);
    router.on_back({
        let open = sheet_open.clone();
        move || {
            if open.get_untracked() { open.set(false); true } else { false }
        }
    });
    router.navigate("/a");

    assert!(router.handle_back());                        // the sheet used it up
    assert!(!sheet_open.get_untracked());
    assert_eq!(router.location().path, "/a");
    assert!(router.handle_back());                        // no handler wants it: history back
    assert_eq!(router.location().path, "/");
    assert!(!router.handle_back());                       // nothing left: leave the app
});
```

The host calls `handle_back()` when the button is pressed and finishes the activity on `false`. Handlers run newest first, so a sheet opened later is closed before one opened earlier. `on_back` is removed when the scope it was registered in is disposed, so register it inside the component that owns the sheet. `kit::sheet` and `kit::dialog` close themselves on Escape and scrim taps but do not know about the back button: pair them with `on_back` as shown.

## 4. A form with rules, blur validation and a server verdict

Goal: an email field that is required and must contain `@`, checked on blur and then on every change; a submit that runs only when valid; a server error that lands on the field.

```rust
use quench_ui::form::{Field, Form, rules};
use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

let r = Rc::new(TestRenderer::new());
let sent = Rc::new(RefCell::new(Vec::<String>::new()));
let ((view, form, email), _scope) = create_root({
    let sent = sent.clone();
    move || {
        let email = Field::new("email", "")
            .rule(rules::required("Enter an email address"))
            .rule(rules::check(|v| v.contains('@'), "Not an email address"));
        let form = Form::new().field(&email);
        let el = form
            .element({
                let (email, sent) = (email.clone(), sent.clone());
                move |f| {
                    f.set_submitting(true);                       // start of the request
                    sent.borrow_mut().push(email.value_untracked());
                }
            })
            .child(email.input().attr("type", "email"))
            .child(email.error_view())
            .child(form.error_view());
        (el, form, email)
    }
});
let m = mount(&r, &r.root(), view);
let form_node = r.find_by_tag("form")[0];
let input = r.find_by_tag("input")[0];

r.fire(form_node, &Event::Submit);                 // empty: every field is checked
assert_eq!(email.error().as_deref(), Some("Enter an email address"));

r.fire(input, &Event::Input("ola".into()));
assert_eq!(email.error(), None);                    // not visited yet: typing clears the error
r.fire(input, &Event::Blur);                        // leaving the input checks it
assert_eq!(email.error().as_deref(), Some("Not an email address"));

r.fire(input, &Event::Input("ola@example.com".into()));   // visited: every change is checked
r.fire(form_node, &Event::Submit);
assert_eq!(*sent.borrow(), ["ola@example.com"]);
assert!(form.submitting());

// The server answers "no": a message for the form and a message for the field.
form.apply_errors("Could not register", &[("email".into(), "Already registered".into())]);
assert_eq!(email.error().as_deref(), Some("Already registered"));
assert!(!form.submitting());
assert_eq!(form.error(), None);                     // the field carries it, the form need not
m.unmount();
```

Effect: `form.element(..)` is `<form novalidate>`; its submit handler ignores a second submit while `submitting()` is true, clears the form error, validates every field and calls the closure only if all passed. The input carries `id="email" name="email"`, `aria-invalid="true"` and `aria-describedby="email-error"` while the error shows; `error_view()` is `<p id="email-error" role="alert" class="q-field-error">`.

Pitfalls:

* **Submitting does not mark fields as visited.** A field that was never blurred shows its error after a failed submit, but the next keystroke clears it instead of re-checking (as the second step above shows); the check comes back on blur. Only `blur` sets the touched flag.
* Rules other than `required` pass an empty value (`min_len`, `date`, `time`, `check`): combine them with `required` when the field is mandatory.
* `apply_errors(message, fields)`: the form's own error is `message` only when `fields` is empty or some named field is not in this form; when all named fields are found the form error is cleared. It also sets `submitting` to false. It does not clear other fields' errors; call `clear_errors()` before a new submission if you need that.
* Field names are the element `id`s: they must be unique on the page, and they are the keys a server's field errors refer to.
* While an input method is composing (`compositionstart` to `compositionend`), typed text is not taken from the input; the final text is committed at `compositionend`.
* The set of components in `kit` (`text_field`, `date_field`, `time_field`) wraps `Field` in a labelled block: `text_field("Email", &email, "email", Some("name@example.com"))`.

Server functions: a failed call's error can be given to `apply_errors` by an adapter of your own. The `quench-fullstack` crate has a helper for that (`FormExt`, described in its own section of the book); this crate does not depend on it.

## 5. Translations with `I18n`

Goal: two languages, plural forms, live switching, and a fallback.

```rust
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use std::rc::Rc;

const EN: &str = "\
hello = Hello, { $name }!
files = { $n ->
    [0] No files
    [one] { $n } file
   *[other] { $n } files
}
";
const PL: &str = "\
hello = Cześć, { $name }!
files = { $n ->
    [one] { $n } plik
    [few] { $n } pliki
    [many] { $n } plików
   *[other] { $n } pliku
}
";

let r = Rc::new(TestRenderer::new());
let ((i18n, view), _scope) = create_root(|| {
    // In an app: include_str!("../i18n/en.ftl")
    let i18n = I18n::new(&[("en", EN), ("pl", PL)], "en").unwrap();   // "en" is also the fallback
    let t = i18n.clone();
    (i18n, p().text_fn(move || t.tr("files", &[("n", 5.into())])))
});
let m = mount(&r, &r.root(), view);
assert_eq!(r.text(), "5 files");

i18n.set_language("pl");                                 // everything that read it updates
assert_eq!(r.text(), "5 plików");
assert_eq!(i18n.tr("hello", &[("name", "Ola".into())]), "Cześć, Ola!");
assert_eq!(i18n.t("no-such-id"), "no-such-id");          // a missing id shows up as itself
m.unmount();
```

Plural categories chosen for `pl`: 1 `one`, 2 to 4 `few` (but not 12 to 14), 0 and 5 and above `many` (except as before), a number with a fraction `other`. For English only 1 is `one`. An exact-number variant (`[0]`) is tried before the category. The full rule table is in [api-i18n](api-i18n.md#plural_category).

Parse errors name the language and the line where the message starts:

```rust
use quench_ui::i18n::I18n;
let (_, _s) = quench_ui::create_root(|| {
    let err = I18n::new(&[("en", "a = { $n")], "en").unwrap_err();
    assert_eq!(err.to_string(), "line 1: en: expected `}`");
});
```

### The Fluent subset, at a glance

| Works | Does not (skipped or an error) |
|---|---|
| `id = text`, multi-line values (indented continuation lines), `#` comments | Attributes (`.attr = ...`): dropped |
| `{ $var }` | Terms (`-brand`, `{ -brand }`): produce nothing |
| `{ other-message }` (depth at most 8) | Functions other than `NUMBER`: produce nothing |
| `{ "literal" }`, `{ 42 }` | `NUMBER(...)` options; locale number formatting |
| `{ NUMBER($n) }` | Escapes inside literals (`A`) |
| `{ $n -> [0] .. [one] .. *[other] .. }` on numbers (exact key, then plural category) | Selectors on anything but `$var` / `NUMBER($var)`: parse error |
| `{ $s -> [text] .. *[other] .. }` on text | Plural rules for languages beyond `pl`, `fr`, `pt` and the CJK set (English rules apply) |

A selector **must** have a `*` default variant. Details and every edge case are in [api-i18n](api-i18n.md#the-supported-fluent-subset).

Pitfalls:

* `tr` reads the language signal: call it **inside** a view closure (`text_fn`, `attr_fn`) so a language switch updates the page; a string computed once at build time is frozen.
* A value that starts on the line after `=` gets a leading newline (`a =` then an indented `value` gives `"\nvalue"`); start the value on the `=` line.
* Language lookup is by the registered name: `pl-PL` finds a catalog named `pl`, but `pl` does not find one named `pl-PL`.
* Numbers print with Rust's `f64` `Display` (`1000` is `1000`, no thousands separator).

## 6. Putting it together: a screen with kit components

A short composition that uses the router, a field and the kit: an app bar, a text field, a bottom navigation tied to the route.

```rust
use quench_ui::form::Field;
use quench_ui::kit::{NavItem, app_bar, bottom_nav, text_field};
use quench_ui::prelude::*;

let (view, _scope) = create_root(|| {
    let name = Field::new("name", "");
    let tab = create_signal(0usize);
    div()
        .child(app_bar("Profile", None, vec![]))
        .child(text_field("Name", &name, "text", Some("Your name")))
        .child(bottom_nav("Main", vec![NavItem::new("Home", "H"), NavItem::new("Me", "M")], &tab))
});
let html = quench_ui::render_to_string(view);
assert!(html.contains(r#"class="q-appbar""#));
assert!(html.contains("q-bottomnav-item"));
```

The rendered markup of this screen starts `<div><header class="q-appbar"><h1 class="q-appbar-title">Profile</h1>...` and contains `<p id="name-error" role="alert" class="q-field-error q-textfield-error"></p>` for the empty error; the exact markup per component is in [api-kit](api-kit.md).
