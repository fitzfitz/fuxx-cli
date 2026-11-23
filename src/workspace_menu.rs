//! The shared right-click menu as the workspace opens, draws and acts on it (batch 3, chore 4):
//! one overlay (`Overlay::ContextMenu`, so browser pages hide under it), at the pointer or, from
//! the keyboard (⇧F10), at the caret, the selection or the tree's row. ↑/↓ and the pointer share
//! one light (nothing is lit on open), ↩ chooses, Esc or a click outside closes it, and a
//! right-click elsewhere opens it again there. The items come from `context_menu`; what they
//! do to the surface they opened from is here.

use std::path::PathBuf;

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, FocusHandle, MouseButton, MouseDownEvent,
    Pixels, PlatformInput, Point, Window, actions, anchored, div, point, prelude::*, px,
};

use crate::code_ask::{AskTo, CodeRef};
use crate::context_menu::{self, Entry, MenuAction};
use crate::layout_model::TabId;
use crate::split_tree::Rect;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

actions!(
    context_menu,
    [
        /// ⇧F10 (and the context-menu key): the focused surface's menu, at its caret or
        /// selection.
        ShowMenu,
        MenuUp,
        MenuDown,
        MenuChoose,
        MenuDismiss
    ]
);

/// Where a menu opened, with what it acts on (taken when it opened, so a choice acts on what
/// was right-clicked whatever changes meanwhile).
#[derive(Clone, Debug)]
pub(crate) enum MenuFrom {
    /// Terminal `sN`, its selection and the link under the pointer.
    Terminal {
        session: u64,
        selection: Option<String>,
    },
    /// A file tab's text: its selection as text and as code for Add to Chat.
    FileTab {
        tab: TabId,
        selection: String,
        code: Option<CodeRef>,
    },
    /// A Files row (chore 7): its path from the tree's root.
    FileTree { rel: PathBuf, is_dir: bool },
}

/// The open menu.
pub(crate) struct OpenMenu {
    pub from: MenuFrom,
    pub entries: Vec<Entry>,
    /// Its top-left corner, placed (`context_menu::place`).
    pub at: Point<Pixels>,
    /// The item lit by the pointer or ↑/↓.
    pub lit: Option<usize>,
    focus: FocusHandle,
    /// What had the keyboard before it opened: it gets it back on close.
    back: Option<FocusHandle>,
}

/// The agent Add to Chat names for a target (`ask_target`): its label ("Claude"); none on
/// another host, where there is nothing to add to.
pub(crate) fn add_target_label(to: AskTo) -> Option<&'static str> {
    match to {
        AskTo::Agent(name) => Some(
            crate::providers::by_name(name)
                .map(|p| p.info().label)
                .unwrap_or(name),
        ),
        AskTo::CopyOnly => None,
    }
}

impl Workspace {
    /// A terminal's right-click, ⌃-click or ⇧F10: its menu, Add to Chat naming the agent the
    /// selection would go to (none on another host).
    pub(crate) fn open_terminal_menu(
        &mut self,
        session: u64,
        at: Point<Pixels>,
        selection: Option<String>,
        link: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .model
            .terminal_tab(session)
            .and_then(|t| self.model.locate(t))
            .and_then(|(ws, _)| add_target_label(self.ask_target(ws, cx)));
        let entries = context_menu::terminal_menu(selection.is_some(), link.as_deref(), target);
        self.end_menu_underline(true, cx);
        self.open_context_menu(
            MenuFrom::Terminal { session, selection },
            entries,
            at,
            window,
            cx,
        );
        if link.is_some() {
            self.menu_underline = Some(session);
        }
    }

