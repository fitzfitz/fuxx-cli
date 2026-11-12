//! Preferences (File > Preferences..., cmd-,): animations and the tips card, theme, font, size
//! and ligatures with a live preview of a terminal, how sessions are kept, the browser tabs, and
//! the sounds.
//!
//! Every click changes one line of fuxx's settings file and reloads the appearance, so it applies
//! at once to every session and is still there next time. The choices and the lines written come
//! from the tested pure functions in `prefs` and `settings`; this file is only the GPUI wiring.
//!
//! Since M11x a panel over the fuxx window (a workspace overlay, `Overlay::Preferences`), not a
//! window of its own: the sections down the left, and each section a page (a title, a line under
//! it, then its groups, each a card of rows: a setting's name and a short explanation at the
//! left, its control at the right). The pages are one table (`Section::groups`), which the ⌘K
//! palette's settings list reads too. It opens on the section last used in this run.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Anchor, AnyElement, App, BoxShadow, ClickEvent, Context, DismissEvent, Div, ElementId,
    EventEmitter, FocusHandle, Focusable, FontWeight, Global, Hsla, KeyDownEvent, MouseButton,
    MouseDownEvent, ScrollHandle, SharedString, Stateful, Subscription, WeakEntity, Window,
    anchored, canvas, deferred, div, font, point, prelude::*, px,
};

use crate::appearance::{self, Appearance, CursorFun, CursorShapeSetting};
use crate::fonts;
use crate::grid::{self, GridSnapshot};
use crate::name_input::{NameInput, NameInputEvent};
use crate::prefs::{self, DEFAULT_CHOICE};
use crate::restore::RestoreMode;
use crate::settings;
use crate::sounds::{self, SoundKey};
use crate::terminal::TermDims;
use crate::terminal_view::{hsla, paint_fixed_screen};
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::text;
use crate::workspace::{CloseSession, Workspace};

/// The panel (M11x): this size, or the window less `PANEL_MARGIN` each side when it's smaller.
const PANEL_WIDTH: f32 = 880.0;
const PANEL_HEIGHT: f32 = 580.0;
const PANEL_MARGIN: f32 = 24.0;
const PANEL_RADIUS: f32 = 22.0;
/// How dark the scrim over the window is behind the panel (the window's shadow colour).
pub(crate) const SCRIM_ALPHA: f32 = 0.5;
/// The section list down the left.
const NAV_WIDTH: f32 = 200.0;
const NAV_ROW: f32 = 32.0;
/// The page: around its content, and its words' sizes.
const PAGE_PAD_X: f32 = 44.0;
const PAGE_PAD_TOP: f32 = 34.0;
const TITLE_SIZE: f32 = 26.0;
const LEDE_SIZE: f32 = 14.0;
const GROUP_TITLE_SIZE: f32 = 15.0;
const ROW_NAME_SIZE: f32 = 14.0;
const EXPLANATION_SIZE: f32 = 12.5;
/// A group's card.
const CARD_RADIUS: f32 = 14.0;
/// The close button at the panel's top right.
const CLOSE: f32 = 28.0;
/// The on/off switch: its track and knob.
const TOGGLE_WIDTH: f32 = 38.0;
const TOGGLE_HEIGHT: f32 = 22.0;
const TOGGLE_KNOB: f32 = 16.0;
/// The theme tiles: four to a row, each with a small screen this tall.
const TILES_PER_ROW: usize = 4;
const TILE_SCREEN: f32 = 46.0;
/// The preview across the top of Appearance.
const PREVIEW_HEIGHT: f32 = 190.0;
/// The font dropdown, and its list (at most `FONT_MENU_ROWS` rows show, then it scrolls).
const DROPDOWN_WIDTH: f32 = 200.0;
const FONT_MENU_WIDTH: f32 = 260.0;
const FONT_MENU_ROW: f32 = 28.0;
const FONT_MENU_ROWS: f32 = 8.0;
/// A sound row's link field.
const SOUND_FIELD: f32 = 200.0;
/// A row's name and explanation never narrower than this; the control wraps under them.
const WORDS_MIN: f32 = 200.0;
/// The size row's square buttons, and the tips card's button.
const SIZE_ROW: f32 = 28.0;
/// A segmented control: options 24 px tall inside 2 px of padding and a 1 px border.
const SWITCH: f32 = 30.0;
const SWITCH_INSET: f32 = 2.0;
/// The size the monospace test measures glyphs at; any size gives the same answer.
const MEASURE_SIZE: f32 = 13.0;

/// Explanations shared with the tests.
const ANIMATIONS_NOTE: &str = "Also off when macOS Reduce motion is on.";
const CHECK_UPDATES_NOTE: &str = "Asks GitHub once a day whether a newer fuxx is out.";
const KEEP_NOTE: &str = "Takes effect the next time fuxx starts.";
const COMMAND_BLOCKS_NOTE: &str = "Marks each command in zsh. New terminals only.";
const CURSOR_NOTE: &str =
    "Programs that set their own cursor (vim, for example) still choose their shape.";

/// A part of Preferences, shown on its own beside the section list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Section {
    #[default]
    General,
    Appearance,
    Sessions,
    Browser,
    /// Key sounds (M11s).
    Sound,
}

impl Section {
    /// In the list's order, top to bottom.
    pub(crate) const ALL: [Section; 5] = [
        Section::General,
        Section::Appearance,
        Section::Sessions,
        Section::Browser,
        Section::Sound,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Sessions => "Sessions",
            Section::Browser => "Browser",
            Section::Sound => "Sound",
        }
    }

    /// The line under the page's title (M11x, note 75).
    pub(crate) fn lede(self) -> &'static str {
        match self {
            Section::General => "How fuxx looks around your terminals and keeps itself up to date.",
            Section::Appearance => "Colours, type and the cursor in every terminal.",
            Section::Sessions => "What fuxx keeps when you quit and opens when you come back.",
            Section::Browser => "The browser tabs inside your panes.",
            Section::Sound => {
                "A sound file or web link for each moment; leave it empty for silence."
            }
        }
    }

    /// The page's groups, top to bottom, each a card of rows (M11x). Each control is in exactly
    /// one group of one section.
    pub(crate) fn groups(self) -> &'static [Group] {
        use Control::*;
        match self {
            Section::General => &[
                Group {
                    title: "Interface",
                    rows: &[
                        Switch(OnOff::Animations),
                        Switch(OnOff::StatusBar),
                        TipsCard,
                    ],
                },
                Group {
                    title: "Updates",
                    rows: &[Switch(OnOff::CheckUpdates)],
                },
            ],
            Section::Appearance => &[
                Group {
                    title: "Theme",
                    rows: &[Theme],
                },
                Group {
                    title: "Text",
                    rows: &[Font, Size, Ligatures],
                },
                Group {
                    title: "Cursor",
                    rows: &[
                        CursorStyle,
                        Switch(OnOff::CursorGlide),
                        Switch(OnOff::CursorGlow),
                        Switch(OnOff::CursorColor),
                        CursorFun,
                    ],
                },
            ],
            Section::Sessions => &[
                Group {
                    title: "Quitting and reopening",
                    rows: &[Restore, Switch(OnOff::KeepRunning)],
                },
                Group {
                    title: "Terminal",
                    rows: &[Switch(OnOff::CommandBlocks)],
                },
            ],
            Section::Browser => &[Group {
                title: "Browsing",
                rows: &[Switch(OnOff::AddressBar), Switch(OnOff::KeepData)],
            }],
            Section::Sound => &[
                Group {
                    title: "Keys",
                    rows: &[Sound(SoundKey::Enter), Sound(SoundKey::CtrlC)],
                },
                Group {
                    title: "Notifications",
                    rows: &[Sound(SoundKey::Banner)],
                },
            ],
        }
    }

    /// The section's controls, top to bottom, its groups' rows in order.
    fn controls(self) -> Vec<Control> {
        self.groups()
            .iter()
            .flat_map(|g| g.rows.iter().copied())
            .collect()
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

/// A Preferences control as the ⌘K palette lists it (M11j): a name that reads alone, its section,
/// and the words it is found by too (a switch's settings-file key first).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteSetting {
    pub name: &'static str,
    pub section: Section,
    pub keywords: Vec<&'static str>,
}

