//! The Preferences window (File > Preferences..., cmd-,): theme, font, size and ligatures.
//!
//! Every click changes one line of fuxx's settings file and reloads the appearance, so it applies
//! at once to every session and is still there next time. The choices and the lines written come
//! from the tested pure functions in `prefs` and `settings`; this file is only the GPUI wiring.

use std::ops::Range;
use std::path::Path;

use gpui::{
    App, Bounds, ClickEvent, Context, Div, FocusHandle, Global, SharedString, Stateful,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    div, font, prelude::*, px, size, uniform_list,
};

use crate::appearance::{self, Appearance};
use crate::prefs::{self, DEFAULT_CHOICE};
use crate::settings;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::workspace::CloseSession;

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 660.0;
const LIST_HEIGHT: f32 = 180.0;
const ROW_HEIGHT: f32 = 24.0;
/// The size the monospace test measures glyphs at; any size gives the same answer.
const MEASURE_SIZE: f32 = 13.0;

/// The open Preferences window, if any, so cmd-, brings it to the front instead of opening another.
struct PreferencesWindow(Option<WindowHandle<PreferencesView>>);

impl Global for PreferencesWindow {}

/// Bring the Preferences window to the front, or open it if it isn't open.
pub fn open_or_activate(cx: &mut App) {
    // Action handlers run while the active window is checked out of the app, and a window that is
    // checked out can't be updated: with Preferences in front, the update below would fail and a
    // second window would open. Deferred, this runs once the current update is over.
    cx.defer(|cx| {
        if let Some(handle) = cx.try_global::<PreferencesWindow>().and_then(|p| p.0)
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(WIDTH), px(HEIGHT)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Preferences".into()),
                ..Default::default()
            }),
            kind: WindowKind::Normal,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| PreferencesView::new(window, cx))
        }) {
            Ok(handle) => cx.set_global(PreferencesWindow(Some(handle))),
            Err(err) => eprintln!("[fuxx] preferences: could not open the window: {err}"),
        }
    });
}

pub struct PreferencesView {
    focus_handle: FocusHandle,
    /// `Default`, then every theme (built-in and the user's), worked out once.
    themes: Vec<String>,
    /// `Default`, then the installed fixed-width families, worked out once (measuring is slow).
    fonts: Vec<String>,
    /// Every installed family, to tell which configured family is really in use.
    installed: Vec<String>,
    /// Re-renders the highlighted rows and the colors when the appearance changes.
    _appearance: Subscription,
}

impl PreferencesView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let appearance_sub = cx.observe_global::<Appearance>(|_, cx| cx.notify());

        let appearance = cx.global::<Appearance>();
        let themes = prefs::theme_choices(&user_theme_names(appearance.user_theme_dir.as_deref()));
        let families = appearance.font_settings.families.clone();

        let text_system = cx.text_system().clone();
        let installed = text_system.all_font_names();
        // Monospace: a narrow "i" advances as far as a wide "m". A missing glyph means "not mono".
        let is_mono = |name: &str| {
            let id = text_system.resolve_font(&font(SharedString::from(name.to_string())));
            let size = px(MEASURE_SIZE);
            match (
                text_system.advance(id, size, 'i'),
                text_system.advance(id, size, 'm'),
            ) {
                (Ok(i), Ok(m)) => prefs::is_monospace(i.width.into(), m.width.into()),
                _ => false,
            }
        };
        let current = current_font(&families, &installed);
        let fonts = prefs::font_choices(&installed, &is_mono, current);

        Self {
            focus_handle,
            themes,
            fonts,
            installed,
            _appearance: appearance_sub,
        }
    }

    /// One scrolling list of choices; the row matching `current` is highlighted, and clicking a
    /// row writes `key` (no value for `Default`, which removes the line).
    fn choice_list(
        &self,
        id: &'static str,
        key: &'static str,
        list: fn(&Self) -> &[String],
        current: fn(&Self, &Appearance) -> String,
        ui: UiColors,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let count = list(self).len();
        div()
            .h(px(LIST_HEIGHT))
            .rounded_md()
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .child(
                uniform_list(
                    id,
                    count,
                    cx.processor(move |this, range: Range<usize>, _window, cx| {
                        let selected = current(this, cx.global::<Appearance>());
                        let names = list(this);
                        range
                            .filter_map(|i| names.get(i).map(|name| (i, name.clone())))
                            .map(|(i, name)| {
                                let is_current = name.eq_ignore_ascii_case(&selected);
                                let values: Vec<String> = if name == DEFAULT_CHOICE {
                                    vec![]
                                } else {
                                    vec![name.clone()]
                                };
                                row((id, i), is_current, ui)
                                    .h(px(ROW_HEIGHT))
                                    .child(name)
                                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                        apply(cx, key, &values)
                                    }))
                            })
                            .collect()
                    }),
                )
                .size_full(),
            )
    }

    fn current_theme(&self, a: &Appearance) -> String {
        a.theme_name
            .clone()
            .unwrap_or_else(|| DEFAULT_CHOICE.to_string())
    }

    fn current_font(&self, a: &Appearance) -> String {
        current_font(&a.font_settings.families, &self.installed)
            .unwrap_or(DEFAULT_CHOICE)
            .to_string()
    }

    fn themes(&self) -> &[String] {
        &self.themes
    }

    fn fonts(&self) -> &[String] {
        &self.fonts
    }
}

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = cx.global::<Appearance>();
        let ui = appearance.theme.ui();
        let size_label = format!("{} pt", format_size(appearance.font_size));
        let ligatures = prefs::ligatures_on(&appearance.font_settings.features);

        let theme_list = self.choice_list(
            "theme-list",
            "theme",
            Self::themes,
            Self::current_theme,
            ui,
            cx,
        );
        let font_list = self.choice_list(
            "font-list",
            "font-family",
            Self::fonts,
            Self::current_font,
            ui,
            cx,
        );

        let step_button = |id: &'static str, label: &'static str, delta: i32| {
            button(id, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    let size = prefs::step_size(cx.global::<Appearance>().font_size, delta);
                    apply(cx, "font-size", &[format_size(size)]);
                }))
        };
        let size_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(step_button("size-down", "\u{2212}", -1))
            .child(
                div()
                    .min_w(px(56.0))
                    .flex()
                    .justify_center()
                    .child(size_label),
            )
            .child(step_button("size-up", "+", 1));

        let ligature_option = |id: &'static str, label: &'static str, on: bool| {
            let chosen = on == ligatures;
            row(id, chosen, ui)
                .px_3()
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    // Clicking the option already chosen changes nothing.
                    let features = cx.global::<Appearance>().font_settings.features.clone();
                    if prefs::ligatures_on(&features) != on {
                        apply(cx, "font-feature", &prefs::feature_lines(&features, on));
                    }
                }))
        };
        let ligature_switch = div()
            .flex()
            .flex_row()
            .p_0p5()
            .gap_0p5()
            .rounded_md()
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .child(ligature_option("ligatures-on", "On", true))
            .child(ligature_option("ligatures-off", "Off", false));

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &CloseSession, window, _| window.remove_window()))
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_2()
            .bg(hsla(ui.sidebar_background))
            .text_sm()
            .text_color(hsla(ui.text))
            .child(heading("Theme", ui))
            .child(theme_list)
            .child(heading("Font", ui).mt_2())
            .child(font_list)
            .child(heading("Size", ui).mt_2())
            .child(size_row)
            .child(heading("Ligatures", ui).mt_2())
            .child(div().flex().child(ligature_switch))
    }
}

