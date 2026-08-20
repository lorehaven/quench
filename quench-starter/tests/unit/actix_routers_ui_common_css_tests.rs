//! Unit tests for `actix/routers/ui/common/css.rs`.

use quench_starter::actix::routers::ui::common::css::{
    home_rules, layout_rules, login_rules, meta_rules,
};

fn render_all(rules: &[quench_web::prelude::CssRule]) -> String {
    rules.iter().map(|rule| rule.render()).collect()
}

#[test]
fn layout_rules_covers_the_page_shell_selectors() {
    let css = render_all(&layout_rules());
    assert!(css.contains(".left-panel"));
    assert!(css.contains(".content-inner"));
    assert!(css.contains(".split-view"));
    assert!(css.contains("@media screen and (max-width: 1024px)"));
    assert!(css.contains(".panel-title"));
}

#[test]
fn home_rules_covers_the_card_grid_and_its_hover_state() {
    let css = render_all(&home_rules());
    assert!(css.contains(".home-grid"));
    assert!(css.contains(".home-card"));
    assert!(css.contains("&:hover"));
    assert!(css.contains("@media screen and (max-width: 768px)"));
    assert!(css.contains(".home-card-arrow"));
}

#[test]
fn login_rules_covers_the_credentials_panel() {
    let css = render_all(&login_rules());
    assert!(css.contains(".login-layout"));
    assert!(css.contains(".login-bar .locale-switch"));
    assert!(css.contains(".login-credentials"));
}

#[test]
fn meta_rules_covers_the_list_and_empty_state() {
    let css = render_all(&meta_rules());
    assert!(css.contains(".meta-list"));
    assert!(css.contains(".empty"));
}
