//! How a [`Record`] becomes a line: readable text, or one JSON object.

use std::fmt::Write as _;

use quench_core::time::Time;
use quench_serde::{Map, Value, json};

use crate::field::FieldValue;
use crate::level::Level;
use crate::record::Record;

/// `2026-10-23T12:00:00.123456Z`: microsecond precision, always UTC.
#[must_use]
pub fn timestamp(time: Time) -> String {
    let c = time.civil();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        c.second,
        c.nanosecond / 1000
    )
}

const RESET: &str = "\x1b[0m";

const fn color(level: Level) -> &'static str {
    match level {
        Level::Trace => "\x1b[35m",
        Level::Debug => "\x1b[34m",
        Level::Info => "\x1b[32m",
        Level::Warn => "\x1b[33m",
        Level::Error => "\x1b[31m",
    }
}

/// `TIMESTAMP  LEVEL span{k=v}: target: message k=v`, newline-terminated.
#[must_use]
pub fn text(record: &Record, ansi: bool) -> String {
    let mut out = String::new();
    let level = if ansi {
        format!("{}{:>5}{RESET}", color(record.level), record.level)
    } else {
        format!("{:>5}", record.level)
    };
    let _ = write!(out, "{} {level} ", timestamp(record.time));
    for span in &record.spans {
        out.push_str(span.name);
        if !span.fields.is_empty() {
            out.push('{');
            for (i, (name, value)) in span.fields.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let _ = write!(out, "{name}={value}");
            }
            out.push('}');
        }
        out.push(':');
        out.push(' ');
    }
    let _ = write!(out, "{}:", record.target);
    if !record.message.is_empty() {
        let _ = write!(out, " {}", record.message);
    }
    for (name, value) in record.fields.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = write!(out, " {name}={value}");
    }
    out.push('\n');
    out
}

fn json_value(value: &FieldValue) -> Value {
    match value {
        FieldValue::Bool(b) => Value::Bool(*b),
        FieldValue::I64(n) => Value::from_i64(*n),
        FieldValue::U64(n) => Value::UInt(*n),
        FieldValue::F64(n) => Value::from_f64(*n),
        FieldValue::Str(s) | FieldValue::Text(s) => Value::String(s.clone()),
        FieldValue::Empty => Value::Null,
    }
}

