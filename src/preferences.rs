//! The Preferences window (File > Preferences..., cmd-,): animations and the tips card, theme,
//! font, size and ligatures with a live preview of a terminal, how sessions are kept, and the
//! browser windows' address bar and saved data.
//!
//! Every click changes one line of fuxx's settings file and reloads the appearance, so it applies
//! at once to every session and is still there next time. The choices and the lines written come
//! from the tested pure functions in `prefs` and `settings`; this file is only the GPUI wiring.
//!
//! Like VS Code's settings, a list of sections runs down the left (General, Appearance, Sessions,
//! Browser) and only the chosen section's settings show beside it, as a two-column form (labels
//! left, controls right). The window keeps one size, the largest section's, and opens on the
//! section last used in this run. Beside the theme and font lists, the preview paints a fixed
//! terminal screen the way a pane does, in the current theme, font, size and ligatures, so it
//! changes with every pick.

use std::cell::RefCell;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, ElementId, FocusHandle, FontWeight, Global,
    KeyDownEvent, SharedString, Stateful, Subscription, TitlebarOptions, WeakEntity, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, canvas, div, font, prelude::*, px, size,
    uniform_list,
};

use crate::appearance::{self, Appearance, CursorFun, CursorShapeSetting};
use crate::fonts;
use crate::grid::{self, GridSnapshot};
use crate::prefs::{self, DEFAULT_CHOICE};
use crate::restore::RestoreMode;
use crate::settings;
use crate::sidebar::small_caps;
use crate::terminal::TermDims;
use crate::terminal_view::{hsla, paint_fixed_screen};
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::{size::SIDEBAR_ROW, text};
use crate::workspace::{CloseSession, Workspace};

/// The section list down the left, its 1 px edge included.
pub(crate) const SECTIONS_WIDTH: f32 = 160.0;
/// Inside the section list, around its rows.
const SECTIONS_INSET: f32 = 8.0;
/// A section's row in the list, as tall as a sidebar row, and the gap between rows.
const SECTION_ROW: f32 = SIDEBAR_ROW;
const SECTION_ROW_GAP: f32 = 2.0;
/// Where the first row starts: its name lines up with the section's title beside it.
const SECTIONS_TOP: f32 = PAD - (SECTION_ROW - 16.0) / 2.0;
/// The chosen section's form, beside the list: as wide as the one-page window was.
const FORM_WIDTH: f32 = 760.0;
const WIDTH: f32 = SECTIONS_WIDTH + FORM_WIDTH;
/// Between the window's edges (and the section list) and the form.
const PAD: f32 = 20.0;
/// A section's title line ("Appearance", "Sessions") and the space under it.
const HEADER: f32 = 24.0;
const FIELD_GAP: f32 = 10.0;
/// The labels' column; they are right-aligned against the controls.
const LABEL_WIDTH: f32 = 150.0;
const LABEL_GAP: f32 = 16.0;
/// The controls' column (as wide as the theme list, name and swatch).
const CONTROL_WIDTH: f32 = 260.0;
/// A note under a switch may run the rest of the form's width: the sections with notes have no
/// preview beside them.
const NOTE_WIDTH: f32 = FORM_WIDTH - 2.0 * PAD - LABEL_WIDTH - LABEL_GAP;
/// Between the controls and the preview.
const PREVIEW_GAP: f32 = 24.0;
/// The preview takes the rest of the width.
const PREVIEW_WIDTH: f32 =
    FORM_WIDTH - 2.0 * PAD - LABEL_WIDTH - LABEL_GAP - CONTROL_WIDTH - PREVIEW_GAP;
/// A row in the theme and font lists.
const LIST_ROW: f32 = 24.0;
/// Inside a list's 1 px border, so its rows' rounded corners clear the box's.
const LIST_INSET: f32 = 2.0;
/// Seven rows, so every built-in theme shows without scrolling.
const THEME_LIST: f32 = list_height(7.0);
const FONT_LIST: f32 = list_height(5.0);
/// The size row's square buttons, and the tips card's button.
const SIZE_ROW: f32 = 28.0;
/// A switch: a 24 px option inside 2 px of padding and a 1 px border, top and bottom.
const SWITCH: f32 = 30.0;
const SWITCH_INSET: f32 = 2.0;
/// A note under a control: a small gap and one 16 px line; each further line adds `NOTE_LINE`.
const NOTE: f32 = 20.0;
const NOTE_LINE: f32 = 16.0;
/// The preview stands beside the two lists.
const PREVIEW_HEIGHT: f32 = THEME_LIST + FIELD_GAP + FONT_LIST;
/// The "Preview" line under it, and the gap above that.
const CAPTION: f32 = 6.0 + 16.0;
/// The size the monospace test measures glyphs at; any size gives the same answer.
const MEASURE_SIZE: f32 = 13.0;

