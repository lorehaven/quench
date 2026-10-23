//! `#[instrument]`.

use quench_log::{
    Dispatcher, FieldValue, Filter, Level, MemorySink, Record, instrument, with_default,
};

fn capture(f: impl FnOnce()) -> Vec<Record> {
    let sink = MemorySink::new();
    with_default(
        Dispatcher::new(Filter::level(Level::Trace)).with_sink(sink.clone()),
        f,
    );
    sink.records()
}

#[instrument]
fn add(a: u32, b: u32) -> u32 {
    quench_log::info!("adding");
    a + b
}

#[instrument(skip(secret), fields(extra = a * 2), level = "debug", name = "renamed")]
fn with_options(a: u32, secret: &str) -> usize {
    quench_log::info!("inside");
    secret.len()
}

#[instrument(skip_all)]
fn nothing_recorded(_a: u32) {
    quench_log::info!("quiet");
}

struct Service {
    name: &'static str,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Service({})", self.name)
    }
}

impl Service {
    #[instrument]
    fn call(&self, n: u8) -> u8 {
        quench_log::info!("calling");
        n + 1
    }
}

#[instrument]
async fn fetch(id: u64) -> u64 {
    quench_log::info!("fetching");
    std::future::ready(()).await;
    quench_log::info!("fetched");
    id * 2
}

#[test]
fn sync_functions_run_inside_a_span_with_their_arguments() {
    let mut result = 0;
    let records = capture(|| result = add(2, 3));
    assert_eq!(result, 5);
    assert_eq!(records[0].spans[0].name, "add");
    assert_eq!(
        records[0].spans[0].fields,
        [
            ("a".to_string(), FieldValue::Text("2".into())),
            ("b".to_string(), FieldValue::Text("3".into()))
        ]
    );
    assert_eq!(records[0].spans.len(), 1);
}

#[test]
fn options_apply() {
    let mut result = 0;
    let records = capture(|| result = with_options(4, "hush"));
    assert_eq!(result, 4);
    let span = &records[0].spans[0];
    assert_eq!(span.name, "renamed");
    assert!(span.fields.iter().all(|(k, _)| k != "secret"));
    assert!(
        span.fields
            .contains(&("a".to_string(), FieldValue::Text("4".into())))
    );
    assert!(
        span.fields
            .contains(&("extra".to_string(), FieldValue::U64(8)))
    );
}

#[test]
fn the_span_level_follows_the_option() {
    let sink = MemorySink::new();
    with_default(
        Dispatcher::new(Filter::level(Level::Info)).with_sink(sink.clone()),
        || {
            with_options(1, "x");
        },
    );
    // the debug-level span is filtered out, the info event inside still arrives
    let records = sink.records();
    assert_eq!(records.len(), 1);
    assert!(records[0].spans.is_empty());
}

#[test]
fn skip_all_records_no_arguments() {
    let records = capture(|| nothing_recorded(9));
    assert_eq!(records[0].spans[0].name, "nothing_recorded");
    assert!(records[0].spans[0].fields.is_empty());
}

#[test]
fn methods_record_self() {
    let service = Service { name: "billing" };
    let mut result = 0;
    let records = capture(|| result = service.call(1));
    assert_eq!(result, 2);
    let fields = &records[0].spans[0].fields;
    assert_eq!(
        fields[0],
        (
            "self".to_string(),
            FieldValue::Text("Service(billing)".into())
        )
    );
    assert_eq!(fields[1], ("n".to_string(), FieldValue::Text("1".into())));
}

#[test]
fn async_functions_keep_the_span_across_awaits() {
    let mut result = 0;
    let records = capture(|| result = quench_core::future::block_on(fetch(21)));
    assert_eq!(result, 42);
    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(record.spans[0].name, "fetch");
        assert_eq!(
            record.spans[0].fields[0],
            ("id".to_string(), FieldValue::Text("21".into()))
        );
    }
}
