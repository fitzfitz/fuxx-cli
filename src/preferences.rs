//! The Preferences window (File > Preferences..., cmd-,): theme, font, size, ligatures and
//! animations, and how sessions are kept, with a live preview of a terminal.
//!
//! Every click changes one line of fuxx's settings file and reloads the appearance, so it applies
//! at once to every session and is still there next time. The choices and the lines written come
//! from the tested pure functions in `prefs` and `settings`; this file is only the GPUI wiring.
//!
//! The window is a two-column form (labels left, controls right) in two sections, Appearance and
//! Sessions. Beside the theme and font lists, the preview paints a fixed terminal screen the way
//! a pane does, in the current theme, font, size and ligatures, so it changes with every pick.

use std::cell::RefCell;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, FocusHandle, FontWeight, Global,
    SharedString, Stateful, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, canvas, div, font, prelude::*, px, size, uniform_list,
};

use crate::appearance::{self, Appearance};
use crate::fonts;
use crate::grid::{self, GridSnapshot};
use crate::prefs::{self, DEFAULT_CHOICE};
use crate::restore::RestoreMode;
use crate::settings;
use crate::sidebar::small_caps;
use crate::terminal::TermDims;
use crate::terminal_view::{hsla, paint_fixed_screen};
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::text;
use crate::workspace::CloseSession;

const WIDTH: f32 = 760.0;
/// Between the window's edges and the form.
const PAD: f32 = 20.0;
/// A section's title line ("Appearance", "Sessions") and the space under it.
const HEADER: f32 = 24.0;
const FIELD_GAP: f32 = 10.0;
/// Above a section's title, after the last field of the one before.
const SECTION_GAP: f32 = 20.0;
/// The labels' column; they are right-aligned against the controls.
const LABEL_WIDTH: f32 = 150.0;
const LABEL_GAP: f32 = 16.0;
/// The controls' column (as wide as the theme list, name and swatch).
const CONTROL_WIDTH: f32 = 260.0;
/// Between the controls and the preview.
const PREVIEW_GAP: f32 = 24.0;
/// The preview takes the rest of the width.
const PREVIEW_WIDTH: f32 =
    WIDTH - 2.0 * PAD - LABEL_WIDTH - LABEL_GAP - CONTROL_WIDTH - PREVIEW_GAP;
/// A row in the theme and font lists.
const LIST_ROW: f32 = 24.0;
/// Inside a list's 1 px border, so its rows' rounded corners clear the box's.
const LIST_INSET: f32 = 2.0;
/// Seven rows, so every built-in theme shows without scrolling.
const THEME_LIST: f32 = list_height(7.0);
const FONT_LIST: f32 = list_height(5.0);
/// The size row's square buttons.
const SIZE_ROW: f32 = 28.0;
/// A switch: a 24 px option inside 2 px of padding and a 1 px border, top and bottom.
const SWITCH: f32 = 30.0;
const SWITCH_INSET: f32 = 2.0;
/// A note under a control: a small gap and one 16 px line.
const NOTE: f32 = 20.0;
/// The preview stands beside the two lists.
const PREVIEW_HEIGHT: f32 = THEME_LIST + FIELD_GAP + FONT_LIST;
/// The fields of each section, top to bottom: what the window's height is made of. `render`
/// draws each field at exactly these heights.
const APPEARANCE_FIELDS: [f32; 5] = [THEME_LIST, FONT_LIST, SIZE_ROW, SWITCH, SWITCH + NOTE];
const SESSIONS_FIELDS: [f32; 2] = [SWITCH, SWITCH + NOTE];
/// The size the monospace test measures glyphs at; any size gives the same answer.
const MEASURE_SIZE: f32 = 13.0;

/// A list `rows` rows tall, with its border and inset.
const fn list_height(rows: f32) -> f32 {
    rows * LIST_ROW + 2.0 * (1.0 + LIST_INSET)
}