/// A list `rows` rows tall, with its border and inset.
const fn list_height(rows: f32) -> f32 {
    rows * LIST_ROW + 2.0 * (1.0 + LIST_INSET)
}

/// A section's part of the window: its title, then its fields with a gap between each.
fn body_height(section: Section) -> f32 {
    let fields = section.controls();
    HEADER
        + fields.iter().map(|c| c.height()).sum::<f32>()
        + FIELD_GAP * fields.len().saturating_sub(1) as f32
}

/// The window's height: the tallest section between the padding, so every section fits and the
/// window never changes size.
fn window_height() -> f32 {
    let tallest = Section::ALL
        .map(body_height)
        .into_iter()
        .fold(0.0, f32::max);
    PAD + tallest + PAD
}

/// The notes under the switches.
const ANIMATIONS_NOTE: &str = "Also off when macOS Reduce motion is on.";
const CHECK_UPDATES_NOTE: &str = "Asks GitHub once a day whether a newer fuxx is out.";
const KEEP_NOTE: &str = "Takes effect the next time fuxx starts.";
const CURSOR_NOTE: &str =
    "Programs that set their own cursor (vim, for example) still choose their shape.";
/// A group's heading inside a section (Appearance's "Cursor"): one 16 px line.
const GROUP_HEADER: f32 = 16.0;
/// One sentence a line, so it never wraps on its own.
const KEEP_DATA_NOTE: [&str; 2] = [
    "Turning this off deletes saved browser data.",
    "Applies to browser windows opened after the change.",
];

/// A part of Preferences, shown on its own beside the section list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Section {
    #[default]
    General,
    Appearance,
    Sessions,
    Browser,
}

impl Section {
    /// In the list's order, top to bottom.
    pub(crate) const ALL: [Section; 4] = [
        Section::General,
        Section::Appearance,
        Section::Sessions,
        Section::Browser,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Sessions => "Sessions",
            Section::Browser => "Browser",
        }
    }

    /// The section's controls, top to bottom: what `render` draws, and what its height is made
    /// of. Each control is in exactly one section.
    fn controls(self) -> &'static [Control] {
        use Control::*;
        match self {
            Section::General => &[
                Switch(OnOff::Animations),
                Switch(OnOff::CheckUpdates),
                TipsCard,
            ],
            Section::Appearance => &[
                Theme,
                Font,
                Size,
                Ligatures,
                CursorHeading,
                CursorStyle,
                Switch(OnOff::CursorGlide),
                Switch(OnOff::CursorGlow),
                Switch(OnOff::CursorColor),
                CursorFun,
            ],
            Section::Sessions => &[Restore, Switch(OnOff::KeepRunning)],
            Section::Browser => &[Switch(OnOff::AddressBar), Switch(OnOff::KeepData)],
        }
    }

    /// The next section down (`down`) or up the list; the ends stay put.
    fn step(self, down: bool) -> Section {
        let i = Section::ALL.iter().position(|&s| s == self).unwrap_or(0);
        let i = if down {
            (i + 1).min(Section::ALL.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        Section::ALL[i]
    }
}

/// One row of a section's form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Theme,
    Font,
    Size,
    Ligatures,
    Restore,
    /// "Show the tips card again": not a setting, the sidebar's card (`hint_dismissed`).
    TipsCard,
    /// The "Cursor" group's heading, over the cursor's rows (M11f).
    CursorHeading,
    /// The cursor's shape: Bar, Underline, Hollow or Block (`cursor-style`).
    CursorStyle,
    /// Off, Trail or Sparks (`cursor-fun`).
    CursorFun,
    Switch(OnOff),
}

impl Control {
    fn label(self) -> &'static str {
        match self {
            Control::Theme => "Theme",
            Control::Font => "Font",
            Control::Size => "Size",
            Control::Ligatures => "Ligatures",
            Control::Restore => "Restore sessions",
            Control::TipsCard => "Tips card",
            Control::CursorHeading => "Cursor",
            Control::CursorStyle => "Style",
            Control::CursorFun => "Fun",
            Control::Switch(OnOff::Animations) => "Animations",
            Control::Switch(OnOff::CheckUpdates) => "Check for updates",
            Control::Switch(OnOff::KeepRunning) => "Keep running after quit",
            Control::Switch(OnOff::AddressBar) => "Show address bar",
            Control::Switch(OnOff::KeepData) => "Keep history and data",
            Control::Switch(OnOff::CursorGlide) => "Glide",
            Control::Switch(OnOff::CursorGlow) => "Breathing glow",
            Control::Switch(OnOff::CursorColor) => "Colour follows the agent",
        }
    }

    /// The lines under the control.
    fn note(self) -> &'static [&'static str] {
        match self {
            Control::Switch(s) => s.note(),
            Control::CursorStyle => &[CURSOR_NOTE],
            _ => &[],
        }
    }

    /// The row's height: `render` draws it at exactly this.
    fn height(self) -> f32 {
        match self {
            Control::Theme => THEME_LIST,
            Control::Font => FONT_LIST,
            Control::Size | Control::TipsCard => SIZE_ROW,
            Control::CursorHeading => GROUP_HEADER,
            Control::Ligatures
            | Control::Restore
            | Control::CursorStyle
            | Control::CursorFun
            | Control::Switch(_) => match self.note().len() {
                0 => SWITCH,
                lines => SWITCH + NOTE + NOTE_LINE * (lines - 1) as f32,
            },
        }
    }

    /// How tall the box the label is centred in is: a list's first row, or the control itself.
    fn label_height(self) -> f32 {
        match self {
            Control::Theme | Control::Font => LIST_ROW + 2.0 * (1.0 + LIST_INSET),
            Control::Size | Control::TipsCard => SIZE_ROW,
            Control::CursorHeading => GROUP_HEADER,
            Control::Ligatures
            | Control::Restore
            | Control::CursorStyle
            | Control::CursorFun
            | Control::Switch(_) => SWITCH,
        }
    }
}

