#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::uri::Uri;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(uri) = Uri::parse(text) {
        let _ = (
            uri.scheme(),
            uri.authority(),
            uri.host(),
            uri.port_u16(),
            uri.query(),
        );
        let _ = uri.resolve("../../x");
        let _ = uri.resolve("//evil.example/");
    }
});
