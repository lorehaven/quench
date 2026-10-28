use quench_android::{call, encode, register, serve};
use std::io::Cursor;

fn frame(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(bytes);
    out
}

fn request(op: &str, input: &[u8]) -> Vec<u8> {
    [frame(op.as_bytes()), frame(input)].concat()
}

/// Feeds `input` to `serve` and returns the answers it wrote, one per frame.
fn answers(input: &[u8]) -> Result<Vec<Vec<u8>>, std::io::Error> {
    let mut out = Vec::new();
    serve(&mut Cursor::new(input.to_vec()), &mut out)?;
    let mut list = Vec::new();
    let mut rest = &out[..];
    while !rest.is_empty() {
        let size = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        list.push(rest[4..4 + size].to_vec());
        rest = &rest[4 + size..];
    }
    Ok(list)
}

#[quench_test::test]
fn the_answer_is_a_status_byte_and_the_payload() {
    assert_eq!(encode(Ok(vec![5, 6])), [0, 5, 6]);
    assert_eq!(encode(Err("no".into())), [&[1][..], b"no"].concat());
    assert_eq!(encode(Ok(vec![])), [0]);
}

#[quench_test::test]
fn a_request_is_an_operation_and_its_input() {
    let got = answers(&request("core.echo", b"hi")).unwrap();
    assert_eq!(got, [[&[0][..], b"hi"].concat()]);
    let version = answers(&request("core.version", b"")).unwrap();
    assert_eq!(version[0][0], 0);
    assert_eq!(
        String::from_utf8_lossy(&version[0][1..]),
        env!("CARGO_PKG_VERSION")
    );
}

#[quench_test::test]
fn requests_are_answered_in_order_until_the_input_ends() {
    let input = [
        request("core.echo", b"one"),
        request("core.echo", b"two"),
        request("core.echo", b""),
    ]
    .concat();
    let got = answers(&input).unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(&got[0][1..], b"one");
    assert_eq!(&got[1][1..], b"two");
    assert_eq!(&got[2][1..], b"");
    assert!(
        answers(b"").unwrap().is_empty(),
        "no requests: nothing to say"
    );
}

#[quench_test::test]
fn an_input_larger_than_a_pipe_buffer_arrives_whole() {
    let big = vec![7_u8; 3 << 20];
    let got = answers(&request("core.echo", &big)).unwrap();
    assert_eq!(got[0].len(), big.len() + 1);
    assert!(got[0][1..].iter().all(|b| *b == 7));
}

#[quench_test::test]
fn an_unknown_operation_a_bad_name_and_a_panic_are_error_answers_and_serving_goes_on() {
    register("test.panic", |_| panic!("boom"));
    register("test.panic_text", |_| {
        panic!("{}", String::from("formatted"))
    });
    let input = [
        request("nope", b""),
        frame(&[0xff, 0xfe]).into_iter().chain(frame(b"")).collect(),
        request("test.panic", b""),
        request("test.panic_text", b""),
        request("core.echo", b"still here"),
    ]
    .concat();
    let got = answers(&input).unwrap();
    assert_eq!(got.len(), 5);
    assert!(
        got[0][0] == 1
            && String::from_utf8_lossy(&got[0][1..]).contains("no native operation `nope`")
    );
    assert!(got[1][0] == 1 && String::from_utf8_lossy(&got[1][1..]).contains("not text"));
    assert!(String::from_utf8_lossy(&got[2][1..]).contains("panicked: boom"));
    assert!(String::from_utf8_lossy(&got[3][1..]).contains("formatted"));
    assert_eq!(
        &got[4][1..],
        b"still here",
        "a failure does not end the session"
    );
}

#[quench_test::test]
fn registering_a_name_again_replaces_its_handler() {
    register("test.swap", |_| Ok(vec![1]));
    assert_eq!(call("test.swap", b"").unwrap(), [1]);
    register("test.swap", |input| {
        Ok(input.iter().rev().copied().collect())
    });
    assert_eq!(call("test.swap", &[1, 2, 3]).unwrap(), [3, 2, 1]);
    assert!(call("test.absent", b"").is_err());
}

#[quench_test::test]
fn a_request_cut_short_or_too_large_is_an_error() {
    let cut_in_the_length = answers(&[0, 0]).unwrap_err();
    assert_eq!(cut_in_the_length.kind(), std::io::ErrorKind::UnexpectedEof);
    let no_input_frame = answers(&frame(b"core.echo")).unwrap_err();
    assert!(
        no_input_frame.to_string().contains("no input frame"),
        "{no_input_frame}"
    );
    let cut_in_the_data = answers(&[&5_u32.to_be_bytes()[..], b"ab"].concat()).unwrap_err();
    assert_eq!(cut_in_the_data.kind(), std::io::ErrorKind::UnexpectedEof);
    let huge = answers(&u32::MAX.to_be_bytes()).unwrap_err();
    assert_eq!(huge.kind(), std::io::ErrorKind::InvalidData);
    assert!(huge.to_string().contains("over the limit"), "{huge}");
}

struct Failing;
impl std::io::Write for Failing {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("closed"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[quench_test::test]
fn an_output_that_cannot_be_written_ends_the_session() {
    let e = serve(&mut Cursor::new(request("core.echo", b"x")), &mut Failing).unwrap_err();
    assert_eq!(e.to_string(), "closed");
}
