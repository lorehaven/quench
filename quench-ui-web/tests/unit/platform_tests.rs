//! The browser platform: clock, timers, reduced motion.

use quench_ui::platform::{self, Platform};
use quench_ui_web::WebPlatform;
use quench_ui_web::mock;
use std::cell::Cell;
use std::rc::Rc;

#[quench_test::test]
fn the_platform_reads_the_hosts_clock_and_motion_preference() {
    mock::reset();
    let p = WebPlatform::new();
    mock::set_now(1234.5);
    assert!((p.now() - 1234.5).abs() < f64::EPSILON);
    assert!(!p.prefers_reduced_motion());
    mock::set_reduced_motion(true);
    assert!(p.prefers_reduced_motion());
    assert!(format!("{p:?}").contains("WebPlatform"));
}

#[quench_test::test]
fn timers_run_once_when_the_host_fires_them_and_can_be_cancelled() {
    mock::reset();
    platform::install(Rc::new(WebPlatform::new()));
    let (ran, cancelled) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let r = ran.clone();
    platform::after(250, move || r.set(r.get() + 1));
    let c = cancelled.clone();
    let doomed = platform::after(300, move || c.set(c.get() + 1));
    doomed.cancel();
    mock::run_ticks();
    let timers = mock::timers();
    assert_eq!(
        timers.len(),
        2,
        "both asked the host for a timer: {timers:?}"
    );
    assert_eq!(timers[0].1, 250);
    for (id, _) in timers {
        mock::fire_timer(id);
    }
    assert_eq!((ran.get(), cancelled.get()), (1, 0));
    assert!((platform::now() - 0.0).abs() < f64::EPSILON);
    mock::set_now(5.0);
    assert!((platform::now() - 5.0).abs() < f64::EPSILON);
    mock::set_reduced_motion(true);
    assert!(platform::prefers_reduced_motion());
}
