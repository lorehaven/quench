#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::ws::decode_frame;

fuzz_target!(|data: &[u8]| {
    for (masked, cap) in [(true, 64), (true, 1 << 20), (false, 1024)] {
        if let Ok(Some((frame, used))) = decode_frame(data, masked, cap) {
            assert!(used <= data.len());
            assert!(frame.payload.len() <= cap);
        }
    }
});
