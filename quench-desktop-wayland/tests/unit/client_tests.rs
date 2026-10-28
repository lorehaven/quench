use quench_desktop_wayland::client::Client;
use quench_desktop_wayland::proto::{Event, Interface, display};
use quench_desktop_wayland::sys::{Socket, memfd};
use quench_desktop_wayland::wire::{Args, Decoder, encode};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// A client, and the server's end of its connection.
fn connected() -> (Client, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    (Client::new(Socket::from_stream(a)), b)
}

fn event(object: u32, opcode: u16, args: Args) -> Vec<u8> {
    encode(object, opcode, &args.build()).unwrap()
}

#[quench_test::test]
fn a_request_arrives_as_the_bytes_of_its_message() {
    let (client, mut server) = connected();
    client
        .send(1, display::GET_REGISTRY, Args::new().object(2), &[])
        .unwrap();
    let mut buffer = [0u8; 64];
    let n = server.read(&mut buffer).unwrap();
    let mut d = Decoder::new();
    d.feed(&buffer[..n]);
    let m = d.next_message().unwrap().unwrap();
    assert_eq!(
        (m.object, m.opcode, m.args),
        (1, display::GET_REGISTRY, 2u32.to_ne_bytes().to_vec())
    );
}

#[quench_test::test]
fn ids_count_up_from_the_display() {
    let (mut client, _server) = connected();
    assert_eq!(client.new_id(Interface::Registry), 2);
    assert_eq!(client.new_id(Interface::Callback), 3);
    assert_eq!(client.new_id(Interface::Surface), 4);
}

#[quench_test::test]
fn events_are_decoded_by_the_kind_of_their_object() {
    let (mut client, mut server) = connected();
    let registry = client.new_id(Interface::Registry);
    server
        .write_all(
            &[
                event(registry, 0, Args::new().uint(7).string("wl_shm").uint(1)),
                event(registry, 1, Args::new().uint(7)),
                event(99, 0, Args::new()), // an object we never made: ignored, not an error
            ]
            .concat(),
        )
        .unwrap();
    let timeout = Some(Duration::from_secs(1));
    assert_eq!(
        client.next_event(timeout).unwrap(),
        Some((
            registry,
            Event::Global {
                name: 7,
                interface: "wl_shm".into(),
                version: 1
            }
        ))
    );
    assert_eq!(
        client.next_event(timeout).unwrap(),
        Some((registry, Event::GlobalRemove(7)))
    );
    assert_eq!(
        client.next_event(timeout).unwrap(),
        Some((99, Event::Ignored))
    );
    assert_eq!(
        client.next_event(Some(Duration::from_millis(20))).unwrap(),
        None,
        "nothing more: a timeout, not an error"
    );
}

#[quench_test::test]
fn a_roundtrip_returns_what_arrived_before_the_answer() {
    let (mut client, mut server) = connected();
    let registry = client.new_id(Interface::Registry);
    // The callback `roundtrip` makes will be id 3; the server answers its `sync` after a global.
    let answer = std::thread::spawn(move || {
        let mut buffer = [0u8; 64];
        let n = server.read(&mut buffer).unwrap();
        let mut d = Decoder::new();
        d.feed(&buffer[..n]);
        let sync = d.next_message().unwrap().unwrap();
        assert_eq!((sync.object, sync.opcode), (1, display::SYNC));
        let callback = u32::from_ne_bytes(sync.args[..4].try_into().unwrap());
        server
            .write_all(&event(
                registry,
                0,
                Args::new().uint(1).string("wl_seat").uint(5),
            ))
            .unwrap();
        server
            .write_all(&event(callback, 0, Args::new().uint(0)))
            .unwrap();
        server
    });
    let seen = client.roundtrip().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(matches!(&seen[0].1, Event::Global { interface, .. } if interface == "wl_seat"));
    drop(answer.join().unwrap());
}

#[quench_test::test]
fn a_protocol_error_ends_the_connection_with_its_message() {
    let (mut client, mut server) = connected();
    server
        .write_all(&event(
            1,
            display::ERROR,
            Args::new().object(5).uint(2).string("invalid size"),
        ))
        .unwrap();
    let error = client.next_event(Some(Duration::from_secs(1))).unwrap_err();
    assert!(
        error.to_string().contains("invalid size") && error.to_string().contains('2'),
        "{error}"
    );
}

#[quench_test::test]
fn a_closed_connection_and_a_corrupt_stream_are_errors() {
    let (mut client, server) = connected();
    drop(server);
    assert_eq!(
        client
            .next_event(Some(Duration::from_secs(1)))
            .unwrap_err()
            .kind(),
        ErrorKind::UnexpectedEof
    );
    let (mut client, mut server) = connected();
    server.write_all(&[1, 0, 0, 0, 3, 0, 0, 0]).unwrap(); // a size of 0 in the header's upper half: not a message
    assert_eq!(
        client
            .next_event(Some(Duration::from_secs(1)))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    // A message whose arguments are cut short for its kind is corrupt too.
    let (mut client, mut server) = connected();
    server
        .write_all(&event(1, display::ERROR, Args::new().object(5)))
        .unwrap();
    assert_eq!(
        client
            .next_event(Some(Duration::from_secs(1)))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}

#[quench_test::test]
fn a_deleted_id_is_forgotten_and_its_events_are_ignored() {
    let (mut client, mut server) = connected();
    let callback = client.new_id(Interface::Callback);
    server
        .write_all(
            &[
                event(1, display::DELETE_ID, Args::new().uint(callback)),
                event(callback, 0, Args::new().uint(0)),
            ]
            .concat(),
        )
        .unwrap();
    let timeout = Some(Duration::from_secs(1));
    assert_eq!(
        client.next_event(timeout).unwrap(),
        Some((1, Event::DeleteId(callback)))
    );
    assert_eq!(
        client.next_event(timeout).unwrap(),
        Some((callback, Event::Ignored))
    );
}

#[quench_test::test]
fn a_roundtrip_gives_up_on_a_compositor_that_does_not_answer() {
    // Only an end-of-file ends it early here: the answer never comes.
    let (mut client, server) = connected();
    drop(server);
    // The request may fail to be sent, or the answer may never come: either way it is an error, quickly.
    let kind = client.roundtrip().unwrap_err().kind();
    assert!(
        matches!(
            kind,
            ErrorKind::BrokenPipe | ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset
        ),
        "{kind:?}"
    );
}

#[quench_test::test]
fn a_descriptor_that_arrives_with_a_message_can_be_claimed() {
    let (a, b) = UnixStream::pair().unwrap();
    let mut client = Client::new(Socket::from_stream(a));
    let server = Socket::from_stream(b);
    let fd = memfd(8).unwrap();
    server
        .send(
            &event(1, display::DELETE_ID, Args::new().uint(5)),
            &[fd.as_fd()],
        )
        .unwrap();
    assert!(client.take_fd().is_none(), "nothing has been read yet");
    client.next_event(Some(Duration::from_secs(1))).unwrap();
    assert!(client.take_fd().is_some());
    assert!(client.take_fd().is_none());
}
