//! The window against a compositor played by the test: it answers the handshake, reads the pixels out of the
//! shared memory the window sends, and sends events.
use quench_desktop_wayland::client::Client;
use quench_desktop_wayland::proto::{
    buffer, callback, data_device, data_device_manager, data_offer, data_source, display, keyboard,
    pointer, registry, seat, shm, shm_pool, surface, text_input, text_input_manager, xdg_surface,
    xdg_toplevel, xdg_wm_base,
};
use quench_desktop_wayland::sys::{Mapping, Socket};
use quench_desktop_wayland::wire::{Args, Decoder, Message, Reader, encode};
use quench_desktop_wayland::{Window, WindowEvent};
use quench_raster::Color;
use std::collections::VecDeque;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// How long to wait for something that should happen (a busy machine is slow) ...
const LONG: Duration = Duration::from_secs(5);
/// ... and for something that should not (nothing is slow about not happening).
const QUIET: Duration = Duration::from_millis(250);

/// The compositor's end.
struct Fake {
    socket: Socket,
    decoder: Decoder,
    fds: VecDeque<OwnedFd>,
}

impl Fake {
    fn read(&mut self) -> Message {
        loop {
            if let Some(m) = self.decoder.next_message().unwrap() {
                return m;
            }
            self.socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0u8; 4096];
            let (n, fds) = self
                .socket
                .recv(&mut buffer)
                .expect("the window sent nothing in 5 s");
            assert!(n > 0, "the window closed the connection");
            self.fds.extend(fds);
            self.decoder.feed(&buffer[..n]);
        }
    }

    fn expect(&mut self, object: u32, opcode: u16) -> Message {
        let m = self.read();
        assert_eq!((m.object, m.opcode), (object, opcode), "{m:?}");
        m
    }

    fn send_with_fd(&self, object: u32, opcode: u16, args: Args, fd: &OwnedFd) {
        self.socket
            .send(
                &encode(object, opcode, &args.build()).unwrap(),
                &[fd.as_fd()],
            )
            .unwrap();
    }

    fn send(&self, object: u32, opcode: u16, args: Args) {
        self.socket
            .send(&encode(object, opcode, &args.build()).unwrap(), &[])
            .unwrap();
    }
}

fn ids(message: &Message) -> Vec<u32> {
    message
        .args
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect()
}

/// What the handshake learned.
struct Setup {
    registry: u32,
    compositor: u32,
    shm: u32,
    wm_base: u32,
    seat: Option<u32>,
    text_manager: Option<u32>,
    surface: u32,
    xdg_surface: u32,
    toplevel: u32,
    title: String,
    app_id: String,
    pool: Pool,
}

/// A pool the window made: its memory, and what it made of it.
struct Pool {
    id: u32,
    size: i32,
    memory: Mapping,
    /// `(id, offset, width, height, stride, format)` of each buffer.
    buffers: Vec<(u32, i32, i32, i32, i32, u32)>,
}

const GLOBALS: [(&str, u32); 4] = [
    ("wl_compositor", 4),
    ("wl_shm", 1),
    ("xdg_wm_base", 2),
    ("wl_seat", 5),
];

/// Reads the pool and its buffers that follow a configuration.
fn read_pool(fake: &mut Fake, shm_id: u32) -> Pool {
    let create = fake.expect(shm_id, shm::CREATE_POOL);
    let [id, size] = ids(&create)[..] else {
        panic!("{create:?}")
    };
    let fd = fake
        .fds
        .pop_front()
        .expect("the pool's descriptor came with the request");
    let memory = Mapping::new(fd.as_fd(), size as usize, false).unwrap();
    let buffers = (0..2)
        .map(|_| {
            let m = fake.expect(id, shm_pool::CREATE_BUFFER);
            let a = ids(&m);
            (
                a[0],
                a[1] as i32,
                a[2] as i32,
                a[3] as i32,
                a[4] as i32,
                a[5],
            )
        })
        .collect();
    Pool {
        id,
        size: size as i32,
        memory,
        buffers,
    }
}

