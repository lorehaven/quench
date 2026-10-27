//! A page that shows every component of the kit, the gestures, the animation and the runtime
//! translations, for looking at on a desktop and a phone-sized screen.

use std::rc::Rc;

use quench_ui::anim::presence;
use quench_ui::form::{Field, Form, rules};
use quench_ui::gesture::{Axis, Config, on_gesture};
use quench_ui::i18n::I18n;
use quench_ui::kit::{
    NavItem, app_bar, bottom_nav, checkbox, chip, date_field, dialog, fab, icon_button, list,
    list_tile, pull_to_refresh, sheet, snackbar, swipe_action, switch, tabs, text_field, tile,
    time_field, toast,
};
use quench_ui::platform;
use quench_ui::prelude::*;
use quench_ui::virtual_list::virtual_list;

const EN: &str = include_str!("../i18n/en.ftl");
const PL: &str = include_str!("../i18n/pl.ftl");

fn controls() -> View {
    let (sheet_open, dialog_open) = (create_signal(false), create_signal(false));
    let message = create_signal(None);
    let (on, agreed, pressed) = (
        create_signal(true),
        create_signal(false),
        create_signal(false),
    );
    let email = Field::new("email", "")
        .rule(rules::required("Enter your email"))
        .rule(rules::check(
            |v| v.contains('@'),
            "That is not an email address",
        ));
    let day = Field::new("day", "").rule(rules::date("Pick a date"));
    let at = Field::new("at", "").rule(rules::time("Pick a time"));
    let form = Form::new().field(&email).field(&day).field(&at);
    let (note, saved) = (message.clone(), message.clone());
    let (close_sheet, close_dialog) = (sheet_open.clone(), dialog_open.clone());
    let (open_sheet, open_dialog) = (sheet_open.clone(), dialog_open.clone());
    div()
        .child(h2().text("Controls"))
        .child(
            button()
                .attr("id", "open-sheet")
                .attr("type", "button")
                .text("Open sheet")
                .on_click(move || open_sheet.set(true)),
        )
        .child(
            button()
                .attr("id", "open-dialog")
                .attr("type", "button")
                .text("Open dialog")
                .on_click(move || open_dialog.set(true)),
        )
        .child(
            button()
                .attr("id", "show-toast")
                .attr("type", "button")
                .text("Show message")
                .on_click(move || toast(&note, "Saved", 3000)),
        )
        .child(switch("Notifications", &on).attr("id", "notifications"))
        .child(checkbox("I agree", &agreed))
        .child(chip("Unread", &pressed).attr("id", "unread"))
        .child(
            form.element(move |_| toast(&saved, "Form sent", 3000))
                .attr("id", "gallery-form")
                .child(text_field(
                    "Email",
                    &email,
                    "email",
                    Some("We never share it"),
                ))
                .child(date_field("Date", &day))
                .child(time_field("Time", &at))
                .child(form.error_view())
                .child(button().attr("type", "submit").text("Send")),
        )
        .child(sheet(&sheet_open, "Filters", move || {
            let close = close_sheet.clone();
            div()
                .child(p().text("Anything can go in a sheet."))
                .child(
                    button()
                        .attr("type", "button")
                        .text("Done")
                        .on_click(move || close.set(false)),
                )
                .into_view()
        }))
        .child(dialog(
            &dialog_open,
            "Delete note?",
            || p().text("This cannot be undone.").into_view(),
            move || {
                let close = close_dialog.clone();
                vec![
                    button()
                        .attr("type", "button")
                        .text("Cancel")
                        .on_click(move || close.set(false))
                        .into_view(),
                ]
            },
        ))
        .child(snackbar(&message, Some(("Undo", Rc::new(|| {})))))
        .into_view()
}

fn lists() -> View {
    let refreshing = create_signal(false);
    let (busy, done) = (refreshing.clone(), refreshing);
    let rows = create_signal((1..=1000_u32).collect::<Vec<_>>());
    let tiles: Vec<View> = (1..=3)
        .map(|n| {
            li().child(swipe_action(
                tile(
                    &format!("Note {n}"),
                    Some("Swipe left to delete"),
                    None,
                    None,
                    Some(Rc::new(|| {})),
                )
                .into_view(),
                "Delete",
                || {},
            ))
            .into_view()
        })
        .collect();
    div()
        .child(h2().text("Lists"))
        .child(
            pull_to_refresh(
                &busy,
                move || {
                    let done = done.clone();
                    platform::after(800, move || done.set(false));
                },
                list(tiles).into_view(),
            )
            .attr("id", "pull"),
        )
        .child(h2().text("1,000 rows"))
        .child(
            virtual_list(
                &rows,
                48.0,
                |n| u64::from(*n),
                |n| list_tile(&format!("Row {n}"), None, None, None, None),
            )
            .attr("id", "rows")
            .class("gallery-rows"),
        )
        .into_view()
}

