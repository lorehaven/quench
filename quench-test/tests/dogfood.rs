//! The runner running its own macros: this binary has no standard harness.

#![allow(clippy::eq_op, clippy::unnecessary_wraps)]

use quench_test::test;

#[test]
fn plain() {
    assert_eq!(2 + 2, 4);
}

#[test]
const fn returns_a_result() -> Result<(), String> {
    Ok(())
}

#[test]
async fn current_thread() {
    tokio::task::yield_now().await;
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_thread() -> Result<(), std::io::Error> {
    tokio::spawn(async { 1 + 1 }).await.unwrap();
    Ok(())
}

#[test]
#[should_panic]
fn panics() {
    panic!("on purpose");
}

#[test]
#[should_panic(expected = "needle")]
fn panics_with_text() {
    panic!("a needle in a haystack");
}

#[test]
#[ignore = "never runs"]
fn ignored() {
    panic!("must not run");
}

#[test]
#[ignore]
fn bare_ignored() {
    panic!("must not run");
}

mod nested {
    use quench_test::test;

    #[test]
    fn name_has_a_module_path() {
        assert!(module_path!().ends_with("nested"));
        assert!(
            quench_test::TESTS
                .iter()
                .any(|t| t.name.ends_with("nested::name_has_a_module_path"))
        );
    }
}

#[test]
fn registered_cases_carry_their_location() {
    let case = quench_test::TESTS
        .iter()
        .find(|t| t.name.ends_with("::plain"))
        .unwrap();
    assert!(case.file.ends_with("dogfood.rs") && case.line > 0);
    assert_eq!(
        quench_test::TESTS
            .iter()
            .filter(|t| t.ignore.is_some())
            .count(),
        2
    );
}

quench_test::main!();
