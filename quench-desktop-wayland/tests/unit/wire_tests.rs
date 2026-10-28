use quench_desktop_wayland::wire::{Args, Decoder, HEADER, MAX_MESSAGE, Reader, WireError, encode};

#[quench_test::test]
fn a_message_has_its_object_size_and_opcode_in_the_header() {
    let bytes = encode(7, 3, &Args::new().uint(0xAABB_CCDD).build()).unwrap();
    assert_eq!(bytes.len(), HEADER + 4);
    assert_eq!(u32::from_ne_bytes(bytes[0..4].try_into().unwrap()), 7);
    let word = u32::from_ne_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!((word >> 16, word & 0xFFFF), (12, 3));
    assert_eq!(&bytes[8..], &0xAABB_CCDD_u32.to_ne_bytes());
}

#[quench_test::test]
fn arguments_round_trip_through_the_reader() {
    let args = Args::new()
        .uint(4_000_000_000)
        .int(-5)
        .fixed(-12.375)
        .object(9)
        .string("héllo")
        .string("")
        .array(&[1, 2, 3, 4, 5])
        .build();
    assert_eq!(args.len() % 4, 0, "every argument is padded to 4");
    let mut r = Reader::new(&args);
    assert_eq!(r.uint().unwrap(), 4_000_000_000);
    assert_eq!(r.int().unwrap(), -5);
    assert!((r.fixed().unwrap() + 12.375).abs() < 1e-9);
    assert_eq!(r.object().unwrap(), 9);
    assert_eq!(r.string().unwrap(), "héllo");
    assert_eq!(
        r.string().unwrap(),
        "",
        "an empty string has a NUL, so a length of 1"
    );
    assert_eq!(r.array().unwrap(), &[1, 2, 3, 4, 5]);
    assert!(r.done());
    assert_eq!(r.uint(), Err(WireError::Short));
}

#[quench_test::test]
fn strings_are_nul_terminated_and_padded() {
    // "abc" is 4 bytes with its NUL: no padding. "abcd" is 5: padded to 8.
    assert_eq!(Args::new().string("abc").build().len(), 4 + 4);
    assert_eq!(Args::new().string("abcd").build().len(), 4 + 8);
    let bytes = Args::new().string("abcd").build();
    assert_eq!(&bytes[..4], &5u32.to_ne_bytes());
    assert_eq!(&bytes[4..9], b"abcd\0");
    assert_eq!(&bytes[9..], &[0, 0, 0]);
    // A null string (length 0) is read as empty and takes no bytes.
    assert_eq!(Reader::new(&0u32.to_ne_bytes()).string().unwrap(), "");
}

#[quench_test::test]
fn bad_strings_and_short_arguments_are_errors() {
    let no_nul = Args::new().array(b"abc").build(); // length 3, bytes "abc" and padding: the last byte is not NUL
    assert_eq!(Reader::new(&no_nul).string(), Err(WireError::BadString));
    let not_utf8 = [2u32.to_ne_bytes().as_slice(), &[0xFF, 0, 0, 0]].concat();
    assert_eq!(Reader::new(&not_utf8).string(), Err(WireError::BadString));
    let overlong = 1000u32.to_ne_bytes();
    assert_eq!(Reader::new(&overlong).string(), Err(WireError::Short));
    assert_eq!(Reader::new(&overlong).array(), Err(WireError::Short));
    assert_eq!(Reader::new(&[]).int(), Err(WireError::Short));
    assert_eq!(Reader::new(&[1, 2, 3]).uint(), Err(WireError::Short));
    assert_eq!(
        Reader::new(&u32::MAX.to_ne_bytes()).string(),
        Err(WireError::Short),
        "a huge length does not overflow"
    );
}

#[quench_test::test]
fn fixed_point_has_eight_fractional_bits() {
    for value in [0.0, 1.0, -1.0, 0.5, 255.996_093_75, -8_388_608.0, 100.25] {
        let bytes = Args::new().fixed(value).build();
        assert_eq!(Reader::new(&bytes).fixed().unwrap(), value, "{value}");
    }
    assert_eq!(Args::new().fixed(1.0).build(), 256i32.to_ne_bytes());
}

#[quench_test::test]
fn the_decoder_cuts_messages_whatever_the_pieces() {
    let a = encode(1, 0, &Args::new().uint(11).build()).unwrap();
    let b = encode(2, 5, &Args::new().string("two").build()).unwrap();
    let c = encode(3, 9, &[]).unwrap();
    let stream = [a, b, c].concat();
    // All at once.
    let mut d = Decoder::new();
    d.feed(&stream);
    let mut got = Vec::new();
    while let Some(m) = d.next_message().unwrap() {
        got.push((m.object, m.opcode));
    }
    assert_eq!(got, [(1, 0), (2, 5), (3, 9)]);
    assert_eq!(d.next_message().unwrap(), None);
    // One byte at a time: nothing comes out early, and everything comes out whole.
    let mut d = Decoder::new();
    let mut got = Vec::new();
    for byte in &stream {
        d.feed(&[*byte]);
        while let Some(m) = d.next_message().unwrap() {
            got.push(m);
        }
    }
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].args, 11u32.to_ne_bytes());
    assert_eq!(Reader::new(&got[1].args).string().unwrap(), "two");
    assert!(got[2].args.is_empty());
}

#[quench_test::test]
fn a_header_that_cannot_be_a_message_is_an_error() {
    for size in [0u32, 4, 7, 9, 10] {
        let mut d = Decoder::new();
        d.feed(&1u32.to_ne_bytes());
        d.feed(&((size << 16) | 1).to_ne_bytes());
        assert_eq!(
            d.next_message(),
            Err(WireError::BadSize(size as usize)),
            "{size}"
        );
    }
    // Fewer than 8 bytes is just not a message yet.
    let mut d = Decoder::new();
    d.feed(&[1, 2, 3]);
    assert_eq!(d.next_message(), Ok(None));
}

#[quench_test::test]
fn a_message_must_fit_the_size_field() {
    let biggest = vec![0u8; MAX_MESSAGE - HEADER - 3]; // 65524 + 8 = 65532: the last multiple of 4 that fits
    assert!(encode(1, 0, &biggest).is_ok());
    let too_big = vec![0u8; MAX_MESSAGE - HEADER + 1 + 3 - ((MAX_MESSAGE - HEADER + 1 + 3) % 4)];
    assert_eq!(encode(1, 0, &too_big), Err(WireError::TooLong));
    assert!(WireError::TooLong.to_string().contains("65535"));
    for e in [
        WireError::BadSize(3),
        WireError::Short,
        WireError::BadString,
    ] {
        assert!(!e.to_string().is_empty());
    }
}
