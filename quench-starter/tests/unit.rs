// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/common_db_tests.rs"]
mod common_db_tests;
#[path = "unit/common_format_tests.rs"]
mod common_format_tests;
#[path = "unit/common_wait_tests.rs"]
mod common_wait_tests;
#[path = "unit/http_pieces_tests.rs"]
mod http_pieces_tests;
#[path = "unit/http_routers_ui_common_css_tests.rs"]
mod http_routers_ui_common_css_tests;
#[path = "unit/http_routers_ui_pages_home_tests.rs"]
mod http_routers_ui_pages_home_tests;
#[path = "unit/metrics_tests.rs"]
mod metrics_tests;
#[path = "unit/misc_tests.rs"]
mod misc_tests;
#[path = "unit/resilience_tests.rs"]
mod resilience_tests;

quench_test::main!();