/// A section: its title, then its fields with a gap between each.
fn section_height(fields: &[f32]) -> f32 {
    HEADER + fields.iter().sum::<f32>() + FIELD_GAP * fields.len().saturating_sub(1) as f32
}

/// The window's height: the form's sections stacked between the padding.
fn window_height() -> f32 {
    PAD + section_height(&APPEARANCE_FIELDS) + SECTION_GAP + section_height(&SESSIONS_FIELDS) + PAD
}

/// The notes under the switches.
const ANIMATIONS_NOTE: &str = "Also off when macOS Reduce motion is on.";
const KEEP_NOTE: &str = "Takes effect the next time fuxx starts.";

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
                window_size(),
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

/// The window's content size (the title bar is extra).
pub fn window_size() -> gpui::Size<gpui::Pixels> {
    size(px(WIDTH), px(window_height()))
}

/// Where the preview is drawn, as (x, y, width, height) in the window: for the screenshots' checks.
#[cfg(feature = "ui-shots")]
pub(crate) fn preview_area() -> (f32, f32, f32, f32) {
    let x = PAD + LABEL_WIDTH + LABEL_GAP + CONTROL_WIDTH + PREVIEW_GAP;
    (x, PAD + HEADER, PREVIEW_WIDTH, PREVIEW_HEIGHT)
}

/// Where the last note's line is drawn (the form's bottom), as (x, y, width, height).
#[cfg(feature = "ui-shots")]
pub(crate) fn last_note_area() -> (f32, f32, f32, f32) {
    let x = PAD + LABEL_WIDTH + LABEL_GAP;
    (x, window_height() - PAD - 16.0, CONTROL_WIDTH, 16.0)
}

pub struct PreferencesView {
    focus_handle: FocusHandle,
    /// `Default`, then every theme (built-in and the user's), worked out once.
    themes: Vec<String>,
    /// Each theme's swatch (`prefs::swatch`), in the order of `themes`.
    swatches: Vec<[Rgb; 8]>,
    /// `Default`, then the installed fixed-width families, worked out once (measuring is slow).
    fonts: Vec<String>,
    /// Every installed family, to tell which configured family is really in use.
    installed: Vec<String>,
    /// The family `Default` stands for, to draw that row in it.
    default_family: SharedString,
    /// The preview's screen, kept between redraws.
    preview: Rc<PreviewScreen>,
    /// Re-renders the highlighted rows and the colors when the appearance changes.
    _appearance: Subscription,
}

