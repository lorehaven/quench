//! What a sink receives for each event.

use quench_core::time::Time;

use crate::field::FieldValue;
use crate::level::Level;

/// A span as it was when the event happened.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanSnapshot {
    pub name: &'static str,
    pub fields: Vec<(String, FieldValue)>,
}

/// One log event.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub time: Time,
    pub level: Level,
    /// The module path the event was logged from.
    pub target: String,
    pub message: String,
    pub fields: Vec<(&'static str, FieldValue)>,
    /// Enclosing spans, outermost first.
    pub spans: Vec<SpanSnapshot>,
}
