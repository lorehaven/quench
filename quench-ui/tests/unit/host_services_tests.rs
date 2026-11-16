use quench_ui::platform::{self, ManualPlatform, PickedFile};
use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

fn file(name: &str, bytes: &[u8]) -> PickedFile {
    PickedFile {
        name: name.to_string(),
        mime: "application/octet-stream".to_string(),
        bytes: bytes.to_vec(),
    }
}

#[quench_test::test]
fn a_picker_waits_for_the_person_and_gets_their_files() {
    let manual = Rc::new(ManualPlatform::new());
    platform::install(manual.clone());
    let got: Rc<RefCell<Vec<Vec<PickedFile>>>> = Rc::default();
    let sink = got.clone();
    platform::pick_files(".xlsx,.csv", true, move |files| {
        sink.borrow_mut().push(files);
    });
    assert_eq!(
        manual.pending_picks(),
        vec![(".xlsx,.csv".to_string(), true)]
    );
    assert!(got.borrow().is_empty(), "nothing before the person answers");
    assert!(manual.answer_pick(vec![file("a.csv", b"1,2")]));
    assert_eq!(got.borrow().len(), 1);
    assert_eq!(got.borrow()[0][0].name, "a.csv");
    assert_eq!(got.borrow()[0][0].bytes, b"1,2");
    assert!(!manual.answer_pick(vec![]), "no picker is waiting now");
    // A cancel is an empty list.
    let sink = got.clone();
    platform::pick_files("", false, move |files| sink.borrow_mut().push(files));
    assert!(manual.answer_pick(Vec::new()));
    assert!(got.borrow()[1].is_empty());
}

#[quench_test::test]
fn dropped_files_go_to_the_handler() {
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let sink = seen.clone();
    platform::deliver_drop(vec![PickedFile::default()]); // nobody listens yet: nothing happens
    platform::on_drop(move |files| sink.borrow_mut().push(files.len()));
    platform::deliver_drop(vec![PickedFile::default(), PickedFile::default()]);
    assert_eq!(*seen.borrow(), vec![2]);
}

#[quench_test::test]
fn showing_a_file_in_a_tab_is_a_download_with_no_name() {
    let manual = Rc::new(ManualPlatform::new());
    platform::install(manual.clone());
    platform::open_in_tab("application/pdf", b"%PDF");
    assert_eq!(
        manual.downloads(),
        vec![(String::new(), "application/pdf".into(), b"%PDF".to_vec())]
    );
}

#[quench_test::test]
fn files_offered_for_saving_are_recorded() {
    let manual = Rc::new(ManualPlatform::new());
    platform::install(manual.clone());
    platform::download("book.xlsx", "application/zip", &[1, 2, 3]);
    platform::download("b.csv", "text/csv", b"x");
    let seen = manual.downloads();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0],
        ("book.xlsx".into(), "application/zip".into(), vec![1, 2, 3])
    );
}

#[quench_test::test]
fn without_a_platform_a_picker_answers_nothing_at_once() {
    // A fresh thread has no platform installed.
    std::thread::spawn(|| {
        let answered = Rc::new(RefCell::new(None));
        let sink = answered.clone();
        platform::pick_files("", false, move |files| *sink.borrow_mut() = Some(files));
        assert_eq!(*answered.borrow(), Some(Vec::new()));
        platform::download("x", "y", b"z"); // nothing to do, and no panic
    })
    .join()
    .unwrap();
}

#[quench_test::test]
fn a_listener_can_put_text_on_the_clipboard_and_read_a_paste() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let pasted = Rc::new(RefCell::new(String::new()));
    let sink = pasted.clone();
    let (_, scope) = create_root(|| {
        mount(
            &r,
            &root,
            div()
                .on(EventKind::Copy, |_| {
                    quench_ui::set_clipboard("a\tb", "<table></table>");
                })
                .on(EventKind::Paste, move |e| {
                    if let Event::Paste(p) = e {
                        *sink.borrow_mut() = format!("{}|{}", p.text, p.html);
                    }
                }),
        )
    });
    let node = r.find_by_tag("div")[0];
    let _ = quench_ui::take_clipboard_request();
    r.fire(node, &Event::Copy);
    let asked = quench_ui::take_clipboard_request().unwrap();
    assert_eq!(
        (asked.text.as_str(), asked.html.as_str()),
        ("a\tb", "<table></table>")
    );
    assert_eq!(quench_ui::take_clipboard_request(), None, "taken once");
    r.fire(
        node,
        &Event::Paste(PasteEvent {
            text: "x".into(),
            html: "<b>x</b>".into(),
        }),
    );
    assert_eq!(*pasted.borrow(), "x|<b>x</b>");
    assert_eq!(EventKind::Paste.dom_name(), "paste");
    assert_eq!(EventKind::Cut.dom_name(), "cut");
    assert_eq!(Event::Copy.kind(), EventKind::Copy);
    assert_eq!(Event::Cut.kind(), EventKind::Cut);
    scope.dispose();
}
