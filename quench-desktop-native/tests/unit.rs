// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used)]
#![cfg(target_os = "linux")]

use std::rc::Rc;
use std::time::Duration;

use std::cell::{Cell, RefCell};
use std::time::Instant;

use quench_desktop_native::{Controller, Host, key_event};
use quench_desktop_wayland::{ImeEvent, Key, Keymap, Modifiers, WindowEvent};
use quench_raster::Canvas;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

#[quench_test::test]
fn a_key_press_becomes_a_dom_key_event_with_its_modifiers() {
    let us = Keymap::us();
    let mods = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    let key = us.key(30, mods).unwrap();
    let event = key_event(&key, mods);
    assert_eq!(event.key, "A");
    assert!(event.ctrl && event.shift && !event.alt && !event.meta);
    let named = Key {
        name: "Enter".into(),
        text: None,
        modifier: false,
    };
    assert_eq!(key_event(&named, Modifiers::default()).key, "Enter");
}

/// Opens a window on whatever compositor is running, shows a page, and lets the loop run a few steps. Without a
/// compositor there is nothing to test: the test says so and passes.
#[quench_test::test]
fn the_loop_draws_the_page_and_keeps_running_without_input() {
    let renderer = Rc::new(NativeRenderer::new());
    renderer.add_stylesheet("body { margin: 0; background: #223 } button { display: block; width: 100px; height: 30px }");
    let _mounted = mount(&renderer, &renderer.root(), button().text("hello"));
    let mut host = match Host::open(
        "quench-desktop-native test",
        "dev.lorehaven.quench.test",
        (200, 100),
        &renderer,
    ) {
        Ok(host) => host,
        Err(e) => {
            println!("no compositor to open a window on ({e}): skipped");
            return;
        }
    };
    assert!(!host.closing());
    assert!(!renderer.take_dirty(), "the first frame was drawn");
    for _ in 0..3 {
        host.step(Duration::from_millis(50)).unwrap();
    }
    renderer.set_caret_visible(false);
    host.step(Duration::from_millis(20)).unwrap();
    // A focused text field enables the input method (when the compositor has one); leaving it disables it.
    let _field = mount(
        &renderer,
        &renderer.root(),
        input().attr("type", "text").attr("value", "ab"),
    );
    host.step(Duration::from_millis(20)).unwrap();
    renderer.pointer_down(5.0, 40.0);
    renderer.pointer_up(5.0, 40.0);
    host.step(Duration::from_millis(20)).unwrap();
    renderer.text("c");
    host.step(Duration::from_millis(20)).unwrap();
    renderer.pointer_down(190.0, 95.0);
    renderer.pointer_up(190.0, 95.0);
    host.step(Duration::from_millis(20)).unwrap();
    let _ = host.has_accessibility();
    assert!(!renderer.take_dirty(), "a change was drawn by the step");
    assert!(format!("{host:?}").contains("Host"));
}

fn press(map: &Keymap, code: u32, mods: Modifiers, down: bool) -> WindowEvent {
    WindowEvent::Key {
        code,
        pressed: down,
        key: map.key(code, mods).unwrap(),
        modifiers: mods,
    }
}