impl PreferencesView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let appearance_sub = cx.observe_global::<Appearance>(|_, cx| cx.notify());

        let appearance = cx.global::<Appearance>();
        let user_dir = appearance.user_theme_dir.clone();
        let themes = prefs::theme_choices(&user_theme_names(user_dir.as_deref()));
        // As the app reads a theme name: the user's folder first, then the built-ins.
        let read_theme = |name: &str| {
            user_dir
                .as_ref()
                .and_then(|dir| std::fs::read_to_string(dir.join(name)).ok())
                .or_else(|| theme::builtin_theme(name).map(str::to_string))
        };
        let swatches = themes
            .iter()
            .map(|name| prefs::swatch(&prefs::choice_theme(name, &read_theme)))
            .collect();
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
        let default_family = fonts::gpui_font(&fonts::from_entries(&[]), &installed).family;

        Self {
            focus_handle,
            themes,
            swatches,
            fonts,
            installed,
            default_family,
            preview: Rc::default(),
            _appearance: appearance_sub,
        }
    }

    /// One scrolling list of choices; the row matching `current` is highlighted, and clicking a
    /// row writes `key` (no value for `Default`, which removes the line). `content` draws a
    /// row's inside from its index and name.
    #[allow(clippy::too_many_arguments)]
    fn choice_list(
        &self,
        id: &'static str,
        key: &'static str,
        height: f32,
        list: fn(&Self) -> &[String],
        current: fn(&Self, &Appearance) -> String,
        content: fn(&Self, usize, &str, UiColors) -> AnyElement,
        ui: UiColors,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let count = list(self).len();
        div()
            .h(px(height))
            .p(px(LIST_INSET))
            .rounded(px(6.0))
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
                                list_row((id, i), is_current, ui)
                                    .child(content(this, i, &name, ui))
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

    /// A theme row: its name, then its swatch.
    fn theme_row(&self, i: usize, name: &str, ui: UiColors) -> AnyElement {
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(name.to_string()),
            )
            .children(self.swatches.get(i).map(|s| swatch(s, ui)))
            .into_any_element()
    }

    /// A font row: its name, drawn in that font.
    fn font_row(&self, _: usize, name: &str, _: UiColors) -> AnyElement {
        let family = if name == DEFAULT_CHOICE {
            self.default_family.clone()
        } else {
            SharedString::from(name.to_string())
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .truncate()
            .font_family(family)
            .child(name.to_string())
            .into_any_element()
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
        let preview_theme = appearance.theme;
        let ui = appearance.theme.ui();
        let size_label = format!("{} pt", format_size(appearance.font_size));
        let ligatures = prefs::ligatures_on(&appearance.font_settings.features);
        let animations = appearance.animations;
        let restore = appearance.restore;
        let persistent = appearance.persistent;

        let theme_list = self.choice_list(
            "theme-list",
            "theme",
            THEME_LIST,
            Self::themes,
            Self::current_theme,
            Self::theme_row,
            ui,
            cx,
        );
        let font_list = self.choice_list(
            "font-list",
            "font-family",
            FONT_LIST,
            Self::fonts,
            Self::current_font,
            Self::font_row,
            ui,
            cx,
        );

        let step_button = |id: &'static str, label: &'static str, step: prefs::FontStep| {
            button(id, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    step_font_size(cx, step);
                }))
        };
        let size_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(step_button(
                "size-down",
                "\u{2212}",
                prefs::FontStep::Smaller,
            ))
            .child(
                div()
                    .min_w(px(56.0))
                    .flex()
                    .justify_center()
                    .font_features(crate::sidebar::tabular())
                    .child(size_label),
            )
            .child(step_button("size-up", "+", prefs::FontStep::Bigger));

        let ligature_option = |id: &'static str, label: &'static str, on: bool| {
            option(id, on == ligatures, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    // Clicking the option already chosen changes nothing.
                    let features = cx.global::<Appearance>().font_settings.features.clone();
                    if prefs::ligatures_on(&features) != on {
                        apply(cx, "font-feature", &prefs::feature_lines(&features, on));
                    }
                }))
        };
        let ligature_switch = switch(ui)
            .child(ligature_option("ligatures-on", "On", true))
            .child(ligature_option("ligatures-off", "Off", false));

        let animation_option = |id: &'static str, label: &'static str, on: bool| {
            option(id, on == animations, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    if cx.global::<Appearance>().animations != on {
                        // On is the default: the absence of the line.
                        apply(cx, "animations", &prefs::animations_values(on));
                    }
                }))
        };
        let animation_switch = switch(ui)
            .child(animation_option("animations-on", "On", true))
            .child(animation_option("animations-off", "Off", false));

        let restore_option = |id: &'static str, label: &'static str, mode: RestoreMode| {
            option(id, mode == restore, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    if cx.global::<Appearance>().restore != mode {
                        // The default (agents) is the absence of the line.
                        let values = if mode == RestoreMode::default() {
                            vec![]
                        } else {
                            vec![mode.as_str().to_string()]
                        };
                        apply(cx, "session-restore", &values);
                    }
                }))
        };
        let restore_switch = switch(ui)
            .child(restore_option("restore-off", "Off", RestoreMode::Off))
            .child(restore_option(
                "restore-layout",
                "Layout",
                RestoreMode::Layout,
            ))
            .child(restore_option(
                "restore-agents",
                "Layout + agents",
                RestoreMode::Agents,
            ));

        let keep_option = |id: &'static str, label: &'static str, on: bool| {
            option(id, on == persistent, ui)
                .child(label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    if cx.global::<Appearance>().persistent != on {
                        // Off is the default: the absence of the line.
                        apply(cx, "persistent-sessions", &prefs::persistence_values(on));
                    }
                }))
        };
        let keep_switch = switch(ui)
            .child(keep_option("keep-on", "On", true))
            .child(keep_option("keep-off", "Off", false));

        // The lists' labels line up with their first row; the others with their control.
        let first_row = LIST_ROW + 2.0 * (1.0 + LIST_INSET);
        let appearance_form = div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(field("Theme", THEME_LIST, first_row, theme_list, ui))
            .child(field("Font", FONT_LIST, first_row, font_list, ui))
            .child(field("Size", SIZE_ROW, SIZE_ROW, size_row, ui))
            .child(field("Ligatures", SWITCH, SWITCH, ligature_switch, ui))
            .child(field(
                "Animations",
                SWITCH + NOTE,
                SWITCH,
                with_note(animation_switch, ANIMATIONS_NOTE, ui),
                ui,
            ));
        let kept = self.preview.clone();
        let preview = div()
            .flex_none()
            .w(px(PREVIEW_WIDTH))
            .h(px(PREVIEW_HEIGHT))
            // Inside the border, clear of its rounded corners: the screen is painted square.
            .p(px(2.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .bg(hsla(preview_theme.background))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let theme = &preview_theme;
                        paint_fixed_screen(theme, bounds, window, cx, |dims| {
                            kept.get(theme, dims, || {
                                grid::fixed_screen(&preview_text(dims.cols), dims, theme)
                            })
                        })
                    },
                )
                .size_full(),
            );

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &CloseSession, window, _| window.remove_window()))
            .flex()
            .flex_col()
            .size_full()
            .p(px(PAD))
            .bg(hsla(ui.sidebar_background))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(heading("Appearance", ui))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(PREVIEW_GAP))
                    .child(appearance_form)
                    .child(
                        div().flex().flex_col().gap(px(6.0)).child(preview).child(
                            div()
                                .text_size(px(text::META))
                                .text_color(hsla(ui.text_faint))
                                .child("Preview"),
                        ),
                    ),
            )
            .child(heading("Sessions", ui).mt(px(SECTION_GAP)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(FIELD_GAP))
                    .child(field(
                        "Restore sessions",
                        SWITCH,
                        SWITCH,
                        restore_switch,
                        ui,
                    ))
                    .child(field(
                        "Keep running after quit",
                        SWITCH + NOTE,
                        SWITCH,
                        with_note(keep_switch, KEEP_NOTE, ui),
                        ui,
                    )),
            )
    }
}

