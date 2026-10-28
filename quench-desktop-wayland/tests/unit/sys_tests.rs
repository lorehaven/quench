use quench_desktop_wayland::sys::{Mapping, Socket, memfd};
use std::io::ErrorKind;
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

fn pair() -> (Socket, Socket) {
    let (a, b) = UnixStream::pair().unwrap();
    (Socket::from_stream(a), Socket::from_stream(b))
}

#[quench_test::test]
fn bytes_and_a_descriptor_arrive_together_and_the_memory_is_shared() {
    let (tx, rx) = pair();
    let fd = memfd(64).unwrap();
    let mut mine = Mapping::new(fd.as_fd(), 64, true).unwrap();
    mine.as_mut_slice()[..5].copy_from_slice(b"hello");
    tx.send(b"hi", &[fd.as_fd()]).unwrap();
    let mut buffer = [0u8; 16];
    let (n, mut fds) = rx.recv(&mut buffer).unwrap();
    assert_eq!(&buffer[..n], b"hi");
    assert_eq!(fds.len(), 1);
    // The descriptor that arrived is the same memory: a read-only mapping sees what the writer wrote, and
    // what the writer writes later.
    let theirs = Mapping::new(fds.remove(0).as_fd(), 64, false).unwrap();
    assert_eq!(&theirs.as_slice()[..5], b"hello");
    mine.as_mut_slice()[0] = b'J';
    assert_eq!(theirs.as_slice()[0], b'J');
    assert_eq!((theirs.len(), theirs.is_empty()), (64, false));
    assert!(format!("{theirs:?}").contains("Mapping"));
}

#[quench_test::test]
fn several_descriptors_come_in_order_and_a_send_without_any_is_plain() {
    let (tx, rx) = pair();
    let (a, b) = (memfd(8).unwrap(), memfd(16).unwrap());
    tx.send(b"x", &[a.as_fd(), b.as_fd()]).unwrap();
    let mut buffer = [0u8; 4];
    let (n, fds) = rx.recv(&mut buffer).unwrap();
    assert_eq!((n, fds.len()), (1, 2));
    // Sizes tell them apart: the second is the 16-byte one.
    let sizes: Vec<_> = fds
        .iter()
        .map(|fd| {
            std::fs::File::from(fd.try_clone().unwrap())
                .metadata()
                .unwrap()
                .len()
        })
        .collect();
    assert_eq!(sizes, [8, 16]);
    tx.send(b"plain", &[]).unwrap();
    let (n, fds) = rx.recv(&mut buffer).unwrap();
    assert_eq!(
        (&buffer[..n], fds.len()),
        (&b"plai"[..], 0),
        "the buffer is smaller than the message"
    );
}

#[quench_test::test]
fn a_large_send_is_completed_while_the_other_side_reads() {
    let (tx, rx) = pair();
    let reader = std::thread::spawn(move || {
        let mut total = 0usize;
        let mut buffer = vec![0u8; 65536];
        let mut fds = 0;
        while total < 4 << 20 {
            let (n, got) = rx.recv(&mut buffer).unwrap();
            assert!(n > 0);
            total += n;
            fds += got.len();
        }
        (total, fds)
    });
    let fd = memfd(8).unwrap();
    tx.send(&vec![7u8; 4 << 20], &[fd.as_fd()]).unwrap();
    assert_eq!(
        reader.join().unwrap(),
        (4 << 20, 1),
        "all the bytes, and the descriptor once"
    );
}

#[quench_test::test]
fn the_end_of_the_connection_and_a_read_timeout_are_told_apart() {
    let (tx, rx) = pair();
    rx.set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    let mut buffer = [0u8; 8];
    let error = rx.recv(&mut buffer).unwrap_err();
    assert!(
        matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
        "{error:?}"
    );
    drop(tx);
    let (n, fds) = rx.recv(&mut buffer).unwrap();
    assert_eq!(
        (n, fds.len()),
        (0, 0),
        "a closed connection reads as zero bytes"
    );
    // And sending to it fails with an error instead of a signal.
    let (a, b) = pair();
    drop(b);
    assert!(a.send(b"gone", &[]).is_err());
}

#[quench_test::test]
fn more_descriptors_than_fit_are_an_error_not_a_silent_loss() {
    let (tx, rx) = pair();
    let fds: Vec<_> = (0..9).map(|_| memfd(4).unwrap()).collect();
    let borrowed: Vec<_> = fds.iter().map(AsFd::as_fd).collect();
    tx.send(b"many", &borrowed).unwrap();
    let mut buffer = [0u8; 8];
    let error = rx.recv(&mut buffer).unwrap_err();
    assert!(
        error.to_string().contains("descriptors were lost"),
        "{error}"
    );
}

#[quench_test::test]
fn shared_memory_is_zeroed_and_a_mapping_has_rules() {
    let fd = memfd(4096).unwrap();
    let mapping = Mapping::new(fd.as_fd(), 4096, false).unwrap();
    assert!(
        mapping.as_slice().iter().all(|b| *b == 0),
        "a new memfd is zero-filled"
    );
    assert_eq!(
        Mapping::new(fd.as_fd(), 0, true).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    // Writing to a read-only mapping is refused up front, not with a fault.
    let mut read_only = Mapping::new(fd.as_fd(), 4096, false).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_only.as_mut_slice();
        }))
        .is_err()
    );
    // A huge length is an error from the kernel or the size conversion, not a crash.
    assert!(memfd(usize::MAX).is_err());
}

#[quench_test::test]
fn a_socket_connects_to_a_path_and_a_missing_one_is_an_error() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let path = dir.path().join("wayland-test");
    let listener = UnixListener::bind(&path).unwrap();
    let client = Socket::connect_path(&path).unwrap();
    let (server, _) = listener.accept().unwrap();
    client.send(b"ping", &[]).unwrap();
    let mut buffer = [0u8; 4];
    let n = std::io::Read::read(&mut &server, &mut buffer).unwrap();
    assert_eq!(&buffer[..n], b"ping");
    assert!(Socket::connect_path(&dir.path().join("missing")).is_err());
}