/// Answers the handshake of a window with these globals, and configures it to `width` x `height`.
fn handshake(fake: &mut Fake, globals: &[(&str, u32)], width: i32, height: i32) -> Setup {
    let reg = fake.expect(1, display::GET_REGISTRY);
    let registry_id = ids(&reg)[0];
    let sync = fake.expect(1, display::SYNC);
    for (i, (name, version)) in globals.iter().enumerate() {
        fake.send(
            registry_id,
            registry::GLOBAL,
            Args::new().uint(i as u32 + 1).string(name).uint(*version),
        );
    }
    fake.send(ids(&sync)[0], callback::DONE, Args::new().uint(0));
    let mut bound = std::collections::HashMap::new();
    let create_surface = loop {
        let m = fake.read();
        if m.object == registry_id {
            assert_eq!(m.opcode, registry::BIND);
            let mut r = Reader::new(&m.args);
            let (_name, interface, version, id) = (
                r.uint().unwrap(),
                r.string().unwrap(),
                r.uint().unwrap(),
                r.object().unwrap(),
            );
            let wanted = globals.iter().find(|g| g.0 == interface).unwrap().1;
            assert!(
                version <= wanted,
                "a version above what the compositor offered"
            );
            bound.insert(interface, id);
        } else {
            break m;
        }
    };
    let compositor = bound["wl_compositor"];
    assert_eq!(
        (create_surface.object, create_surface.opcode),
        (compositor, 0)
    );
    let surface_id = ids(&create_surface)[0];
    let get_xdg = fake.expect(bound["xdg_wm_base"], xdg_wm_base::GET_XDG_SURFACE);
    assert_eq!(ids(&get_xdg)[1], surface_id);
    let xdg = ids(&get_xdg)[0];
    let toplevel = ids(&fake.expect(xdg, xdg_surface::GET_TOPLEVEL))[0];
    let title = Reader::new(&fake.expect(toplevel, xdg_toplevel::SET_TITLE).args)
        .string()
        .unwrap();
    let app_id = Reader::new(&fake.expect(toplevel, xdg_toplevel::SET_APP_ID).args)
        .string()
        .unwrap();
    fake.expect(surface_id, surface::COMMIT);
    fake.send(
        toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(width).int(height).array(&[]),
    );
    fake.send(xdg, xdg_surface::CONFIGURE, Args::new().uint(77));
    assert_eq!(ids(&fake.expect(xdg, xdg_surface::ACK_CONFIGURE)), [77]);
    let shm_id = bound["wl_shm"];
    let pool = read_pool(fake, shm_id);
    Setup {
        registry: registry_id,
        compositor,
        shm: shm_id,
        wm_base: bound["xdg_wm_base"],
        seat: bound.get("wl_seat").copied(),
        text_manager: bound.get("zwp_text_input_manager_v3").copied(),
        surface: surface_id,
        xdg_surface: xdg,
        toplevel,
        title,
        app_id,
        pool,
    }
}

enum Cmd {
    Draw(Color),
    Present,
    RequestFrame,
    Next(Option<Duration>),
    Size,
    ImeEnable(String, i32, i32, (i32, i32, i32, i32)),
    ImeUpdate(String, i32, i32, (i32, i32, i32, i32)),
    ImeDisable,
    SetClipboard(String),
    GetClipboard,
}

enum Out {
    Opened(Result<(usize, usize), String>),
    Drawn,
    Presented,
    Event(Option<WindowEvent>),
    Size((usize, usize)),
    Text(Option<String>),
    Failed(String),
}

/// The window, on a thread of its own, driven by commands.
struct App {
    tx: Sender<Cmd>,
    rx: Receiver<Out>,
}

impl App {
    fn start(socket: Socket, width: usize, height: usize) -> Self {
        let (tx, commands) = channel::<Cmd>();
        let (results, rx) = channel::<Out>();
        std::thread::spawn(move || {
            let mut window =
                match Window::open_on(Client::new(socket), "Test", "dev.test.app", width, height) {
                    Ok(window) => window,
                    Err(e) => {
                        let _ = results.send(Out::Opened(Err(format!("{:?}:{e}", e.kind()))));
                        return;
                    }
                };
            let _ = results.send(Out::Opened(Ok(window.size())));
            for command in commands {
                let out = match command {
                    Cmd::Draw(color) => {
                        window.begin_frame().unwrap().clear(color);
                        Out::Drawn
                    }
                    Cmd::Present => {
                        window.present().unwrap();
                        Out::Presented
                    }
                    Cmd::RequestFrame => {
                        window.request_frame().unwrap();
                        Out::Presented
                    }
                    Cmd::Next(timeout) => match window.next_event(timeout) {
                        Ok(event) => Out::Event(event),
                        Err(e) => Out::Failed(e.to_string()),
                    },
                    Cmd::Size => Out::Size(window.size()),
                    Cmd::ImeEnable(text, cursor, anchor, rect) => {
                        match window.ime_enable(&text, cursor, anchor, rect) {
                            Ok(()) => Out::Drawn,
                            Err(e) => Out::Failed(e.to_string()),
                        }
                    }
                    Cmd::ImeUpdate(text, cursor, anchor, rect) => {
                        match window.ime_update(&text, cursor, anchor, rect) {
                            Ok(()) => Out::Drawn,
                            Err(e) => Out::Failed(e.to_string()),
                        }
                    }
                    Cmd::SetClipboard(text) => match window.set_clipboard(&text) {
                        Ok(()) => Out::Drawn,
                        Err(e) => Out::Failed(e.to_string()),
                    },
                    Cmd::GetClipboard => match window.clipboard() {
                        Ok(text) => Out::Text(text),
                        Err(e) => Out::Failed(e.to_string()),
                    },
                    Cmd::ImeDisable => match window.ime_disable() {
                        Ok(()) => Out::Drawn,
                        Err(e) => Out::Failed(e.to_string()),
                    },
                };
                if results.send(out).is_err() {
                    return;
                }
            }
        });
        Self { tx, rx }
    }