/// The preview's screen, kept between frames: built again only when the theme or the grid
/// (font, size, window) changes, not on every hover redraw.
#[derive(Default)]
struct PreviewScreen(RefCell<Option<(Theme, TermDims, GridSnapshot)>>);

impl PreviewScreen {
    /// The screen for `theme` at `dims`, from `build` unless the last one was for the same.
    fn get(
        &self,
        theme: &Theme,
        dims: TermDims,
        build: impl FnOnce() -> GridSnapshot,
    ) -> GridSnapshot {
        let mut kept = self.0.borrow_mut();
        match &*kept {
            Some((t, d, screen)) if t == theme && *d == dims => screen.clone(),
            _ => {
                let screen = build();
                *kept = Some((*theme, dims, screen.clone()));
                screen
            }
        }
    }
}

/// What the preview shows, for a screen `cols` wide: a prompt, a coloured `ls`, a diff with code
/// that ligatures join (`==`, `!=`, `&&`, `>=`), and an agent asking a question in a box. Every
/// line fits: the `ls` lists what fits, the box is at most 26 columns inside and its text wraps,
/// and below `BOX_MIN_COLS` there is no box. Palette colours only, so it is drawn in the theme's
/// own. The cursor is hidden.
fn preview_text(cols: usize) -> String {
    let prompt = if cols >= 16 {
        "\x1b[34m~/api\x1b[0m $ "
    } else {
        "$ "
    };
    // `ls`: as many entries as fit (at least one), two spaces apart.
    let entries = [
        ("src", "\x1b[1;34m"),
        ("tests", "\x1b[1;34m"),
        ("run.sh", "\x1b[32m"),
        ("Cargo.toml", ""),
    ];
    let mut ls = String::new();
    let mut used = 0;
    for (i, (name, style)) in entries.iter().enumerate() {
        let width = name.len() + if i == 0 { 0 } else { 2 };
        if i > 0 && used + width > cols {
            break;
        }
        let gap = if i == 0 { "" } else { "  " };
        ls.push_str(&format!("{gap}{style}{name}\x1b[0m"));
        used += width;
    }
    let mut lines = vec![
        format!("\x1b[?25l{prompt}ls"),
        ls,
        // 16 lines in all with the box, the rows the preview has at the default size.
        format!("{prompt}git diff"),
        "\x1b[31m- ok = a == b\x1b[0m".to_string(),
        "\x1b[32m+ ok = a != b\x1b[0m".to_string(),
        "\x1b[32m+   && n >= 8\x1b[0m".to_string(),
        format!("{prompt}claude"),
    ];
    if cols >= BOX_MIN_COLS {
        lines.push(String::new());
        lines.extend(agent_box(cols));
    }
    // No newline after the last line: it would take a row, and scroll the first away.
    lines.join("\r\n")
}

