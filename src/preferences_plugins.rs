//! The two controls of Settings → Plugins: the link field with its Install button, and the
//! list of installed plugins. Kept out of `preferences.rs`, which is already long.

use gpui::StatefulInteractiveElement;
use gpui::prelude::FluentBuilder;
use gpui::{ClickEvent, Context, Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::plugin_source::{Installed, parse_link};
use crate::plugins::{self, Busy, Plugins};
use crate::preferences::PreferencesView;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;

/// What goes faint in the empty link field.
pub const FIELD_HINT: &str = "github.com/owner/repo";
/// Where "How to make a plugin" goes.
pub const MAKING_URL: &str = "https://github.com/fitzfitz/fuxx-cli#making-a-plugin";

const FIELD_WIDTH: f32 = 260.0;
const CONTROL_HEIGHT: f32 = 30.0;
const NAME_SIZE: f32 = 14.0;
const NOTE_SIZE: f32 = 12.0;

/// The text a busy plugin page shows in place of buttons.
pub fn busy_text(busy: &Busy) -> &'static str {
    match busy {
        Busy::Downloading => "Downloading\u{2026}",
        Busy::Checking => "Checking\u{2026}",
        Busy::Removing(_) => "Removing\u{2026}",
        Busy::Updating(_) => "Updating\u{2026}",
    }
}

/// "1 theme", "3 themes".
pub fn themes_text(count: usize) -> String {
    format!("{count} {}", if count == 1 { "theme" } else { "themes" })
}

/// The version as the card shows it: the plugin's own, plus the release it came from when that
/// reads differently (`1.0 (v1.0.2)`).
pub fn version_text(version: &str, installed: &Installed) -> String {
    let label = installed.label();
    if label == version {
        version.to_string()
    } else {
        format!("{version} ({label})")
    }
}

/// A button that does something while `enabled`; faint, with no hover and no click, otherwise.
fn action(label: String, enabled: bool, id: gpui::ElementId, ui: UiColors) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(CONTROL_HEIGHT))
        .flex()
        .items_center()
        .px(px(14.0))
        .rounded(px(9.0))
        .bg(hsla(ui.row_active))
        .text_size(px(13.0))
        .text_color(hsla(if enabled { ui.text } else { ui.text_faint }))
        .child(label)
        .when_enabled(enabled, ui)
}

trait WhenEnabled {
    fn when_enabled(self, enabled: bool, ui: UiColors) -> Self;
}

impl WhenEnabled for gpui::Stateful<Div> {
    fn when_enabled(self, enabled: bool, ui: UiColors) -> Self {
        if enabled {
            self.hover(|s| s.bg(hsla(ui.row_hover))).cursor_pointer()
        } else {
            self
        }
    }
}

/// The link field, the Install button, and the line under them (what is running, or how the
/// last thing went).
pub(crate) fn install_card(
    view: &PreferencesView,
    ui: UiColors,
    cx: &Context<PreferencesView>,
) -> Div {
    let state = cx.try_global::<Plugins>();
    let free = state.is_none_or(plugins::can_start);
    let empty = view.plugin_link.read(cx).text().is_empty();
    let field = div()
        .relative()
        .w(px(FIELD_WIDTH))
        .h(px(CONTROL_HEIGHT))
        .flex()
        .items_center()
        .text_size(px(NOTE_SIZE))
        .child(view.plugin_link.clone())
        .when(empty, |f| {
            f.child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_color(hsla(ui.text_faint))
                    .child(FIELD_HINT),
            )
        });
    let link = view.plugin_link.clone();
    let mut install = action("Install".into(), free, "plugin-install".into(), ui);
    if free {
        install = install.on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
            let text = link.read(cx).text().to_string();
            plugins::install(&text, window, cx);
        }));
    }
    let line = match state {
        Some(Plugins {
            busy: Some(busy), ..
        }) => Some((busy_text(busy).to_string(), false)),
        Some(Plugins {
            message: Some(m), ..
        }) => Some((m.text.clone(), m.error)),
        _ => None,
    };
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(field)
                .child(install),
        )
        .children(line.map(|(line, error)| {
            div()
                .max_w(px(FIELD_WIDTH + 80.0))
                .text_size(px(NOTE_SIZE))
                .text_color(hsla(if error { ui.ring_waiting } else { ui.text_dim }))
                .child(line)
        }))
}

/// One installed plugin: what it is at the left, Update and Uninstall at the right.
fn plugin_card(index: usize, state: &Plugins, ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let record = &state.records[index];
    let free = plugins::can_start(state);
    let repo = parse_link(&record.repo);
    let mut meta = div()
        .flex()
        .flex_wrap()
        .gap_x(px(8.0))
        .text_size(px(NOTE_SIZE))
        .text_color(hsla(ui.text_dim));
    if let Some(repo) = repo {
        meta = meta.child(
            div()
                .id(("plugin-repo", index))
                .text_color(hsla(ui.accent))
                .cursor_pointer()
                .child(format!("github.com/{}", record.repo))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&repo.page()))),
        );
    }
    meta = meta
        .children(record.description.clone())
        .child(themes_text(record.themes.len()));
    let words = div()
        .flex_1()
        .min_w(px(220.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(8.0))
                .child(div().text_size(px(NAME_SIZE)).child(record.name.clone()))
                .child(
                    div()
                        .text_size(px(NOTE_SIZE))
                        .text_color(hsla(ui.text_dim))
                        .child(version_text(&record.version, &record.installed)),
                ),
        )
        .child(meta);

    // While this plugin is being changed its buttons give way to what is happening.
    let working = match &state.busy {
        Some(b @ (Busy::Removing(id) | Busy::Updating(id))) if *id == record.id => Some(b),
        _ => None,
    };
    let mut buttons = div().flex_none().flex().items_center().gap(px(8.0));
    if let Some(busy) = working {
        buttons = buttons.child(
            div()
                .text_size(px(NOTE_SIZE))
                .text_color(hsla(ui.text_dim))
                .child(busy_text(busy)),
        );
    } else {
        if let Some(newer) = state.updates.get(&record.id) {
            let id = record.id.clone();
            let mut update = action(
                format!("Update to {}", newer.label()),
                free,
                ("plugin-update", index).into(),
                ui,
            );
            if free {
                update = update.on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    plugins::update(&id, window, cx)
                }));
            }
            buttons = buttons.child(update);
        }
        let id = record.id.clone();
        let mut remove = action(
            "Uninstall".into(),
            free,
            ("plugin-uninstall", index).into(),
            ui,
        );
        if free {
            remove = remove.on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                plugins::uninstall(&id, window, cx)
            }));
        }
        buttons = buttons.child(remove);
    }
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(16.0))
        .gap_y(px(10.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(words)
        .child(buttons)
}

/// The installed plugins, split by hairlines; or, with none, how to get one.
pub(crate) fn plugin_cards(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let empty = Plugins::default();
    let state = cx.try_global::<Plugins>().unwrap_or(&empty);
    if state.records.is_empty() {
        return div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_dim))
                    .child("No plugins yet. Paste a GitHub link above."),
            )
            .child(
                div()
                    .id("plugin-making")
                    .text_size(px(text::META))
                    .text_color(hsla(ui.accent))
                    .cursor_pointer()
                    .child("How to make a plugin")
                    .on_click(|_, _, cx| cx.open_url(MAKING_URL)),
            );
    }
    div()
        .flex()
        .flex_col()
        .children((0..state.records.len()).map(|i| {
            plugin_card(i, state, ui, cx)
                .when(i > 0, |c| c.border_t_1().border_color(hsla(ui.divider)))
        }))
}