/// The settings that are an On / Off switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnOff {
    Animations,
    CheckUpdates,
    KeepRunning,
    AddressBar,
    KeepData,
    CursorGlide,
    CursorGlow,
    /// "Colour follows the agent": on is `cursor-color = status` (the default).
    CursorColor,
}

impl OnOff {
    /// The settings line it writes.
    fn key(self) -> &'static str {
        match self {
            OnOff::Animations => "animations",
            OnOff::CheckUpdates => "check-updates",
            OnOff::KeepRunning => "persistent-sessions",
            OnOff::AddressBar => "browser-address-bar",
            OnOff::KeepData => "browser-keep-data",
            OnOff::CursorGlide => "cursor-glide",
            OnOff::CursorGlow => "cursor-glow",
            OnOff::CursorColor => "cursor-color",
        }
    }

    /// The values to write for `on`: none for the default, which removes the line.
    fn values(self, on: bool) -> Vec<String> {
        match self {
            OnOff::Animations => prefs::animations_values(on),
            OnOff::CheckUpdates => prefs::check_updates_values(on),
            OnOff::KeepRunning => prefs::persistence_values(on),
            OnOff::AddressBar => prefs::browser_address_bar_values(on),
            OnOff::KeepData => prefs::browser_keep_data_values(on),
            OnOff::CursorGlide => prefs::cursor_glide_values(on),
            OnOff::CursorGlow => prefs::cursor_glow_values(on),
            OnOff::CursorColor => prefs::cursor_color_values(on),
        }
    }

    /// Whether it is on now.
    fn current(self, a: &Appearance) -> bool {
        match self {
            OnOff::Animations => a.animations,
            OnOff::CheckUpdates => a.check_updates,
            OnOff::KeepRunning => a.persistent,
            OnOff::AddressBar => a.browser_address_bar,
            OnOff::KeepData => a.browser_keep_data,
            OnOff::CursorGlide => a.cursor_glide,
            OnOff::CursorGlow => a.cursor_glow,
            OnOff::CursorColor => a.cursor_follows_status,
        }
    }

    /// The lines under the switch.
    fn note(self) -> &'static [&'static str] {
        match self {
            OnOff::Animations => &[ANIMATIONS_NOTE],
            OnOff::CheckUpdates => &[CHECK_UPDATES_NOTE],
            OnOff::KeepRunning => &[KEEP_NOTE],
            OnOff::AddressBar => &[],
            OnOff::KeepData => &KEEP_DATA_NOTE,
            OnOff::CursorGlide | OnOff::CursorGlow | OnOff::CursorColor => &[],
        }
    }
}

/// The open Preferences window, if any, so cmd-, brings it to the front instead of opening another.
struct PreferencesWindow(Option<WindowHandle<PreferencesView>>);

impl Global for PreferencesWindow {}

/// The section last shown, so Preferences opens on it again for the rest of the run.
#[derive(Default)]
struct LastSection(Section);

impl Global for LastSection {}

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
    let x = SECTIONS_WIDTH + PAD + LABEL_WIDTH + LABEL_GAP + CONTROL_WIDTH + PREVIEW_GAP;
    (x, PAD + HEADER, PREVIEW_WIDTH, PREVIEW_HEIGHT)
}

/// Where the tallest section's last control is drawn (the form's bottom), as (x, y, width,
/// height).
#[cfg(feature = "ui-shots")]
pub(crate) fn last_control_area() -> (f32, f32, f32, f32) {
    let x = SECTIONS_WIDTH + PAD + LABEL_WIDTH + LABEL_GAP;
    (x, window_height() - PAD - SWITCH, CONTROL_WIDTH, SWITCH)
}