/// The narrowest screen the agent's box is drawn on: 11 columns of text ("src/auth.rs"), a
/// space either side and the two borders.
const BOX_MIN_COLS: usize = 15;

/// The agent's question in a box at most `cols` wide (26 columns inside at most), its text
/// wrapped to fit.
fn agent_box(cols: usize) -> Vec<String> {
    let inner = 26.min(cols - 2);
    let width = inner - 2;
    let edge = |l: char, r: char| format!("\x1b[35m{l}{}{r}\x1b[0m", "─".repeat(inner));
    let line = |text: &str, style: &str| {
        let pad = width - text.chars().count();
        format!(
            "\x1b[35m│\x1b[0m {style}{text}\x1b[0m{} \x1b[35m│\x1b[0m",
            " ".repeat(pad)
        )
    };
    let mut lines = vec![edge('╭', '╮')];
    let bold = "\x1b[1m";
    lines.extend(
        wrap("Edit src/auth.rs", width)
            .iter()
            .map(|t| line(t, bold)),
    );
    lines.push(line("", ""));
    let question = "Do you want to make this edit to auth.rs?";
    lines.extend(wrap(question, width).iter().map(|t| line(t, "")));
    lines.extend(
        wrap("\u{276f} 1. Yes", width)
            .iter()
            .map(|t| line(t, "\x1b[36m")),
    );
    lines.extend(wrap("  2. No", width).iter().map(|t| line(t, "")));
    lines.push(edge('╰', '╯'));
    lines
}

