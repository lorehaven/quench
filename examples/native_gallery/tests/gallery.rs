// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

//! The component gallery used the way a person would use it: pointer and keyboard events into the native
//! renderer, and what the page does about them.

use std::rc::Rc;

use quench_example_ui_web_app::gallery;
use quench_raster::Canvas;
use quench_style::theme::Theme;
use quench_ui::{KeyEvent, mount};
use quench_ui_native::{NativeRenderer, NodeId, NodeKind};

struct Page {
    r: Rc<NativeRenderer>,
    canvas: Canvas,
    _mounted: quench_ui::Mount<NativeRenderer>,
}

fn open(width: usize, height: usize) -> Page {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet(&format!(
        "@media (prefers-color-scheme: dark) {{\n{}\n}}",
        Theme::theme(Theme::DefaultDark)
    ));
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
        gallery::GALLERY_STYLE.to_string(),
    ] {
        r.add_stylesheet(&sheet);
    }
    r.set_viewport(width as f32, height as f32);
    let mounted = mount(&r, &r.root(), gallery::gallery());
    let mut canvas = Canvas::new(width, height);
    r.render(&mut canvas);
    Page {
        r,
        canvas,
        _mounted: mounted,
    }
}

impl Page {
    fn draw(&mut self) {
        self.r.render(&mut self.canvas);
    }