/// Every control the palette can open Preferences at, section by section, top to bottom (the
/// Cursor heading isn't one), so a control added later is listed by itself.
pub(crate) fn palette_settings() -> Vec<PaletteSetting> {
    Section::ALL
        .into_iter()
        .flat_map(|section| {
            section.controls().into_iter().filter_map(move |control| {
                Some(PaletteSetting {
                    name: control.palette_name()?,
                    section,
                    keywords: control.keywords(),
                })
            })
        })
        .collect()
}

/// A group on a section's page (M11x): its name over a card of rows.
pub(crate) struct Group {
    pub title: &'static str,
    pub rows: &'static [Control],
}

/// One row of a section's form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    Theme,
    Font,
    Size,
    Ligatures,
    Restore,
    /// "Show the tips card again": not a setting, the sidebar's card (`hint_dismissed`).
    TipsCard,
    /// The cursor's shape: Bar, Underline, Hollow or Block (`cursor-style`).
    CursorStyle,
    /// Off, Trail or Sparks (`cursor-fun`).
    CursorFun,
    Switch(OnOff),
    /// A key's sound link (M11s): a field, ▶, and the problem under it.
    Sound(SoundKey),
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
            Control::CursorStyle => "Style",
            Control::CursorFun => "Fun",
            Control::Sound(key) => key.label(),
            Control::Switch(OnOff::Animations) => "Animations",
            Control::Switch(OnOff::StatusBar) => "Status Bar",
            Control::Switch(OnOff::CheckUpdates) => "Check for updates",
            Control::Switch(OnOff::KeepRunning) => "Keep running after quit",
            Control::Switch(OnOff::CommandBlocks) => "Command blocks",
            Control::Switch(OnOff::AddressBar) => "Show address bar",
            Control::Switch(OnOff::KeepData) => "Keep history and data",
            Control::Switch(OnOff::CursorGlide) => "Glide",
            Control::Switch(OnOff::CursorGlow) => "Breathing glow",
            Control::Switch(OnOff::CursorColor) => "Colour follows the agent",
        }
    }

    /// The short explanation under its name on the page (M11x, note 75); none for the theme
    /// tiles.
    fn explanation(self) -> &'static str {
        match self {
            Control::Theme => "",
            Control::Font => "Monospaced fonts installed on this Mac.",
            Control::Size => "⌘+ and ⌘− change it too.",
            Control::Ligatures => "Joins pairs like != and => into one sign.",
            Control::Restore => "Reopen your workspaces, and the agents in them, next time.",
            Control::TipsCard => "The welcome tips in the sidebar.",
            Control::CursorStyle => CURSOR_NOTE,
            Control::CursorFun => "A little extra as you type.",
            Control::Sound(SoundKey::Enter) => "When you press Enter in a terminal.",
            Control::Sound(SoundKey::CtrlC) => "When you press Ctrl + C in a terminal.",
            Control::Sound(SoundKey::Banner) => {
                "Plays when fuxx shows a banner; never for what only goes to the 🔔 list."
            }
            Control::Switch(OnOff::Animations) => ANIMATIONS_NOTE,
            Control::Switch(OnOff::StatusBar) => "The line along the window's foot.",
            Control::Switch(OnOff::CheckUpdates) => CHECK_UPDATES_NOTE,
            Control::Switch(OnOff::KeepRunning) => KEEP_NOTE,
            Control::Switch(OnOff::CommandBlocks) => COMMAND_BLOCKS_NOTE,
            Control::Switch(OnOff::AddressBar) => {
                "The bar with the address, back and reload above each page."
            }
            Control::Switch(OnOff::KeepData) => {
                "Turning this off deletes saved browser data. Applies to browser windows opened after the change."
            }
            Control::Switch(OnOff::CursorGlide) => "Slides to where you type instead of jumping.",
            Control::Switch(OnOff::CursorGlow) => "A soft pulse while the terminal waits.",
            Control::Switch(OnOff::CursorColor) => "Tinted by what the agent is doing.",
        }
    }

    /// Its name in the ⌘K palette, which reads without its section (`Cursor glide`); none for
    /// the Cursor heading, which sets nothing.
    fn palette_name(self) -> Option<&'static str> {
        Some(match self {
            Control::Size => "Font size",
            Control::CursorStyle => "Cursor style",
            Control::CursorFun => "Cursor fun",
            Control::Switch(OnOff::CursorGlide) => "Cursor glide",
            Control::Switch(OnOff::CursorGlow) => "Cursor breathing glow",
            Control::Switch(OnOff::CursorColor) => "Cursor colour follows the agent",
            Control::Sound(SoundKey::Enter) => "Sound for Enter",
            Control::Sound(SoundKey::CtrlC) => "Sound for Ctrl + C",
            other => other.label(),
        })
    }

    /// The words the palette also finds it by: a switch's settings-file key first, then a few
    /// more.
    fn keywords(self) -> Vec<&'static str> {
        let (key, more): (Option<&'static str>, &[&'static str]) = match self {
            Control::Switch(s) => (
                Some(s.key()),
                match s {
                    OnOff::Animations => &["motion"],
                    OnOff::CursorGlow => &["blink"],
                    OnOff::CursorColor => &["color"],
                    _ => &[],
                },
            ),
            Control::Theme => (None, &["theme", "colours", "dark", "light"]),
            Control::Font => (None, &["font-family"]),
            Control::Size => (None, &["font-size"]),
            Control::Ligatures => (None, &["font-feature"]),
            Control::Restore => (None, &["session-restore"]),
            Control::TipsCard => (None, &["tips", "welcome"]),
            Control::CursorStyle => (None, &["cursor-style", "bar", "block", "underline"]),
            Control::CursorFun => (None, &["cursor-fun", "trail", "sparks"]),
            Control::Sound(SoundKey::Banner) => (
                Some(SoundKey::Banner.key()),
                &["sound", "audio", "notification", "banner"],
            ),
            Control::Sound(key) => (Some(key.key()), &["sound", "audio", "key"]),
        };
        key.into_iter().chain(more.iter().copied()).collect()
    }
}