    fn ask(&self, command: Cmd) -> Out {
        self.tx.send(command).unwrap();
        self.rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the window did not answer")
    }

    fn opened(&self) -> Result<(usize, usize), String> {
        match self.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            Out::Opened(result) => result,
            _ => panic!("not opened"),
        }
    }

    fn next(&self) -> Option<WindowEvent> {
        let Out::Event(event) = self.ask(Cmd::Next(Some(LONG))) else {
            panic!()
        };
        event
    }

    /// Nothing the application can see happens.
    fn quiet(&self) {
        let Out::Event(event) = self.ask(Cmd::Next(Some(QUIET))) else {
            panic!()
        };
        assert_eq!(event, None);
    }
}

fn open(width: i32, height: i32) -> (Fake, Setup, App) {
    open_with(&GLOBALS, width, height)
}

fn open_with(globals: &[(&str, u32)], width: i32, height: i32) -> (Fake, Setup, App) {
    let (a, b) = UnixStream::pair().unwrap();
    let app = App::start(Socket::from_stream(a), 640, 480);
    let mut fake = Fake {
        socket: Socket::from_stream(b),
        decoder: Decoder::new(),
        fds: VecDeque::new(),
    };
    let setup = handshake(&mut fake, globals, width, height);
    assert_eq!(app.opened().unwrap(), (width as usize, height as usize));
    (fake, setup, app)
}

fn bgra(memory: &Mapping, frame: usize, slot: usize, x: usize, y: usize, width: usize) -> [u8; 4] {
    let at = slot * frame + (y * width + x) * 4;
    memory.as_slice()[at..at + 4].try_into().unwrap()
}

#[quench_test::test]
fn the_window_names_itself_and_takes_the_size_the_compositor_chose() {
    let (_fake, setup, app) = open(300, 200);
    assert_eq!(
        (setup.title.as_str(), setup.app_id.as_str()),
        ("Test", "dev.test.app")
    );
    // The compositor chose 300x200 although 640x480 was asked: two frames of memory in one pool, two buffers
    // in ARGB8888 with a stride of 4 bytes a pixel.
    assert_eq!(setup.pool.size, 2 * 300 * 200 * 4);
    assert_eq!(setup.pool.buffers.len(), 2);
    assert_eq!(setup.pool.buffers[0].1, 0);
    assert_eq!(setup.pool.buffers[1].1, 300 * 200 * 4);
    for b in &setup.pool.buffers {
        assert_eq!((b.2, b.3, b.4, b.5), (300, 200, 1200, 0));
    }
    assert!(setup.seat.is_some());
    let Out::Size(size) = app.ask(Cmd::Size) else {
        panic!()
    };
    assert_eq!(size, (300, 200));
}

#[quench_test::test]
fn a_compositor_that_leaves_the_size_to_the_window_gets_the_size_that_was_asked_for() {
    let (a, b) = UnixStream::pair().unwrap();
    let app = App::start(Socket::from_stream(a), 640, 480);
    let mut fake = Fake {
        socket: Socket::from_stream(b),
        decoder: Decoder::new(),
        fds: VecDeque::new(),
    };
    let setup = handshake(&mut fake, &GLOBALS, 0, 0);
    assert_eq!(app.opened().unwrap(), (640, 480));
    assert_eq!(setup.pool.size, 2 * 640 * 480 * 4);
}

