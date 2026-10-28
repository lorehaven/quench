//! The connection: objects, requests, and events read off the socket.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::time::Duration;

use crate::proto::{self, DISPLAY, Event, Interface};
use crate::sys::Socket;
use crate::wire::{Args, Decoder, encode};

/// A client of the compositor: it allocates object ids, sends requests and decodes events.
#[derive(Debug)]
pub struct Client {
    socket: Socket,
    decoder: Decoder,
    /// Descriptors that arrived and have not been claimed by the message they belong to.
    fds: VecDeque<OwnedFd>,
    interfaces: HashMap<u32, Interface>,
    next_id: u32,
    ready: VecDeque<(u32, Event)>,
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

impl Client {
    /// Connects to the compositor named by the environment.
    ///
    /// # Errors
    /// When there is none to connect to.
    pub fn connect() -> io::Result<Self> {
        Ok(Self::new(Socket::connect_display()?))
    }

    #[must_use]
    pub fn new(socket: Socket) -> Self {
        Self {
            socket,
            decoder: Decoder::new(),
            fds: VecDeque::new(),
            interfaces: HashMap::from([(DISPLAY, Interface::Display)]),
            next_id: DISPLAY + 1,
            ready: VecDeque::new(),
        }
    }

    /// A new object id of `interface`; its events are decoded as that interface's.
    pub fn new_id(&mut self, interface: Interface) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.interfaces.insert(id, interface);
        id
    }

    /// Sends a request to `object`, with descriptors if the request carries any.
    ///
    /// # Errors
    /// The socket's error, or a request too long for the protocol.
    pub fn send(
        &self,
        object: u32,
        opcode: u16,
        args: Args,
        fds: &[BorrowedFd<'_>],
    ) -> io::Result<()> {
        let bytes = encode(object, opcode, &args.build()).map_err(invalid)?;
        self.socket.send(&bytes, fds)
    }

    /// Takes the oldest descriptor that arrived (the one a message such as a keymap refers to).
    pub fn take_fd(&mut self) -> Option<OwnedFd> {
        self.fds.pop_front()
    }

    /// The next event with the id of the object it is for, waiting up to `timeout` (`None`: for ever).
    /// `Ok(None)` when the time ran out.
    ///
    /// # Errors
    /// The socket's error, a connection the compositor closed, a message that is not valid, or a protocol
    /// error the compositor reported (after which the connection is no use).
    pub fn next_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<(u32, Event)>> {
        loop {
            if let Some(ready) = self.ready.pop_front() {
                return Ok(Some(ready));
            }
            while let Some(message) = self.decoder.next_message().map_err(invalid)? {
                let interface = self.interfaces.get(&message.object).copied();
                let event = interface
                    .map_or(Ok(Event::Ignored), |i| proto::decode(i, &message))
                    .map_err(invalid)?;
                match &event {
                    Event::Error {
                        object,
                        code,
                        message,
                    } => {
                        return Err(io::Error::other(format!(
                            "protocol error {code} on object {object}: {message}"
                        )));
                    }
                    // The compositor has finished with the id; a client that reuses ids would take it back here.
                    Event::DeleteId(id) => {
                        self.interfaces.remove(id);
                    }
                    _ => {}
                }
                self.ready.push_back((message.object, event));
            }
            if !self.ready.is_empty() {
                continue;
            }
            self.socket
                .set_read_timeout(timeout.map(|t| t.max(Duration::from_millis(1))))?;
            let mut buffer = [0u8; 4096];
            match self.socket.recv(&mut buffer) {
                Ok((0, _)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the compositor closed the connection",
                    ));
                }
                Ok((n, fds)) => {
                    self.fds.extend(fds);
                    self.decoder.feed(&buffer[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Waits until the compositor has handled everything sent so far, and returns what it said meanwhile.
    ///
    /// # Errors
    /// As [`next_event`](Self::next_event).
    pub fn roundtrip(&mut self) -> io::Result<Vec<(u32, Event)>> {
        let callback = self.new_id(Interface::Callback);
        self.send(
            DISPLAY,
            proto::display::SYNC,
            Args::new().object(callback),
            &[],
        )?;
        let mut seen = Vec::new();
        loop {
            match self.next_event(Some(Duration::from_secs(5)))? {
                Some((id, Event::CallbackDone)) if id == callback => return Ok(seen),
                Some(other) => seen.push(other),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the compositor did not answer",
                    ));
                }
            }
        }
    }
}