/// The settings that are an On / Off switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnOff {
    Animations,
    /// `status-bar`: the bar at the window's bottom (M11v part 3).
    StatusBar,
    CheckUpdates,
    KeepRunning,
    /// `command-blocks`: new terminals' zsh gets fuxx's integration (M11l).
    CommandBlocks,
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
            OnOff::StatusBar => "status-bar",
            OnOff::CheckUpdates => "check-updates",
            OnOff::KeepRunning => "persistent-sessions",
            OnOff::CommandBlocks => "command-blocks",
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
            OnOff::StatusBar => prefs::status_bar_values(on),
            OnOff::CheckUpdates => prefs::check_updates_values(on),
            OnOff::KeepRunning => prefs::persistence_values(on),
            OnOff::CommandBlocks => prefs::command_blocks_values(on),
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
            OnOff::StatusBar => a.status_bar,
            OnOff::CheckUpdates => a.check_updates,
            OnOff::KeepRunning => a.persistent,
            OnOff::CommandBlocks => a.command_blocks,
            OnOff::AddressBar => a.browser_address_bar,
            OnOff::KeepData => a.browser_keep_data,
            OnOff::CursorGlide => a.cursor_glide,
            OnOff::CursorGlow => a.cursor_glow,
            OnOff::CursorColor => a.cursor_follows_status,
        }
    }
}

/// The section last shown, so Preferences opens on it again for the rest of the run.
#[derive(Default)]
struct LastSection(Section);

impl Global for LastSection {}

/// ⌘, (and File › Preferences…, the sidebar's gear, the title bar's button): in the fuxx window
/// in front, open the panel or close it; from another window (a browser window), bring the
/// first fuxx window forward and open it there.
pub fn open_or_activate(cx: &mut App) {
    // Action handlers run while the active window is checked out of the app, and a window that is
    // checked out can't be updated: deferred, this runs once the current update is over.
    cx.defer(|cx| {
        if let Some(front) = cx.active_window().and_then(|w| w.downcast::<Workspace>()) {
            let _ = front.update(cx, |ws, window, cx| ws.toggle_preferences(window, cx));
            return;
        }
        let first = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<Workspace>());
        if let Some(first) = first {
            let _ = first.update(cx, |ws, window, cx| {
                window.activate_window();
                ws.open_preferences_at(last_section(cx), window, cx);
            });
        }
    });
}

/// The section Preferences opens on next: the one last shown in this run, else General.
pub(crate) fn last_section(cx: &App) -> Section {
    cx.try_global::<LastSection>()
        .map_or_else(Section::default, |l| l.0)
}

/// The panel's size in a `width` × `height` window (M11x): 880 × 580 pt, or the window less 24 pt
/// each side when that's smaller (never below nothing).
pub(crate) fn panel_size(width: f32, height: f32) -> (f32, f32) {
    (
        (width - 2.0 * PANEL_MARGIN).clamp(0.0, PANEL_WIDTH),
        (height - 2.0 * PANEL_MARGIN).clamp(0.0, PANEL_HEIGHT),
    )
}

/// The sound fields (by index) whose text, trimmed, isn't the saved link: what closing the panel
/// saves.
pub(crate) fn unsaved_fields(texts: &[String], saved: &[String]) -> Vec<usize> {
    texts
        .iter()
        .zip(saved)
        .enumerate()
        .filter(|(_, (text, saved))| text.trim() != saved.as_str())
        .map(|(i, _)| i)
        .collect()
}

/// Esc: a text field's own when it has the keys (it puts back what was saved), else it closes
/// the panel.
pub(crate) fn dismiss_on_escape(field_focused: bool) -> bool {
    !field_focused
}

pub struct PreferencesView {
    focus_handle: FocusHandle,
    /// The section shown beside the list.
    section: Section,
    /// `Default`, then every theme (built-in and the user's), worked out once.
    themes: Vec<String>,
    /// Each theme's swatch (`prefs::swatch`: background, foreground, palette 1-6), in the order
    /// of `themes`.
    swatches: Vec<[Rgb; 8]>,
    /// `Default`, then the installed fixed-width families, worked out once (measuring is slow).
    fonts: Vec<String>,
    /// Every installed family, to tell which configured family is really in use.
    installed: Vec<String>,
    /// The family `Default` stands for, to draw that row in it.
    default_family: SharedString,
    /// The preview's screen, kept between redraws.
    preview: Rc<PreviewScreen>,
    /// The workspace the panel is over, whose sidebar has the tips card.
    workspace: WeakEntity<Workspace>,
    /// Whether the tips card is dismissed, kept in step with the workspace.
    tips_dismissed: bool,
    /// The font dropdown's list is open.
    font_menu: bool,
    /// The page's scroll (back to the top on a section change).
    scroll: ScrollHandle,
    /// Re-renders the highlighted rows and the colors when the appearance changes.
    _appearance: Subscription,
    /// Keeps `tips_dismissed` in step with the workspace.
    _workspace: Option<Subscription>,
    /// Each sound's link field (M11s, M11x; `SoundKey::ALL` order).
    sound_inputs: Vec<gpui::Entity<NameInput>>,
    /// Saves a field on Enter or as it loses focus; redraws when a sound loads or fails.
    _sounds: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for PreferencesView {}

impl Focusable for PreferencesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PreferencesView {
    /// The panel over `workspace` (it opens it, so it is being updated: it passes the tips card's
    /// state, `tips_dismissed`, rather than be read here).
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        tips_dismissed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let appearance_sub = cx.observe_global::<Appearance>(|_, cx| cx.notify());
        let section = last_section(cx);

