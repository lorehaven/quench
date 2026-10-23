//! The logging macros: field syntax, messages, targets, levels and spans.

use quench_log::{
    Dispatcher, FieldValue, Filter, Instrument, Level, MemorySink, Record, Span, with_default,
};

fn capture(level: Level, f: impl FnOnce()) -> Vec<Record> {
    let sink = MemorySink::new();
    with_default(
        Dispatcher::new(Filter::level(level)).with_sink(sink.clone()),
        f,
    );
    sink.records()
}

fn value<'a>(record: &'a Record, name: &str) -> &'a FieldValue {
    &record
        .fields
        .iter()
        .find(|(k, _)| *k == name)
        .unwrap_or_else(|| panic!("no field {name}: {record:?}"))
        .1
}

#[test]
fn every_level_macro_logs_at_its_level() {
    let records = capture(Level::Trace, || {
        quench_log::trace!("t");
        quench_log::debug!("d");
        quench_log::info!("i");
        quench_log::warn!("w");
        quench_log::error!("e");
    });
    let seen: Vec<_> = records
        .iter()
        .map(|r| (r.level, r.message.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (Level::Trace, "t"),
            (Level::Debug, "d"),
            (Level::Info, "i"),
            (Level::Warn, "w"),
            (Level::Error, "e")
        ]
    );
}

#[test]
fn disabled_levels_do_not_evaluate_their_arguments() {
    let mut evaluated = false;
    let records = capture(Level::Warn, || {
        quench_log::info!("{}", {
            evaluated = true;
            "never"
        });
    });
    assert!(records.is_empty() && !evaluated);
}

#[test]
fn messages_are_formatted() {
    let name = "ada";
    let records = capture(Level::Info, || {
        quench_log::info!("plain");
        quench_log::info!("hello {name}, you are {}", 36);
        quench_log::info!("trailing comma {}", 1,);
    });
    let messages: Vec<_> = records.iter().map(|r| r.message.as_str()).collect();
    assert_eq!(
        messages,
        ["plain", "hello ada, you are 36", "trailing comma 1"]
    );
    assert!(records[0].fields.is_empty());
}

#[test]
fn fields_come_in_every_form() {
    let who = "ada";
    let err = std::io::Error::other("disk");
    let records = capture(Level::Info, || {
        quench_log::info!(
            count = 42,
            ok = true,
            ratio = 0.5,
            name = "x",
            "with message"
        );
        quench_log::info!(shown = %err, dbg = ?Some(1), who = %who);
        quench_log::info!(who, "bare name is a field");
    });
    // an untyped integer literal is an `i32`
    assert_eq!(value(&records[0], "count"), &FieldValue::I64(42));
    assert_eq!(value(&records[0], "ok"), &FieldValue::Bool(true));
    assert_eq!(value(&records[0], "ratio"), &FieldValue::F64(0.5));
    assert_eq!(value(&records[0], "name"), &FieldValue::Str("x".into()));
    assert_eq!(records[0].message, "with message");
    assert_eq!(
        value(&records[1], "shown"),
        &FieldValue::Text("disk".into())
    );
    assert_eq!(
        value(&records[1], "dbg"),
        &FieldValue::Text("Some(1)".into())
    );
    assert_eq!(value(&records[1], "who"), &FieldValue::Text("ada".into()));
    assert_eq!(records[1].message, "");
    // a bare `name,` is the variable's field, and the rest is the message
    assert_eq!(value(&records[2], "who"), &FieldValue::Str("ada".into()));
    assert_eq!(records[2].message, "bare name is a field");
}

#[test]
fn shorthand_fields_record_the_variable_by_name() {
    let id = 7_u32;
    let path = String::from("/tmp/x");
    let records = capture(Level::Info, || {
        quench_log::info!(%path, ?id, "shorthand");
        quench_log::info!(?id);
        quench_log::info!(%path);
    });
    assert_eq!(
        value(&records[0], "path"),
        &FieldValue::Text("/tmp/x".into())
    );
    assert_eq!(value(&records[0], "id"), &FieldValue::Text("7".into()));
    assert_eq!(records[1].fields.len(), 1);
    assert_eq!(records[2].fields.len(), 1);
}