/// Where the section list's row for `section` is drawn, as (x, y, width, height).
#[cfg(feature = "ui-shots")]
pub(crate) fn section_row_area(section: Section) -> (f32, f32, f32, f32) {
    let i = Section::ALL.iter().position(|&s| s == section).unwrap_or(0) as f32;
    let y = SECTIONS_TOP + i * (SECTION_ROW + SECTION_ROW_GAP);
    let width = SECTIONS_WIDTH - 1.0 - 2.0 * SECTIONS_INSET;
    (SECTIONS_INSET, y, width, SECTION_ROW)
}

pub struct PreferencesView {
    focus_handle: FocusHandle,
    /// The section shown beside the list.
    section: Section,
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
    /// The main window's workspace, whose sidebar has the tips card (none in the screenshots).
    workspace: Option<WeakEntity<Workspace>>,
    /// Whether the tips card is dismissed, kept in step with the workspace.
    tips_dismissed: bool,
    /// Re-renders the highlighted rows and the colors when the appearance changes.
    _appearance: Subscription,
    /// Keeps `tips_dismissed` in step with the workspace.
    _workspace: Option<Subscription>,
}

impl PreferencesView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let appearance_sub = cx.observe_global::<Appearance>(|_, cx| cx.notify());
        let section = cx
            .try_global::<LastSection>()
            .map_or_else(Section::default, |l| l.0);

        // The tips card lives in the main window's sidebar. Its workspace notifies often, so
        // redraw only when the card comes or goes.
        let workspace = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<Workspace>())
            .and_then(|w| w.entity(cx).ok());
        let tips_dismissed = workspace
            .as_ref()
            .is_some_and(|w| w.read(cx).sidebar.hint_dismissed);
        let workspace_sub = workspace.as_ref().map(|w| {
            cx.observe(w, |this, workspace, cx| {
                let dismissed = workspace.read(cx).sidebar.hint_dismissed;
                if dismissed != this.tips_dismissed {
                    this.tips_dismissed = dismissed;
                    cx.notify();
                }
            })
        });

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
            section,
            themes,
            swatches,
            fonts,
            installed,
            default_family,
            preview: Rc::default(),
            workspace: workspace.as_ref().map(|w| w.downgrade()),
            tips_dismissed,
            _appearance: appearance_sub,
            _workspace: workspace_sub,
        }
    }

    /// For the screenshots: show `section`, with the tips card dismissed or not.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn shown_as(mut self, section: Section, tips_dismissed: bool) -> Self {
        self.section = section;
        self.tips_dismissed = tips_dismissed;
        self
    }

    /// Show `section`, and open on it next time in this run.
    fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.set_global(LastSection(section));
        cx.notify();
    }

    /// Up and down arrows move through the section list (nothing else here takes them).
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let down = match event.keystroke.key.as_str() {
            "down" => true,
            "up" => false,
            _ => return,
        };
        self.select(self.section.step(down), cx);
        cx.stop_propagation();
    }

    /// "Show the tips card again": the sidebar shows it, and remembers that.
    fn show_tips_card(&mut self, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.as_ref().and_then(WeakEntity::upgrade) {
            workspace.update(cx, |workspace, cx| workspace.show_hint(cx));
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

    /// The list of sections down the left; the shown one is highlighted.
    fn section_list(&self, ui: UiColors, cx: &Context<Self>) -> Div {
        div()
            .flex_none()
            .w(px(SECTIONS_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(SECTION_ROW_GAP))
            .pt(px(SECTIONS_TOP))
            .px(px(SECTIONS_INSET))
            .border_r_1()
            .border_color(hsla(ui.sidebar_border))
            .children(Section::ALL.iter().enumerate().map(|(i, &section)| {
                let current = section == self.section;
                list_row(("section", i), current, ui)
                    .h(px(SECTION_ROW))
                    .when(!current, |r| r.text_color(hsla(ui.text_dim)))
                    .child(section.title())
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.select(section, cx)),
                    )
            }))
    }

    /// One row of the form, label and control, `control.height()` tall.
    fn form_row(&self, control: Control, ui: UiColors, cx: &Context<Self>) -> Div {
        let element = match control {
            Control::Theme => self
                .choice_list(
                    "theme-list",
                    "theme",
                    THEME_LIST,
                    Self::themes,
                    Self::current_theme,
                    Self::theme_row,
                    ui,
                    cx,
                )
                .into_any_element(),
            Control::Font => self
                .choice_list(
                    "font-list",
                    "font-family",
                    FONT_LIST,
                    Self::fonts,
                    Self::current_font,
                    Self::font_row,
                    ui,
                    cx,
                )
                .into_any_element(),
            Control::Size => size_row(ui, cx).into_any_element(),
            Control::Ligatures => ligature_switch(ui, cx).into_any_element(),
            Control::Restore => restore_switch(ui, cx).into_any_element(),
            Control::TipsCard => self.tips_card_button(ui, cx).into_any_element(),
            Control::CursorHeading => return group_heading(control.label(), ui),
            Control::CursorStyle => cursor_style_switch(ui, cx).into_any_element(),
            Control::CursorFun => cursor_fun_switch(ui, cx).into_any_element(),
            Control::Switch(s) => on_off_switch(s, ui, cx).into_any_element(),
        };
        let element = if control.note().is_empty() {
            element
        } else {
            with_note(element, control.note(), ui).into_any_element()
        };
        field(
            control.label(),
            control.height(),
            control.label_height(),
            element,
            ui,
        )
    }

    /// "Show the tips card again"; greyed, with "Shown" beside it, while the card shows.
    fn tips_card_button(&self, ui: UiColors, cx: &Context<Self>) -> Div {
        let dismissed = self.tips_dismissed;
        let button = div()
            .id("tips-card")
            .flex_none()
            .h(px(SIZE_ROW))
            .flex()
            .items_center()
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .text_color(hsla(if dismissed { ui.text } else { ui.text_faint }))
            .child("Show the tips card again")
            .when(dismissed, |b| {
                b.hover(|s| s.bg(hsla(ui.row_hover)))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.show_tips_card(cx)))
            });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(button)
            .when(!dismissed, |row| {
                row.child(
                    div()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child("Shown"),
                )
            })
    }

    /// The live preview: a terminal screen in the current theme, font, size and ligatures, with
    /// its caption.
    fn preview(&self, theme: Theme, ui: UiColors) -> Div {
        let kept = self.preview.clone();
        let screen = div()
            .flex_none()
            .w(px(PREVIEW_WIDTH))
            .h(px(PREVIEW_HEIGHT))
            // Inside the border, clear of its rounded corners: the screen is painted square.
            .p(px(2.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .bg(hsla(theme.background))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let theme = &theme;
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
            .flex()
            .flex_col()
            .gap(px(CAPTION - 16.0))
            .child(screen)
            .child(
                div()
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child("Preview"),
            )
    }
}

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = cx.global::<Appearance>();
        let theme = appearance.theme;
        let ui = theme.ui();

        let controls = self.section.controls();
        let rows = |controls: &[Control]| {
            controls
                .iter()
                .map(|&control| self.form_row(control, ui, cx))
                .collect::<Vec<_>>()
        };
        let column = || div().flex().flex_col().gap(px(FIELD_GAP));
        // Appearance has the preview beside its lists; its Cursor group runs under both.
        let body = if self.section == Section::Appearance {
            let group = controls
                .iter()
                .position(|&c| c == Control::CursorHeading)
                .unwrap_or(controls.len());
            let (top, below) = controls.split_at(group);
            column()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap(px(PREVIEW_GAP))
                        .child(column().children(rows(top)))
                        .child(self.preview(theme, ui)),
                )
                .children(rows(below))
        } else {
            column().children(rows(controls))
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &CloseSession, window, _| window.remove_window()))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_row()
            .size_full()
            .bg(hsla(ui.sidebar_background))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(self.section_list(ui, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p(px(PAD))
                    .child(heading(self.section.title(), ui))
                    .child(body),
            )
    }
}