        // The tips card lives in the workspace's sidebar. Its workspace notifies often, so
        // redraw only when the card comes or goes.
        let workspace_sub = workspace.upgrade().map(|w| {
            cx.observe(&w, |this, workspace, cx| {
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

        // The sound fields: saved on Enter, and as they lose focus; Escape puts back what was
        // saved (M11s).
        let links = cx.global::<Appearance>().sound_links.clone();
        let sound_inputs: Vec<gpui::Entity<NameInput>> = SoundKey::ALL
            .iter()
            .map(|&key| {
                let text = links[key as usize].clone();
                cx.new(|cx| NameInput::new(text, window, cx))
            })
            .collect();
        let mut sound_subs: Vec<Subscription> = SoundKey::ALL
            .iter()
            .zip(&sound_inputs)
            .map(|(&key, input)| {
                cx.subscribe_in(input, window, move |_, input, event, window, cx| {
                    let saved = cx.global::<Appearance>().sound_links[key as usize].clone();
                    let text = match event {
                        NameInputEvent::Confirm(text) => text.clone(),
                        // Escape (the box still has focus): back to what was saved.
                        NameInputEvent::Cancel
                            if gpui::Focusable::focus_handle(input.read(cx), cx)
                                .is_focused(window) =>
                        {
                            input.update(cx, |input, cx| {
                                input.set_text(saved);
                                cx.notify();
                            });
                            return;
                        }
                        NameInputEvent::Cancel => input.read(cx).text().to_string(),
                    };
                    if text.trim() != saved {
                        apply(cx, key.key(), &sounds::values(&text));
                    }
                })
            })
            .collect();
        sound_subs.push(cx.observe_global::<sounds::Sounds>(|_, cx| cx.notify()));
        // However the panel closes (✕, Esc, ⌘W, ⌘,, a click outside, another box), a link typed
        // but not yet saved is kept.
        sound_subs.push(cx.on_release(|this, cx| this.save_fields(cx)));

        Self {
            focus_handle,
            section,
            themes,
            swatches,
            fonts,
            installed,
            default_family,
            preview: Rc::default(),
            workspace,
            tips_dismissed,
            font_menu: false,
            scroll: ScrollHandle::new(),
            _appearance: appearance_sub,
            _workspace: workspace_sub,
            sound_inputs,
            _sounds: sound_subs,
        }
    }

    /// The section shown.
    #[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
    pub(crate) fn section(&self) -> Section {
        self.section
    }

    /// For the screenshots: show `section`, with the tips card dismissed or not.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn shown_as(&mut self, section: Section, tips_dismissed: bool) {
        self.section = section;
        self.tips_dismissed = tips_dismissed;
    }

    /// For the screenshots: the page's scroll.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn scroll_for_shots(&self) -> ScrollHandle {
        self.scroll.clone()
    }

    /// Show `section` from its top, and open on it next time in this run.
    pub(crate) fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        if section != self.section {
            self.scroll.set_offset(point(px(0.0), px(0.0)));
        }
        self.section = section;
        self.font_menu = false;
        cx.set_global(LastSection(section));
        cx.notify();
    }

    /// Save each sound field whose text isn't its saved link.
    fn save_fields(&self, cx: &mut App) {
        let texts: Vec<String> = self
            .sound_inputs
            .iter()
            .map(|input| input.read(cx).text().to_string())
            .collect();
        let saved = cx.global::<Appearance>().sound_links.clone();
        for i in unsaved_fields(&texts, &saved) {
            let key = SoundKey::ALL[i];
            apply(cx, key.key(), &sounds::values(&texts[i]));
        }
    }

    /// A sound's link field has the keys.
    fn field_focused(&self, window: &Window, cx: &App) -> bool {
        self.sound_inputs
            .iter()
            .any(|input| input.focus_handle(cx).contains_focused(window, cx))
    }

