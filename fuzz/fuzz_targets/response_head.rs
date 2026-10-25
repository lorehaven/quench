#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::h1::parse::{Limits, parse_response_head};

fuzz_target!(|data: &[u8]| {
    let _ = parse_response_head(data, &Limits::default());
});
