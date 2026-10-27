use {{ snake }}_core::step;

#[quench_test::test]
fn a_press_moves_the_count() {
    assert_eq!(step(0, 1), 1);
    assert_eq!(step(0, -1), -1);
}

#[quench_test::test]
fn the_count_stops_at_the_end() {
    assert_eq!(step(i64::MAX, 1), i64::MAX);
}

quench_test::main!();