    /// Esc closes the font list, else the panel (not while a field has the keys: it's the
    /// field's); up and down move through the sections.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            if self.font_menu {
                self.font_menu = false;
                cx.notify();
                cx.stop_propagation();
            } else if dismiss_on_escape(self.field_focused(window, cx)) {
                cx.emit(DismissEvent);
                cx.stop_propagation();
            }
            return;
        }
        if self.field_focused(window, cx) {
            return;
        }
        let down = match key {
            "down" => true,
            "up" => false,
            _ => return,
        };
        self.select(self.section.step(down), cx);
        cx.stop_propagation();
    }

    /// "Show again": the sidebar shows the tips card, and remembers that.
    fn show_tips_card(&mut self, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.show_hint(cx));
        }
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

    /// The family a font choice is drawn in (`Default`: the one it stands for).
    fn family_of(&self, name: &str) -> SharedString {
        if name == DEFAULT_CHOICE {
            self.default_family.clone()
        } else {
            SharedString::from(name.to_string())
        }
    }

    /// The sections down the left: "Settings", the sections (a soft fill on the one shown), and
    /// fuxx's version at the foot.
    fn nav(&self, ui: UiColors, cx: &Context<Self>) -> Div {
        div()
            .flex_none()
            .w(px(NAV_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(10.0))
            .pt(px(18.0))
            .pb(px(14.0))
            .rounded_l(px(PANEL_RADIUS))
            .bg(hsla(ui.sidebar_background))
            .border_r_1()
            .border_color(hsla(ui.divider))
            .child(
                div()
                    .pl(px(10.0))
                    .pb(px(6.0))
                    .text_size(px(12.0))
                    .text_color(hsla(ui.text_faint))
                    .child("Settings"),
            )
            .children(Section::ALL.iter().enumerate().map(|(i, &section)| {
                let current = section == self.section;
                div()
                    .id(("section", i))
                    .h(px(NAV_ROW))
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .rounded(px(9.0))
                    .text_size(px(ROW_NAME_SIZE))
                    .text_color(hsla(if current { ui.text } else { ui.text_dim }))
                    .when(current, |r| r.bg(hsla(ui.row_active)))
                    .when(!current, |r| r.hover(|s| s.bg(hsla(ui.row_hover))))
                    .cursor_pointer()
                    .child(section.title())
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.select(section, cx)),
                    )
            }))
            .child(div().flex_1())
            .child(
                div()
                    .px(px(10.0))
                    .text_size(px(EXPLANATION_SIZE))
                    .text_color(hsla(ui.text_faint))
                    .child(format!("fuxx {}", env!("CARGO_PKG_VERSION"))),
            )
    }

    /// The page: its title, the line under it, (Appearance) the preview, then its groups.
    fn page(&self, theme: Theme, ui: UiColors, cx: &Context<Self>) -> impl IntoElement {
        let section = self.section;
        div()
            .id("preferences-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(PAGE_PAD_X))
            .pt(px(PAGE_PAD_TOP))
            .pb(px(PAGE_PAD_X))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(TITLE_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(section.title()),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .mb(px(24.0))
                    .text_size(px(LEDE_SIZE))
                    .text_color(hsla(ui.text_dim))
                    .child(section.lede()),
            )
            .when(section == Section::Appearance, |page| {
                page.child(self.preview(theme, ui))
            })
            .children(section.groups().iter().map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .mb(px(24.0))
                    .child(
                        div()
                            .mb(px(10.0))
                            .text_size(px(GROUP_TITLE_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(group.title),
                    )
                    .child(if group.rows == [Control::Theme] {
                        self.theme_tiles(ui, cx).into_any_element()
                    } else {
                        self.card(group, ui, cx).into_any_element()
                    })
            }))
    }

    /// A group's card: its rows, split by hairlines.
    fn card(&self, group: &Group, ui: UiColors, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(hsla(ui.card_border))
            .bg(hsla(ui.card))
            .children(group.rows.iter().enumerate().map(|(i, &control)| {
                self.row(control, ui, cx)
                    .when(i > 0, |r| r.border_t_1().border_color(hsla(ui.divider)))
            }))
    }

    /// One row: the setting's name and its explanation (and a sound's problem) at the left, its
    /// control at the right.
    fn row(&self, control: Control, ui: UiColors, cx: &Context<Self>) -> Div {
        let problem = match control {
            Control::Sound(key) => sounds::problem(key, cx),
            _ => None,
        };
        // At least `WORDS_MIN` wide: in a narrow panel the control goes under the words.
        let words = div()
            .flex_1()
            .min_w(px(WORDS_MIN))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(div().text_size(px(ROW_NAME_SIZE)).child(control.label()))
            .when(!control.explanation().is_empty(), |w| {
                w.child(
                    div()
                        .text_size(px(EXPLANATION_SIZE))
                        .text_color(hsla(ui.text_dim))
                        .child(control.explanation()),
                )
            })
            .children(problem.map(|problem| {
                div()
                    .text_size(px(EXPLANATION_SIZE))
                    .text_color(hsla(match problem {
                        sounds::DOWNLOADING => ui.text_faint,
                        _ => ui.ring_waiting,
                    }))
                    .child(problem)
            }));
        let element = match control {
            Control::Theme => div().into_any_element(),
            Control::Font => self.font_dropdown(ui, cx).into_any_element(),
            Control::Size => size_row(ui, cx).into_any_element(),
            Control::Ligatures => ligature_switch(ui, cx).into_any_element(),
            Control::Restore => restore_switch(ui, cx).into_any_element(),
            Control::TipsCard => self.tips_card_button(ui, cx).into_any_element(),
            Control::CursorStyle => cursor_style_switch(ui, cx).into_any_element(),
            Control::CursorFun => cursor_fun_switch(ui, cx).into_any_element(),
            Control::Switch(s) => on_off_switch(s, ui, cx).into_any_element(),
            Control::Sound(key) => self.sound_control(key, ui, cx).into_any_element(),
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(16.0))
            .gap_y(px(10.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(words)
            .child(div().flex_none().ml_auto().child(element))
    }

    /// The themes as tiles, four to a row: each a small screen in its colours and its name; the
    /// one in use outlined in the accent colour, with a ✓.
    fn theme_tiles(&self, ui: UiColors, cx: &Context<Self>) -> Div {
        let selected = self.current_theme(cx.global::<Appearance>());
        let blank = [Rgb { r: 0, g: 0, b: 0 }; 8];
        let tiles: Vec<AnyElement> = self
            .themes
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let current = name.eq_ignore_ascii_case(&selected);
                let colours = self.swatches.get(i).copied().unwrap_or(blank);
                let values: Vec<String> = if name == DEFAULT_CHOICE {
                    vec![]
                } else {
                    vec![name.clone()]
                };
                let screen = div()
                    .h(px(TILE_SCREEN))
                    .rounded(px(7.0))
                    .bg(hsla(colours[0]))
                    .px(px(8.0))
                    .py(px(7.0))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(
                        div()
                            .w(px(46.0))
                            .h(px(4.0))
                            .rounded(px(2.0))
                            .bg(hsla(colours[1])),
                    )
                    .child(
                        div().flex().gap(px(3.0)).children(
                            colours[2..7]
                                .iter()
                                .map(|&c| div().size(px(9.0)).rounded(px(3.0)).bg(hsla(c))),
                        ),
                    );
                div()
                    .id(("theme-tile", i))
                    .flex_1()
                    .min_w(px(0.0))
                    .p(px(8.0))
                    .rounded(px(12.0))
                    .bg(hsla(ui.card))
                    .border_1()
                    .border_color(hsla(if current { ui.accent } else { ui.card_border }))
                    .when(!current, |t| t.hover(|s| s.bg(hsla(ui.card_hover))))
                    .cursor_pointer()
                    .child(screen)
                    .child(
                        div()
                            .mt(px(7.0))
                            .flex()
                            .justify_between()
                            .gap(px(4.0))
                            .text_size(px(EXPLANATION_SIZE))
                            .child(div().min_w(px(0.0)).truncate().child(name.clone()))
                            .when(current, |n| {
                                n.child(div().text_color(hsla(ui.accent)).child("\u{2713}"))
                            }),
                    )
                    .on_click(
                        cx.listener(move |_, _: &ClickEvent, _, cx| apply(cx, "theme", &values)),
                    )
                    .into_any_element()
            })
            .collect();
        let mut rows = div().flex().flex_col().gap(px(10.0));
        let mut tiles = tiles.into_iter().peekable();
        while tiles.peek().is_some() {
            let mut row = div().flex().gap(px(10.0));
            for _ in 0..TILES_PER_ROW {
                row = row.child(match tiles.next() {
                    Some(tile) => tile,
                    // The last row's empty places, sized as a tile is (its padding and
                    // border), keep the tiles their width.
                    None => div()
                        .flex_1()
                        .min_w(px(0.0))
                        .p(px(8.0))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .into_any_element(),
                });
            }
            rows = rows.child(row);
        }
        rows
    }

    /// The font: a pill with the font in use (in its face); a click opens the list under it.
    fn font_dropdown(&self, ui: UiColors, cx: &Context<Self>) -> Stateful<Div> {
        let current = self.current_font(cx.global::<Appearance>());
        div()
            .id("font-dropdown")
            .relative()
            .w(px(DROPDOWN_WIDTH))
            .h(px(SIZE_ROW + 2.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(9.0))
            .bg(hsla(ui.row_active))
            .hover(|s| s.bg(hsla(ui.row_hover)))
            .cursor_pointer()
            .text_size(px(13.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(self.family_of(&current))
                    .child(current),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(hsla(ui.text_dim))
                    .child("\u{2304}"),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.font_menu = !this.font_menu;
                cx.notify();
            }))
            .when(self.font_menu, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(SIZE_ROW + 6.0))
                        .right_0()
                        .child(deferred(
                            anchored()
                                .anchor(Anchor::TopRight)
                                .snap_to_window_with_margin(px(8.0))
                                .child(self.font_menu_list(ui, cx)),
                        )),
                )
            })
    }

    /// The font list: Default first, each name in its own face; a pick closes it.
    fn font_menu_list(&self, ui: UiColors, cx: &Context<Self>) -> Stateful<Div> {
        let selected = self.current_font(cx.global::<Appearance>());
        div()
            .id("font-menu")
            .occlude()
            .w(px(FONT_MENU_WIDTH))
            .max_h(px(FONT_MENU_ROWS * FONT_MENU_ROW + 10.0))
            .overflow_y_scroll()
            .p(px(4.0))
            .rounded(px(10.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .shadow(vec![raised(ui)])
            .text_size(px(13.0))
            .text_color(hsla(ui.text))
            .children(self.fonts.iter().enumerate().map(|(i, name)| {
                let current = name.eq_ignore_ascii_case(&selected);
                let values: Vec<String> = if name == DEFAULT_CHOICE {
                    vec![]
                } else {
                    vec![name.clone()]
                };
                div()
                    .id(("font", i))
                    .h(px(FONT_MENU_ROW))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .when(current, |r| r.bg(hsla(ui.row_active)))
                    .hover(|s| s.bg(hsla(ui.row_hover)))
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family(self.family_of(name))
                            .child(name.clone()),
                    )
                    .when(current, |r| {
                        r.child(div().text_color(hsla(ui.accent)).child("\u{2713}"))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.font_menu = false;
                        cx.notify();
                        apply(cx, "font-family", &values);
                    }))
            }))
    }

    /// A sound's control (M11s, M11x): its switch, its link's field and ▶ (plays it).
    fn sound_control(&self, key: SoundKey, ui: UiColors, cx: &Context<Self>) -> Div {
        let input = self.sound_inputs[key as usize].clone();
        let on_now = cx.global::<Appearance>().sound_on[key as usize];
        // Empty: what goes in it, faint, until it's typed in.
        let empty = input.read(cx).text().is_empty();
        // The field is the input's own box; while it's empty, what goes in it lies over it, faint.
        let field = div()
            .relative()
            .w(px(SOUND_FIELD))
            .h(px(SWITCH))
            .flex()
            .items_center()
            .text_size(px(EXPLANATION_SIZE))
            .child(input)
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
                        .child(sounds::HINT),
                )
            });
        let play = div()
            .id(("sound-play", key as usize))
            .flex_none()
            .size(px(SWITCH))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.0))
            .bg(hsla(ui.row_active))
            .text_size(px(11.0))
            .text_color(hsla(ui.text_dim))
            .cursor_pointer()
            .hover(|s| s.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
            .child("\u{25b6}")
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| sounds::play(key, cx)));
        // On / Off (2026-11-11, your call): off silences it and keeps its link.
        let toggle = toggle((key.switch_key(), 0usize), on_now, ui).on_click(cx.listener(
            move |_, _: &ClickEvent, _, cx| {
                let on = !cx.global::<Appearance>().sound_on[key as usize];
                apply(cx, key.switch_key(), &sounds::switch_values(on));
            },
        ));
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(toggle)
            .child(field)
            .child(play)
    }

    /// "Show again"; greyed, with "Shown" beside it, while the card shows.
    fn tips_card_button(&self, ui: UiColors, cx: &Context<Self>) -> Div {
        let dismissed = self.tips_dismissed;
        let button = div()
            .id("tips-card")
            .flex_none()
            .h(px(SIZE_ROW + 2.0))
            .flex()
            .items_center()
            .px(px(14.0))
            .rounded(px(9.0))
            .bg(hsla(ui.row_active))
            .text_size(px(13.0))
            .text_color(hsla(if dismissed { ui.text } else { ui.text_faint }))
            .child("Show again")
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
            .when(!dismissed, |row| {
                row.child(
                    div()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child("Shown"),
                )
            })
            .child(button)
    }

    /// The live preview across the page: a terminal screen in the current theme, font, size and
    /// ligatures, with its caption.
    fn preview(&self, theme: Theme, ui: UiColors) -> Div {
        let kept = self.preview.clone();
        let screen = div()
            .w_full()
            .h(px(PREVIEW_HEIGHT))
            // Inside the border, clear of its rounded corners: the screen is painted square.
            .p(px(8.0))
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(hsla(ui.card_border))
            .bg(hsla(theme.background))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let theme = &theme;
                        paint_fixed_screen(theme, bounds, window, cx, |dims| {
                            kept.get(theme, dims, || {
                                grid::fixed_screen(
                                    &preview_text_for(dims.cols, dims.rows),
                                    dims,
                                    theme,
                                )
                            })
                        })
                    },
                )
                .size_full(),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .mb(px(24.0))
            .child(screen)
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(ui.text_faint))
                    .child("Preview \u{2014} changes as you pick."),
            )
    }
}

