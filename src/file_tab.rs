//! A file tab (M11g): a pane tab that shows a file, read-only. Nothing here writes to the file.
//!
//! For now (Task 3) its view is a stub that shows the file's path; the viewer itself (colours,
//! line numbers, change bars, live reload) replaces its body in Task 6. The workspace keeps one
//! view per file tab of the model (`ensure_file_tabs`, as for browser tabs), and `sync` drops
//! those whose tab has gone.

use std::path::PathBuf;
use std::time::Instant;

use gpui::{App, Context, Entity, FocusHandle, Focusable, Window, div, prelude::*, px};

use crate::appearance::Appearance;
use crate::layout_model::{PaneId, TabId, TabKind};
use crate::terminal_view::hsla;
use crate::ui_tokens::text;
use crate::workspace::Workspace;

/// A file tab's view.
pub struct FileTab {
    pub path: PathBuf,
    /// When it was opened (an age for a workspace with no terminal, as a browser tab's).
    pub since: Instant,
    focus: FocusHandle,
}

impl FileTab {
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            path,
            since: Instant::now(),
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for FileTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for FileTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("FileTab")
            .bg(hsla(theme.background))
            .text_color(hsla(ui.text))
            .text_size(px(text::NAME))
            .p(px(12.0))
            .child(self.path.display().to_string())
    }
}

impl Workspace {
    /// Open file `path` in the focused pane (the sidebar's click): the pane's tab that already
    /// shows it, else a new tab at the end of its bar. It is shown and takes the keyboard.
    pub(crate) fn open_file_tab(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.model.focused_pane() else {
            return;
        };
        let Some(opened) = self.model.open_file(pane, path.clone()) else {
            return;
        };
        self.sync();
        if !self.file_tabs.contains_key(&opened.tab) {
            self.make_file_tab(opened.tab, path, cx);
        }
        self.save();
        self.activate(window, cx);
        cx.notify();
    }

    /// Close file tab `tab` (its ×, ⌘W, its menu): its view goes (`sync`), and the focused tab
    /// takes the keyboard as after any close; with nothing left, the window.
    pub(crate) fn close_file_tab(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.close_tab(tab).is_none() {
            return;
        }
        if let Some(menu) = &self.pane_tab_menu
            && menu.tab == tab
        {
            self.pane_tab_menu = None;
        }
        self.sync();
        self.save();
        if self.all_gone() {
            let conns = self.subscribers.clone();
            self.quit_after_flush(conns, None, window, cx);
            return;
        }
        self.activate(window, cx);
    }

    /// Every file tab of the model has its view (a restored one, one dragged in). A new view
    /// for the focused tab takes the keyboard.
    pub(crate) fn ensure_file_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let missing: Vec<(TabId, PathBuf)> = self
            .model
            .workspaces()
            .iter()
            .flat_map(|w| &w.panes)
            .flat_map(|p| &p.tabs)
            .filter(|t| !self.file_tabs.contains_key(&t.id))
            .filter_map(|t| match &t.kind {
                TabKind::File { path, .. } => Some((t.id, path.clone())),
                TabKind::Terminal(_) | TabKind::Browser { .. } => None,
            })
            .collect();
        let focused = self.model.focused_tab();
        let mut focus_it = false;
        for (tab, path) in missing {
            self.make_file_tab(tab, path, cx);
            focus_it |= Some(tab) == focused;
        }
        if focus_it {
            self.focus_active(window, cx);
        }
    }

    /// The view for file tab `tab`, showing `path`.
    fn make_file_tab(&mut self, tab: TabId, path: PathBuf, cx: &mut Context<Self>) {
        let view = cx.new(|cx| FileTab::new(path, cx));
        self.file_tabs.insert(tab, view);
    }

    /// The file tab the focused pane shows, if it shows one.
    pub(crate) fn focused_file_tab(&self) -> Option<&Entity<FileTab>> {
        self.file_tabs.get(&self.model.focused_tab()?)
    }

    /// The file tab pane `pane` shows, if it shows one.
    pub(crate) fn pane_file_tab(&self, pane: PaneId) -> Option<Entity<FileTab>> {
        self.file_tabs.get(&self.model.pane(pane)?.active).cloned()
    }
}