#[test]
fn last_field_without_a_trailing_comma() {
    let records = capture(Level::Info, || {
        quench_log::info!(a = 1, b = %"two");
        quench_log::info!(a = 1, b = ?"two");
        quench_log::info!(a = 1, b = 2);
    });
    assert_eq!(records.len(), 3);
    assert_eq!(value(&records[0], "b"), &FieldValue::Text("two".into()));
    assert_eq!(value(&records[1], "b"), &FieldValue::Text("\"two\"".into()));
    assert_eq!(value(&records[2], "b"), &FieldValue::I64(2));
}

#[test]
fn optional_fields_vanish_when_none() {
    let records = capture(Level::Info, || {
        let missing: Option<u8> = None;
        quench_log::info!(present = Some(1_u8), missing = missing, "opt");
    });
    assert_eq!(value(&records[0], "present"), &FieldValue::U64(1));
    assert_eq!(value(&records[0], "missing"), &FieldValue::Empty);
}

#[test]
fn the_target_is_the_module_path_unless_given() {
    let records = capture(Level::Info, || {
        quench_log::info!("default target");
        quench_log::event!(target: "audit", Level::Warn, user = "ada", "custom target");
    });
    assert!(
        records[0].target.ends_with("macros_tests"),
        "{}",
        records[0].target
    );
    assert_eq!(records[1].target, "audit");
    assert_eq!(records[1].level, Level::Warn);
}

#[test]
fn event_takes_a_runtime_level() {
    let records = capture(Level::Debug, || {
        for level in [Level::Debug, Level::Error] {
            quench_log::event!(level, "dynamic");
        }
    });
    assert_eq!(
        records.iter().map(|r| r.level).collect::<Vec<_>>(),
        [Level::Debug, Level::Error]
    );
}

#[test]
fn per_target_filters_apply() {
    let sink = MemorySink::new();
    let dispatcher =
        Dispatcher::new(Filter::parse("error,audit=trace").unwrap()).with_sink(sink.clone());
    with_default(dispatcher, || {
        quench_log::info!("dropped");
        quench_log::event!(target: "audit", Level::Trace, "kept");
    });
    assert_eq!(sink.records().len(), 1);
}

#[test]
fn spans_attach_their_fields_to_events_inside() {
    let records = capture(Level::Trace, || {
        let outer = quench_log::info_span!("request", id = 7, path = %"/x");
        let _outer = outer.enter();
        quench_log::info!("in outer");
        {
            let inner = quench_log::debug_span!("db", table = "users");
            let _inner = inner.enter();
            quench_log::info!("in inner");
        }
        quench_log::info!("back in outer");
    });
    let names = |r: &Record| r.spans.iter().map(|s| s.name).collect::<Vec<_>>();
    assert_eq!(names(&records[0]), ["request"]);
    assert_eq!(names(&records[1]), ["request", "db"]);
    assert_eq!(names(&records[2]), ["request"]);
    assert_eq!(
        records[0].spans[0].fields[0],
        ("id".to_string(), FieldValue::I64(7))
    );
    assert_eq!(
        records[0].spans[0].fields[1],
        ("path".to_string(), FieldValue::Text("/x".into()))
    );
    assert_eq!(
        records[1].spans[1].fields[0].1,
        FieldValue::Str("users".into())
    );
}

#[test]
fn spans_can_be_recorded_into_later() {
    let records = capture(Level::Info, || {
        let span = quench_log::info_span!("request", correlation_id = quench_log::field::Empty);
        let _entered = span.enter();
        quench_log::info!("before");
        Span::current().record("correlation_id", "abc-123");
        quench_log::info!("after");
        span.record("extra", 5_u8);
        span.record_display("shown", &"text");
        quench_log::info!("end");
    });
    assert!(
        records[0].spans[0].fields.is_empty(),
        "an unrecorded field is not shown"
    );
    assert_eq!(
        records[1].spans[0].fields[0],
        (
            "correlation_id".to_string(),
            FieldValue::Str("abc-123".into())
        )
    );
    let last = &records[2].spans[0].fields;
    assert_eq!(last.len(), 3);
    assert_eq!(last[1], ("extra".to_string(), FieldValue::U64(5)));
    assert_eq!(
        last[2],
        ("shown".to_string(), FieldValue::Text("text".into()))
    );
}

