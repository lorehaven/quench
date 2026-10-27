//! Fields, rules and forms, mounted into a `TestRenderer`.

use quench_ui::form::{Field, Form, rules};
use quench_ui::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

fn setup() -> (Rc<TestRenderer>, quench_ui::test_renderer::NodeId) {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    (r, root)
}

#[quench_test::test]
fn rules_report_the_first_failure_and_empty_values_pass_the_optional_ones() {
    let (_, _scope) = create_root(|| {
        let f = Field::new("name", "")
            .rule(rules::required("required"))
            .rule(rules::min_len(3, "short"))
            .rule(rules::max_len(5, "long"))
            .rule(rules::check(
                |v| v.chars().all(char::is_alphabetic),
                "letters",
            ));
        assert!(!f.validate());
        assert_eq!(f.error().as_deref(), Some("required"));
        for (value, error) in [
            ("  ", Some("required")),
            ("ab", Some("short")),
            ("abcdef", Some("long")),
            ("ab1", Some("letters")),
            ("abc", None),
            ("żółć", None),
        ] {
            f.set(value);
            assert_eq!(f.validate(), error.is_none(), "{value}");
            assert_eq!(f.error().as_deref(), error, "{value}");
        }
        assert_eq!(f.name(), "name");
        assert_eq!(f.value_untracked(), "żółć");
        assert!(format!("{f:?}").contains("name"));
        let optional = Field::new("o", "")
            .rule(rules::min_len(3, "s"))
            .rule(rules::check(|_| false, "c"));
        assert!(
            optional.validate(),
            "empty passes rules that are not `required`"
        );
    });
}

#[quench_test::test]
fn typing_edits_the_value_and_an_error_appears_once_the_field_was_left() {
    let (r, root) = setup();
    let email = Field::new("email", "").rule(rules::required("Enter an address"));
    let view = div().child(email.input()).child(email.error_view());
    let m = mount(&r, &root, view);
    let input = r.find_by_tag("input")[0];
    let error = r.find_by_tag("p")[0];
    assert_eq!(r.attr(input, "id").as_deref(), Some("email"));
    assert_eq!(r.attr(input, "name").as_deref(), Some("email"));
    assert_eq!(r.attr(input, "aria-invalid"), None);
    assert_eq!(r.attr(error, "role").as_deref(), Some("alert"));

    r.fire(input, &Event::Input("o".into()));
    assert_eq!(email.value_untracked(), "o");
    assert_eq!(
        r.text_of(error),
        "",
        "no error before the user has left the field"
    );
    r.fire(input, &Event::Input(String::new()));
    r.fire(input, &Event::Blur);
    assert_eq!(r.text_of(error), "Enter an address");
    assert_eq!(r.attr(input, "aria-invalid").as_deref(), Some("true"));
    assert_eq!(
        r.attr(input, "aria-describedby").as_deref(),
        Some("email-error")
    );
    r.fire(input, &Event::Input("ola".into()));
    assert_eq!(
        r.text_of(error),
        "",
        "after the first visit every change is checked"
    );
    assert_eq!(r.attr(input, "aria-invalid"), None);
    assert_eq!(r.attr(input, "value").as_deref(), Some("ola"));
    m.unmount();
}

#[quench_test::test]
fn nothing_is_taken_from_the_input_while_an_input_method_composes() {
    let (r, root) = setup();
    let f = Field::new("q", "");
    let m = mount(&r, &root, f.input());
    let input = r.find_by_tag("input")[0];
    r.fire(input, &Event::CompositionStart(String::new()));
    for partial in ["n", "ni", "ni\u{2019}"] {
        r.fire(input, &Event::Input(partial.into()));
    }
    assert_eq!(
        f.value_untracked(),
        "",
        "the half-typed text is not the value"
    );
    r.fire(input, &Event::CompositionEnd("\u{4f60}".into()));
    assert_eq!(f.value_untracked(), "\u{4f60}");
    r.fire(input, &Event::Input("\u{4f60}\u{597d}".into()));
    assert_eq!(
        f.value_untracked(),
        "\u{4f60}\u{597d}",
        "typing counts again once it is over"
    );
    m.unmount();
}

#[quench_test::test]
fn submitting_checks_every_field_and_runs_the_handler_only_when_all_pass() {
    let (r, root) = setup();
    let name = Field::new("name", "").rule(rules::required("name?"));
    let age = Field::new("age", "x").rule(rules::check(|v| v.parse::<u8>().is_ok(), "a number"));
    let form = Form::new().field(&name).field(&age);
    let sent = Rc::new(Cell::new(0));
    let s = sent.clone();
    let view = form
        .element(move |_| s.set(s.get() + 1))
        .child(name.input())
        .child(age.input())
        .child(form.error_view());
    let m = mount(&r, &root, view);
    let el = r.find_by_tag("form")[0];
    assert_eq!(r.attr(el, "novalidate").as_deref(), Some(""));
    r.fire(el, &Event::Submit);
    assert_eq!(sent.get(), 0);
    assert_eq!(
        (name.error().as_deref(), age.error().as_deref()),
        (Some("name?"), Some("a number")),
        "both show at once"
    );
    name.set("Ola");
    age.set("30");
    r.fire(el, &Event::Submit);
    assert_eq!(sent.get(), 1);
    form.set_submitting(true);
    r.fire(el, &Event::Submit);
    assert_eq!(
        sent.get(),
        1,
        "a second submit while one is under way is ignored"
    );
    assert!(form.submitting());
    m.unmount();
}

#[quench_test::test]
fn a_servers_verdict_lands_on_the_fields_it_names() {
    let (r, root) = setup();
    let text = Field::new("text", "");
    let form = Form::new().field(&text);
    let m = mount(
        &r,
        &root,
        div().child(text.error_view()).child(form.error_view()),
    );
    form.set_submitting(true);
    form.apply_errors("text: too long", &[("text".into(), "too long".into())]);
    assert_eq!(text.error().as_deref(), Some("too long"));
    assert_eq!(
        form.error(),
        None,
        "the field shows it; the form need not repeat it"
    );
    assert!(!form.submitting());
    form.apply_errors("bad", &[("nope".into(), "unknown field".into())]);
    assert_eq!(
        form.error().as_deref(),
        Some("bad"),
        "a field this form lacks falls to the form"
    );
    form.apply_errors("offline", &[]);
    assert_eq!(form.error().as_deref(), Some("offline"));
    assert_eq!(r.text(), "too longoffline");
    form.clear_errors();
    assert_eq!((text.error(), form.error()), (None, None));
    text.set_error("x");
    text.reset("again");
    assert_eq!(
        (text.value_untracked().as_str(), text.error()),
        ("again", None)
    );
    assert!(format!("{form:?} {:?}", Form::default()).contains("Form"));
    m.unmount();
}
