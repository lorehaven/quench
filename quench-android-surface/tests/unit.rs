// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use std::cell::Cell;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::rc::Rc;

use quench_android_surface::proto::{Event, Out, encode_event, encode_tree, read_event, write_out};
use quench_android_surface::{Bridge, Surface, serve};
use quench_serde::Value;
use quench_ui::prelude::*;
use quench_ui_native::{AccessNode, NativeRenderer, Role};

#[quench_test::test]
fn every_event_survives_the_wire() {
    let events = [
        Event::Resize {
            width: 1080,
            height: 2400,
            density: 2.625,
            insets: [63.0, 0.0, 126.0, 0.0],
        },
        Event::Touch {
            action: 1,
            x: 10.5,
            y: 20.25,
        },
        Event::Key {
            down: true,
            mods: 5,
            name: "ArrowLeft".into(),
        },
        Event::Text("zażółć".into()),
        Event::Compose {
            text: "ni".into(),
            cursor: 2,
        },
        Event::Compose {
            text: String::new(),
            cursor: -1,
        },
        Event::Delete {
            before: 1,
            after: 2,
        },
        Event::Access { id: 77, action: 1 },
        Event::Scheme { dark: false },
        Event::Back,
        Event::Visible(false),
        Event::Clipboard("copied \u{1f600}".into()),
        Event::Reply("{\"id\":1,\"ok\":null}".into()),
    ];
    for e in &events {
        let bytes = encode_event(e);
        assert_eq!(read_event(&mut &bytes[..]).unwrap().as_ref(), Some(e));
    }
    assert_eq!(
        read_event(&mut &b""[..]).unwrap(),
        None,
        "a closed pipe is the end"
    );
}

#[quench_test::test]
fn malformed_messages_are_errors() {
    assert!(read_event(&mut &[0u8, 0, 0, 0][..]).is_err(), "empty");
    assert!(
        read_event(&mut &[0xff, 0xff, 0xff, 0xff, 1][..]).is_err(),
        "too long"
    );
    assert!(
        read_event(&mut &[0u8, 0, 0, 1, 99][..]).is_err(),
        "unknown kind"
    );
    assert!(
        read_event(&mut &[0u8, 0, 0, 2, 2, 0][..]).is_err(),
        "a touch without its position"
    );
    assert!(
        read_event(&mut &[0u8, 0, 0, 6, 4, 0, 3, 0xff, 0xfe, 0xfd][..]).is_err(),
        "text that is not UTF-8"
    );
    assert!(
        read_event(&mut &[0u8, 0, 0, 5, 4, 0, 9, b'a'][..]).is_err(),
        "a string longer than the message"
    );
}

/// What the program wrote, split into messages: `(kind, payload)`.
fn messages(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        out.push((bytes[at + 4], bytes[at + 5..at + 4 + n].to_vec()));
        at += 4 + n;
    }
    out
}

fn kinds(bytes: &[u8]) -> Vec<u8> {
    messages(bytes).iter().map(|m| m.0).collect()
}