/// `text` in lines at most `width` wide, broken between words; a word too long for a line is
/// cut. Leading spaces (the options' indent) are kept on the first line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let words = text.trim_start();
    let mut lines = Vec::new();
    let mut line = text[..text.len() - words.len()].to_string();
    let mut has_word = false;
    for word in words.split(' ') {
        let mut word: Vec<char> = word.chars().collect();
        loop {
            let used = line.chars().count() + usize::from(has_word);
            if used + word.len() <= width {
                if has_word {
                    line.push(' ');
                }
                line.extend(&word);
                has_word = true;
                break;
            }
            if !line.is_empty() {
                // Onto a new line.
                lines.push(std::mem::take(&mut line));
                has_word = false;
                continue;
            }
            let rest = word.split_off(width);
            lines.push(word.into_iter().collect());
            word = rest;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A section title: small capitals, then a quiet line to the right edge.
fn heading(title: &'static str, ui: UiColors) -> Div {
    div()
        .flex_none()
        .h(px(HEADER))
        .flex()
        .flex_row()
        .items_start()
        .gap(px(8.0))
        .child(
            div()
                .h(px(16.0))
                .flex()
                .items_center()
                .text_size(px(text::GROUP_HEADER))
                .font_weight(FontWeight::SEMIBOLD)
                .font_features(small_caps())
                .text_color(hsla(ui.text_dim))
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .h(px(16.0))
                .flex()
                .items_center()
                .child(div().w_full().h(px(1.0)).bg(hsla(ui.sidebar_border))),
        )
}

/// One row of the form: the label, right-aligned and centred on the control's first
/// `label_height` px, then the control. Always `height` tall.
fn field(
    label: &'static str,
    height: f32,
    label_height: f32,
    control: impl IntoElement,
    ui: UiColors,
) -> Div {
    div()
        .flex_none()
        .h(px(height))
        .flex()
        .flex_row()
        .gap(px(LABEL_GAP))
        .child(
            div()
                .flex_none()
                .w(px(LABEL_WIDTH))
                .h(px(label_height))
                .flex()
                .items_center()
                .justify_end()
                .text_color(hsla(ui.text_dim))
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .w(px(CONTROL_WIDTH))
                .flex()
                .flex_col()
                .child(control),
        )
}

/// A switch with a one-line note under it.
fn with_note(control: impl IntoElement, note: &'static str, ui: UiColors) -> Div {
    div()
        .flex()
        .flex_col()
        .child(div().flex().child(control))
        .child(
            div()
                .mt(px(NOTE - 16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .text_size(px(text::META))
                .text_color(hsla(ui.text_dim))
                .child(note),
        )
}

/// The box a switch's options sit in, side by side.
fn switch(ui: UiColors) -> Div {
    div()
        .flex_none()
        .h(px(SWITCH))
        .flex()
        .flex_row()
        .p(px(SWITCH_INSET))
        .gap(px(SWITCH_INSET))
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(ui.sidebar_border))
}

/// One option of a switch, filled when it is the current choice.
fn option(id: &'static str, current: bool, ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(SWITCH - 2.0 * (1.0 + SWITCH_INSET)))
        .flex()
        .items_center()
        .px_3()
        .rounded(px(4.0))
        .text_color(hsla(if current { ui.text } else { ui.text_dim }))
        .when(current, |r| r.bg(hsla(ui.row_active)))
        .hover(|s| s.bg(hsla(ui.row_hover)))
        .cursor_pointer()
}

/// A row of the theme or font list. The current one has the selection colour and an accent
/// edge, as the sidebar's active row does.
fn list_row(id: impl Into<gpui::ElementId>, current: bool, ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        // The list lays each row out on its own, so it would otherwise shrink to its content.
        .w_full()
        .h(px(LIST_ROW))
        .flex()
        .items_center()
        .px(px(10.0))
        .rounded(px(4.0))
        .text_color(hsla(ui.text))
        .when(current, |r| {
            r.bg(hsla(ui.selection)).child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(5.0))
                    .bottom(px(5.0))
                    .w(px(2.0))
                    .rounded(px(1.0))
                    .bg(hsla(ui.accent)),
            )
        })
        .when(!current, |r| r.hover(|s| s.bg(hsla(ui.row_hover))))
        .cursor_pointer()
}

/// A theme's colours as a little strip: its background, with the foreground and palette 1-6
/// on it as chips.
fn swatch(colours: &[Rgb; 8], ui: UiColors) -> Div {
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(hsla(ui.sidebar_border))
        .bg(hsla(colours[0]))
        .children(
            colours[1..]
                .iter()
                .map(|&c| div().size(px(10.0)).rounded(px(2.0)).bg(hsla(c))),
        )
}

/// A small square push button (there is no built-in button widget).
fn button(id: &'static str, ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(SIZE_ROW))
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
    if let Err(err) = write_setting(&path, key, values) {
        eprintln!("[fuxx] preferences: {err}");
        return;
    }
    appearance::reload(cx);
}

/// Rewrite only `key`'s lines in the settings file at `path` (created if missing).
fn write_setting(path: &Path, key: &str, values: &[String]) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        // Writing anyway would replace the user's file with only this one setting.
        Err(err) => return Err(format!("could not read {}: {err}", path.display())),
    };
    let text = settings::set_key(&text, key, values);
    settings::write_atomic(path, &text)
        .map_err(|err| format!("could not write {}: {err}", path.display()))
}

