//! Keyed list reconciliation against a reference, over random edits.

use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

fn html(r: &TestRenderer) -> String {
    r.html().replace("<!---->", "")
}

#[quench_test::test]
fn random_edits_always_match_the_expected_list_and_keep_node_identity() {
    for seed in 0..60 {
        let mut rng = Lcg(seed + 7);
        let r = Rc::new(TestRenderer::new());
        let root = r.root();
        let items = create_signal(Vec::<u32>::new());
        let list = items.clone();
        let built = Rc::new(RefCell::new(Vec::<u32>::new()));
        let b2 = built.clone();
        let m = mount(
            &r,
            &root,
            ul().child(View::keyed(
                move || list.get(),
                |i| *i,
                move |i| {
                    b2.borrow_mut().push(i);
                    li().attr("data-id", i.to_string())
                        .text(i.to_string())
                        .into_view()
                },
            )),
        );
        let mut current: Vec<u32> = Vec::new();
        let mut nodes: std::collections::HashMap<u32, _> = std::collections::HashMap::new();
        for _ in 0..40 {
            // Keep a random subset, shuffle it, and add a few new ids.
            let mut next: Vec<u32> = current
                .iter()
                .copied()
                .filter(|_| rng.next(4) != 0)
                .collect();
            for i in (1..next.len()).rev() {
                next.swap(i, rng.next(i as u64 + 1) as usize);
            }
            for _ in 0..rng.next(4) {
                let id = rng.next(60) as u32;
                if !next.contains(&id) {
                    let at = rng.next(next.len() as u64 + 1) as usize;
                    next.insert(at, id);
                }
            }
            built.borrow_mut().clear();
            items.set(next.clone());
            let expected: String = next
                .iter()
                .map(|i| format!("<li data-id=\"{i}\">{i}</li>"))
                .collect();
            assert_eq!(html(&r), format!("<ul>{expected}</ul>"), "seed {seed}");
            let new_ids: Vec<u32> = next
                .iter()
                .copied()
                .filter(|i| !current.contains(i))
                .collect();
            let mut built_now = built.borrow().clone();
            built_now.sort_unstable();
            let mut want = new_ids.clone();
            want.sort_unstable();
            assert_eq!(built_now, want, "only new keys are built (seed {seed})");
            // surviving items are the same nodes as before
            for (n, id) in r.find_by_tag("li").into_iter().zip(&next) {
                if let Some(old) = nodes.get(id)
                    && current.contains(id)
                {
                    assert_eq!(*old, n, "key {id} kept its node (seed {seed})");
                }
                nodes.insert(*id, n);
            }
            current = next;
        }
        m.unmount();
        assert_eq!(r.live_nodes(), 1);
    }
}

#[quench_test::test]
fn a_move_touches_only_what_moved() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let items = create_signal(vec![1, 2, 3, 4, 5]);
    let list = items.clone();
    let m = mount(
        &r,
        &root,
        View::keyed(
            move || list.get(),
            |i| *i,
            |i| span().text(i.to_string()).into_view(),
        ),
    );
    r.clear_ops();
    items.set(vec![1, 3, 4, 5, 2]); // 2 moves to the end
    let moves = r
        .ops()
        .iter()
        .filter(|o| matches!(o, quench_ui::test_renderer::Op::Insert { .. }))
        .count();
    assert_eq!(moves, 1);
    assert_eq!(r.text(), "13452");
    m.unmount();
}

#[quench_test::test]
fn keys_can_be_strings_or_integers_and_items_may_build_nothing() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let items = create_signal(vec!["a", "b", "c"]);
    let list = items.clone();
    let m = mount(
        &r,
        &root,
        View::keyed(
            move || list.get(),
            |s| s.to_string(),
            |s| if s == "b" { View::Empty } else { s.into_view() },
        ),
    );
    assert_eq!(r.text(), "ac");
    items.set(vec!["c", "b", "a"]);
    assert_eq!(r.text(), "ca");
    items.set(vec!["b"]);
    assert_eq!(r.text(), "");
    m.unmount();

    for key in [
        Key::from(1_i8),
        Key::from(2_i16),
        Key::from(3_i32),
        Key::from(4_i64),
        Key::from(5_u8),
        Key::from(6_u16),
        Key::from(7_u32),
        Key::from(8_u64),
        Key::from(9_usize),
        Key::from("s"),
        Key::from(String::from("t")),
    ] {
        assert_eq!(key.clone(), key);
    }
    assert_eq!(Key::from(u64::MAX), Key::Str(u64::MAX.to_string()));
    assert_eq!(Key::from(7_u64), Key::Int(7));
}

#[quench_test::test]
#[should_panic(expected = "duplicate key")]
fn duplicate_keys_are_rejected() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let _ = mount(
        &r,
        &root,
        View::keyed(|| vec![1, 1], |i| *i, |i| i.into_view()),
    );
}

#[quench_test::test]
fn nested_keyed_lists_inside_a_keyed_list_survive_outer_edits() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let rows = create_signal(vec![1, 2]);
    let cells = create_signal(vec!['x', 'y']);
    let (rs, cs) = (rows.clone(), cells.clone());
    let m = mount(
        &r,
        &root,
        View::keyed(
            move || rs.get(),
            |i| *i,
            move |row| {
                let cs = cs.clone();
                div()
                    .text(format!("r{row}:"))
                    .child(View::keyed(
                        move || cs.get(),
                        |c| u32::from(*c),
                        |c| c.to_string().into_view(),
                    ))
                    .into_view()
            },
        ),
    );
    assert_eq!(r.text(), "r1:xyr2:xy");
    cells.set(vec!['y', 'z']);
    assert_eq!(r.text(), "r1:yzr2:yz");
    rows.set(vec![2]);
    assert_eq!(r.text(), "r2:yz");
    m.unmount();
    assert_eq!(r.live_nodes(), 1);
}
