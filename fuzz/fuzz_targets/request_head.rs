#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::h1::parse::{Limits, find_head_end, parse_request_head, request_framing};

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    if let Some(end) = find_head_end(data) {
        assert!(end <= data.len());
    }
    if let Ok(head) = parse_request_head(data, &limits) {
        let clean = |b: &[u8]| !b.iter().any(|c| matches!(c, b'\r' | b'\n' | 0));
        assert!(clean(head.uri.as_str().as_bytes()));
        for (name, value) in head.headers.iter() {
            assert!(clean(name.as_str().as_bytes()) && clean(value.as_bytes()));
        }
        // Smuggling invariant: CL together with TE must never get a framing.
        if head.headers.get("content-length").is_some()
            && head.headers.get("transfer-encoding").is_some()
        {
            assert!(request_framing(head.version, &head.headers).is_err());
        }
    }
});