/// The panel's raised shadow, and the font list's.
fn raised(ui: UiColors) -> BoxShadow {
    BoxShadow {
        color: Hsla {
            a: 0.45,
            ..hsla(ui.shadow)
        },
        offset: point(px(0.0), px(24.0)),
        blur_radius: px(64.0),
        spread_radius: px(0.0),
        inset: false,
    }
}

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        let close = div()
            .id("preferences-close")
            .absolute()
            .top(px(16.0))
            .right(px(18.0))
            .size(px(CLOSE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .text_size(px(15.0))
            .text_color(hsla(ui.text_dim))
            .hover(|s| s.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
            .cursor_pointer()
            .child("\u{2715}")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        // While the font list is open, a click anywhere else on the panel closes it.
        let backdrop = self.font_menu.then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        this.font_menu = false;
                        cx.notify();
                    }),
                )
        });
        div()
            .key_context("Preferences")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &CloseSession, _, cx| cx.emit(DismissEvent)))
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .flex()
            .flex_row()
            .rounded(px(PANEL_RADIUS))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .shadow(vec![raised(ui)])
            .text_size(px(ROW_NAME_SIZE))
            .text_color(hsla(ui.text))
            .child(self.nav(ui, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .child(self.page(theme, ui, cx))
                    .child(close),
            )
            .children(backdrop)
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

/// Ligatures on / off (a switch): rewrites the `font-feature` lines, keeping every other
/// feature.
fn ligature_switch(ui: UiColors, cx: &Context<PreferencesView>) -> Stateful<Div> {
    let ligatures = prefs::ligatures_on(&cx.global::<Appearance>().font_settings.features);
    toggle("ligatures", ligatures, ui).on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
        let features = cx.global::<Appearance>().font_settings.features.clone();
        let on = !prefs::ligatures_on(&features);
        apply(cx, "font-feature", &prefs::feature_lines(&features, on));
    }))
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

/// An on / off switch for `setting` (M11x): a click turns it the other way.
fn on_off_switch(setting: OnOff, ui: UiColors, cx: &Context<PreferencesView>) -> Stateful<Div> {
    let on_now = setting.current(cx.global::<Appearance>());
    toggle(setting.key(), on_now, ui).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
        let on = !setting.current(cx.global::<Appearance>());
        apply(cx, setting.key(), &setting.values(on));
    }))
}

