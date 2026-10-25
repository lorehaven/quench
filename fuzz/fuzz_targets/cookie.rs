#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::cookie::{Cookie, cookie_value, parse_cookie_header};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = Cookie::parse(text);
    let _ = parse_cookie_header(text).count();
    let _ = cookie_value(text, "session");
});