/// The size row: smaller, the size, bigger.
fn size_row(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let size_label = format!("{} pt", format_size(cx.global::<Appearance>().font_size));
    let step_button = |id: &'static str, label: &'static str, step: prefs::FontStep| {
        button(id, ui)
            .child(label)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                step_font_size(cx, step);
            }))
    };
    div()
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
        .child(step_button("size-up", "+", prefs::FontStep::Bigger))
}

/// Ligatures On / Off: rewrites the `font-feature` lines, keeping every other feature.
fn ligature_switch(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let ligatures = prefs::ligatures_on(&cx.global::<Appearance>().font_settings.features);
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
    switch(ui)
        .child(ligature_option("ligatures-on", "On", true))
        .child(ligature_option("ligatures-off", "Off", false))
}

/// Restore sessions: Off, Layout, or Layout + agents.
fn restore_switch(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let restore = cx.global::<Appearance>().restore;
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
    switch(ui)
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
        ))
}

/// The cursor's shape when no program has chosen one: Bar, Underline, Hollow or Block.
fn cursor_style_switch(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let now = cx.global::<Appearance>().cursor_style;
    switch(ui).children(
        CursorShapeSetting::ALL
            .into_iter()
            .enumerate()
            .map(|(i, style)| {
                option(("cursor-style", i), style == now, ui)
                    .child(style.label())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        // Clicking the option already chosen changes nothing.
                        if cx.global::<Appearance>().cursor_style != style {
                            apply(cx, "cursor-style", &prefs::cursor_style_values(style));
                        }
                    }))
            }),
    )
}