/// One JSON object per line:
/// `{"timestamp":..,"level":..,"fields":{"message":..,..},"target":..,"span":{..},"spans":[..]}`.
#[must_use]
pub fn json(record: &Record) -> String {
    let mut fields = Map::new();
    if !record.message.is_empty() {
        fields.insert("message", Value::from(record.message.as_str()));
    }
    for (name, value) in record.fields.iter().filter(|(_, v)| !v.is_empty()) {
        fields.insert(*name, json_value(value));
    }

    let mut object = Map::new();
    object.insert("timestamp", Value::from(timestamp(record.time)));
    object.insert("level", Value::from(record.level.as_str()));
    object.insert("fields", Value::Object(fields));
    object.insert("target", Value::from(record.target.as_str()));
    if !record.spans.is_empty() {
        let spans: Vec<Value> = record
            .spans
            .iter()
            .map(|span| {
                let mut map = Map::new();
                for (name, value) in &span.fields {
                    map.insert(name.as_str(), json_value(value));
                }
                map.insert("name", Value::from(span.name));
                Value::Object(map)
            })
            .collect();
        if let Some(innermost) = spans.last() {
            object.insert("span", innermost.clone());
        }
        object.insert("spans", Value::Array(spans));
    }
    let mut line = json::to_string(&Value::Object(object));
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::SpanSnapshot;

    fn record() -> Record {
        Record {
            time: Time::from_unix_nanos(1_790_683_200, 123_456_000),
            level: Level::Info,
            target: "svc::http".into(),
            message: "hello".into(),
            fields: vec![
                ("count", FieldValue::U64(42)),
                ("who", FieldValue::Str("ada".into())),
                ("skipped", FieldValue::Empty),
                ("note", FieldValue::Text("a b".into())),
            ],
            spans: vec![
                SpanSnapshot {
                    name: "request",
                    fields: vec![("id".into(), FieldValue::U64(7))],
                },
                SpanSnapshot {
                    name: "inner",
                    fields: vec![],
                },
            ],
        }
    }

    #[test]
    fn timestamps_have_microseconds() {
        assert_eq!(
            timestamp(Time::from_unix_nanos(1_790_683_200, 123_456_789)),
            "2026-09-29T12:00:00.123456Z"
        );
        assert_eq!(timestamp(Time::UNIX_EPOCH), "1970-01-01T00:00:00.000000Z");
    }

    #[test]
    fn text_line() {
        assert_eq!(
            text(&record(), false),
            "2026-09-29T12:00:00.123456Z  INFO request{id=7}: inner: svc::http: hello count=42 who=\"ada\" note=a b\n"
        );
        let bare = Record {
            spans: vec![],
            fields: vec![],
            message: String::new(),
            ..record()
        };
        assert_eq!(
            text(&bare, false),
            "2026-09-29T12:00:00.123456Z  INFO svc::http:\n"
        );
    }

    #[test]
    fn ansi_colours_only_the_level() {
        for (level, code) in [
            (Level::Trace, "35"),
            (Level::Debug, "34"),
            (Level::Info, "32"),
            (Level::Warn, "33"),
            (Level::Error, "31"),
        ] {
            let line = text(&Record { level, ..record() }, true);
            assert!(
                line.contains(&format!("\x1b[{code}m")) && line.contains("\x1b[0m"),
                "{level}: {line:?}"
            );
        }
        assert!(!text(&record(), false).contains('\x1b'));
    }

    #[test]
    fn json_line() {
        let line = json(&record());
        assert!(line.ends_with("}\n"));
        let value = json::parse(line.trim_end()).unwrap();
        assert_eq!(value["timestamp"], "2026-09-29T12:00:00.123456Z");
        assert_eq!(value["level"], "INFO");
        assert_eq!(value["fields"]["message"], "hello");
        assert_eq!(value["fields"]["count"], 42);
        assert_eq!(value["fields"]["who"], "ada");
        assert_eq!(value["fields"]["note"], "a b");
        assert!(value["fields"].get("skipped").is_none());
        assert_eq!(value["target"], "svc::http");
        assert_eq!(value["span"]["name"], "inner");
        assert_eq!(value["spans"][0]["id"], 7);
        assert_eq!(value["spans"][0]["name"], "request");
        // the key order matches tracing-subscriber's json layer
        let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["timestamp", "level", "fields", "target", "span", "spans"]
        );
    }

    #[test]
    fn json_without_spans_or_message() {
        let bare = Record {
            spans: vec![],
            message: String::new(),
            fields: vec![
                ("ok", FieldValue::Bool(true)),
                ("n", FieldValue::I64(-2)),
                ("f", FieldValue::F64(0.5)),
            ],
            ..record()
        };
        let value = json::parse(json(&bare).trim_end()).unwrap();
        assert!(value.get("span").is_none() && value.get("spans").is_none());
        assert_eq!(value["fields"]["ok"], true);
        assert_eq!(value["fields"]["n"], -2);
        assert_eq!(value["fields"]["f"], 0.5);
        assert!(value["fields"].get("message").is_none());
    }

    #[test]
    fn json_escapes_awkward_text() {
        let tricky = Record {
            message: "quote \" newline \n tab \t".into(),
            ..record()
        };
        let line = json(&tricky);
        assert_eq!(line.matches('\n').count(), 1, "one physical line: {line:?}");
        assert_eq!(
            json::parse(line.trim_end()).unwrap()["fields"]["message"],
            "quote \" newline \n tab \t"
        );
    }
}