    /// Open the menu of `entries` for `from` at window point `at` (the pointer; flipped where
    /// it wouldn't fit). It takes the keyboard; nothing is lit.
    pub(crate) fn open_context_menu(
        &mut self,
        from: MenuFrom,
        entries: Vec<Entry>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            return;
        }
        let back = self
            .context_menu
            .take()
            .and_then(|m| m.back)
            .or_else(|| window.focused(cx));
        self.close_overlays(Overlay::ContextMenu);
        self.close_menus();
        let size = window.viewport_size();
        let (x, y) = context_menu::place(
            (f32::from(at.x), f32::from(at.y)),
            context_menu::WIDTH,
            context_menu::height(&entries),
            Rect {
                x: 0.0,
                y: 0.0,
                w: f32::from(size.width),
                h: f32::from(size.height),
            },
        );
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.context_menu = Some(OpenMenu {
            from,
            entries,
            at: point(px(x), px(y)),
            lit: None,
            focus,
            back,
        });
        cx.notify();
    }

    /// Escape, a click outside, or a choice: it goes, and the keyboard goes back.
    pub(crate) fn dismiss_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        self.end_menu_underline(false, cx);
        match menu.back {
            Some(back) => back.focus(window, cx),
            None => self.focus_active(window, cx),
        }
        cx.notify();
    }

    /// Once the menu is gone (by any route; `force`: now, as another opens), the terminal it
    /// opened from stops underlining the link it was opened on.
    pub(crate) fn end_menu_underline(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.context_menu.is_some() && !force {
            return;
        }
        if let Some(session) = self.menu_underline.take()
            && let Some(i) = self.index_of(session)
        {
            let view = self.sessions.items()[i].view.clone();
            view.update(cx, |view, cx| view.menu_closed(cx));
        }
    }

    /// The pointer over item `i` (or off them all, `None`): the one light, drawn again only
    /// when it moves.
    fn light_menu(&mut self, lit: Option<usize>, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.context_menu
            && menu.lit != lit
        {
            menu.lit = lit;
            cx.notify();
        }
    }

    /// ↑ / ↓: the light moves to the next enabled item.
    fn step_menu(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.context_menu {
            let lit = context_menu::step(&menu.entries, menu.lit, down);
            if lit != menu.lit {
                menu.lit = lit;
                cx.notify();
            }
        }
    }

    /// Item `i` chosen (a click, or ↩ on the lit one): the menu closes, then it acts.
    fn choose_menu_item(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let Some(Entry::Item(item)) = menu.entries.get(i) else {
            return;
        };
        if !item.enabled {
            return;
        }
        let action = item.action.clone();
        let from = menu.from.clone();
        self.dismiss_context_menu(window, cx);
        self.run_menu_action(from, action, window, cx);
    }

    /// What a choice does to the surface the menu opened from.
    pub(crate) fn run_menu_action(
        &mut self,
        from: MenuFrom,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source_tab = match &from {
            MenuFrom::Terminal { session, .. } => self.model.terminal_tab(*session),
            MenuFrom::FileTab { tab, .. } => Some(*tab),
            MenuFrom::FileTree { .. } => self.model.focused_tab(),
        };
        match (action, from) {
            (MenuAction::OpenLink(url), _) => {
                if let Some(tab) = source_tab {
                    self.open_link(tab, &url, window, cx);
                }
            }
            (MenuAction::OpenInBrowser(url), _) => {
                // Only the schemes a link may have (`links::allowed`): opened by the OS.
                if crate::links::allowed(&url) {
                    self.open_url_outside(&url, cx);
                }
            }
            (MenuAction::CopyLink(url), _) => {
                cx.write_to_clipboard(ClipboardItem::new_string(url));
            }
            (
                MenuAction::Copy,
                MenuFrom::Terminal {
                    selection: Some(text),
                    ..
                },
            ) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            (MenuAction::Copy, MenuFrom::FileTab { selection, .. }) => {
                if !selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selection));
                }
            }
            (MenuAction::Paste, MenuFrom::Terminal { session, .. }) => {
                if let Some(i) = self.index_of(session) {
                    let view = self.sessions.items()[i].view.clone();
                    view.update(cx, |view, cx| view.paste_from_clipboard(cx));
                }
            }
            (MenuAction::SelectAll, MenuFrom::Terminal { session, .. }) => {
                if let Some(i) = self.index_of(session) {
                    let view = self.sessions.items()[i].view.clone();
                    view.update(cx, |view, cx| view.select_all(cx));
                }
            }
            (MenuAction::AddToChat, MenuFrom::Terminal { session, selection }) => {
                let (Some(tab), Some(text)) = (source_tab, selection) else {
                    return;
                };
                let label = self
                    .index_of(session)
                    .map(|i| self.sessions.items()[i].label().to_string())
                    .unwrap_or_default();
                let chip = crate::agent_tab::Attachment::Quote {
                    text,
                    from: format!("Terminal {label}").trim_end().to_string(),
                };
                self.attach_to_agent(tab, chip, window, cx);
            }
            (
                MenuAction::AddToChat,
                MenuFrom::FileTab {
                    tab,
                    code: Some(code),
                    ..
                },
            ) => self.ask_code(tab, code, window, cx),
            (action, MenuFrom::FileTree { rel, is_dir }) => {
                self.files_menu_action(&rel, is_dir, action, window, cx);
            }
            _ => {}
        }
    }

    /// A URL to the OS's own handler (the default browser, Mail): never in the scenes.
    pub(crate) fn open_url_outside(&mut self, url: &str, cx: &mut Context<Self>) {
        #[cfg(feature = "ui-shots")]
        if self.places.frozen {
            self.shot_opened_outside.push(url.to_string());
            return;
        }
        cx.open_url(url);
    }

    /// The open menu, over the window, with its click-away backdrop.
    pub(crate) fn render_context_menu(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        let rows = menu.entries.iter().enumerate().map(|(i, entry)| {
            let Entry::Item(item) = entry else {
                return div()
                    .my(px(4.0))
                    .mx(px(8.0))
                    .h(px(1.0))
                    .bg(hsla(ui.panel_border))
                    .into_any_element();
            };
            let lit = menu.lit == Some(i) && item.enabled;
            let ink = if !item.enabled {
                ui.text_faint
            } else if item.danger {
                ui.menu_danger
            } else {
                ui.text
            };
            div()
                .id(("context-menu", i))
                .flex()
                .items_center()
                .justify_between()
                .h(px(context_menu::ITEM))
                .px(px(8.0))
                .rounded(px(5.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ink))
                .when(lit, |row| row.bg(hsla(ui.panel_hover)))
                .child(div().truncate().child(item.label.clone()))
                .children(item.keys.map(|keys| {
                    div()
                        .flex_none()
                        .ml(px(12.0))
                        .text_size(px(text::KEYS))
                        .text_color(hsla(if item.enabled {
                            ui.text_dim
                        } else {
                            ui.text_faint
                        }))
                        .child(keys)
                }))
                .when(item.enabled, |row| {
                    row.on_hover(cx.listener(move |this, over: &bool, _, cx| {
                        let lit = this.context_menu.as_ref().and_then(|m| m.lit);
                        if *over {
                            this.light_menu(Some(i), cx);
                        } else if lit == Some(i) {
                            this.light_menu(None, cx);
                        }
                    }))
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, window, cx| {
                            this.choose_menu_item(i, window, cx)
                        },
                    ))
                })
                .into_any_element()
        });
        let panel = crate::overlay_panel::shell(&ui, context_menu::WIDTH)
            .p(px(context_menu::PAD))
            .gap(px(0.0))
            .track_focus(&menu.focus)
            .key_context("ContextMenu")
            .on_action(cx.listener(|this, _: &MenuUp, _, cx| this.step_menu(false, cx)))
            .on_action(cx.listener(|this, _: &MenuDown, _, cx| this.step_menu(true, cx)))
            .on_action(cx.listener(|this, _: &MenuChoose, window, cx| {
                if let Some(i) = this.context_menu.as_ref().and_then(|m| m.lit) {
                    this.choose_menu_item(i, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MenuDismiss, window, cx| {
                this.dismiss_context_menu(window, cx)
            }))
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .children(rows);
        Some(
            div()
                .id("context-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_context_menu(window, cx)
                    }),
                )
                // A right-click elsewhere: this one closes, and once the window is drawn
                // without it the press goes to what is under the pointer (which opens its own).
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, e: &MouseDownEvent, window, cx| {
                        this.dismiss_context_menu(window, cx);
                        redispatch_after_frames(e.clone(), window);
                    }),
                )
                .child(
                    anchored()
                        .position(menu.at)
                        .snap_to_window_with_margin(px(context_menu::EDGE_MARGIN))
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}

/// Send `press` again once the window has been drawn without the menu's backdrop (two frames:
/// the next one draws it, the one after hit-tests against that drawing).
fn redispatch_after_frames(press: MouseDownEvent, window: &mut Window) {
    window.on_next_frame(move |window, _| {
        window.on_next_frame(move |window, cx| {
            window.dispatch_event(PlatformInput::MouseDown(press), cx);
        });
    });
}

/// The keyboard's place for a menu (⇧F10) when a surface has no caret to give: its bounds'
/// top-left, a little in.
pub(crate) fn keyboard_at(bounds: Option<gpui::Bounds<Pixels>>) -> Point<Pixels> {
    bounds.map_or(point(px(100.0), px(100.0)), |b| {
        b.origin + point(px(16.0), px(16.0))
    })
}