/// The cursor's Fun: Off, Trail or Sparks.
fn cursor_fun_switch(ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let now = cx.global::<Appearance>().cursor_fun;
    switch(ui).children(CursorFun::ALL.into_iter().enumerate().map(|(i, fun)| {
        option(("cursor-fun", i), fun == now, ui)
            .child(fun.label())
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                if cx.global::<Appearance>().cursor_fun != fun {
                    apply(cx, "cursor-fun", &prefs::cursor_fun_values(fun));
                }
            }))
    }))
}

/// An On / Off switch for `setting`.
fn on_off_switch(setting: OnOff, ui: UiColors, cx: &Context<PreferencesView>) -> Div {
    let on_now = setting.current(cx.global::<Appearance>());
    let choice = |label: &'static str, on: bool| {
        option((setting.key(), usize::from(on)), on == on_now, ui)
            .child(label)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                // Clicking the option already chosen changes nothing.
                if setting.current(cx.global::<Appearance>()) != on {
                    apply(cx, setting.key(), &setting.values(on));
                }
            }))
    };
    switch(ui)
        .child(choice("On", true))
        .child(choice("Off", false))
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

/// A group's heading inside a section ("Cursor"): its name over the labels' column, in the
/// section title's small capitals but quieter, then a quiet line across the controls.
fn group_heading(title: &'static str, ui: UiColors) -> Div {
    div()
        .flex_none()
        .h(px(GROUP_HEADER))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(LABEL_GAP))
        .child(
            div()
                .flex_none()
                .w(px(LABEL_WIDTH))
                .flex()
                .justify_end()
                .text_size(px(text::GROUP_HEADER))
                .font_weight(FontWeight::SEMIBOLD)
                .font_features(small_caps())
                .text_color(hsla(ui.text_faint))
                .child(title),
        )
        .child(
            div()
                .flex_none()
                .w(px(CONTROL_WIDTH))
                .h(px(1.0))
                .bg(hsla(ui.sidebar_border)),
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

/// A switch with a note under it, one line per entry of `note`.
fn with_note(control: impl IntoElement, note: &'static [&'static str], ui: UiColors) -> Div {
    div()
        .flex()
        .flex_col()
        .child(div().flex().child(control))
        .child(
            div()
                .mt(px(NOTE - NOTE_LINE))
                .w(px(NOTE_WIDTH))
                .flex()
                .flex_col()
                .text_size(px(text::META))
                .text_color(hsla(ui.text_dim))
                .children(
                    note.iter()
                        .map(|&line| div().h(px(NOTE_LINE)).flex().items_center().child(line)),
                ),
        )
}