/// A section title.
fn heading(text: &'static str, ui: UiColors) -> Div {
    div().text_color(hsla(ui.text_dim)).child(text)
}

/// A clickable row (a list entry or a switch option), highlighted when it is the current choice.
fn row(id: impl Into<gpui::ElementId>, current: bool, ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .px_2()
        .py_0p5()
        .rounded_sm()
        .text_color(hsla(if current { ui.text } else { ui.text_dim }))
        .when(current, |r| r.bg(hsla(ui.row_active)))
        .hover(|s| s.bg(hsla(ui.row_hover)))
        .cursor_pointer()
}

/// A small square push button (there is no built-in button widget).
fn button(id: &'static str, ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded_md()
        .border_1()
        .border_color(hsla(ui.sidebar_border))
        .hover(|s| s.bg(hsla(ui.row_hover)))
        .cursor_pointer()
}

/// The names of the files in the user's theme folder (none if there is no folder). Hidden files
/// such as `.DS_Store` are not themes.
fn user_theme_names(dir: Option<&Path>) -> Vec<String> {
    let Some(entries) = dir.and_then(|d| std::fs::read_dir(d).ok()) else {
        return vec![];
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect()
}

/// The font family in use: the first configured one that is installed (as `fonts::gpui_font`
/// picks it), or `None` for the default.
fn current_font<'a>(families: &'a [String], installed: &[String]) -> Option<&'a str> {
    families
        .iter()
        .find(|f| installed.contains(f))
        .map(String::as_str)
}

/// A size as written to the file: whole sizes without a trailing ".0" (14, not 14.0), others as-is.
fn format_size(size: f32) -> String {
    // `Display` for f32 already omits ".0" for whole numbers and keeps the fraction otherwise.
    format!("{size}")
}

/// Change one setting: rewrite only its lines in the settings file, then reload the appearance so
/// every view picks it up. On any failure, log one line and keep the current look.
fn apply(cx: &mut App, key: &str, values: &[String]) {
    let Some(path) = cx.global::<Appearance>().settings_path.clone() else {
        eprintln!("[fuxx] preferences: $HOME is unset, so there is no settings file to change");
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            // Writing anyway would replace the user's file with only this one setting.
            eprintln!(
                "[fuxx] preferences: could not read {}: {err}",
                path.display()
            );
            return;
        }
    };
    let text = settings::set_key(&text, key, values);
    if let Err(err) = settings::write_atomic(&path, &text) {
        eprintln!(
            "[fuxx] preferences: could not write {}: {err}",
            path.display()
        );
        return;
    }
    appearance::reload(cx);
}