/// A switch (M11x): a 38 × 22 pt track, filled in the accent colour with its knob at the right
/// when on, grey with its knob at the left when off.
fn toggle(id: impl Into<ElementId>, on: bool, ui: UiColors) -> Stateful<Div> {
    let inset = (TOGGLE_HEIGHT - TOGGLE_KNOB) / 2.0;
    div()
        .id(id)
        .flex_none()
        .w(px(TOGGLE_WIDTH))
        .h(px(TOGGLE_HEIGHT))
        .px(px(inset))
        .flex()
        .items_center()
        .when(on, |t| t.justify_end())
        .rounded(px(TOGGLE_HEIGHT / 2.0))
        .bg(hsla(if on { ui.accent } else { ui.divider }))
        .cursor_pointer()
        .child(
            div()
                .size(px(TOGGLE_KNOB))
                .rounded(px(TOGGLE_KNOB / 2.0))
                .bg(hsla(if on { ui.on_accent } else { ui.text_dim })),
        )
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

/// The preview's text for a `cols` × `rows` screen (M11x): on a short, wide screen (the page's
/// preview) the agent's box goes beside the text, so no line scrolls away; else `preview_text`.
fn preview_text_for(cols: usize, rows: usize) -> String {
    const FULL_ROWS: usize = 16;
    let text: Vec<String> = preview_text(BESIDE_TEXT_COLS)
        .split("\r\n")
        .take_while(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let boxed = agent_box(BOX_COLS);
    if rows >= FULL_ROWS || cols < BESIDE_TEXT_COLS + BESIDE_GAP + BOX_COLS {
        return preview_text(cols);
    }
    (0..text.len().max(boxed.len()))
        .map(|i| {
            let left = text.get(i).map_or("", String::as_str);
            let pad = (BESIDE_TEXT_COLS + BESIDE_GAP).saturating_sub(visible_width(left));
            format!(
                "{left}{}{}",
                " ".repeat(pad),
                boxed.get(i).map_or("", String::as_str)
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Beside the text (M11x): the text's columns, the gap, and the box's (its widest).
const BESIDE_TEXT_COLS: usize = 30;
const BESIDE_GAP: usize = 4;
const BOX_COLS: usize = 28;

/// How many columns `text` takes on screen: its characters, less its escape sequences.
fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
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
pub(crate) fn apply(cx: &mut App, key: &str, values: &[String]) {
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

    /// Review fix: closing the panel keeps what was typed in a sound field (not yet saved):
    /// the fields whose text, trimmed, differs from the saved link.
    #[test]
    fn closing_keeps_typed_links() {
        let texts = [
            "~/a.wav ".to_string(),
            String::new(),
            "https://x.io/c.wav".into(),
        ];
        let saved = ["~/a.wav".to_string(), String::new(), String::new()];
        assert_eq!(unsaved_fields(&texts, &saved), [2]);
        assert!(unsaved_fields(&saved, &saved).is_empty());
    }

    /// M11x: 880 × 580 pt, or the window less 24 pt each side; never below nothing.
    #[test]
    fn the_panel_fits_the_window() {
        assert_eq!(panel_size(1400.0, 900.0), (880.0, 580.0));
        assert_eq!(panel_size(700.0, 500.0), (652.0, 452.0));
        assert_eq!(panel_size(30.0, 20.0), (0.0, 0.0));
    }

    /// M11x: Esc in a text field is the field's (it puts back what was saved); elsewhere it
    /// closes the panel.
    #[test]
    fn escape_in_a_field_keeps_the_panel() {
        assert!(!dismiss_on_escape(true));
        assert!(dismiss_on_escape(false));
    }

    #[test]
    fn palette_settings_cover_every_control() {
        let all = palette_settings();
        assert_eq!(all.len(), 21);
        let mut names: Vec<&str> = all.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 21, "names unique");
        let order = |s: Section| Section::ALL.iter().position(|&x| x == s);
        assert!(
            all.windows(2)
                .all(|w| order(w[0].section) <= order(w[1].section))
        );
        let glow = all
            .iter()
            .find(|s| s.name == "Cursor breathing glow")
            .expect("glow listed");
        assert_eq!(glow.section, Section::Appearance);
        assert!(glow.keywords.contains(&"cursor-glow"));
        assert!(all.iter().all(|s| s.name != "Cursor"));
        assert_eq!(all[0].name, "Animations");
        assert_eq!(all[1].name, "Status Bar");
        assert_eq!(all[17].name, "Keep history and data");
        for section in Section::ALL {
            for control in section.controls() {
                if let Control::Switch(switch) = control {
                    assert!(
                        all.iter()
                            .any(|s| s.keywords.first() == Some(&switch.key())),
                        "{switch:?}"
                    );
                }
            }
        }
    }

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

    /// M11x: the preview is wide and short across the page: the agent's box beside the text,
    /// so nothing scrolls away; tall enough, or too narrow, it stays under the text.
    #[test]
    fn a_wide_short_preview_puts_the_box_beside_the_text() {
        let lines: Vec<String> = preview_text_for(100, 10)
            .split("\r\n")
            .map(visible)
            .collect();
        assert_eq!(lines.len(), 8, "{lines:?}");
        assert!(lines[0].starts_with("~/api $ ls"), "{lines:?}");
        assert!(
            lines[0].contains('\u{256d}'),
            "the box's top beside the first line"
        );
        assert!(lines.iter().any(|l| l.contains("git diff")));
        assert!(lines.iter().any(|l| l.contains("Do you want")));
        assert!(lines.iter().all(|l| l.chars().count() <= 100));
        assert_eq!(preview_text_for(100, 16), preview_text(100));
        assert_eq!(preview_text_for(40, 10), preview_text(40));
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

    /// M11x: each page's words, groups and rows, word for word from note 75.
    #[test]
    fn pages_follow_note_75() {
        use Control::*;
        let page = |section: Section| {
            (
                section.lede(),
                section
                    .groups()
                    .iter()
                    .map(|g| {
                        (
                            g.title,
                            g.rows
                                .iter()
                                .map(|&c| (c.label(), c.explanation()))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            page(Section::General),
            (
                "How fuxx looks around your terminals and keeps itself up to date.",
                vec![
                    (
                        "Interface",
                        vec![
                            ("Animations", "Also off when macOS Reduce motion is on."),
                            ("Status Bar", "The line along the window's foot."),
                            ("Tips card", "The welcome tips in the sidebar."),
                        ]
                    ),
                    (
                        "Updates",
                        vec![(
                            "Check for updates",
                            "Asks GitHub once a day whether a newer fuxx is out."
                        )]
                    ),
                ]
            )
        );
        assert_eq!(
            page(Section::Appearance),
            (
                "Colours, type and the cursor in every terminal.",
                vec![
                    ("Theme", vec![("Theme", "")]),
                    (
                        "Text",
                        vec![
                            ("Font", "Monospaced fonts installed on this Mac."),
                            ("Size", "⌘+ and ⌘− change it too."),
                            ("Ligatures", "Joins pairs like != and => into one sign."),
                        ]
                    ),
                    (
                        "Cursor",
                        vec![
                            (
                                "Style",
                                "Programs that set their own cursor (vim, for example) still choose their shape."
                            ),
                            ("Glide", "Slides to where you type instead of jumping."),
                            ("Breathing glow", "A soft pulse while the terminal waits."),
                            (
                                "Colour follows the agent",
                                "Tinted by what the agent is doing."
                            ),
                            ("Fun", "A little extra as you type."),
                        ]
                    ),
                ]
            )
        );
        assert_eq!(
            page(Section::Sessions),
            (
                "What fuxx keeps when you quit and opens when you come back.",
                vec![
                    (
                        "Quitting and reopening",
                        vec![
                            (
                                "Restore sessions",
                                "Reopen your workspaces, and the agents in them, next time."
                            ),
                            (
                                "Keep running after quit",
                                "Takes effect the next time fuxx starts."
                            ),
                        ]
                    ),
                    (
                        "Terminal",
                        vec![(
                            "Command blocks",
                            "Marks each command in zsh. New terminals only."
                        )]
                    ),
                ]
            )
        );
        assert_eq!(
            page(Section::Browser),
            (
                "The browser tabs inside your panes.",
                vec![(
                    "Browsing",
                    vec![
                        (
                            "Show address bar",
                            "The bar with the address, back and reload above each page."
                        ),
                        (
                            "Keep history and data",
                            "Turning this off deletes saved browser data. Applies to browser windows opened after the change."
                        ),
                    ]
                )]
            )
        );
        assert_eq!(
            page(Section::Sound),
            (
                "A sound file or web link for each moment; leave it empty for silence.",
                vec![
                    (
                        "Keys",
                        vec![
                            ("Enter", "When you press Enter in a terminal."),
                            ("Ctrl + C", "When you press Ctrl + C in a terminal."),
                        ]
                    ),
                    (
                        "Notifications",
                        vec![(
                            "Banner sound",
                            "Plays when fuxx shows a banner; never for what only goes to the 🔔 list."
                        )]
                    ),
                ]
            )
        );
        // The Banner sound is found in ⌘K and opens Sound.
        let banner = palette_settings()
            .into_iter()
            .find(|p| p.name == "Banner sound")
            .expect("listed");
        assert_eq!(banner.section, Section::Sound);
        for word in ["sound-banner", "notification", "banner"] {
            assert!(banner.keywords.contains(&word), "{word}");
        }
        let _ = Sound(SoundKey::Banner);
    }

    #[test]
    fn sections_list_every_control_once() {
        use Control::*;
        // The sections, in order, and what each holds (the 2026-11-06 amendment to note 53).
        let titles: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(
            titles,
            ["General", "Appearance", "Sessions", "Browser", "Sound"]
        );
        assert_eq!(
            Section::General.controls(),
            [
                Switch(OnOff::Animations),
                Switch(OnOff::StatusBar),
                TipsCard,
                Switch(OnOff::CheckUpdates)
            ]
        );
        assert_eq!(
            Section::Appearance.controls(),
            [
                Theme,
                Font,
                Size,
                Ligatures,
                CursorStyle,
                Switch(OnOff::CursorGlide),
                Switch(OnOff::CursorGlow),
                Switch(OnOff::CursorColor),
                CursorFun,
            ]
        );
        assert_eq!(
            Section::Sessions.controls(),
            [
                Restore,
                Switch(OnOff::KeepRunning),
                Switch(OnOff::CommandBlocks)
            ]
        );
        assert_eq!(
            Section::Browser.controls(),
            [Switch(OnOff::AddressBar), Switch(OnOff::KeepData)]
        );
        // Key sounds (M11s): Enter, then Ctrl + C.
        assert_eq!(
            Section::Sound.controls(),
            [
                Sound(SoundKey::Enter),
                Sound(SoundKey::CtrlC),
                Sound(SoundKey::Banner)
            ]
        );
        // Every control, in exactly one section, under its own label.
        let every = [
            Theme,
            Font,
            Size,
            Ligatures,
            Restore,
            TipsCard,
            CursorStyle,
            CursorFun,
            Switch(OnOff::Animations),
            Switch(OnOff::StatusBar),
            Switch(OnOff::CheckUpdates),
            Switch(OnOff::KeepRunning),
            Switch(OnOff::CommandBlocks),
            Switch(OnOff::AddressBar),
            Switch(OnOff::KeepData),
            Switch(OnOff::CursorGlide),
            Switch(OnOff::CursorGlow),
            Switch(OnOff::CursorColor),
            Sound(SoundKey::Enter),
            Sound(SoundKey::CtrlC),
            Sound(SoundKey::Banner),
        ];
        let listed: Vec<Control> = Section::ALL.iter().flat_map(|s| s.controls()).collect();
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
        assert_eq!(Section::Browser.step(true), Section::Sound);
        assert_eq!(Section::Sound.step(true), Section::Sound);
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
        assert_eq!(
            Control::Switch(OnOff::Animations).explanation(),
            ANIMATIONS_NOTE
        );
        assert_eq!(Control::Switch(OnOff::KeepRunning).explanation(), KEEP_NOTE);
    }

    #[test]
    fn command_blocks_switch() {
        // Preferences → Sessions, after Keep running (note 63 §6), word for word.
        assert_eq!(
            Section::Sessions.controls().last(),
            Some(&Control::Switch(OnOff::CommandBlocks))
        );
        assert_eq!(OnOff::CommandBlocks.key(), "command-blocks");
        assert_eq!(
            Control::Switch(OnOff::CommandBlocks).label(),
            "Command blocks"
        );
        assert_eq!(
            Control::Switch(OnOff::CommandBlocks).explanation(),
            "Marks each command in zsh. New terminals only."
        );
        // On by default: only off is written.
        assert!(OnOff::CommandBlocks.values(true).is_empty());
        assert_eq!(OnOff::CommandBlocks.values(false), ["off"]);
        let dir = std::env::temp_dir().join(format!("fuxx-blocks-values-{}", std::process::id()));
        let path = dir.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "theme = Nord\n").unwrap();
        let read = || std::fs::read_to_string(&path).unwrap();
        let set = |on: bool| {
            let s = OnOff::CommandBlocks;
            write_setting(&path, s.key(), &s.values(on)).unwrap()
        };
        set(false);
        assert_eq!(read(), "theme = Nord\ncommand-blocks = off\n");
        set(true);
        assert_eq!(read(), "theme = Nord\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn appearance_lists_the_cursor_rows_in_order() {
        use crate::appearance::{CursorFun, CursorShapeSetting};
        // The "Cursor" group (note 58; a group of its own since M11x): Style, Glide, Breathing
        // glow, Colour follows the agent and Fun, under the theme, font, size and ligatures.
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
            Control::CursorStyle.explanation(),
            "Programs that set their own cursor (vim, for example) still choose their shape."
        );
        // The switches write their lines, only when not the default.
        assert_eq!(OnOff::CursorGlide.key(), "cursor-glide");
        assert_eq!(OnOff::CursorGlow.key(), "cursor-glow");
        assert_eq!(OnOff::CursorColor.key(), "cursor-color");
        for s in [OnOff::CursorGlide, OnOff::CursorGlow, OnOff::CursorColor] {
            assert!(s.values(true).is_empty(), "{s:?}");
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
            Control::Switch(OnOff::CheckUpdates).explanation(),
            "Asks GitHub once a day whether a newer fuxx is out."
        );
    }

    /// M11v part 3: Settings → General → Status Bar, on by default (only off is written).
    #[test]
    fn status_bar_values() {
        use Control::Switch;
        assert_eq!(Switch(OnOff::StatusBar).label(), "Status Bar");
        assert_eq!(OnOff::StatusBar.key(), "status-bar");
        assert!(OnOff::StatusBar.values(true).is_empty());
        assert_eq!(OnOff::StatusBar.values(false), ["off"]);
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
        assert_eq!(
            Control::Switch(OnOff::KeepData).explanation(),
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