/// The box a switch's options sit in, side by side, only as wide as they are.
fn switch(ui: UiColors) -> Div {
    div()
        // The control column would otherwise stretch it to its width.
        .self_start()
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
fn option(id: impl Into<ElementId>, current: bool, ui: UiColors) -> Stateful<Div> {
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
    fn sections_list_every_control_once() {
        use Control::*;
        // The sections, in order, and what each holds (the 2026-11-06 amendment to note 53).
        let titles: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(titles, ["General", "Appearance", "Sessions", "Browser"]);
        assert_eq!(
            Section::General.controls(),
            [
                Switch(OnOff::Animations),
                Switch(OnOff::CheckUpdates),
                TipsCard
            ]
        );
        assert_eq!(
            Section::Appearance.controls(),
            [
                Theme,
                Font,
                Size,
                Ligatures,
                CursorHeading,
                CursorStyle,
                Switch(OnOff::CursorGlide),
                Switch(OnOff::CursorGlow),
                Switch(OnOff::CursorColor),
                CursorFun,
            ]
        );
        assert_eq!(
            Section::Sessions.controls(),
            [Restore, Switch(OnOff::KeepRunning)]
        );
        assert_eq!(
            Section::Browser.controls(),
            [Switch(OnOff::AddressBar), Switch(OnOff::KeepData)]
        );
        // Every control, in exactly one section, under its own label.
        let every = [
            Theme,
            Font,
            Size,
            Ligatures,
            Restore,
            TipsCard,
            CursorHeading,
            CursorStyle,
            CursorFun,
            Switch(OnOff::Animations),
            Switch(OnOff::CheckUpdates),
            Switch(OnOff::KeepRunning),
            Switch(OnOff::AddressBar),
            Switch(OnOff::KeepData),
            Switch(OnOff::CursorGlide),
            Switch(OnOff::CursorGlow),
            Switch(OnOff::CursorColor),
        ];
        let listed: Vec<Control> = Section::ALL
            .iter()
            .flat_map(|s| s.controls().iter().copied())
            .collect();
        for control in every {
            let times = listed.iter().filter(|&&c| c == control).count();
            assert_eq!(times, 1, "{control:?} is listed {times} times");
        }
        assert_eq!(listed.len(), every.len());
        let mut labels: Vec<&str> = every.iter().map(|c| c.label()).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), every.len(), "two controls share a label");
        // The arrow keys move through the list and stop at its ends.
        assert_eq!(Section::General.step(true), Section::Appearance);
        assert_eq!(Section::Sessions.step(false), Section::Appearance);
        assert_eq!(Section::Browser.step(true), Section::Browser);
        assert_eq!(Section::General.step(false), Section::General);
        // A fresh run opens on General.
        assert_eq!(Section::default(), Section::General);
    }

    #[test]
    fn on_off_switches_write_as_before() {
        // Moved into their sections, the switches write the same lines as the one-page window.
        assert_eq!(OnOff::Animations.key(), "animations");
        assert_eq!(OnOff::KeepRunning.key(), "persistent-sessions");
        for on in [true, false] {
            assert_eq!(OnOff::Animations.values(on), prefs::animations_values(on));
            assert_eq!(OnOff::KeepRunning.values(on), prefs::persistence_values(on));
        }
        assert_eq!(OnOff::Animations.note(), [ANIMATIONS_NOTE]);
        assert_eq!(OnOff::KeepRunning.note(), [KEEP_NOTE]);
    }

    #[test]
    fn appearance_lists_the_cursor_rows_in_order() {
        use crate::appearance::{CursorFun, CursorShapeSetting};
        // The "Cursor" group (note 58): its heading, then Style, Glide, Breathing glow,
        // Colour follows the agent and Fun, under the theme, font, size and ligatures.
        let labels: Vec<&str> = Section::Appearance
            .controls()
            .iter()
            .map(|c| c.label())
            .collect();
        assert_eq!(
            labels,
            [
                "Theme",
                "Font",
                "Size",
                "Ligatures",
                "Cursor",
                "Style",
                "Glide",
                "Breathing glow",
                "Colour follows the agent",
                "Fun",
            ]
        );
        // The choices, in order, under their own names.
        assert_eq!(
            CursorShapeSetting::ALL.map(CursorShapeSetting::label),
            ["Bar", "Underline", "Hollow", "Block"]
        );
        assert_eq!(
            CursorFun::ALL.map(CursorFun::label),
            ["Off", "Trail", "Sparks"]
        );
        // The note, word for word, under Style.
        assert_eq!(
            Control::CursorStyle.note(),
            ["Programs that set their own cursor (vim, for example) still choose their shape."]
        );
        assert_eq!(Control::CursorStyle.height(), SWITCH + NOTE);
        assert!(Control::CursorFun.note().is_empty());
        assert_eq!(Control::CursorFun.height(), SWITCH);
        // The switches write their lines, only when not the default.
        assert_eq!(OnOff::CursorGlide.key(), "cursor-glide");
        assert_eq!(OnOff::CursorGlow.key(), "cursor-glow");
        assert_eq!(OnOff::CursorColor.key(), "cursor-color");
        for s in [OnOff::CursorGlide, OnOff::CursorGlow, OnOff::CursorColor] {
            assert!(s.values(true).is_empty(), "{s:?}");
            assert!(s.note().is_empty(), "{s:?}");
        }
        assert_eq!(OnOff::CursorGlide.values(false), ["off"]);
        assert_eq!(OnOff::CursorGlow.values(false), ["off"]);
        assert_eq!(OnOff::CursorColor.values(false), ["theme"]);
        // Through the file: only that key's line changes, and the default removes it.
        let dir = std::env::temp_dir().join(format!("fuxx-cursor-values-{}", std::process::id()));
        let path = dir.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "theme = Nord\n").unwrap();
        let read = || std::fs::read_to_string(&path).unwrap();
        write_setting(
            &path,
            "cursor-style",
            &prefs::cursor_style_values(CursorShapeSetting::Hollow),
        )
        .unwrap();
        write_setting(
            &path,
            "cursor-fun",
            &prefs::cursor_fun_values(CursorFun::Sparks),
        )
        .unwrap();
        write_setting(
            &path,
            OnOff::CursorColor.key(),
            &OnOff::CursorColor.values(false),
        )
        .unwrap();
        assert_eq!(
            read(),
            "theme = Nord\ncursor-style = hollow\ncursor-fun = sparks\ncursor-color = theme\n"
        );
        write_setting(
            &path,
            "cursor-style",
            &prefs::cursor_style_values(CursorShapeSetting::Bar),
        )
        .unwrap();
        write_setting(
            &path,
            "cursor-fun",
            &prefs::cursor_fun_values(CursorFun::Off),
        )
        .unwrap();
        write_setting(
            &path,
            OnOff::CursorColor.key(),
            &OnOff::CursorColor.values(true),
        )
        .unwrap();
        assert_eq!(read(), "theme = Nord\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_updates_switch() {
        use Control::Switch;
        assert_eq!(Switch(OnOff::CheckUpdates).label(), "Check for updates");
        assert_eq!(OnOff::CheckUpdates.key(), "check-updates");
        // On by default: only off is written.
        assert!(OnOff::CheckUpdates.values(true).is_empty());
        assert_eq!(OnOff::CheckUpdates.values(false), ["off"]);
        assert_eq!(
            OnOff::CheckUpdates.note(),
            ["Asks GitHub once a day whether a newer fuxx is out."]
        );
    }

    #[test]
    fn browser_values() {
        use Control::Switch;
        assert_eq!(Switch(OnOff::AddressBar).label(), "Show address bar");
        assert_eq!(Switch(OnOff::KeepData).label(), "Keep history and data");
        // The address bar is on by default: only off is written.
        assert_eq!(OnOff::AddressBar.key(), "browser-address-bar");
        assert!(OnOff::AddressBar.values(true).is_empty());
        assert_eq!(OnOff::AddressBar.values(false), ["off"]);
        // Keeping data is off by default: only on is written.
        assert_eq!(OnOff::KeepData.key(), "browser-keep-data");
        assert_eq!(OnOff::KeepData.values(true), ["on"]);
        assert!(OnOff::KeepData.values(false).is_empty());
        // The note under Keep history and data, word for word.
        assert!(OnOff::AddressBar.note().is_empty());
        assert_eq!(
            OnOff::KeepData.note().join(" "),
            "Turning this off deletes saved browser data. Applies to browser windows opened \
             after the change."
        );
        // Through the file: only that key's line changes.
        let dir = std::env::temp_dir().join(format!("fuxx-browser-values-{}", std::process::id()));
        let path = dir.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "theme = Nord\n").unwrap();
        let read = || std::fs::read_to_string(&path).unwrap();
        let set = |s: OnOff, on: bool| write_setting(&path, s.key(), &s.values(on)).unwrap();
        set(OnOff::KeepData, true);
        set(OnOff::AddressBar, false);
        assert_eq!(
            read(),
            "theme = Nord\nbrowser-keep-data = on\nbrowser-address-bar = off\n"
        );
        set(OnOff::KeepData, false);
        set(OnOff::AddressBar, true);
        assert_eq!(read(), "theme = Nord\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn window_fits_the_largest_section() {
        // Appearance: its title, the theme list (seven rows, so every built-in shows without
        // scrolling) and the font list (five), each with its border and inset (2 x 3 px), the
        // size buttons and the ligatures switch, with a gap between fields.
        // Then the Cursor group: its 16 px heading, Style with its note, three switches and Fun.
        let (theme_list, font_list) = (7.0 * LIST_ROW + 6.0, 5.0 * LIST_ROW + 6.0);
        let cursor = 16.0 + (SWITCH + NOTE) + 4.0 * SWITCH;
        let appearance =
            HEADER + theme_list + font_list + SIZE_ROW + SWITCH + cursor + 9.0 * FIELD_GAP;
        assert_eq!(body_height(Section::Appearance), appearance);
        // Every section fits; the tallest sets the height, and it is Appearance.
        let inside = window_height() - 2.0 * PAD;
        for section in Section::ALL {
            assert!(body_height(section) <= inside, "{section:?} doesn't fit");
            let fields: f32 = section.controls().iter().map(|c| c.height()).sum();
            assert!(fields > 0.0, "{section:?} has no fields");
        }
        assert_eq!(window_height(), PAD + appearance + PAD);
        // Browser: two switches, the second with its two-line note.
        assert_eq!(
            body_height(Section::Browser),
            HEADER + SWITCH + (SWITCH + NOTE + 16.0) + FIELD_GAP
        );
        // The preview and its caption stand beside the lists, within the section.
        assert_eq!(PREVIEW_HEIGHT, theme_list + FIELD_GAP + font_list);
        assert!(HEADER + PREVIEW_HEIGHT + CAPTION <= appearance);
        // The preview stands beside the controls above the Cursor group, not beside the group.
        assert!(
            PREVIEW_HEIGHT + CAPTION
                <= theme_list + font_list + SIZE_ROW + SWITCH + 3.0 * FIELD_GAP
        );
        // The section list fits too.
        assert!(SECTIONS_TOP + 4.0 * (SECTION_ROW + SECTION_ROW_GAP) <= window_height());
        // As wide as the list and the old one-page form; the preview keeps its width.
        assert_eq!(SECTIONS_WIDTH, 160.0);
        assert_eq!(WIDTH, SECTIONS_WIDTH + FORM_WIDTH);
        assert_eq!(PREVIEW_WIDTH, 270.0);
        // As the constants are today.
        assert_eq!((WIDTH, window_height()), (920.0, 698.0));
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