#[quench_test::test]
fn a_frame_is_drawn_into_the_shared_memory_and_presented_with_its_damage() {
    let (mut fake, setup, app) = open(8, 4);
    let frame = 8 * 4 * 4;
    assert!(matches!(
        app.ask(Cmd::Draw(Color::rgb(255, 0, 0))),
        Out::Drawn
    ));
    // The pixels are in the memory the compositor has, in premultiplied BGRA, before anything is presented.
    assert_eq!(
        bgra(&setup.pool.memory, frame, 0, 3, 2, 8),
        [0, 0, 255, 255]
    );
    assert_eq!(
        bgra(&setup.pool.memory, frame, 1, 3, 2, 8),
        [0, 0, 0, 0],
        "the other frame is untouched"
    );
    assert!(matches!(app.ask(Cmd::Present), Out::Presented));
    let attach = ids(&fake.expect(setup.surface, surface::ATTACH));
    assert_eq!(attach, [setup.pool.buffers[0].0, 0, 0]);
    assert_eq!(
        ids(&fake.expect(setup.surface, surface::DAMAGE_BUFFER)),
        [0, 0, 8, 4]
    );
    fake.expect(setup.surface, surface::COMMIT);
    // The next frame is drawn into the other buffer, so the one the compositor holds is not written.
    app.ask(Cmd::Draw(Color::rgb(0, 255, 0)));
    assert_eq!(
        bgra(&setup.pool.memory, frame, 1, 0, 0, 8),
        [0, 255, 0, 255]
    );
    assert_eq!(
        bgra(&setup.pool.memory, frame, 0, 0, 0, 8),
        [0, 0, 255, 255],
        "still the red one"
    );
    app.ask(Cmd::Present);
    assert_eq!(
        ids(&fake.expect(setup.surface, surface::ATTACH))[0],
        setup.pool.buffers[1].0
    );
}

#[quench_test::test]
fn drawing_waits_for_the_compositor_to_release_a_buffer() {
    let (mut fake, setup, app) = open(4, 4);
    for color in [Color::rgb(1, 1, 1), Color::rgb(2, 2, 2)] {
        app.ask(Cmd::Draw(color));
        app.ask(Cmd::Present);
        fake.expect(setup.surface, surface::ATTACH);
        fake.expect(setup.surface, surface::DAMAGE_BUFFER);
        fake.expect(setup.surface, surface::COMMIT);
    }
    // Both buffers are held: the third frame cannot start until one is released.
    app.tx.send(Cmd::Draw(Color::rgb(3, 3, 3))).unwrap();
    assert!(
        app.rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "it must wait"
    );
    fake.send(setup.pool.buffers[0].0, buffer::RELEASE, Args::new());
    assert!(matches!(
        app.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Out::Drawn
    ));
    assert_eq!(
        bgra(&setup.pool.memory, 4 * 4 * 4, 0, 0, 0, 4),
        [3, 3, 3, 255],
        "into the buffer that was released"
    );
}

#[quench_test::test]
fn pings_are_answered_and_the_pointer_reaches_the_application() {
    let (mut fake, setup, app) = open(100, 100);
    fake.send(setup.wm_base, xdg_wm_base::PING, Args::new().uint(1234));
    // Pings are answered while the window waits for events, and the pointer needs a seat that has one.
    fake.send(
        setup.seat.unwrap(),
        seat::CAPABILITIES,
        Args::new().uint(seat::POINTER | 2),
    );
    app.quiet();
    assert_eq!(ids(&fake.expect(setup.wm_base, xdg_wm_base::PONG)), [1234]);
    let pointer_id = ids(&fake.expect(setup.seat.unwrap(), seat::GET_POINTER))[0];
    fake.expect(setup.seat.unwrap(), seat::GET_KEYBOARD);
    // The same capabilities again do not make a second pointer.
    fake.send(
        setup.seat.unwrap(),
        seat::CAPABILITIES,
        Args::new().uint(seat::POINTER),
    );
    fake.send(
        pointer_id,
        pointer::ENTER,
        Args::new()
            .uint(5)
            .object(setup.surface)
            .fixed(10.5)
            .fixed(20.25),
    );
    fake.send(
        pointer_id,
        pointer::MOTION,
        Args::new().uint(100).fixed(30.0).fixed(40.0),
    );
    fake.send(
        pointer_id,
        pointer::BUTTON,
        Args::new().uint(6).uint(101).uint(0x110).uint(1),
    );
    fake.send(
        pointer_id,
        pointer::BUTTON,
        Args::new().uint(7).uint(102).uint(0x110).uint(0),
    );
    fake.send(
        pointer_id,
        pointer::AXIS,
        Args::new().uint(103).uint(0).fixed(15.0),
    );
    fake.send(pointer_id, pointer::FRAME, Args::new());
    fake.send(
        pointer_id,
        pointer::LEAVE,
        Args::new().uint(8).object(setup.surface),
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::PointerEntered { x: 10.5, y: 20.25 })
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::PointerMoved { x: 30.0, y: 40.0 })
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::PointerButton {
            x: 30.0,
            y: 40.0,
            button: 0x110,
            pressed: true
        })
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::PointerButton {
            x: 30.0,
            y: 40.0,
            button: 0x110,
            pressed: false
        })
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::Scroll {
            axis: 0,
            value: 15.0
        })
    );
    assert_eq!(app.next(), Some(WindowEvent::PointerLeft));
    app.quiet();
}

