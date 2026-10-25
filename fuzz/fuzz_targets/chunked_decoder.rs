#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_proto::h1::Framing;
use quench_proto::h1::decode::BodyDecoder;

fn run(input: &[u8], step: usize) -> Result<(Vec<u8>, usize, bool), ()> {
    let mut decoder = BodyDecoder::new(Framing::Chunked);
    let (mut out, mut offset) = (Vec::new(), 0);
    while offset < input.len() && !decoder.is_done() {
        let end = (offset + step).min(input.len());
        let used = decoder
            .feed(&input[offset..end], &mut out)
            .map_err(|_| ())?;
        assert!(used <= end - offset);
        offset += used;
    }
    Ok((out, offset, decoder.is_done()))
}

// A split-dependent decoder is a request-smuggling primitive: the result must not depend on how
// the bytes arrived.
fuzz_target!(|data: &[u8]| {
    let whole = run(data, usize::MAX);
    assert_eq!(whole, run(data, 1));
    assert_eq!(whole, run(data, 7));
});
