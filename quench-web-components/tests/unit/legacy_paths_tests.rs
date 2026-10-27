//! Every public path that existed before the styling code moved to `quench-style` must still resolve.
//!
//! `cargo semver-checks` cannot see through re-exports of another crate and reports these paths as
//! removed; this file is the compile-time proof that they are not. It only has to compile.

#![allow(unused_imports, clippy::wildcard_imports)]

use quench_web_components::FooterBuilder as _;
use quench_web_components::HeaderBuilder as _;
use quench_web_components::NavPanelBuilder as _;
use quench_web_components::components::FooterBuilder as _;
use quench_web_components::components::HeaderBuilder as _;
use quench_web_components::components::NavPanelBuilder as _;
use quench_web_components::components::footer as _;
use quench_web_components::components::footer::FooterBuilder as _;
use quench_web_components::components::header as _;
use quench_web_components::components::header::HeaderBuilder as _;
use quench_web_components::components::nav as _;
use quench_web_components::components::nav::NavPanelBuilder as _;
use quench_web_components::components::nav::nav_button as _;
use quench_web_components::components::nav_button as _;
use quench_web_components::dom::events as _;
use quench_web_components::dom::events::on_dom_ready as _;
use quench_web_components::dom::modal as _;
use quench_web_components::dom::modal::toggle_modal as _;
use quench_web_components::dom::on_dom_ready as _;
use quench_web_components::dom::select as _;
use quench_web_components::dom::select::set_select_value as _;
use quench_web_components::dom::select::update_from_select as _;
use quench_web_components::dom::set_select_value as _;
use quench_web_components::dom::toggle_modal as _;
use quench_web_components::dom::update_from_select as _;
use quench_web_components::events as _;
use quench_web_components::events::on_dom_ready as _;
use quench_web_components::footer as _;
use quench_web_components::footer::FooterBuilder as _;
use quench_web_components::header as _;
use quench_web_components::header::HeaderBuilder as _;
use quench_web_components::modal as _;
use quench_web_components::modal::toggle_modal as _;
use quench_web_components::nav as _;
use quench_web_components::nav::NavPanelBuilder as _;
use quench_web_components::nav::nav_button as _;
use quench_web_components::nav_button as _;
use quench_web_components::on_dom_ready as _;
use quench_web_components::prelude::FooterBuilder as _;
use quench_web_components::prelude::HeaderBuilder as _;
use quench_web_components::prelude::NavPanelBuilder as _;
use quench_web_components::prelude::nav_button as _;
use quench_web_components::prelude::on_dom_ready as _;
use quench_web_components::prelude::set_select_value as _;
use quench_web_components::prelude::toggle_modal as _;
use quench_web_components::prelude::update_from_select as _;
use quench_web_components::select as _;
use quench_web_components::select::set_select_value as _;
use quench_web_components::select::update_from_select as _;
use quench_web_components::set_select_value as _;
use quench_web_components::toggle_modal as _;
use quench_web_components::update_from_select as _;

#[quench_test::test]
const fn the_pre_split_public_paths_still_resolve() {}