#[test]
fn disabled_spans_are_inert() {
    let records = capture(Level::Warn, || {
        let span = quench_log::debug_span!("quiet", id = 1);
        assert!(span.is_none());
        let _entered = span.enter();
        span.record("x", 1_u8);
        assert!(Span::current().is_none());
        quench_log::warn!("still logs");
    });
    assert!(records[0].spans.is_empty());
    assert!(Span::none().is_none() && Span::default().is_none());
}

#[test]
fn span_targets_and_levels() {
    let records = capture(Level::Trace, || {
        let a = quench_log::span!(target: "custom", Level::Warn, "named", k = 1);
        let _a = a.enter();
        let (b, c, d) = (
            quench_log::trace_span!("t"),
            quench_log::warn_span!("w"),
            quench_log::error_span!("e", flag = true, text = ?"x"),
        );
        let (_b, _c, _d) = (b.enter(), c.enter(), d.enter());
        quench_log::info!("deep");
    });
    let names: Vec<_> = records[0].spans.iter().map(|s| s.name).collect();
    assert_eq!(names, ["named", "t", "w", "e"]);
}

#[test]
fn in_scope_runs_inside_the_span() {
    let records = capture(Level::Info, || {
        let span = quench_log::info_span!("scoped");
        let answer = span.in_scope(|| {
            quench_log::info!("inside");
            42
        });
        assert_eq!(answer, 42);
        quench_log::info!("outside");
    });
    assert_eq!(records[0].spans.len(), 1);
    assert!(records[1].spans.is_empty());
}

#[test]
fn instrumented_futures_enter_the_span_on_every_poll() {
    let records = capture(Level::Info, || {
        quench_core::future::block_on(
            async {
                quench_log::info!("first");
                std::future::ready(()).await;
                quench_log::info!("second");
            }
            .instrument(quench_log::info_span!("task", n = 1)),
        );
        quench_core::future::block_on(async { quench_log::info!("no span") });
    });
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].spans[0].name, "task");
    assert_eq!(records[1].spans[0].name, "task");
    assert!(records[2].spans.is_empty());
}

#[test]
fn in_current_span_carries_the_span_into_a_future() {
    let records = capture(Level::Info, || {
        let outer = quench_log::info_span!("outer");
        let entered = outer.enter();
        let future = async { quench_log::info!("from the future") }.in_current_span();
        drop(entered);
        quench_core::future::block_on(future);
    });
    assert_eq!(records[0].spans[0].name, "outer");
    assert!(format!("{:?}", async {}.instrument(Span::none())).contains("Instrumented"));
}

#[test]
fn level_macros_and_spans_take_an_explicit_target() {
    let records = capture(Level::Trace, || {
        quench_log::trace!(target: "audit", "t");
        quench_log::debug!(target: "audit", n = 1, "d");
        quench_log::info!(target: "audit", user = %"ann", "i");
        quench_log::warn!(target: "audit", "w");
        quench_log::error!(target: "audit", "e");
        let span = quench_log::info_span!(target: "audit", "step", id = 7);
        span.in_scope(|| quench_log::info!("inside"));
        quench_log::trace_span!(target: "audit", "a");
        quench_log::debug_span!(target: "audit", "b");
        quench_log::warn_span!(target: "audit", "c");
        quench_log::error_span!(target: "audit", "d");
    });
    let audit: Vec<_> = records
        .iter()
        .filter(|r| r.target == "audit")
        .map(|r| r.message.as_str())
        .collect();
    assert_eq!(audit, ["t", "d", "i", "w", "e"]);
    assert_eq!(value(&records[2], "user"), &FieldValue::Text("ann".into()));
}