/// The View menu's Bigger, Smaller and Actual Size: write the new `font-size` as the
/// Preferences window does, so it applies to every session and is kept.
pub fn step_font_size(cx: &mut App, step: prefs::FontStep) {
    let Some(path) = cx.global::<Appearance>().settings_path.clone() else {
        eprintln!("[fuxx] preferences: $HOME is unset, so there is no settings file to change");
        return;
    };
    let size = cx.global::<Appearance>().font_size;
    if let Err(err) = write_font_step(&path, size, step) {
        eprintln!("[fuxx] preferences: {err}");
        return;
    }
    appearance::reload(cx);
}

/// Write the `font-size` that `step` from `size` gives to the settings file at `path`.
fn write_font_step(path: &Path, size: f32, step: prefs::FontStep) -> Result<(), String> {
    let size = prefs::font_size_for(size, step);
    write_setting(path, "font-size", &[format_size(size)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::FontStep;

    /// `text` without its escape sequences (CSI `ESC [ ... letter`), as it shows on screen.
    fn visible(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip '[' and the parameters, up to and including the final letter.
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn preview_text_fits_its_width() {
        for cols in [80, 40, 30, 28, 24, 20, 16, 15] {
            let text = visible(&preview_text(cols));
            let lines: Vec<&str> = text.split("\r\n").collect();
            for line in &lines {
                assert!(
                    line.chars().count() <= cols,
                    "{cols} columns: {line:?} is too wide"
                );
            }
            // The agent's box is whole: one top and one bottom edge, every line between them
            // closed on both sides, all as wide as each other.
            let top = lines.iter().position(|l| l.starts_with('╭'));
            let bottom = lines.iter().position(|l| l.starts_with('╰'));
            let (Some(top), Some(bottom)) = (top, bottom) else {
                panic!("{cols} columns: no box in {lines:#?}");
            };
            let width = lines[top].chars().count();
            assert!(
                lines[top].ends_with('╮') && lines[bottom].ends_with('╯'),
                "{cols}"
            );
            for line in &lines[top + 1..bottom] {
                assert!(
                    line.starts_with('│') && line.ends_with('│'),
                    "{cols}: {line:?}"
                );
                assert_eq!(line.chars().count(), width, "{cols}: {line:?}");
            }
            // The question is all there, however it wraps.
            let inside: Vec<&str> = lines[top + 1..bottom]
                .iter()
                .map(|l| l.trim_matches('│').trim())
                .collect();
            assert!(
                inside
                    .join(" ")
                    .contains("Do you want to make this edit to auth.rs?"),
                "{cols}: {inside:?}"
            );
            // The ligature samples are never cut.
            for sample in ["a == b", "a != b", "&& n >= 8"] {
                assert!(text.contains(sample), "{cols}: {sample:?} is missing");
            }
        }
        // Too narrow for the box: no box, still a prompt.
        for cols in [14, 8, 2] {
            let text = visible(&preview_text(cols));
            assert!(
                !text.contains('╭') && !text.contains('│'),
                "{cols}: {text:?}"
            );
            assert!(text.contains("$ claude"), "{cols}");
        }
        // Wide enough, the box is as wide as before (26 inside), not the whole screen.
        let wide = visible(&preview_text(80));
        let top = wide.split("\r\n").find(|l| l.starts_with('╭')).unwrap();
        assert_eq!(top.chars().count(), 28);
    }

    #[test]
    fn preview_screen_is_built_once_per_theme_and_size() {
        let cache = PreviewScreen::default();
        let builds = std::cell::Cell::new(0);
        let dims = TermDims { cols: 30, rows: 16 };
        let theme = Theme::default();
        let get = |theme: &Theme, dims: TermDims| {
            cache.get(theme, dims, || {
                builds.set(builds.get() + 1);
                grid::fixed_screen(&preview_text(dims.cols), dims, theme)
            })
        };
        let first = get(&theme, dims);
        let again = get(&theme, dims);
        assert_eq!(
            builds.get(),
            1,
            "a redraw with nothing changed rebuilt the screen"
        );
        assert_eq!(first.lines, again.lines);
        // A new size (font, size or window) or a new theme builds it again.
        get(&theme, TermDims { cols: 24, rows: 13 });
        assert_eq!(builds.get(), 2);
        let mut other = theme;
        other.background = other.foreground;
        get(&other, TermDims { cols: 24, rows: 13 });
        assert_eq!(builds.get(), 3);
    }

    #[test]
    fn window_height_is_computed_from_the_form() {
        // Appearance: its title, the theme list (seven rows, so every built-in shows without
        // scrolling) and the font list (five), each with its border and inset (2 x 3 px), the
        // size buttons, two switches and the note under the second, with a gap between fields.
        let (theme_list, font_list) = (7.0 * LIST_ROW + 6.0, 5.0 * LIST_ROW + 6.0);
        let appearance =
            HEADER + theme_list + font_list + SIZE_ROW + SWITCH + (SWITCH + NOTE) + 4.0 * FIELD_GAP;
        // Sessions: its title, the restore switch, the keep-running switch and its note.
        let sessions = HEADER + SWITCH + (SWITCH + NOTE) + FIELD_GAP;
        assert_eq!(
            window_height(),
            PAD + appearance + SECTION_GAP + sessions + PAD
        );
        // As the constants are today.
        assert_eq!(window_height(), 646.0);
        // The preview sits beside the two lists.
        assert_eq!(PREVIEW_HEIGHT, theme_list + FIELD_GAP + font_list);
    }

    #[test]
    fn font_size_steps_write_the_setting() {
        let dir = std::env::temp_dir().join(format!("fuxx-font-step-{}", std::process::id()));
        let path = dir.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "# mine\ntheme = Nord\nfont-size = 14\n").unwrap();
        let read = || std::fs::read_to_string(&path).unwrap();

        write_font_step(&path, 14.0, FontStep::Bigger).unwrap();
        assert_eq!(read(), "# mine\ntheme = Nord\nfont-size = 15\n");
        write_font_step(&path, 15.0, FontStep::Smaller).unwrap();
        assert_eq!(read(), "# mine\ntheme = Nord\nfont-size = 14\n");
        // Actual Size: the default size (13), written out.
        write_font_step(&path, 14.0, FontStep::ActualSize).unwrap();
        assert_eq!(read(), "# mine\ntheme = Nord\nfont-size = 13\n");
        // A fractional size steps from its rounded value, and is written without ".0".
        write_font_step(&path, 13.4, FontStep::Bigger).unwrap();
        assert_eq!(read(), "# mine\ntheme = Nord\nfont-size = 14\n");

        // No file yet: it is created with only that line.
        let fresh = dir.join("fresh/config");
        write_font_step(&fresh, 13.0, FontStep::Smaller).unwrap();
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "font-size = 12\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
