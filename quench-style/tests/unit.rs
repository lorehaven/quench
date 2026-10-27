// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/assets_tests.rs"]
mod assets_tests;
#[path = "unit/components_tests.rs"]
mod components_tests;
#[path = "unit/framework_app_i18n_tests.rs"]
mod framework_app_i18n_tests;
#[path = "unit/framework_styles_common_elements_tests.rs"]
mod framework_styles_common_elements_tests;
#[path = "unit/framework_styles_common_layout_tests.rs"]
mod framework_styles_common_layout_tests;
#[path = "unit/framework_styles_common_modal_tests.rs"]
mod framework_styles_common_modal_tests;
#[path = "unit/framework_styles_common_root_tests.rs"]
mod framework_styles_common_root_tests;
#[path = "unit/framework_theme_bootstrap_dark_tests.rs"]
mod framework_theme_bootstrap_dark_tests;
#[path = "unit/framework_theme_bootstrap_light_tests.rs"]
mod framework_theme_bootstrap_light_tests;
#[path = "unit/framework_theme_default_dark_tests.rs"]
mod framework_theme_default_dark_tests;
#[path = "unit/framework_theme_default_light_tests.rs"]
mod framework_theme_default_light_tests;
#[path = "unit/framework_theme_mod_tests.rs"]
mod framework_theme_mod_tests;
#[path = "unit/platform_layers_tests.rs"]
mod platform_layers_tests;
#[path = "unit/styling_css_tests.rs"]
mod styling_css_tests;
#[path = "unit/theme_palette_tests.rs"]
mod theme_palette_tests;
#[path = "unit/tokens_tests.rs"]
mod tokens_tests;

quench_test::main!();