    /// Every element, in document order.
    fn all(&self) -> Vec<NodeId> {
        fn walk(r: &NativeRenderer, n: NodeId, out: &mut Vec<NodeId>) {
            for c in r.children(n) {
                if matches!(r.kind(c), NodeKind::Element(_)) {
                    out.push(c);
                    walk(r, c, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.r, self.r.root(), &mut out);
        out
    }

    fn by_id(&self, id: &str) -> NodeId {
        self.all()
            .into_iter()
            .find(|n| self.r.attr(*n, "id").as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no #{id}"))
    }

    /// The first element of `tag` whose text is exactly `text`.
    fn with_text(&self, tag: &str, text: &str) -> NodeId {
        self.all()
            .into_iter()
            .find(|n| self.r.tag(*n).as_deref() == Some(tag) && self.r.text_of(*n).trim() == text)
            .unwrap_or_else(|| panic!("no <{tag}>{text}"))
    }

    fn center(&self, n: NodeId) -> (f32, f32) {
        let boxes = self.r.layout();
        let (x, y) = boxes
            .position_of(n.index())
            .unwrap_or_else(|| panic!("{n:?} has no box"));
        let b = boxes.find(n.index()).unwrap();
        (x + b.width / 2.0, y + b.height / 2.0)
    }

    fn click(&mut self, n: NodeId) {
        let (x, y) = self.center(n);
        self.r.pointer_down(x, y);
        self.r.pointer_up(x, y);
        self.draw();
    }

    fn key(&mut self, k: &str) {
        self.r.key_down(&KeyEvent::new(k));
        self.r.key_up(&KeyEvent::new(k));
        self.draw();
    }

    fn text(&self) -> String {
        self.r.text_of(self.r.root())
    }
}

#[quench_test::test]
fn the_controls_tab_works_with_the_mouse() {
    let mut page = open(390, 844);
    assert!(
        !page.text().contains("This cannot be undone."),
        "the dialog starts closed"
    );
    page.click(page.by_id("open-dialog"));
    assert!(
        page.text().contains("This cannot be undone."),
        "a click opens the dialog"
    );
    page.click(page.with_text("button", "Open sheet"));
    // The notifications switch is a button that says whether it is on.
    let switch = page.by_id("notifications");
    assert_eq!(page.r.attr(switch, "aria-checked").as_deref(), Some("true"));
    // The dialog is modal and covers the page, so close what is open first (Escape, or its own button).
    page.key("Escape");
    page.draw();
}

#[quench_test::test]
fn the_switch_the_checkbox_and_the_chip_toggle() {
    let mut page = open(390, 844);
    let switch = page.by_id("notifications");
    page.click(switch);
    assert_eq!(
        page.r.attr(switch, "aria-checked").as_deref(),
        Some("false"),
        "one click turns it off"
    );
    page.click(switch);
    assert_eq!(page.r.attr(switch, "aria-checked").as_deref(), Some("true"));
    let chip = page.by_id("unread");
    assert_eq!(page.r.attr(chip, "aria-pressed").as_deref(), Some("false"));
    page.click(chip);
    assert_eq!(page.r.attr(chip, "aria-pressed").as_deref(), Some("true"));
    // "I agree": the label's text toggles its checkbox.
    let label = page.with_text("span", "I agree");
    let boxes = page
        .all()
        .into_iter()
        .find(|n| {
            page.r.tag(*n).as_deref() == Some("input")
                && page.r.attr(*n, "type").as_deref() == Some("checkbox")
        })
        .unwrap();
    assert!(page.r.attr(boxes, "checked").is_none());
    page.click(label);
    assert!(
        page.r.attr(boxes, "checked").is_some(),
        "clicking the words checks the box"
    );
}

#[quench_test::test]
fn typing_in_the_email_field_and_tabbing_through_the_page() {
    let mut page = open(390, 844);
    let field = page
        .all()
        .into_iter()
        .find(|n| {
            page.r.tag(*n).as_deref() == Some("input")
                && page.r.attr(*n, "type").as_deref() == Some("email")
        })
        .unwrap();
    // Clicking the label focuses the field.
    let label = page.with_text("label", "Email");
    page.click(label);
    assert_eq!(page.r.focused(), Some(field));
    page.r.text("ann@example.org");
    assert_eq!(
        page.r.attr(field, "value").as_deref(),
        Some("ann@example.org")
    );
    page.draw();
    // Tab moves on to the next field, Shift+Tab back.
    page.key("Tab");
    assert_ne!(page.r.focused(), Some(field));
    let next = page.r.focused().unwrap();
    page.r.key_down(&KeyEvent {
        shift: true,
        ..KeyEvent::new("Tab")
    });
    assert_eq!(page.r.focused(), Some(field));
    assert_ne!(Some(next), page.r.focused());
    for _ in 0..4 {
        page.key("Backspace");
    }
    assert_eq!(page.r.attr(field, "value").as_deref(), Some("ann@example"));
}

#[quench_test::test]
fn the_tabs_switch_panels_by_click_and_the_page_scrolls() {
    let mut page = open(390, 844);
    assert!(page.text().contains("Open sheet"));
    page.click(page.with_text("button", "Lists"));
    assert!(
        !page.text().contains("Open sheet"),
        "the Controls panel is gone: {}",
        page.text()
    );
    page.click(page.with_text("button", "Controls"));
    assert!(page.text().contains("Open sheet"), "and back");
    // The page is taller than the window: the wheel scrolls it, and what is fixed stays.
    let fab_before = page.center(
        page.all()
            .into_iter()
            .find(|n| {
                page.r
                    .attr(*n, "class")
                    .is_some_and(|c| c.contains("q-fab"))
            })
            .unwrap(),
    );
    page.r.wheel(200.0, 400.0, 0.0, 100.0);
    page.draw();
    assert!(page.r.scroll_of(0).1 > 0.0, "the document scrolled");
    let fab_after = page.center(
        page.all()
            .into_iter()
            .find(|n| {
                page.r
                    .attr(*n, "class")
                    .is_some_and(|c| c.contains("q-fab"))
            })
            .unwrap(),
    );
    assert_eq!(
        fab_before, fab_after,
        "the floating button does not move with the page"
    );
    let (x, y) = fab_after;
    assert!(page.r.element_at(x, y).is_some());
    let target = page.r.element_at(x, y).unwrap();
    assert!(
        page.r
            .attr(target, "class")
            .is_some_and(|c| c.contains("q-fab"))
            || page.r.text_of(target).contains('+'),
        "a click lands on the fixed button even after scrolling"
    );
}

quench_test::main!();
