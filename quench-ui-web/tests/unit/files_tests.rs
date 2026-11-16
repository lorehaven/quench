//! Picking files, saving downloads, and the clipboard.

use quench_ui::platform::{self, PickedFile};
use quench_ui::prelude::*;
use quench_ui_web::mock;
use quench_ui_web::{WebPlatform, decode_event, kind_code, q_alloc, q_dispatch_event};
use std::cell::RefCell;
use std::rc::Rc;

fn web() {
    mock::reset();
    platform::install(Rc::new(WebPlatform::new()));
}

#[quench_test::test]
fn files_dropped_on_the_page_reach_the_handler_once_all_have_arrived() {
    web();
    let got: Rc<RefCell<Vec<Vec<PickedFile>>>> = Rc::default();
    let sink = got.clone();
    platform::on_drop(move |files| sink.borrow_mut().push(files));
    // The browser reports a drop as a pick with the largest id.
    mock::answer_file_pick(
        u32::MAX,
        &[("a.xlsx", "", b"1"), ("b.csv", "text/csv", b"2")],
    );
    let all = got.borrow().clone();
    assert_eq!(all.len(), 1, "one call for the whole drop");
    assert_eq!(
        all[0].iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["a.xlsx", "b.csv"]
    );
    platform::on_drop(|_| {});
    mock::answer_file_pick(u32::MAX, &[("c", "", b"3")]);
    assert_eq!(
        got.borrow().len(),
        1,
        "a later handler replaces the earlier one"
    );
}

#[quench_test::test]
fn a_picker_opens_in_the_browser_and_gets_the_files_one_by_one() {
    web();
    let got: Rc<RefCell<Option<Vec<PickedFile>>>> = Rc::default();
    let sink = got.clone();
    platform::pick_files(".xlsx,.csv", true, move |files| {
        *sink.borrow_mut() = Some(files);
    });
    let picks = mock::file_picks();
    assert_eq!(picks.len(), 1);
    assert_eq!((picks[0].1.as_str(), picks[0].2), (".xlsx,.csv", true));
    assert!(got.borrow().is_none(), "nothing until the person answers");
    let id = picks[0].0;
    mock::answer_file_pick(
        id,
        &[("a.csv", "text/csv", b"1,2"), ("b.bin", "", &[0, 255, 7])],
    );
    let files = got.borrow().clone().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files[0],
        PickedFile {
            name: "a.csv".into(),
            mime: "text/csv".into(),
            bytes: b"1,2".to_vec()
        }
    );
    assert_eq!(files[1].bytes, vec![0, 255, 7], "bytes, not text");
    assert_eq!(quench_ui_web::pending_picks(), 0);
}

#[quench_test::test]
fn the_file_arrives_only_when_all_of_them_have() {
    web();
    let got: Rc<RefCell<Option<Vec<PickedFile>>>> = Rc::default();
    let sink = got.clone();
    platform::pick_files("", true, move |files| {
        *sink.borrow_mut() = Some(files);
    });
    let id = mock::file_picks()[0].0;
    let (np, nl) = mock::alloc_copy(b"one");
    quench_ui_web::q_file_picked(id, 0, 2, np, nl, 0, 0, 0, 0);
    assert!(got.borrow().is_none(), "one of two");
    assert_eq!(quench_ui_web::pending_picks(), 1);
    let (np, nl) = mock::alloc_copy(b"two");
    quench_ui_web::q_file_picked(id, 1, 2, np, nl, 0, 0, 0, 0);
    assert_eq!(got.borrow().as_ref().unwrap().len(), 2);
}

#[quench_test::test]
fn cancelling_gives_no_files_and_stray_answers_are_ignored() {
    web();
    let got: Rc<RefCell<Option<Vec<PickedFile>>>> = Rc::default();
    let sink = got.clone();
    platform::pick_files("", false, move |files| *sink.borrow_mut() = Some(files));
    let id = mock::file_picks()[0].0;
    mock::answer_file_pick(id, &[]);
    assert_eq!(got.borrow().clone(), Some(Vec::new()));
    // Nobody is waiting for these now, or ever.
    mock::answer_file_pick(id, &[("late", "", b"x")]);
    mock::answer_file_pick(999, &[("never asked", "", b"x")]);
    mock::answer_file_pick(999, &[]);
    assert_eq!(quench_ui_web::pending_picks(), 0);
}

