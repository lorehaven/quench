//! Signals, memos, effects, batching, scopes and context.

use quench_ui::reactive::{
    Scope, batch, create_effect, create_memo, create_root, create_signal, on_cleanup,
    provide_context, untrack, use_context,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

#[quench_test::test]
fn a_signal_reads_back_what_was_written() {
    let s = create_signal(1);
    assert_eq!(s.get(), 1);
    s.set(2);
    assert_eq!(s.get_untracked(), 2);
    assert_eq!(
        s.update(|v| {
            *v += 5;
            *v * 2
        }),
        14
    );
    assert_eq!(s.with(|v| *v), 7);
    assert_eq!(s.with_untracked(|v| *v + 1), 8);
    assert_eq!(format!("{s:?}"), "Signal(7)");
}

#[quench_test::test]
fn set_if_changed_notifies_only_on_a_difference() {
    let s = create_signal(1);
    let runs = Rc::new(Cell::new(0));
    let (s2, r2) = (s.clone(), runs.clone());
    let (_, root) = create_root(|| {
        create_effect(move || {
            let _ = s2.get();
            r2.set(r2.get() + 1);
        })
    });
    s.set_if_changed(1);
    assert_eq!(runs.get(), 1);
    s.set_if_changed(2);
    assert_eq!(runs.get(), 2);
    s.set(2);
    assert_eq!(runs.get(), 3, "plain set always notifies");
    root.dispose();
}

#[quench_test::test]
fn an_effect_runs_at_once_and_again_on_each_change() {
    let s = create_signal(0);
    let seen = log();
    let (s2, seen2) = (s.clone(), seen.clone());
    let (_, root) = create_root(|| {
        create_effect(move || seen2.borrow_mut().push(format!("v{}", s2.get())));
    });
    s.set(1);
    s.set(2);
    assert_eq!(*seen.borrow(), ["v0", "v1", "v2"]);
    root.dispose();
    s.set(3);
    assert_eq!(seen.borrow().len(), 3, "a disposed scope's effects stop");
}

#[quench_test::test]
fn reads_in_untrack_and_untracked_reads_do_not_subscribe() {
    let a = create_signal(0);
    let b = create_signal(0);
    let runs = Rc::new(Cell::new(0));
    let (a2, b2, r2) = (a.clone(), b.clone(), runs.clone());
    let (_, root) = create_root(|| {
        create_effect(move || {
            let _ = a2.get();
            let _ = untrack(|| b2.get());
            let _ = b2.get_untracked();
            b2.with_untracked(|_| ());
            r2.set(r2.get() + 1);
        });
    });
    b.set(1);
    assert_eq!(runs.get(), 1);
    a.set(1);
    assert_eq!(runs.get(), 2);
    root.dispose();
}

#[quench_test::test]
fn dependencies_follow_the_branch_taken() {
    let flag = create_signal(true);
    let a = create_signal("a");
    let b = create_signal("b");
    let seen = log();
    let (f, a2, b2, s2) = (flag.clone(), a.clone(), b.clone(), seen.clone());
    let (_, root) = create_root(|| {
        create_effect(move || {
            let v = if f.get() { a2.get() } else { b2.get() };
            s2.borrow_mut().push(v.to_string());
        });
    });
    b.set("b1"); // not read yet
    assert_eq!(seen.borrow().len(), 1);
    flag.set(false);
    a.set("a1"); // no longer read
    b.set("b2");
    assert_eq!(*seen.borrow(), ["a", "b1", "b2"]);
    root.dispose();
}

#[quench_test::test]
fn a_memo_recomputes_only_when_its_inputs_change() {
    let s = create_signal(2);
    let computed = Rc::new(Cell::new(0));
    let (s2, c2) = (s.clone(), computed.clone());
    let (m, root) = create_root(|| {
        create_memo(move || {
            c2.set(c2.get() + 1);
            s2.get() * 10
        })
    });
    assert_eq!((m.get(), m.get_untracked(), computed.get()), (20, 20, 1));
    s.set(3);
    assert_eq!(m.get(), 30);
    assert_eq!(m.with(|v| *v + 1), 31);
    assert_eq!(computed.get(), 2);
    assert!(format!("{m:?}").starts_with("Memo("));
    root.dispose();
}

#[quench_test::test]
fn a_memo_whose_value_is_unchanged_does_not_wake_its_observers() {
    let s = create_signal(1);
    let s2 = s.clone();
    let runs = Rc::new(Cell::new(0));
    let r2 = runs.clone();
    let (_, root) = create_root(|| {
        let parity = create_memo(move || s2.get() % 2);
        create_effect(move || {
            let _ = parity.get();
            r2.set(r2.get() + 1);
        });
    });
    s.set(3); // still odd
    s.set(5);
    assert_eq!(runs.get(), 1);
    s.set(4);
    assert_eq!(runs.get(), 2);
    root.dispose();
}

#[quench_test::test]
fn a_diamond_runs_the_effect_once_and_never_shows_a_mixed_state() {
    let a = create_signal(1);
    let (a1, a2) = (a.clone(), a.clone());
    let seen = log();
    let s2 = seen.clone();
    let (_, root) = create_root(|| {
        let double = create_memo(move || a1.get() * 2);
        let triple = create_memo(move || a2.get() * 3);
        create_effect(move || {
            s2.borrow_mut()
                .push(format!("{}/{}", double.get(), triple.get()))
        });
    });
    a.set(2);
    a.set(5);
    assert_eq!(*seen.borrow(), ["2/3", "4/6", "10/15"]);
    root.dispose();
}

#[quench_test::test]
fn a_memo_chain_settles_in_one_pass() {
    let a = create_signal(1);
    let a2 = a.clone();
    let runs = Rc::new(Cell::new(0));
    let r2 = runs.clone();
    let (last, root) = create_root(|| {
        let m1 = create_memo(move || a2.get() + 1);
        let m2 = create_memo({
            let m1 = m1.clone();
            move || m1.get() + 1
        });
        let m3 = create_memo({
            let m2 = m2.clone();
            move || m2.get() + 1
        });
        let m3b = m3.clone();
        create_effect(move || {
            let _ = m3b.get();
            r2.set(r2.get() + 1);
        });
        m3
    });
    assert_eq!(last.get(), 4);
    a.set(10);
    assert_eq!(last.get(), 13);
    assert_eq!(runs.get(), 2);
    root.dispose();
}

#[quench_test::test]
fn batching_runs_each_effect_once_with_the_final_values() {
    let a = create_signal(0);
    let b = create_signal(0);
    let seen = log();
    let (a2, b2, s2) = (a.clone(), b.clone(), seen.clone());
    let (_, root) = create_root(|| {
        create_effect(move || s2.borrow_mut().push(format!("{}+{}", a2.get(), b2.get())));
    });
    let out = batch(|| {
        a.set(1);
        assert_eq!(a.get_untracked(), 1, "writes are visible inside the batch");
        b.set(2);
        batch(|| a.set(3));
        assert_eq!(seen.borrow().len(), 1, "nothing ran yet");
        "done"
    });
    assert_eq!(out, "done");
    assert_eq!(*seen.borrow(), ["0+0", "3+2"]);
    root.dispose();
}

#[quench_test::test]
fn an_effect_may_write_signals_and_dependents_catch_up() {
    let input = create_signal(1);
    let mirror = create_signal(0);
    let (i2, m2, m3) = (input.clone(), mirror.clone(), mirror.clone());
    let seen = log();
    let s2 = seen.clone();
    let (_, root) = create_root(|| {
        create_effect(move || m2.set(i2.get() * 100));
        create_effect(move || s2.borrow_mut().push(m3.get().to_string()));
    });
    input.set(2);
    assert_eq!(seen.borrow().last().unwrap(), "200");
    root.dispose();
}

#[quench_test::test]
#[should_panic(expected = "reactive cycle")]
fn effects_that_keep_triggering_each_other_are_stopped() {
    let s = create_signal(0);
    let s2 = s.clone();
    let (_, _root) = create_root(|| {
        create_effect(move || {
            let v = s2.get();
            s2.set(v + 1);
        });
    });
    s.set(100);
}

#[quench_test::test]
fn a_panic_in_an_effect_does_not_wedge_the_runtime() {
    let s = create_signal(0);
    let s2 = s.clone();
    let (_, root) = create_root(|| {
        create_effect(move || {
            assert!(s2.get() < 1, "boom");
        });
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.set(1)));
    assert!(result.is_err());
    // A fresh effect still runs and reacts afterwards.
    let t = create_signal(0);
    let (t2, runs) = (t.clone(), Rc::new(Cell::new(0)));
    let r2 = runs.clone();
    let (_, root2) = create_root(|| {
        create_effect(move || {
            let _ = t2.get();
            r2.set(r2.get() + 1);
        })
    });
    t.set(1);
    assert_eq!(runs.get(), 2);
    root.dispose();
    root2.dispose();
}

#[quench_test::test]
fn disposing_an_effect_releases_what_it_captured_and_unsubscribes() {
    let payload = Rc::new(());
    let s = create_signal(0);
    let (s2, p2) = (s.clone(), payload.clone());
    let (effect, root) = create_root(|| {
        create_effect(move || {
            let _ = (s2.get(), &p2);
        })
    });
    assert_eq!(Rc::strong_count(&payload), 2);
    assert!(!effect.is_disposed());
    effect.dispose();
    effect.dispose(); // idempotent
    assert!(effect.is_disposed());
    assert_eq!(Rc::strong_count(&payload), 1, "the closure was dropped");
    assert!(format!("{effect:?}").contains("disposed: true"));
    s.set(1); // nobody left to notify
    root.dispose();
}

#[quench_test::test]
fn disposing_a_scope_frees_nested_effects_memos_and_cleanups() {
    let payload = Rc::new(());
    let s = create_signal(0);
    let ran_cleanup = Rc::new(Cell::new(false));
    let (s2, p2, c2) = (s.clone(), payload.clone(), ran_cleanup.clone());
    let (_, root) = create_root(|| {
        let p3 = p2.clone();
        let memo = create_memo({
            let s = s2.clone();
            move || {
                let _ = &p3;
                s.get()
            }
        });
        create_effect(move || {
            let _ = memo.get();
            let p4 = p2.clone();
            create_effect(move || {
                let _ = &p4;
            });
        });
        let c2 = c2.clone();
        on_cleanup(move || c2.set(true));
    });
    assert!(Rc::strong_count(&payload) > 1);
    root.dispose();
    assert!(root.is_disposed());
    assert!(ran_cleanup.get());
    assert_eq!(Rc::strong_count(&payload), 1);
}

#[quench_test::test]
fn rerunning_an_effect_disposes_what_the_previous_run_created() {
    let trigger = create_signal(0);
    let inner = create_signal(0);
    let inner_runs = Rc::new(Cell::new(0));
    let cleanups = Rc::new(Cell::new(0));
    let (t2, i2, ir, c2) = (
        trigger.clone(),
        inner.clone(),
        inner_runs.clone(),
        cleanups.clone(),
    );
    let (_, root) = create_root(|| {
        create_effect(move || {
            let _ = t2.get();
            let (i3, ir2) = (i2.clone(), ir.clone());
            create_effect(move || {
                let _ = i3.get();
                ir2.set(ir2.get() + 1);
            });
            let c3 = c2.clone();
            on_cleanup(move || c3.set(c3.get() + 1));
        });
    });
    for n in 1..=3 {
        trigger.set(n);
    }
    assert_eq!(cleanups.get(), 3);
    inner.set(1);
    assert_eq!(inner_runs.get(), 4 + 1, "one live inner effect, not four");
    root.dispose();
}

#[quench_test::test]
fn an_effect_can_dispose_itself() {
    let s = create_signal(0);
    let runs = Rc::new(Cell::new(0));
    let slot: Rc<RefCell<Option<quench_ui::Effect>>> = Rc::new(RefCell::new(None));
    let (s2, r2, slot2) = (s.clone(), runs.clone(), slot.clone());
    let (effect, root) = create_root(|| {
        create_effect(move || {
            let _ = s2.get();
            r2.set(r2.get() + 1);
            if r2.get() == 2
                && let Some(e) = &*slot2.borrow()
            {
                e.dispose();
            }
        })
    });
    *slot.borrow_mut() = Some(effect);
    s.set(1);
    s.set(2);
    assert_eq!(runs.get(), 2);
    root.dispose();
}

#[quench_test::test]
fn context_is_found_in_nested_scopes_and_the_nearest_wins() {
    let ((), root) = create_root(|| {
        assert_eq!(use_context::<i32>(), None);
        provide_context(1_i32);
        provide_context("theme");
        provide_context(2_i32); // replaces
        assert_eq!(use_context::<i32>(), Some(2));
        let child = Scope::new();
        child.run(|| {
            assert_eq!(use_context::<&str>(), Some("theme"));
            provide_context(3_i32);
            assert_eq!(use_context::<i32>(), Some(3));
        });
        assert_eq!(use_context::<i32>(), Some(2));
        create_effect(|| {
            assert_eq!(
                use_context::<i32>(),
                Some(2),
                "effects see their creator's context"
            )
        });
        child.dispose();
    });
    root.dispose();
    root.dispose();
}

#[quench_test::test]
#[should_panic(expected = "needs a scope")]
fn provide_context_without_a_scope_panics() {
    provide_context(1);
}

#[quench_test::test]
fn cleanup_without_a_scope_is_dropped_and_scopes_report_their_state() {
    let ran = Rc::new(Cell::new(false));
    let r2 = ran.clone();
    on_cleanup(move || r2.set(true));
    assert!(!ran.get());
    assert!(Scope::current().is_none());
    let scope = Scope::default();
    scope.run(|| assert!(Scope::current().is_some()));
    let nested = Scope::new_in(&scope);
    scope.reset();
    assert!(nested.is_disposed(), "reset disposes children");
    assert!(!scope.is_disposed());
    assert!(format!("{scope:?}").contains("Scope"));
    scope.dispose();
    assert!(scope.is_disposed());
}

/// A tiny deterministic generator, enough to drive the fuzz test.
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

#[quench_test::test]
fn random_graphs_always_match_a_direct_recomputation() {
    for seed in 0..40 {
        let mut rng = Lcg(seed + 1);
        let inputs: Vec<_> = (0..4).map(|i| create_signal(i64::from(i))).collect();
        let mut memos: Vec<quench_ui::Memo<i64>> = Vec::new();
        // Each memo sums two earlier nodes (inputs or memos) with a weight; mirror it in plain data.
        let mut shape: Vec<(usize, usize, i64)> = Vec::new();
        let observed = Rc::new(RefCell::new(Vec::<i64>::new()));
        let ((), root) = create_root(|| {
            for n in 0..8 {
                let pool = inputs.len() + n;
                let (x, y) = (
                    rng.next(pool as u64) as usize,
                    rng.next(pool as u64) as usize,
                );
                let w = rng.next(5) as i64 + 1;
                shape.push((x, y, w));
                let (ins, mems) = (inputs.clone(), memos.clone());
                let read = move |k: usize| {
                    if k < ins.len() {
                        ins[k].get()
                    } else {
                        mems[k - ins.len()].get()
                    }
                };
                memos.push(create_memo(move || read(x) * w + read(y)));
            }
            let last = memos.last().unwrap().clone();
            let o = observed.clone();
            create_effect(move || o.borrow_mut().push(last.get()));
        });
        let eval = |vals: &[i64]| -> i64 {
            let mut all = vals.to_vec();
            for &(x, y, w) in &shape {
                all.push(all[x] * w + all[y]);
            }
            *all.last().unwrap()
        };
        let mut current: Vec<i64> = (0..4).collect();
        for _ in 0..30 {
            let which = rng.next(4) as usize;
            let v = rng.next(9) as i64 - 4;
            current[which] = v;
            if rng.next(2) == 0 {
                inputs[which].set(v);
            } else {
                batch(|| {
                    inputs[which].set(v);
                    inputs[(which + 1) % 4].set(current[(which + 1) % 4]);
                });
            }
            assert_eq!(
                *observed.borrow().last().unwrap(),
                eval(&current),
                "seed {seed}"
            );
            assert_eq!(memos.last().unwrap().get_untracked(), eval(&current));
        }
        root.dispose();
    }
}