/// A buffer the program writes to and the test can look at meanwhile.
#[derive(Clone, Default)]
struct Sink(Rc<std::cell::RefCell<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn page(view: impl IntoView) -> Rc<NativeRenderer> {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet("body { margin: 0 } button, input { display: block; box-sizing: border-box; width: 100px; height: 30px; margin: 0; padding: 0; border: 0; background: white; font-size: 16px } @media (prefers-color-scheme: light) { body { background: white } } @media (hover: none) { button { background: #eee } }");
    std::mem::forget(mount(&r, &r.root(), view));
    r
}

const fn resize(width: u32, height: u32, density: f32) -> Event {
    Event::Resize {
        width,
        height,
        density,
        insets: [0.0; 4],
    }
}

#[quench_test::test]
fn a_frame_goes_out_as_rgba_when_something_changed() {
    let r = page(
        div()
            .style("background", "rgb(255, 0, 0)")
            .style("height", "20px"),
    );
    let mut surface = Surface::new(&r, Vec::new());
    surface.flush().unwrap();
    // Without a size there is nothing to draw.
    assert!(!format!("{surface:?}").is_empty());
    surface.handle(resize(40, 30, 1.0));
    let mut bytes = Vec::new();
    {
        let mut s = Surface::new(&r, &mut bytes);
        s.handle(resize(40, 30, 1.0));
        s.flush().unwrap();
        s.flush().unwrap();
    }
    let all = messages(&bytes);
    assert_eq!(all[0].0, 1, "a frame");
    assert_eq!(
        all.iter().filter(|m| m.0 == 1).count(),
        1,
        "the second flush found nothing new"
    );
    let frame = &all[0].1;
    assert_eq!(u32::from_be_bytes(frame[0..4].try_into().unwrap()), 40);
    assert_eq!(u32::from_be_bytes(frame[4..8].try_into().unwrap()), 30);
    let pixels = &frame[8..];
    assert_eq!(pixels.len(), 40 * 30 * 4);
    assert_eq!(&pixels[..4], &[255, 0, 0, 255], "red, in RGBA order");
    assert_eq!(kinds(&bytes).last(), Some(&4), "and the accessibility tree");
}

#[quench_test::test]
fn a_tap_clicks_and_a_drag_scrolls_and_does_not() {
    let taps = Rc::new(Cell::new(0));
    let t = taps.clone();
    let view = div()
        .child(button().text("go").on_click(move || t.set(t.get() + 1)))
        .child(div().style("height", "800px"));
    let r = page(view);
    let mut surface = Surface::new(&r, Vec::new());
    surface.handle(resize(200, 100, 1.0));
    let touch = |s: &mut Surface<Vec<u8>>, a: u8, x: f32, y: f32| {
        s.handle(Event::Touch { action: a, x, y });
    };
    touch(&mut surface, 0, 10.0, 10.0);
    touch(&mut surface, 2, 10.0, 10.0);
    assert_eq!(taps.get(), 1, "a tap");
    // A drag beyond the slop scrolls the page and is not a tap.
    touch(&mut surface, 0, 10.0, 10.0);
    touch(&mut surface, 1, 10.0, 25.0);
    touch(&mut surface, 1, 10.0, 60.0);
    touch(&mut surface, 2, 10.0, 60.0);
    assert_eq!(taps.get(), 1);
    assert!(
        r.scroll_of(0).1.abs() < f32::EPSILON || r.scroll_of(0).1 <= 0.0,
        "dragging down scrolls up: already at the top"
    );
    touch(&mut surface, 0, 10.0, 80.0);
    touch(&mut surface, 1, 10.0, 40.0);
    touch(&mut surface, 1, 10.0, 20.0);
    touch(&mut surface, 2, 10.0, 20.0);
    assert!(
        r.scroll_of(0).1 > 50.0,
        "dragging up scrolls down: {:?}",
        r.scroll_of(0)
    );
    // A cancelled touch clicks nothing and leaves no hover.
    touch(&mut surface, 0, 10.0, 10.0);
    touch(&mut surface, 3, 10.0, 10.0);
    touch(&mut surface, 1, 5.0, 5.0);
    assert_eq!(taps.get(), 1);
}

#[quench_test::test]
fn a_fast_flick_keeps_the_page_moving_and_slows_it_and_a_touch_stops_it() {
    let r = page(div().style("height", "5000px"));
    let mut surface = Surface::new(&r, Vec::new());
    surface.handle(resize(200, 100, 1.0));
    let touch = |s: &mut Surface<Vec<u8>>, a: u8, y: f32| {
        s.handle(Event::Touch {
            action: a,
            x: 50.0,
            y,
        });
    };
    let flick = |s: &mut Surface<Vec<u8>>, from: f32, to: f32| {
        touch(s, 0, from);
        for step in 1..=4 {
            std::thread::sleep(std::time::Duration::from_millis(8));
            touch(s, 1, from + (to - from) * step as f32 / 4.0);
        }
        touch(s, 2, to);
    };
    flick(&mut surface, 90.0, 10.0);
    let released = r.scroll_of(0).1;
    assert!(released > 50.0, "the drag itself scrolled: {released}");
    // The page keeps going, by less and less, and stops.
    let mut now = std::time::Instant::now();
    let mut moved = Vec::new();
    for _ in 0..400 {
        let before = r.scroll_of(0).1;
        now += std::time::Duration::from_millis(16);
        surface.timers(now);
        moved.push(r.scroll_of(0).1 - before);
    }
    assert!(moved[0] > 0.0, "{:?}", &moved[..4]);
    assert!(moved[10] < moved[0], "slowing down");
    assert!(
        moved.last().copied().unwrap_or(1.0).abs() < f32::EPSILON,
        "it stopped"
    );
    assert!(
        r.scroll_of(0).1 > released + 100.0,
        "a flick goes well past where the finger left it"
    );
    // A slow drag leaves the page where it is, and a touch on a moving page stops it.
    let still = r.scroll_of(0).1;
    touch(&mut surface, 0, 90.0);
    std::thread::sleep(std::time::Duration::from_millis(80));
    touch(&mut surface, 1, 70.0);
    std::thread::sleep(std::time::Duration::from_millis(80));
    touch(&mut surface, 1, 60.0);
    touch(&mut surface, 2, 60.0);
    let dragged = r.scroll_of(0).1;
    assert!(dragged > still);
    now += std::time::Duration::from_millis(200);
    surface.timers(now);
    assert!(
        (r.scroll_of(0).1 - dragged).abs() < f32::EPSILON,
        "too slow to fling"
    );
    flick(&mut surface, 90.0, 10.0);
    touch(&mut surface, 0, 50.0);
    let held = r.scroll_of(0).1;
    now += std::time::Duration::from_millis(100);
    surface.timers(now);
    assert!(
        (r.scroll_of(0).1 - held).abs() < f32::EPSILON,
        "a finger on the page stops it"
    );
}

#[quench_test::test]
fn density_makes_the_slop_and_the_page_bigger() {
    let r = page(div().style("height", "30px").style("background", "red"));
    let mut surface = Surface::new(&r, Vec::new());
    surface.handle(resize(300, 200, 3.0));
    let boxes = r.layout();
    assert_eq!(
        boxes.children[0].children[0].height, 90.0,
        "30 CSS pixels at 3x"
    );
    // On a coarse pointer without hover the media queries say so.
    let probe = page(button().text("x"));
    let mut s = Surface::new(&probe, Vec::new());
    s.handle(resize(200, 100, 1.0));
    let b = probe.layout();
    assert_eq!(
        probe.layout().children[0].children[0]
            .style
            .resolve(b.children[0].children[0].style.background),
        quench_css::Color::rgba(238, 238, 238, 255),
        "hover: none applies"
    );
}

#[quench_test::test]
fn keys_and_text_reach_the_focused_field_and_the_soft_keyboard_follows_focus() {
    let typed = Rc::new(std::cell::RefCell::new(String::new()));
    let t = typed.clone();
    let r = page(
        input()
            .attr("type", "text")
            .on_input(move |v| *t.borrow_mut() = v.to_string()),
    );
    let mut bytes = Vec::new();
    let mut s = Surface::new(&r, &mut bytes);
    s.handle(resize(200, 100, 1.0));
    s.flush().unwrap();
    s.handle(Event::Touch {
        action: 0,
        x: 10.0,
        y: 10.0,
    });
    s.handle(Event::Touch {
        action: 2,
        x: 10.0,
        y: 10.0,
    });
    s.flush().unwrap();
    s.handle(Event::Key {
        down: true,
        mods: 0,
        name: "a".into(),
    });
    s.handle(Event::Key {
        down: false,
        mods: 0,
        name: "a".into(),
    });
    s.handle(Event::Key {
        down: true,
        mods: 1,
        name: "B".into(),
    });
    s.handle(Event::Key {
        down: true,
        mods: 2,
        name: "c".into(),
    });
    assert_eq!(*typed.borrow(), "aB", "a shortcut is not text");
    s.handle(Event::Text("é".into()));
    assert_eq!(*typed.borrow(), "aBé");
    s.handle(Event::Compose {
        text: "ni".into(),
        cursor: 2,
    });
    s.handle(Event::Compose {
        text: String::new(),
        cursor: -1,
    });
    s.handle(Event::Delete {
        before: 1,
        after: 0,
    });
    assert_eq!(
        *typed.borrow(),
        "aB",
        "delete counts UTF-16 units before the caret"
    );
    s.flush().unwrap();
    drop(s);
    let all = messages(&bytes);
    let keyboard: Vec<_> = all.iter().filter(|m| m.0 == 2).map(|m| m.1[0]).collect();
    assert_eq!(
        keyboard,
        [1],
        "the keyboard came up when the field got focus"
    );
    let ime = all.iter().rev().find(|m| m.0 == 3).unwrap();
    let n = u32::from_be_bytes(ime.1[0..4].try_into().unwrap()) as usize;
    assert_eq!(&ime.1[4..4 + n], b"aB");
    assert_eq!(
        i32::from_be_bytes(ime.1[4 + n..8 + n].try_into().unwrap()),
        2,
        "the caret, in UTF-16 units"
    );
}

#[quench_test::test]
fn back_is_used_by_the_page_or_leaves() {
    let dialog = Rc::new(Cell::new(true));
    let d = dialog;
    let view = div().on(quench_ui::EventKind::KeyDown, move |e| {
        if let quench_ui::Event::KeyDown(k) = e
            && k.key == "Escape"
            && d.get()
        {
            d.set(false);
            quench_ui::event::prevent_default();
        }
    });
    let r = page(view.child(button().text("x")));
    let mut bytes = Vec::new();
    let mut s = Surface::new(&r, &mut bytes);
    s.handle(resize(200, 100, 1.0));
    s.flush().unwrap();
    // Focus the page so the key goes into it.
    s.handle(Event::Touch {
        action: 0,
        x: 10.0,
        y: 10.0,
    });
    s.handle(Event::Touch {
        action: 2,
        x: 10.0,
        y: 10.0,
    });
    s.handle(Event::Back);
    drop(s);
    assert!(
        kinds(&bytes).contains(&5),
        "the page handled it (a dialog closed): {:?}",
        kinds(&bytes)
    );
}

#[quench_test::test]
fn accessibility_actions_scheme_and_visibility() {
    let taps = Rc::new(Cell::new(0));
    let t = taps.clone();
    let r = page(button().text("go").on_click(move || t.set(t.get() + 1)));
    let sink = Sink::default();
    let mut s = Surface::new(&r, sink.clone());
    s.handle(resize(200, 100, 1.0));
    s.flush().unwrap();
    let tree = r.accessibility_tree();
    let id = tree.children[0].id;
    s.handle(Event::Access {
        id: id as u32,
        action: 0,
    });
    assert_eq!(taps.get(), 1, "a click from a screen reader");
    s.handle(Event::Access {
        id: id as u32,
        action: 1,
    });
    s.handle(Event::Access {
        id: id as u32,
        action: 99,
    });
    s.handle(Event::Scheme { dark: false });
    s.handle(Event::Visible(false));
    let before = sink.0.borrow().len();
    s.flush().unwrap();
    assert_eq!(
        sink.0.borrow().len(),
        before,
        "nothing is drawn while the app is in the background"
    );
    s.handle(Event::Visible(true));
    s.flush().unwrap();
    assert!(sink.0.borrow().len() > before);
    s.timers(s.next_timer() + std::time::Duration::from_millis(1));
    assert!(!s.finished());
}

fn node(id: usize, role: Role, name: &str, children: Vec<AccessNode>) -> AccessNode {
    AccessNode {
        id,
        role,
        level: 0,
        name: name.into(),
        description: "d".into(),
        value: Some("v".into()),
        bounds: (1.0, 2.0, 3.0, 4.0),
        focusable: true,
        focused: false,
        disabled: false,
        checked: Some(true),
        selected: None,
        expanded: Some(false),
        required: true,
        read_only: false,
        caret: None,
        range: None,
        actions: vec![quench_ui_native::AccessAction::Click],
        children,
    }
}

#[quench_test::test]
fn the_tree_is_flattened_with_parents_flags_and_strings() {
    let tree = node(
        0,
        Role::Document,
        "",
        vec![node(
            5,
            Role::Button,
            "Save",
            vec![node(6, Role::Text, &"é".repeat(3000), Vec::new())],
        )],
    );
    let bytes = encode_tree(&tree);
    assert_eq!(u32::from_be_bytes(bytes[0..4].try_into().unwrap()), 3);
    // The second node: id 5, parent 0, role Button (4).
    let first_len = 4 + 4 + 1 + 2 + 1 + 16 + 2 + (2 + 1) + (2 + 1) + 1;
    let second = &bytes[4 + first_len..];
    assert_eq!(u32::from_be_bytes(second[0..4].try_into().unwrap()), 5);
    assert_eq!(u32::from_be_bytes(second[4..8].try_into().unwrap()), 0);
    assert_eq!(second[8], 4);
    let flags = u16::from_be_bytes(second[9..11].try_into().unwrap());
    assert!(
        flags & 1 != 0
            && flags & (1 << 3) != 0
            && flags & (1 << 4) != 0
            && flags & (1 << 7) != 0
            && flags & (1 << 8) == 0
            && flags & (1 << 9) != 0
            && flags & (1 << 11) != 0,
        "{flags:b}"
    );
    // A slider carries its range after its strings: a flag, then minimum, maximum and current (NaN when unknown).
    let mut slider = node(1, Role::Slider, "Volume", Vec::new());
    slider.range = Some((0.0, 10.0, Some(4.0)));
    let bytes = encode_tree(&slider);
    let flag_at = 4 + 4 + 4 + 1 + 2 + 1 + 16 + (2 + 6) + (2 + 1) + (2 + 1);
    assert_eq!(bytes[flag_at], 1);
    let floats: Vec<f32> = (0..3)
        .map(|i| {
            f32::from_be_bytes(
                bytes[flag_at + 1 + 4 * i..flag_at + 5 + 4 * i]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(floats, [0.0, 10.0, 4.0]);
    slider.range = Some((0.0, 1.0, None));
    let bytes = encode_tree(&slider);
    assert!(f32::from_be_bytes(bytes[flag_at + 9..flag_at + 13].try_into().unwrap()).is_nan());
    // A name too long is cut at a character boundary.
    let last = encode_tree(&node(1, Role::Text, &"é".repeat(5000), Vec::new()));
    assert!(
        std::str::from_utf8(&last[4 + 4 + 4 + 1 + 2 + 1 + 16 + 2 - 4..]).is_err()
            || last.len() < 9000
    );
}

#[quench_test::test]
fn outgoing_messages_have_their_length_and_kind() {
    let mut out = Vec::new();
    write_out(&mut out, &Out::Keyboard(true)).unwrap();
    write_out(&mut out, &Out::Exit).unwrap();
    write_out(&mut out, &Out::BackHandled).unwrap();
    write_out(
        &mut out,
        &Out::Ime {
            text: "a",
            cursor: 1,
            anchor: 1,
            rect: [1.0, 2.0, 3.0, 4.0],
        },
    )
    .unwrap();
    assert_eq!(kinds(&out), [2, 6, 5, 3]);
}

#[quench_test::test]
fn the_program_runs_over_a_pair_of_streams_until_the_host_closes_them() {
    let (host, program_in) = UnixStream::pair().unwrap();
    let (program_out, mut host_in) = UnixStream::pair().unwrap();
    let r = page(div().style("background", "blue").style("height", "10px"));
    let handle = {
        let (input, output) = (program_in, program_out);
        // `serve` needs the renderer on its own thread: run it here and drive the host from another.
        let writer = std::thread::spawn(move || {
            let mut host = host;
            host.write_all(&encode_event(&resize(20, 10, 1.0))).unwrap();
            host.write_all(&encode_event(&Event::Touch {
                action: 0,
                x: 1.0,
                y: 1.0,
            }))
            .unwrap();
            host.write_all(&encode_event(&Event::Touch {
                action: 2,
                x: 1.0,
                y: 1.0,
            }))
            .unwrap();
            host.write_all(&encode_event(&Event::Back)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            // Closing the pipe ends the program.
        });
        let result = serve(&r, input, output);
        writer.join().unwrap();
        result
    };
    handle.unwrap();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut host_in, &mut bytes).unwrap();
    let k = kinds(&bytes);
    assert!(
        k.contains(&1) && k.contains(&6),
        "a frame, and an exit for the back that nothing used: {k:?}"
    );
}

#[quench_test::test]
fn calls_to_the_hosts_plugins_go_out_and_their_answers_and_events_come_back() {
    let r = Rc::new(NativeRenderer::new());
    let bridge = Bridge::new();
    let mut bytes = Vec::new();
    let got = Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let mut s = Surface::with_bridge(&r, &mut bytes, bridge.clone());
        let (a, b, ev) = (got.clone(), got.clone(), got.clone());
        bridge.call("device", "state", Value::Null, move |r| {
            a.borrow_mut().push(format!("{r:?}"));
        });
        bridge.call("nope", "x", Value::Null, move |r| {
            b.borrow_mut().push(r.unwrap_err().to_string());
        });
        bridge.on("device", "changed", move |data| {
            ev.borrow_mut().push(format!("event {data:?}"));
        });
        s.flush().unwrap();
        s.handle(Event::Reply("{\"id\":1,\"ok\":{\"battery\":80}}".into()));
        s.handle(Event::Reply(
            "{\"id\":2,\"err\":{\"code\":\"unknown_plugin\",\"message\":\"no plugin\"}}".into(),
        ));
        s.handle(Event::Reply("{\"id\":2,\"ok\":1}".into()));
        s.handle(Event::Reply(
            "{\"event\":\"changed\",\"plugin\":\"device\",\"data\":{\"online\":true}}".into(),
        ));
        s.handle(Event::Reply("not json".into()));
    }
    let k = kinds(&bytes);
    let calls: usize = k.iter().map(|k| usize::from(*k == 8)).sum();
    assert_eq!(calls, 2, "two calls: {k:?}");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("\"plugin\":\"device\"") && text.contains("\"method\":\"state\""));
    let got = got.borrow();
    assert!(got[0].contains("battery"), "{got:?}");
    assert_eq!(got[1], "unknown_plugin: no plugin", "answered once");
    assert!(got[2].contains("online"), "{got:?}");
    assert_eq!(got.len(), 3);
}

#[quench_test::test]
fn the_clipboard_goes_both_ways() {
    let r = page(input());
    let sink = Sink::default();
    let mut s = Surface::new(&r, sink.clone());
    s.handle(resize(200, 100, 1.0));
    s.flush().unwrap();
    for action in [0, 2] {
        s.handle(Event::Touch {
            action,
            x: 10.0,
            y: 10.0,
        });
    }
    s.handle(Event::Text("hello".into()));
    let key = |mods, name: &str| Event::Key {
        down: true,
        mods,
        name: name.into(),
    };
    s.handle(key(2, "a"));
    s.handle(key(2, "c"));
    s.flush().unwrap();
    let sent = messages(&sink.0.borrow());
    let copy = sent
        .iter()
        .find(|m| m.0 == 7)
        .expect("a copy goes to the system");
    assert!(String::from_utf8_lossy(&copy.1).contains("hello"));
    // The system's clipboard is what a paste inserts, and is not sent back.
    s.handle(Event::Clipboard("PASTED".into()));
    s.handle(key(2, "v"));
    s.flush().unwrap();
    assert_eq!(
        messages(&sink.0.borrow())
            .iter()
            .filter(|m| m.0 == 7)
            .count(),
        1
    );
}

#[quench_test::test]
fn a_long_press_selects_a_word_and_handles_move_its_ends() {
    let r = page(input().attr("type", "text").attr("value", "ab cd ef"));
    let sink = Sink::default();
    let mut s = Surface::new(&r, sink.clone());
    s.handle(resize(200, 100, 1.0));
    s.flush().unwrap();
    let touch =
        |s: &mut Surface<Sink>, a: u8, x: f32, y: f32| s.handle(Event::Touch { action: a, x, y });
    // A short press is a tap: the timer does nothing and no selection appears.
    touch(&mut s, 0, 6.0, 12.0);
    s.timers(std::time::Instant::now());
    touch(&mut s, 2, 6.0, 12.0);
    assert!(r.selection_handles().is_none());
    // Held for half a second, it selects the word and lets go without clicking.
    touch(&mut s, 0, 6.0, 12.0);
    assert!(s.next_timer() <= std::time::Instant::now() + std::time::Duration::from_millis(600));
    s.timers(std::time::Instant::now() + std::time::Duration::from_millis(600));
    touch(&mut s, 2, 6.0, 12.0);
    let handles = r.selection_handles().expect("a word is selected");
    assert!(handles[0].0 < handles[1].0);
    let before = sink.0.borrow().len();
    s.flush().unwrap();
    assert!(
        sink.0.borrow().len() > before,
        "the handles are drawn in a new frame"
    );
    // A finger on the end handle's knob drags that end to the right.
    let (kx, ky) = (handles[1].0, handles[1].1 + handles[1].2 + 9.0);
    touch(&mut s, 0, kx, ky);
    touch(&mut s, 1, kx + 40.0, ky);
    touch(&mut s, 2, kx + 40.0, ky);
    let moved = r.selection_handles().unwrap();
    assert!(moved[1].0 > handles[1].0 + 20.0, "{handles:?} -> {moved:?}");
    assert!((moved[0].0 - handles[0].0).abs() < 0.5, "the start stayed");
    // Now the start handle.
    let (sx, sy) = (moved[0].0, moved[0].1 + moved[0].2 + 9.0);
    touch(&mut s, 0, sx, sy);
    touch(&mut s, 1, sx - 4.0, sy);
    touch(&mut s, 3, sx - 4.0, sy);
    assert!(r.selection_handles().is_some());
    // A finger far from the handles that moves is a scroll, as before, and the timer leaves it alone.
    touch(&mut s, 0, 150.0, 80.0);
    touch(&mut s, 1, 150.0, 40.0);
    s.timers(std::time::Instant::now() + std::time::Duration::from_secs(2));
    touch(&mut s, 2, 150.0, 40.0);
}

quench_test::main!();