fn gestures() -> View {
    let last = create_signal("Touch or click the pad".to_string());
    let shown = last.clone();
    let pad = on_gesture(
        div()
            .attr("id", "pad")
            .class("gallery-pad")
            .attr("tabindex", "0")
            .attr("role", "application")
            .attr("aria-label", "Gesture pad")
            .text_fn(move || shown.get()),
        Config {
            axis: Axis::Both,
            ..Config::default()
        },
        move |g| last.set(format!("{g:?}")),
    );
    div().child(h2().text("Gestures")).child(pad).into_view()
}

fn motion() -> View {
    let shown = create_signal(false);
    let (flag, toggle) = (shown.clone(), shown);
    div()
        .child(h2().text("Motion"))
        .child(
            button()
                .attr("id", "toggle-card")
                .attr("type", "button")
                .text("Toggle")
                .on_click(move || toggle.update(|v| *v = !*v)),
        )
        .child(presence(
            move || flag.get(),
            300,
            || {
                div()
                    .class("gallery-card")
                    .text("I fade in and out")
                    .into_view()
            },
        ))
        .into_view()
}

fn language(i18n: &I18n) -> View {
    let files = create_signal(1_u32);
    let (more, fewer, count) = (files.clone(), files.clone(), files);
    let (heading, selected, add, remove) = (i18n.clone(), i18n.clone(), i18n.clone(), i18n.clone());
    div()
        .child(h2().text_fn(move || heading.t("language")))
        .child(
            div()
                .child(language_chip("EN", i18n, "en").attr("id", "lang-en"))
                .child(language_chip("PL", i18n, "pl").attr("id", "lang-pl")),
        )
        .child(
            p().attr("id", "files")
                .text_fn(move || selected.tr("selected", &[("n", count.get().into())])),
        )
        .child(
            button()
                .attr("id", "more")
                .attr("type", "button")
                .text_fn(move || add.t("add"))
                .on_click(move || more.update(|n| *n += 1)),
        )
        .child(
            button()
                .attr("id", "fewer")
                .attr("type", "button")
                .text_fn(move || remove.t("remove"))
                .on_click(move || fewer.update(|n| *n = n.saturating_sub(1))),
        )
        .into_view()
}

fn language_chip(label: &str, i18n: &I18n, code: &'static str) -> Element {
    let (current, pressed, pick) = (i18n.clone(), i18n.clone(), i18n.clone());
    button()
        .attr("type", "button")
        .class("q-chip")
        .class_if("q-chip-on", move || current.language() == code)
        .attr_fn("aria-pressed", move || {
            (pressed.language() == code).to_string()
        })
        .text(label)
        .on_click(move || pick.set_language(code))
}

/// The whole gallery.
#[must_use]
pub fn gallery() -> View {
    View::lazy(|| {
        let i18n =
            I18n::new(&[("en", EN), ("pl", PL)], "en").expect("the bundled translations parse");
        let selected = create_signal(0_usize);
        let screen = create_signal(0_usize);
        let heading = i18n.clone();
        let panel_i18n = i18n;
        div()
            .attr("id", "gallery")
            .child(app_bar(
                "Gallery",
                Some(icon_button("Menu", "\u{2630}", || {}).into_view()),
                Vec::new(),
            ))
            .child(
                h2().attr("id", "gallery-title")
                    .text_fn(move || heading.t("title")),
            )
            .child(tabs(
                "g",
                &["Controls", "Lists", "Gestures", "Motion", "Language"],
                &selected,
                move |i| match i {
                    0 => controls(),
                    1 => lists(),
                    2 => gestures(),
                    3 => motion(),
                    _ => language(&panel_i18n),
                },
            ))
            .child(fab("New", "+", || {}))
            .child(bottom_nav(
                "Main",
                vec![
                    NavItem::new("Home", "\u{2302}"),
                    NavItem::new("Search", "\u{1f50d}"),
                    NavItem::new("Profile", "\u{263a}"),
                ],
                &screen,
            ))
            .into_view()
    })
}