#[quench_test::test]
fn two_pickers_do_not_mix_their_answers() {
    web();
    let first: Rc<RefCell<Option<Vec<PickedFile>>>> = Rc::default();
    let second: Rc<RefCell<Option<Vec<PickedFile>>>> = Rc::default();
    let (a, b) = (first.clone(), second.clone());
    platform::pick_files("", false, move |f| *a.borrow_mut() = Some(f));
    platform::pick_files("", false, move |f| *b.borrow_mut() = Some(f));
    let ids: Vec<u32> = mock::file_picks().iter().map(|p| p.0).collect();
    assert_ne!(ids[0], ids[1]);
    mock::answer_file_pick(ids[1], &[("two", "", b"2")]);
    assert!(first.borrow().is_none());
    assert_eq!(second.borrow().as_ref().unwrap()[0].name, "two");
    mock::answer_file_pick(ids[0], &[("one", "", b"1")]);
    assert_eq!(first.borrow().as_ref().unwrap()[0].name, "one");
}

#[quench_test::test]
fn a_download_reaches_the_browser_with_its_bytes() {
    web();
    platform::download("book.xlsx", "application/zip", &[80, 75, 3, 4, 0, 255]);
    assert_eq!(
        mock::downloads(),
        vec![(
            "book.xlsx".to_string(),
            "application/zip".to_string(),
            vec![80, 75, 3, 4, 0, 255]
        )]
    );
}

#[quench_test::test]
fn clipboard_events_are_decoded_from_the_page() {
    let copy = kind_code(EventKind::Copy);
    assert_eq!(
        decode_event(copy, String::new(), 0.0, 0.0, 0, 0),
        Some(Event::Copy)
    );
    let cut = kind_code(EventKind::Cut);
    assert_eq!(
        decode_event(cut, String::new(), 0.0, 0.0, 0, 0),
        Some(Event::Cut)
    );
    let paste = kind_code(EventKind::Paste);
    let json = r#"{"text":"a\tb\n1\t2","html":"<table><tr><td>a</td></tr></table>"}"#;
    assert_eq!(
        decode_event(paste, json.to_string(), 0.0, 0.0, 0, 0),
        Some(Event::Paste(PasteEvent {
            text: "a\tb\n1\t2".into(),
            html: "<table><tr><td>a</td></tr></table>".into()
        }))
    );
    // Half of it, or garbage, is an empty paste, not a panic.
    assert_eq!(
        decode_event(paste, r#"{"text":"only"}"#.to_string(), 0.0, 0.0, 0, 0),
        Some(Event::Paste(PasteEvent {
            text: "only".into(),
            html: String::new()
        }))
    );
    assert_eq!(
        decode_event(paste, "not json".to_string(), 0.0, 0.0, 0, 0),
        Some(Event::Paste(PasteEvent::default()))
    );
    assert_eq!(
        decode_event(26, String::new(), 0.0, 0.0, 0, 0),
        None,
        "past the table"
    );
}

#[quench_test::test]
fn what_a_copy_listener_chooses_goes_to_the_page_and_a_paste_reaches_its_listener() {
    mock::reset();
    let pasted = Rc::new(RefCell::new(String::new()));
    let sink = pasted.clone();
    let m = quench_ui_web::mount_root(
        div()
            .on(EventKind::Copy, |_| {
                quench_ui::set_clipboard("1\t2", "<table></table>");
            })
            .on(EventKind::Paste, move |e| {
                if let Event::Paste(p) = e {
                    sink.borrow_mut().clone_from(&p.text);
                    quench_ui::prevent_default();
                }
            }),
    );
    let h = mock::handles_by_tag("div")[0];
    assert_eq!(
        q_dispatch_event(h, kind_code(EventKind::Copy), 0, 0, 0.0, 0.0, 0, 0),
        0
    );
    assert_eq!(
        mock::clipboard_sets(),
        vec![("1\t2".to_string(), "<table></table>".to_string())]
    );
    // A copy nobody answers puts nothing on the clipboard, so the browser copies the selection as usual.
    let before = mock::clipboard_sets().len();
    q_dispatch_event(h, kind_code(EventKind::Cut), 0, 0, 0.0, 0.0, 0, 0);
    assert_eq!(mock::clipboard_sets().len(), before);
    let payload = br#"{"text":"x\ty","html":""}"#;
    let (ptr, len) = mock::alloc_copy(payload);
    let prevent = q_dispatch_event(h, kind_code(EventKind::Paste), ptr, len, 0.0, 0.0, 0, 0);
    assert_eq!(
        prevent, 1,
        "the listener used it, so the browser should not also insert it"
    );
    assert_eq!(*pasted.borrow(), "x\ty");
    let _ = q_alloc(0);
    m.unmount();
}