#[quench_test::test]
fn window_events_drive_a_page_the_way_a_user_would() {
    let clicks = Rc::new(Cell::new(0));
    let typed = Rc::new(RefCell::new(String::new()));
    let (c, t) = (clicks.clone(), typed.clone());
    let view = div()
        .child(button().text("go").on_click(move || c.set(c.get() + 1)))
        .child(
            input()
                .attr("type", "text")
                .on_input(move |v| *t.borrow_mut() = v.to_string()),
        );
    let renderer = Rc::new(NativeRenderer::new());
    renderer.add_stylesheet("body { margin: 0 } button, input { display: block; box-sizing: border-box; width: 100px; height: 30px; margin: 0; padding: 0; border: 0 }");
    let _m = mount(&renderer, &renderer.root(), view);
    let mut canvas = Canvas::new(200, 100);
    renderer.render(&mut canvas);
    let mut control = Controller::new(&renderer, (200, 100));
    assert_eq!(control.size(), (200, 100));
    let us = Keymap::us();
    let none = Modifiers::default();

    control.handle(WindowEvent::PointerEntered { x: 10.0, y: 10.0 });
    control.handle(WindowEvent::PointerButton {
        x: 10.0,
        y: 10.0,
        button: 0x110,
        pressed: true,
    });
    control.handle(WindowEvent::PointerButton {
        x: 10.0,
        y: 10.0,
        button: 0x110,
        pressed: false,
    });
    assert_eq!(clicks.get(), 1, "a click");
    control.handle(WindowEvent::PointerButton {
        x: 10.0,
        y: 10.0,
        button: 0x111,
        pressed: true,
    });
    control.handle(WindowEvent::PointerButton {
        x: 10.0,
        y: 10.0,
        button: 0x111,
        pressed: false,
    });
    assert_eq!(clicks.get(), 1, "only the primary button clicks");
    // Tab to the field, type, correct.
    control.handle(press(&us, 15, none, true));
    control.handle(press(&us, 15, none, false));
    assert_eq!(
        renderer.tag(renderer.focused().unwrap()).as_deref(),
        Some("input")
    );
    for code in [35, 23] {
        control.handle(press(&us, code, none, true));
        control.handle(press(&us, code, none, false));
    }
    assert_eq!(*typed.borrow(), "hi");
    let shift = Modifiers {
        shift: true,
        ..none
    };
    control.handle(press(&us, 42, shift, true));
    control.handle(press(&us, 30, shift, true));
    control.handle(press(&us, 30, shift, false));
    assert_eq!(
        *typed.borrow(),
        "hiA",
        "shift gives the capital; the shift key alone types nothing"
    );
    // Held keys repeat after the delay, at the rate; releasing stops it.
    control.set_repeat((100, 1));
    control.handle(press(&us, 48, none, true));
    assert_eq!(*typed.borrow(), "hiAb");
    std::thread::sleep(std::time::Duration::from_millis(30));
    control.timers(Instant::now());
    assert!(typed.borrow().len() > 4, "repeated: {}", typed.borrow());
    control.handle(press(&us, 48, none, false));
    let len = typed.borrow().len();
    std::thread::sleep(std::time::Duration::from_millis(30));
    control.timers(Instant::now());
    assert_eq!(typed.borrow().len(), len, "released");
    // Ctrl+A then a letter does not type the letter (it is a shortcut).
    let ctrl = Modifiers { ctrl: true, ..none };
    control.handle(press(&us, 30, ctrl, true));
    control.handle(press(&us, 30, ctrl, false));
    assert_eq!(typed.borrow().len(), len);
    // The wheel, a resize and the caret's blink reach the renderer; closing is remembered.
    control.handle(WindowEvent::Scroll {
        axis: 0,
        value: 10.0,
    });
    control.handle(WindowEvent::Scroll {
        axis: 1,
        value: 10.0,
    });
    control.handle(WindowEvent::PointerLeft);
    control.handle(WindowEvent::Resized {
        width: 300,
        height: 150,
    });
    assert_eq!(control.size(), (300, 150));
    renderer.take_dirty();
    control.timers(control.next_timer() + std::time::Duration::from_millis(1));
    assert!(renderer.take_dirty(), "the caret blinked");
    assert!(!control.closing());
    control.handle(WindowEvent::CloseRequested);
    assert!(control.closing());
    control.handle(WindowEvent::Frame);
    assert!(format!("{control:?}").contains("Controller"));
    // The input method is told when a field has focus, when its text changes, and when focus leaves it.
    let mut control = Controller::new(&renderer, (200, 100));
    let Some(Some(state)) = control.ime_change() else {
        panic!("the field still has focus")
    };
    assert!(state.text.starts_with("hiA"));
    assert!(control.ime_enabled());
    assert_eq!(control.ime_change(), None, "told once");
    control.handle(WindowEvent::Ime(ImeEvent {
        commit: Some("é".into()),
        ..ImeEvent::default()
    }));
    let Some(Some(after)) = control.ime_change() else {
        panic!("the text changed")
    };
    assert!(after.text.ends_with('é'), "{}", after.text);
    control.handle(WindowEvent::PointerButton {
        x: 150.0,
        y: 90.0,
        button: 0x110,
        pressed: true,
    });
    control.handle(WindowEvent::PointerButton {
        x: 150.0,
        y: 90.0,
        button: 0x110,
        pressed: false,
    });
    assert_eq!(control.ime_change(), Some(None), "focus left the field");
    assert!(!control.ime_enabled());
}

quench_test::main!();