#[quench_test::test]
fn a_new_size_reallocates_the_buffers_and_is_reported_once() {
    let (mut fake, setup, app) = open(100, 80);
    // A suggestion of 0x0 means "your choice": nothing changes.
    fake.send(
        setup.toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(0).int(0).array(&[]),
    );
    fake.send(
        setup.xdg_surface,
        xdg_surface::CONFIGURE,
        Args::new().uint(80),
    );
    app.quiet();
    assert_eq!(
        ids(&fake.expect(setup.xdg_surface, xdg_surface::ACK_CONFIGURE)),
        [80]
    );
    // The same size again is not a change either.
    fake.send(
        setup.toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(100).int(80).array(&[]),
    );
    fake.send(
        setup.xdg_surface,
        xdg_surface::CONFIGURE,
        Args::new().uint(81),
    );
    app.quiet();
    fake.expect(setup.xdg_surface, xdg_surface::ACK_CONFIGURE);
    // A real one: acknowledged, new memory, the old buffers and pool destroyed, one event.
    fake.send(
        setup.toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(160).int(90).array(&[1, 0, 0, 0]),
    );
    fake.send(
        setup.xdg_surface,
        xdg_surface::CONFIGURE,
        Args::new().uint(82),
    );
    assert_eq!(
        app.next(),
        Some(WindowEvent::Resized {
            width: 160,
            height: 90
        })
    );
    assert_eq!(
        ids(&fake.expect(setup.xdg_surface, xdg_surface::ACK_CONFIGURE)),
        [82]
    );
    let fresh = read_pool(&mut fake, setup.shm);
    assert_eq!(fresh.size, 2 * 160 * 90 * 4);
    assert_ne!(fresh.id, setup.pool.id);
    assert_eq!(
        (fresh.buffers[0].2, fresh.buffers[0].3, fresh.buffers[0].4),
        (160, 90, 640)
    );
    for old in &setup.pool.buffers {
        fake.expect(old.0, buffer::DESTROY);
    }
    fake.expect(setup.pool.id, shm_pool::DESTROY);
    // The next frame is the new size.
    app.ask(Cmd::Draw(Color::rgb(9, 8, 7)));
    assert_eq!(
        bgra(&fresh.memory, 160 * 90 * 4, 0, 159, 89, 160),
        [7, 8, 9, 255]
    );
    let Out::Size(size) = app.ask(Cmd::Size) else {
        panic!()
    };
    assert_eq!(size, (160, 90));
    // Absurd sizes are not taken.
    fake.send(
        setup.toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(-5).int(90).array(&[]),
    );
    fake.send(
        setup.toplevel,
        xdg_toplevel::CONFIGURE,
        Args::new().int(100_000).int(90).array(&[]),
    );
    fake.send(
        setup.xdg_surface,
        xdg_surface::CONFIGURE,
        Args::new().uint(83),
    );
    app.quiet();
}

#[quench_test::test]
fn a_close_request_and_frame_callbacks_reach_the_application() {
    let (mut fake, setup, app) = open(10, 10);
    assert!(matches!(app.ask(Cmd::RequestFrame), Out::Presented));
    let callback_id = ids(&fake.expect(setup.surface, surface::FRAME))[0];
    // Another callback (a sync that is not ours) is not a frame.
    fake.send(callback_id + 100, callback::DONE, Args::new().uint(1));
    app.quiet();
    fake.send(callback_id, callback::DONE, Args::new().uint(1234));
    assert_eq!(app.next(), Some(WindowEvent::Frame));
    fake.send(callback_id, callback::DONE, Args::new().uint(1235));
    app.quiet();
    fake.send(setup.toplevel, xdg_toplevel::CLOSE, Args::new());
    assert_eq!(app.next(), Some(WindowEvent::CloseRequested));
    // The compositor going away is an error to the application, not a hang.
    let _ = (&setup.registry, &setup.compositor);
    drop(fake);
    let Out::Failed(error) = app.ask(Cmd::Next(Some(LONG))) else {
        panic!("an error was expected")
    };
    assert!(error.contains("closed the connection"), "{error}");
}

