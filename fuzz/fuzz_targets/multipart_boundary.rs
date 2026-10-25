#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::multipart::parse_boundary;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(boundary) = parse_boundary(text) {
        assert!(!boundary.is_empty() && boundary.len() <= 70);
    }
});