#[quench_test::test]
fn a_compositor_without_what_a_window_needs_is_refused_by_name() {
    for missing in ["wl_compositor", "wl_shm", "xdg_wm_base"] {
        let (a, b) = UnixStream::pair().unwrap();
        let app = App::start(Socket::from_stream(a), 100, 100);
        let mut fake = Fake {
            socket: Socket::from_stream(b),
            decoder: Decoder::new(),
            fds: VecDeque::new(),
        };
        let reg = fake.expect(1, display::GET_REGISTRY);
        let sync = fake.expect(1, display::SYNC);
        for (i, (name, version)) in GLOBALS.iter().filter(|g| g.0 != missing).enumerate() {
            fake.send(
                ids(&reg)[0],
                registry::GLOBAL,
                Args::new().uint(i as u32).string(name).uint(*version),
            );
        }
        fake.send(ids(&sync)[0], callback::DONE, Args::new().uint(0));
        let error = app.opened().unwrap_err();
        assert!(
            error.starts_with("Unsupported:") && error.contains(missing),
            "{missing}: {error}"
        );
    }
}

#[quench_test::test]
fn a_window_without_a_seat_still_works_and_ignores_pointer_capabilities() {
    let (a, b) = UnixStream::pair().unwrap();
    let app = App::start(Socket::from_stream(a), 50, 50);
    let mut fake = Fake {
        socket: Socket::from_stream(b),
        decoder: Decoder::new(),
        fds: VecDeque::new(),
    };
    let setup = handshake(&mut fake, &GLOBALS[..3], 50, 50);
    assert!(setup.seat.is_none());
    assert_eq!(app.opened().unwrap(), (50, 50));
    assert!(matches!(app.ask(Cmd::Draw(Color::BLACK)), Out::Drawn));
    assert_eq!(setup.pool.size, 2 * 50 * 50 * 4);
}

const POLISH: &str = r#"xkb_keymap {
xkb_keycodes "evdev+aliases(qwerty)" {
    minimum = 8; maximum = 255;
    <AC01> = 38;
    <AE01> = 10;
    <LFSH> = 50;
    <RALT> = 108;
    alias <LatA> = <AC01>;
};
xkb_types "complete" { virtual_modifiers NumLock; };
xkb_symbols "pc+pl+us" {
    name[Group1]="Polish";
    key <AC01> { type= "FOUR_LEVEL_SEMIALPHABETIC", symbols[Group1]= [ a, A, aogonek, Aogonek ] };
    key <AE01> { [ 1, exclam ] };
    key <LFSH> { [ Shift_L ] };
    key <RALT> { [ ISO_Level3_Shift ] };
};
};"#;

/// A file with `text` in it, to send as the keymap.
fn keymap_file(text: &str) -> OwnedFd {
    use std::io::Write as _;
    let path = std::env::temp_dir().join(format!(
        "quench-keymap-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let file = std::fs::File::open(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    file.into()
}

#[quench_test::test]
fn the_keyboard_sends_keys_read_through_the_layout_it_announced() {
    let (mut fake, setup, app) = open(100, 100);
    let seat_id = setup.seat.unwrap();
    fake.send(
        seat_id,
        seat::CAPABILITIES,
        Args::new().uint(seat::KEYBOARD),
    );
    app.quiet();
    let keyboard_id = ids(&fake.expect(seat_id, seat::GET_KEYBOARD))[0];
    // The layout arrives in a file; the window reads it.
    let fd = keymap_file(POLISH);
    fake.socket
        .send(
            &encode(
                keyboard_id,
                keyboard::KEYMAP,
                &Args::new().uint(1).uint(POLISH.len() as u32).build(),
            )
            .unwrap(),
            &[fd.as_fd()],
        )
        .unwrap();
    fake.send(
        keyboard_id,
        keyboard::REPEAT_INFO,
        Args::new().int(30).int(400),
    );
    fake.send(
        keyboard_id,
        keyboard::ENTER,
        Args::new().uint(1).object(setup.surface).array(&[]),
    );
    let press = |fake: &Fake, code: u32, down: bool| {
        fake.send(
            keyboard_id,
            keyboard::KEY,
            Args::new().uint(2).uint(0).uint(code).uint(u32::from(down)),
        );
    };
    press(&fake, 30, true);
    let Some(WindowEvent::Key {
        code, pressed, key, ..
    }) = app.next()
    else {
        panic!("a key")
    };
    assert_eq!(
        (code, pressed, key.name.as_str(), key.text.as_deref()),
        (30, true, "a", Some("a"))
    );
    // Shift (mask bit 0) gives the capital, AltGr (bit 7) the Polish letter.
    fake.send(
        keyboard_id,
        keyboard::MODIFIERS,
        Args::new().uint(3).uint(1).uint(0).uint(0).uint(0),
    );
    press(&fake, 30, false);
    let Some(WindowEvent::Key {
        pressed,
        key,
        modifiers,
        ..
    }) = app.next()
    else {
        panic!("a key")
    };
    assert!(!pressed && modifiers.shift);
    assert_eq!(key.text.as_deref(), Some("A"));
    fake.send(
        keyboard_id,
        keyboard::MODIFIERS,
        Args::new().uint(4).uint(128).uint(0).uint(0).uint(0),
    );
    press(&fake, 30, true);
    let Some(WindowEvent::Key { key, modifiers, .. }) = app.next() else {
        panic!("a key")
    };
    assert!(modifiers.altgr);
    assert_eq!(key.text.as_deref(), Some("ą"));
    fake.send(
        keyboard_id,
        keyboard::MODIFIERS,
        Args::new().uint(5).uint(0).uint(0).uint(0).uint(0),
    );
    press(&fake, 200, true);
    fake.send(
        keyboard_id,
        keyboard::LEAVE,
        Args::new().uint(6).object(setup.surface),
    );
    app.quiet(); // a key the layout does not have is not reported
}

#[quench_test::test]
fn an_input_method_composes_text_and_commits_it() {
    let mut globals = GLOBALS.to_vec();
    globals.push(("zwp_text_input_manager_v3", 1));
    let (mut fake, setup, app) = open_with(&globals, 100, 100);
    let seat_id = setup.seat.unwrap();
    fake.send(
        seat_id,
        seat::CAPABILITIES,
        Args::new().uint(seat::KEYBOARD),
    );
    app.quiet();
    fake.expect(seat_id, seat::GET_KEYBOARD);
    let get = fake.expect(
        setup.text_manager.unwrap(),
        text_input_manager::GET_TEXT_INPUT,
    );
    let input = ids(&get)[0];
    assert_eq!(ids(&get)[1], seat_id);
    // A text field takes focus: the input method is enabled with the text around the caret and where it is.
    app.ask(Cmd::ImeEnable("héllo wörld".into(), 6, 6, (10, 20, 1, 16)));
    fake.expect(input, text_input::ENABLE);
    let surrounding = fake.expect(input, text_input::SET_SURROUNDING_TEXT);
    let mut r = Reader::new(&surrounding.args);
    assert_eq!(
        (
            r.string().unwrap().as_str(),
            r.int().unwrap(),
            r.int().unwrap()
        ),
        ("héllo wörld", 6, 6)
    );
    fake.expect(input, text_input::SET_CONTENT_TYPE);
    let rect = fake.expect(input, text_input::SET_CURSOR_RECTANGLE);
    let mut r = Reader::new(&rect.args);
    assert_eq!(
        [
            r.int().unwrap(),
            r.int().unwrap(),
            r.int().unwrap(),
            r.int().unwrap()
        ],
        [10, 20, 1, 16]
    );
    fake.expect(input, text_input::COMMIT);
    // Composition, then the commit, arrive as one change at `done`.
    fake.send(input, text_input::ENTER, Args::new().object(setup.surface));
    fake.send(
        input,
        text_input::PREEDIT_STRING,
        Args::new().string("ni").int(0).int(2),
    );
    fake.send(input, text_input::DONE, Args::new().uint(1));
    let Some(WindowEvent::Ime(change)) = app.next() else {
        panic!("an ime change")
    };
    assert_eq!(change.preedit, Some(("ni".to_string(), 0, 2)));
    assert_eq!(change.commit, None);
    fake.send(
        input,
        text_input::DELETE_SURROUNDING_TEXT,
        Args::new().uint(1).uint(0),
    );
    fake.send(input, text_input::COMMIT_STRING, Args::new().string("你"));
    fake.send(
        input,
        text_input::PREEDIT_STRING,
        Args::new().string("").int(0).int(0),
    );
    fake.send(input, text_input::DONE, Args::new().uint(2));
    let Some(WindowEvent::Ime(change)) = app.next() else {
        panic!("an ime change")
    };
    assert_eq!((change.delete_before, change.delete_after), (1, 0));
    assert_eq!(change.commit.as_deref(), Some("你"));
    assert_eq!(change.preedit, Some((String::new(), 0, 0)));
    // The text changed, then focus left the field.
    app.ask(Cmd::ImeUpdate(
        "héllo 你wörld".into(),
        9,
        9,
        (12, 20, 1, 16),
    ));
    fake.expect(input, text_input::SET_SURROUNDING_TEXT);
    fake.expect(input, text_input::SET_CONTENT_TYPE);
    fake.expect(input, text_input::SET_CURSOR_RECTANGLE);
    fake.expect(input, text_input::COMMIT);
    app.ask(Cmd::ImeDisable);
    fake.expect(input, text_input::DISABLE);
    fake.expect(input, text_input::COMMIT);
    fake.send(input, text_input::LEAVE, Args::new().object(setup.surface));
    app.quiet();
}

#[quench_test::test]
fn the_clipboard_is_read_from_another_program_and_offered_to_others() {
    let mut globals = GLOBALS.to_vec();
    globals.push(("wl_data_device_manager", 3));
    let (mut fake, setup, app) = open_with(&globals, 100, 100);
    let seat_id = setup.seat.unwrap();
    let manager = {
        // The manager was bound in the handshake: find it through the request that uses it.
        fake.send(
            seat_id,
            seat::CAPABILITIES,
            Args::new().uint(seat::KEYBOARD),
        );
        app.quiet();
        fake.expect(seat_id, seat::GET_KEYBOARD);
        fake.read()
    };
    assert_eq!(manager.opcode, data_device_manager::GET_DATA_DEVICE);
    let manager_id = manager.object;
    let device = ids(&manager)[0];
    assert_eq!(ids(&manager)[1], seat_id);
    // Nothing is on it yet.
    assert!(matches!(app.ask(Cmd::GetClipboard), Out::Text(None)));
    // Another program copied: the offer, its types, then the selection.
    let offer = 0xff00_0001;
    fake.send(device, data_device::DATA_OFFER, Args::new().object(offer));
    fake.send(offer, data_offer::OFFER, Args::new().string("image/png"));
    fake.send(
        offer,
        data_offer::OFFER,
        Args::new().string("text/plain;charset=utf-8"),
    );
    fake.send(device, data_device::SELECTION, Args::new().object(offer));
    app.quiet();
    app.tx.send(Cmd::GetClipboard).unwrap();
    let receive = fake.expect(offer, data_offer::RECEIVE);
    assert_eq!(
        Reader::new(&receive.args).string().unwrap(),
        "text/plain;charset=utf-8"
    );
    let fd = fake.fds.pop_front().expect("the descriptor to write to");
    std::io::Write::write_all(&mut std::fs::File::from(fd), b"from elsewhere").unwrap();
    let Out::Text(text) = app.rx.recv_timeout(LONG).unwrap() else {
        panic!()
    };
    assert_eq!(text.as_deref(), Some("from elsewhere"));
    // This program copies: a source offering text, set as the selection.
    app.ask(Cmd::SetClipboard("mine".into()));
    let create = fake.expect(manager_id, data_device_manager::CREATE_DATA_SOURCE);
    let source = ids(&create)[0];
    for _ in 0..3 {
        fake.expect(source, data_source::OFFER);
    }
    let set = fake.expect(device, data_device::SET_SELECTION);
    assert_eq!(ids(&set)[0], source);
    // Another program pastes: the window writes the text to the descriptor.
    let (mut reader, writer) = std::io::pipe().unwrap();
    fake.send_with_fd(
        source,
        data_source::SEND,
        Args::new().string("text/plain"),
        &OwnedFd::from(writer),
    );
    app.quiet();
    let mut got = String::new();
    std::io::Read::read_to_string(&mut reader, &mut got).unwrap();
    assert_eq!(got, "mine");
    // And the clipboard it reads back is its own.
    let Out::Text(text) = app.ask(Cmd::GetClipboard) else {
        panic!()
    };
    assert_eq!(text.as_deref(), Some("mine"));
    // Another program took the clipboard over.
    fake.send(source, data_source::CANCELLED, Args::new());
    app.quiet();
    fake.expect(source, data_source::DESTROY);
}
