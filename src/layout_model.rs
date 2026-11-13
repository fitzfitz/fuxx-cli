//! Workspaces → panes → tabs (M11b): the sidebar lists workspaces, a workspace is a split layout
//! of panes, a pane holds an ordered row of terminal, browser and (M11g) file tabs. Every rule lives here
//! (new, split, add tab, close-last cascades, move, select, roll-up, pinned order, ⌘1–⌘9, the
//! saved shape and the loader for older files). Pure and GPUI-free, so the rules are unit-tested.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::groups;
use crate::restore::SavedState;
use crate::split_tree::{self, Axis, Node};
use crate::status::Status;
use crate::tab_color::{self, TabColor};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

/// What a tab shows: a terminal (by its scripting id, `sN`), a web page, or a file, read-only
/// (M11g).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabKind {
    Terminal(u64),
    Browser { url: Option<String> },
    File { path: PathBuf, mode: FileMode },
}

/// How a file tab shows its file: as it is (with change bars), or as a diff against the last
/// commit (M11h: Files opens `Plain`, Changes `Diff`; the toolbar toggles it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    #[default]
    Plain,
    Diff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
}

/// One pane: its tabs in tab-bar order (never empty) and the one it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Pane {
    pub id: PaneId,
    pub tabs: Vec<Tab>,
    pub active: TabId,
}

/// Where a sidebar drag put a workspace, when not in its own folder's (or host's) group
/// (2026-11-10, your call: "I also cannot drop to outside folder… from folder workspace to
/// another folder workspace").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placed {
    /// With the ungrouped rows, above every group.
    Ungrouped,
    /// In this folder's group.
    Folder(PathBuf),
    /// In this SSH host's group.
    Host(String),
}

/// The group a workspace shows in: where it was put (`placed`), else its own (`own`).
pub fn shown_group(
    placed: Option<&Placed>,
    own: Option<groups::GroupKey>,
) -> Option<groups::GroupKey> {
    match placed {
        None => own,
        Some(Placed::Ungrouped) => None,
        Some(Placed::Folder(path)) => Some(groups::GroupKey::Folder(path.clone())),
        Some(Placed::Host(host)) => Some(groups::GroupKey::Host(host.clone())),
    }
}

/// What a drop into group `target` (none: out of every group) records for a workspace whose
/// own group is `own`: nothing when that is its own group.
pub fn placed_for(
    target: Option<&groups::GroupKey>,
    own: Option<&groups::GroupKey>,
) -> Option<Placed> {
    if target == own {
        return None;
    }
    Some(match target {
        None => Placed::Ungrouped,
        Some(groups::GroupKey::Folder(path)) => Placed::Folder(path.clone()),
        Some(groups::GroupKey::Host(host)) => Placed::Host(host.clone()),
    })
}

/// One sidebar row: a split layout of panes (never empty) and the pane with the keyboard.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
    /// Your name for it; none: the caller names it after its first terminal.
    pub name: Option<String>,
    pub color: Option<TabColor>,
    pub pinned: bool,
    /// Where a sidebar drag put it: none, its own folder's (or host's) group; else out of every
    /// group, or in another one (2026-11-10, your call).
    pub placed: Option<Placed>,
    pub layout: Node<PaneId>,
    /// In no particular order; the layout's reading order is the panes' order.
    pub panes: Vec<Pane>,
    pub focused: PaneId,
}

/// Where a new tab went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opened {
    pub workspace: WorkspaceId,
    pub pane: PaneId,
    pub tab: TabId,
}

/// What an op ended, so the caller can drop the views: the tabs (with what they showed), and
/// the panes and workspaces that closed because they were left empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Closed {
    pub tabs: Vec<Tab>,
    pub panes: Vec<PaneId>,
    pub workspaces: Vec<WorkspaceId>,
}

/// Where a dragged tab is dropped (M11b Task 7): in a pane's tab bar, at a gap between its tabs
/// (0: before the first; the bar's length: after the last), or on a workspace's sidebar row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDrop {
    Bar { pane: PaneId, gap: usize },
    Workspace(WorkspaceId),
}

/// The workspaces in sidebar order, which one is focused, and the next id to hand out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    workspaces: Vec<Workspace>,
    focused: Option<WorkspaceId>,
    next_id: u64,
    /// The panes' preview tabs (M11t): at most one per pane, a file tab a single click shows
    /// and the next single click shows another file in. Not saved.
    previews: Vec<TabId>,
    /// The file tabs showing their Markdown rendered (M11ac: Preview on). Saved with each tab.
    md_previews: Vec<TabId>,
}

/// The saved-file version that holds workspaces (version 1 held sessions and their layouts).
pub const SAVED_VERSION: u32 = 2;

/// One workspace as saved. Its layout's leaves are the panes themselves, in reading order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWorkspace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "tab_color::lenient"
    )]
    pub color: Option<TabColor>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placed: Option<Placed>,
    pub layout: Node<SavedPane>,
    /// The focused pane, by its place in reading order.
    #[serde(default)]
    pub focused: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPane {
    pub tabs: Vec<SavedTab>,
    /// The tab it shows, by its place in `tabs`.
    #[serde(default)]
    pub active: usize,
}

/// A terminal tab names its saved session (an index into the file's `sessions`); a browser tab
/// keeps its page; a file tab its file and how it shows it (none saved: `Plain`). Files from
/// before file tabs have none, so the version stays 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SavedTab {
    Terminal {
        session: usize,
    },
    Browser {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    File {
        path: PathBuf,
        #[serde(default)]
        mode: FileMode,
        /// M11ac: a Markdown file shown rendered (its Preview on). Off when missing, and not
        /// written when off, so older files load and the file reads as before.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        preview: bool,
    },
}

impl Model {
    pub fn new() -> Self {
        Self::default()
    }

    /// In sidebar order (pinned ones are not moved up here; see `display_order`).
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes().find(|p| p.id == id)
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.panes().flat_map(|p| &p.tabs).find(|t| t.id == id)
    }

    /// The workspace and pane that hold `tab`.
    pub fn locate(&self, tab: TabId) -> Option<(WorkspaceId, PaneId)> {
        self.workspaces.iter().find_map(|w| {
            w.panes
                .iter()
                .find(|p| p.tabs.iter().any(|t| t.id == tab))
                .map(|p| (w.id, p.id))
        })
    }

    /// The tab showing terminal `sN`.
    pub fn terminal_tab(&self, terminal: u64) -> Option<TabId> {
        self.panes()
            .flat_map(|p| &p.tabs)
            .find(|t| t.kind == TabKind::Terminal(terminal))
            .map(|t| t.id)
    }

    /// A workspace's terminals: panes in reading order, tabs in tab-bar order.
    pub fn terminals(&self, workspace: WorkspaceId) -> Vec<u64> {
        let Some(w) = self.workspace(workspace) else {
            return Vec::new();
        };
        w.layout
            .leaves()
            .into_iter()
            .filter_map(|id| w.pane(id))
            .flat_map(|p| &p.tabs)
            .filter_map(|t| match t.kind {
                TabKind::Terminal(id) => Some(id),
                TabKind::Browser { .. } | TabKind::File { .. } => None,
            })
            .collect()
    }

    /// Every tab of a workspace, browser and file tabs too: panes in reading order, tabs in tab-bar
    /// order.
    pub fn tabs_of(&self, workspace: WorkspaceId) -> Vec<TabId> {
        let Some(w) = self.workspace(workspace) else {
            return Vec::new();
        };
        w.layout
            .leaves()
            .into_iter()
            .filter_map(|id| w.pane(id))
            .flat_map(|p| p.tabs.iter().map(|t| t.id))
            .collect()
    }

    pub fn focused_workspace(&self) -> Option<WorkspaceId> {
        self.focused
    }

    /// The focused workspace's focused pane.
    pub fn focused_pane(&self) -> Option<PaneId> {
        self.focused
            .and_then(|id| self.workspace(id))
            .map(|w| w.focused)
    }

    /// The tab the focused pane shows.
    pub fn focused_tab(&self) -> Option<TabId> {
        self.focused_pane()
            .and_then(|id| self.pane(id))
            .map(|p| p.active)
    }

    /// The sidebar's order: pinned workspaces first, then the rest, each in sidebar order.
    /// Pinning moves nothing in the sidebar order, so unpinning puts a workspace back.
    pub fn display_order(&self) -> Vec<WorkspaceId> {
        let pinned = self.workspaces.iter().filter(|w| w.pinned);
        let rest = self.workspaces.iter().filter(|w| !w.pinned);
        pinned.chain(rest).map(|w| w.id).collect()
    }

    /// The workspace ⌘`digit` selects.
    pub fn by_shortcut(&self, digit: u8) -> Option<WorkspaceId> {
        if !(1..=9).contains(&digit) {
            return None;
        }
        self.display_order().get(digit as usize - 1).copied()
    }

    /// A new workspace at `at` in sidebar order (clamped): one pane, one tab; it gets the focus.
    /// None (nothing changes) when `kind` is a terminal another tab already shows.
    pub fn new_workspace(&mut self, at: usize, kind: TabKind) -> Option<Opened> {
        if self.shown_elsewhere(&kind, None) {
            return None;
        }
        let workspace = WorkspaceId(self.next());
        let pane = self.new_pane(kind);
        let opened = Opened {
            workspace,
            pane: pane.id,
            tab: pane.active,
        };
        let at = at.min(self.workspaces.len());
        self.workspaces.insert(
            at,
            Workspace {
                id: workspace,
                name: None,
                color: None,
                pinned: false,
                placed: None,
                layout: Node::Leaf(pane.id),
                panes: vec![pane],
                focused: opened.pane,
            },
        );
        self.focused = Some(workspace);
        Some(opened)
    }

    /// A new pane with one tab beside `pane`: right of it (`Row`) or under it (`Column`). It
    /// gets the focus. None when `pane` is unknown or the terminal is already shown.
    pub fn split(&mut self, pane: PaneId, axis: Axis, kind: TabKind) -> Option<Opened> {
        let w = self.index_of_pane(pane)?;
        if self.shown_elsewhere(&kind, None) {
            return None;
        }
        let new = self.new_pane(kind);
        let opened = Opened {
            workspace: self.workspaces[w].id,
            pane: new.id,
            tab: new.active,
        };
        let ws = &mut self.workspaces[w];
        ws.layout.split(pane, new.id, axis);
        ws.panes.push(new);
        ws.focused = opened.pane;
        self.focused = Some(opened.workspace);
        Some(opened)
    }

    /// A new tab at the end of `pane`'s bar; it gets the focus. None as for `split`.
    pub fn add_tab(&mut self, pane: PaneId, kind: TabKind) -> Option<Opened> {
        let w = self.index_of_pane(pane)?;
        if self.shown_elsewhere(&kind, None) {
            return None;
        }
        let tab = TabId(self.next());
        let ws = &mut self.workspaces[w];
        let p = ws.pane_mut(pane)?;
        p.tabs.push(Tab { id: tab, kind });
        p.active = tab;
        ws.focused = pane;
        self.focused = Some(ws.id);
        Some(Opened {
            workspace: ws.id,
            pane,
            tab,
        })
    }

    /// Open file `path` in `pane` in `mode`: the pane's tab that already shows it (in either
    /// mode), now in `mode`, else a new tab at the end of its bar. Either way it is shown and
    /// gets the focus. None when `pane` is unknown.
    pub fn open_file(&mut self, pane: PaneId, path: PathBuf, mode: FileMode) -> Option<Opened> {
        let w = self.index_of_pane(pane)?;
        let ws = &self.workspaces[w];
        let found = ws
            .pane(pane)?
            .tabs
            .iter()
            .find(|t| matches!(&t.kind, TabKind::File { path: p, .. } if *p == path));
        let Some(tab) = found else {
            return self.add_tab(pane, TabKind::File { path, mode });
        };
        let opened = Opened {
            workspace: ws.id,
            pane,
            tab: tab.id,
        };
        // Opened for good (a double click on what a single click previewed): kept.
        self.pin(opened.tab);
        self.set_file_mode(opened.tab, mode);
        self.select_tab(opened.tab);
        Some(opened)
    }

    /// Preview file `path` in `pane` in `mode` (M11t: a single click): the pane's tab that
    /// already shows it is selected (a kept one stays kept); else the pane's preview tab shows
    /// it instead; else a new tab at the end of its bar becomes the pane's preview. None when
    /// `pane` is unknown.
    pub fn preview_file(&mut self, pane: PaneId, path: PathBuf, mode: FileMode) -> Option<Opened> {
        let w = self.index_of_pane(pane)?;
        let ws = &self.workspaces[w];
        let tabs = &ws.pane(pane)?.tabs;
        let showing = tabs
            .iter()
            .find(|t| matches!(&t.kind, TabKind::File { path: p, .. } if *p == path))
            .map(|t| t.id);
        let preview = tabs
            .iter()
            .map(|t| t.id)
            .find(|id| self.previews.contains(id));
        let workspace = ws.id;
        let tab = match (showing, preview) {
            (Some(tab), _) => {
                self.set_file_mode(tab, mode);
                tab
            }
            (None, Some(tab)) => {
                let t = self
                    .workspaces
                    .iter_mut()
                    .flat_map(|w| &mut w.panes)
                    .flat_map(|p| &mut p.tabs)
                    .find(|t| t.id == tab)?;
                t.kind = TabKind::File { path, mode };
                // Another file: shown as source (M11ac).
                self.md_previews.retain(|&t| t != tab);
                tab
            }
            (None, None) => {
                let opened = self.add_tab(pane, TabKind::File { path, mode })?;
                self.previews.push(opened.tab);
                return Some(opened);
            }
        };
        self.select_tab(tab);
        Some(Opened {
            workspace,
            pane,
            tab,
        })
    }

    /// Whether `tab` is its pane's preview tab (M11t: drawn in italics).
    pub fn is_preview(&self, tab: TabId) -> bool {
        self.previews.contains(&tab) && self.tab(tab).is_some()
    }

    /// Keep `tab`: no longer its pane's preview. False when it wasn't one.
    pub fn pin(&mut self, tab: TabId) -> bool {
        let before = self.previews.len();
        self.previews.retain(|&t| t != tab);
        self.previews.len() != before
    }

    /// Whether file tab `tab` shows its Markdown rendered (M11ac: its Preview on).
    pub fn md_preview(&self, tab: TabId) -> bool {
        self.md_previews.contains(&tab)
    }

    /// Turn file tab `tab`'s Markdown Preview on or off (M11ac). False when `tab` is unknown,
    /// not a file tab, or already so.
    pub fn set_md_preview(&mut self, tab: TabId, on: bool) -> bool {
        let is_file = self
            .tab(tab)
            .is_some_and(|t| matches!(t.kind, TabKind::File { .. }));
        if !is_file || self.md_preview(tab) == on {
            return false;
        }
        if on {
            self.md_previews.push(tab);
        } else {
            self.md_previews.retain(|&t| t != tab);
        }
        true
    }

    /// Show file tab `tab` in `mode`. False when `tab` is unknown, not a file tab, or already
    /// in `mode`.
    pub fn set_file_mode(&mut self, tab: TabId, mode: FileMode) -> bool {
        let found = self
            .workspaces
            .iter_mut()
            .flat_map(|w| &mut w.panes)
            .flat_map(|p| &mut p.tabs)
            .find(|t| t.id == tab);
        match found.map(|t| &mut t.kind) {
            Some(TabKind::File { mode: m, .. }) if *m != mode => {
                *m = mode;
                true
            }
            _ => false,
        }
    }

    /// Show something else in `tab` (a reconnected terminal, a browser's new page); returns
    /// what it showed. None when `tab` is unknown or another tab already shows the terminal.
    /// The caller ends the terminal it showed (the model only places terminals).
    pub fn set_kind(&mut self, tab: TabId, kind: TabKind) -> Option<TabKind> {
        if self.tab(tab).is_none() || self.shown_elsewhere(&kind, Some(tab)) {
            return None;
        }
        let t = self
            .workspaces
            .iter_mut()
            .flat_map(|w| &mut w.panes)
            .flat_map(|p| &mut p.tabs)
            .find(|t| t.id == tab)?;
        Some(std::mem::replace(&mut t.kind, kind))
    }

    /// Close `tab`. Its pane shows its right-hand neighbour (else its left) if it was showing
    /// it; a pane left with no tab closes (the pane that takes its space gets the focus), and a
    /// workspace left with no pane closes (the next in display order gets the focus, else the
    /// one before).
    pub fn close_tab(&mut self, tab: TabId) -> Option<Closed> {
        let (taken, mut closed) = self.take_tab(tab)?;
        self.md_previews.retain(|&t| t != tab);
        closed.tabs.insert(0, taken);
        Some(closed)
    }

    /// Make the terminal tabs match the terminals that run (`live`), a repair (by the rules
    /// they always match): a tab whose terminal is gone closes (as `close_tab`), and a live
    /// terminal no tab shows gets a workspace of its own at the end. What it repaired, for a
    /// log line; empty when they matched.
    pub fn reconcile(&mut self, live: &[u64]) -> Vec<String> {
        let mut repaired = Vec::new();
        let gone: Vec<(TabId, u64)> = self
            .panes()
            .flat_map(|p| &p.tabs)
            .filter_map(|t| match t.kind {
                TabKind::Terminal(id) if !live.contains(&id) => Some((t.id, id)),
                _ => None,
            })
            .collect();
        for (tab, id) in gone {
            self.close_tab(tab);
            repaired.push(format!("closed the tab of s{id}, which no longer runs"));
        }
        for &id in live {
            if self.terminal_tab(id).is_none() {
                let at = self.workspaces.len();
                self.new_workspace(at, TabKind::Terminal(id));
                repaired.push(format!("gave s{id}, shown by no tab, a workspace"));
            }
        }
        repaired
    }

    /// Move `tab` to `index` (clamped) in `pane`'s bar, in the same or another pane; it is shown
    /// and gets the focus. The pane it left closes if it is left empty, as for `close_tab` (the
    /// moved tab is not in `Closed::tabs`). None when either is unknown.
    pub fn move_tab(&mut self, tab: TabId, pane: PaneId, index: usize) -> Option<Closed> {
        let (_, from) = self.locate(tab)?;
        self.index_of_pane(pane)?;
        let closed = if from == pane {
            let p = self.pane_mut(pane)?;
            let at = p.tabs.iter().position(|t| t.id == tab)?;
            let moved = p.tabs.remove(at);
            p.tabs.insert(index.min(p.tabs.len()), moved);
            Closed::default()
        } else {
            // The target isn't the pane the tab leaves, so it outlives any close this causes.
            let (moved, closed) = self.take_tab(tab)?;
            let p = self.pane_mut(pane)?;
            p.tabs.insert(index.min(p.tabs.len()), moved);
            closed
        };
        self.select_tab(tab);
        Some(closed)
    }

    /// Where `tab` dropped at `drop` goes, as `move_tab` takes it: a pane, and an index in its
    /// bar once `tab` is out of it. A bar's gap; a workspace's row: the end of its focused (or
    /// last focused) pane. None when it would land where it is, or either is unknown.
    pub fn drop_place(&self, tab: TabId, drop: TabDrop) -> Option<(PaneId, usize)> {
        let (_, from) = self.locate(tab)?;
        let (pane, gap) = match drop {
            TabDrop::Bar { pane, gap } => (pane, gap),
            TabDrop::Workspace(ws) => {
                let ws = self.workspace(ws)?;
                (ws.focused, ws.pane(ws.focused)?.tabs.len())
            }
        };
        let tabs = &self.pane(pane)?.tabs;
        let gap = gap.min(tabs.len());
        if pane != from {
            return Some((pane, gap));
        }
        // Taking it out shifts the gaps after it one to the left.
        let at = tabs.iter().position(|t| t.id == tab)?;
        let index = if gap > at { gap - 1 } else { gap };
        (index != at).then_some((pane, index))
    }

    /// Show `tab` in its pane and give that pane, and its workspace, the focus.
    pub fn select_tab(&mut self, tab: TabId) -> bool {
        let Some((_, pane)) = self.locate(tab) else {
            return false;
        };
        if let Some(p) = self.pane_mut(pane) {
            p.active = tab;
        }
        self.focus_pane(pane)
    }

    /// Give `pane`, and its workspace, the focus.
    pub fn focus_pane(&mut self, pane: PaneId) -> bool {
        let Some(w) = self.index_of_pane(pane) else {
            return false;
        };
        self.workspaces[w].focused = pane;
        self.focused = Some(self.workspaces[w].id);
        true
    }

    pub fn focus_workspace(&mut self, workspace: WorkspaceId) -> bool {
        let found = self.workspace(workspace).is_some();
        if found {
            self.focused = Some(workspace);
        }
        found
    }

    /// ⌃Tab / ⌃⇧Tab: show the next (or previous) tab of the focused pane, wrapping. None when
    /// it has only one.
    pub fn cycle_tab(&mut self, forward: bool) -> Option<TabId> {
        let p = self.pane_mut(self.focused_pane()?)?;
        let n = p.tabs.len();
        if n < 2 {
            return None;
        }
        let at = p.tabs.iter().position(|t| t.id == p.active)?;
        let next = if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        p.active = p.tabs[next].id;
        Some(p.active)
    }

    /// Move a workspace to `to` in sidebar order. False (no change) when out of range.
    pub fn move_workspace(&mut self, workspace: WorkspaceId, to: usize) -> bool {
        let Some(from) = self.workspaces.iter().position(|w| w.id == workspace) else {
            return false;
        };
        if to >= self.workspaces.len() {
            return false;
        }
        let ws = self.workspaces.remove(from);
        self.workspaces.insert(to, ws);
        true
    }

    pub fn set_name(&mut self, workspace: WorkspaceId, name: Option<String>) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.name = name)
            .is_some()
    }

    pub fn set_color(&mut self, workspace: WorkspaceId, color: Option<TabColor>) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.color = color)
            .is_some()
    }

    /// Where a sidebar drag put a workspace (`Placed`); none: its own group. False when there
    /// is no such workspace.
    pub fn set_placed(&mut self, workspace: WorkspaceId, placed: Option<Placed>) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.placed = placed)
            .is_some()
    }

    pub fn set_pinned(&mut self, workspace: WorkspaceId, pinned: bool) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.pinned = pinned)
            .is_some()
    }

    /// Unpin a workspace and put it back with its group (by `group`; the ungrouped ones are one
    /// too): right after the last unpinned workspace of that group, so the groups' order stays;
    /// with none, where it is.
    /// False when there is no such workspace.
    pub fn unpin<K: PartialEq>(
        &mut self,
        workspace: WorkspaceId,
        group: impl Fn(&Workspace) -> Option<K>,
    ) -> bool {
        let Some(w) = self.workspace(workspace) else {
            return false;
        };
        let key = group(w);
        let last = self
            .workspaces
            .iter()
            .rposition(|w| w.id != workspace && !w.pinned && group(w) == key);
        self.set_pinned(workspace, false);
        let Some(last) = last else {
            return true;
        };
        let from = self.workspaces.iter().position(|w| w.id == workspace);
        // Right after it: one place less when it came from before it.
        let to = if from.is_some_and(|f| f < last) {
            last
        } else {
            last + 1
        };
        self.move_workspace(workspace, to);
        true
    }

    /// A divider drag: see `Node::set_ratio`.
    pub fn set_ratio(&mut self, workspace: WorkspaceId, path: &[bool], ratio: f32) -> bool {
        self.workspace_mut(workspace)
            .is_some_and(|w| w.layout.set_ratio(path, ratio))
    }

    pub fn equalize(&mut self, workspace: WorkspaceId) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.layout.equalize())
            .is_some()
    }

    /// The workspace's ring: its most urgent terminal (Waiting > Done > Working > Idle; a tie
    /// goes to the first in reading order) and that status. None when it has no terminal.
    pub fn rollup(
        &self,
        workspace: WorkspaceId,
        status: impl Fn(u64) -> Status,
    ) -> Option<(u64, Status)> {
        let mut best: Option<(u64, Status)> = None;
        for id in self.terminals(workspace) {
            let s = status(id);
            if best.is_none_or(|(_, b)| groups::urgency(s) > groups::urgency(b)) {
                best = Some((id, s));
            }
        }
        best
    }

    /// Every rule the ops keep; the first one broken, if any.
    pub fn check(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        let mut terminals = HashSet::new();
        let mut fresh = |id: u64, what: &str| {
            if id >= self.next_id {
                Err(format!("{what} {id} was never handed out"))
            } else if !ids.insert(id) {
                Err(format!("{what} {id}: the id is used twice"))
            } else {
                Ok(())
            }
        };
        for ws in &self.workspaces {
            fresh(ws.id.0, "workspace")?;
            let leaves = ws.layout.leaves();
            let mut placed: Vec<PaneId> = ws.panes.iter().map(|p| p.id).collect();
            let mut in_layout = leaves.clone();
            placed.sort();
            in_layout.sort();
            if placed != in_layout {
                return Err(format!(
                    "workspace {:?}: layout {leaves:?} vs panes {placed:?}",
                    ws.id
                ));
            }
            if !ratios_in_range(&ws.layout) {
                return Err(format!("workspace {:?}: a ratio out of range", ws.id));
            }
            if !leaves.contains(&ws.focused) {
                return Err(format!("workspace {:?}: focuses a pane it hasn't", ws.id));
            }
            for p in &ws.panes {
                fresh(p.id.0, "pane")?;
                if !p.tabs.iter().any(|t| t.id == p.active) {
                    return Err(format!("pane {:?}: shows a tab it hasn't", p.id));
                }
                for t in &p.tabs {
                    fresh(t.id.0, "tab")?;
                    if let TabKind::Terminal(id) = t.kind
                        && !terminals.insert(id)
                    {
                        return Err(format!("terminal s{id} is in two tabs"));
                    }
                }
            }
        }
        match self.focused {
            None if self.workspaces.is_empty() => Ok(()),
            None => Err("no workspace is focused".into()),
            Some(id) if self.workspace(id).is_some() => Ok(()),
            Some(id) => Err(format!("the focused workspace {id:?} is gone")),
        }
    }

    /// The model for the file, workspaces in sidebar order, and the focused one's index.
    /// `session` gives a terminal's index in the file's `sessions`; a terminal without one is
    /// left out (and a pane or workspace left empty by that with it).
    pub fn to_saved(
        &self,
        session: impl Fn(u64) -> Option<usize>,
    ) -> (Vec<SavedWorkspace>, Option<usize>) {
        let mut saved = Vec::new();
        let mut active = None;
        for ws in &self.workspaces {
            let mut kept = Vec::new();
            let layout = filter_leaves(&ws.layout, &mut |&id: &PaneId| {
                let pane = ws.pane(id)?;
                let mut tabs = Vec::new();
                let mut shown = 0;
                for t in &pane.tabs {
                    let tab = match &t.kind {
                        TabKind::Terminal(id) => {
                            let Some(session) = session(*id) else {
                                continue;
                            };
                            SavedTab::Terminal { session }
                        }
                        TabKind::Browser { url } => SavedTab::Browser { url: url.clone() },
                        TabKind::File { path, mode } => SavedTab::File {
                            path: path.clone(),
                            mode: *mode,
                            preview: self.md_preview(t.id),
                        },
                    };
                    if t.id == pane.active {
                        shown = tabs.len();
                    }
                    tabs.push(tab);
                }
                (!tabs.is_empty()).then(|| {
                    kept.push(id);
                    SavedPane {
                        tabs,
                        active: shown,
                    }
                })
            });
            let Some(layout) = layout else { continue };
            if self.focused == Some(ws.id) {
                active = Some(saved.len());
            }
            saved.push(SavedWorkspace {
                name: ws.name.clone(),
                color: ws.color,
                pinned: ws.pinned,
                placed: ws.placed.clone(),
                layout,
                focused: kept.iter().position(|&p| p == ws.focused).unwrap_or(0),
            });
        }
        (saved, active)
    }

    /// The model from a saved file. `terminals[i]` is the terminal opened for saved session `i`
    /// (none: it didn't start). A tab whose session is unknown, didn't start or is already shown
    /// is dropped, then empty panes and workspaces; a started session no tab shows becomes a
    /// workspace of its own at the end, so no terminal is lost. `active` is the focused
    /// workspace's index in `saved`.
    pub fn from_saved(
        saved: &[SavedWorkspace],
        active: Option<usize>,
        terminals: &[Option<u64>],
    ) -> Self {
        let mut m = Model::new();
        let mut used = vec![false; terminals.len()];
        let mut shown = HashSet::new();
        let mut focused = None;
        for (i, sw) in saved.iter().enumerate() {
            let mut panes = Vec::new();
            let mut by_place = Vec::new(); // the saved panes in reading order: kept or not
            let layout = filter_leaves(&sw.layout, &mut |sp: &SavedPane| {
                let pane = m.restored_pane(sp, terminals, &mut used, &mut shown);
                by_place.push(pane.as_ref().map(|p| p.id));
                let id = pane.as_ref().map(|p| p.id);
                panes.extend(pane);
                id
            });
            let Some(layout) = layout else { continue };
            let id = WorkspaceId(m.next());
            let first = layout.leaves()[0];
            m.workspaces.push(Workspace {
                id,
                name: sw.name.clone(),
                color: sw.color,
                pinned: sw.pinned,
                placed: sw.placed.clone(),
                focused: by_place.get(sw.focused).copied().flatten().unwrap_or(first),
                layout,
                panes,
            });
            if active == Some(i) {
                focused = Some(id);
            }
        }
        for (session, terminal) in terminals.iter().enumerate() {
            let Some(terminal) = *terminal else { continue };
            if used[session] || !shown.insert(terminal) {
                continue;
            }
            used[session] = true;
            let at = m.workspaces.len();
            m.new_workspace(at, TabKind::Terminal(terminal));
        }
        m.focused = focused.or_else(|| m.display_order().first().copied());
        m
    }

    /// The model from a version-1 file: each saved layout becomes a workspace and each of its
    /// panes a pane with one terminal tab; a session in no layout is a workspace of its own.
    /// Bad layouts (an unknown or repeated session) are dropped, so their sessions stand alone,
    /// as version 1 did. A workspace takes the first name and colour its sessions had (those
    /// were the sidebar row's). `terminals` is as for `from_saved`.
    pub fn from_old(state: &SavedState, terminals: &[Option<u64>]) -> Self {
        let (mut trees, _) = split_tree::sanitize(state.layouts.clone(), state.sessions.len());
        // Layouts were kept in the order they were made; the sidebar listed the sessions in
        // order, so each workspace goes where its first row was.
        trees.sort_by_key(|t| t.leaves().into_iter().min());
        let mut active = None;
        let saved: Vec<SavedWorkspace> = trees
            .iter()
            .enumerate()
            .map(|(i, tree)| {
                let order = tree.leaves();
                let sessions = order.iter().map(|&s| &state.sessions[s]);
                let at = state
                    .active
                    .and_then(|a| order.iter().position(|&s| s == a));
                if at.is_some() {
                    active = Some(i);
                }
                SavedWorkspace {
                    name: sessions.clone().find_map(|s| s.name.clone()),
                    color: sessions.clone().find_map(|s| s.color),
                    pinned: false,
                    placed: None,
                    layout: tree.map(&|session| SavedPane {
                        tabs: vec![SavedTab::Terminal { session }],
                        active: 0,
                    }),
                    focused: at.unwrap_or(0),
                }
            })
            .collect();
        Self::from_saved(&saved, active, terminals)
    }

    /// The model from a saved file: by its workspaces (version 2), or else by the layouts of
    /// a version-1 file (`from_old`). `terminals` is as for `from_saved`.
    pub fn from_state(state: &SavedState, terminals: &[Option<u64>]) -> Self {
        if state.workspaces.is_empty() {
            Self::from_old(state, terminals)
        } else {
            Self::from_saved(&state.workspaces, state.active_workspace, terminals)
        }
    }

    /// A sidebar drag: move a workspace to `to` in display order, inside its own run (pinned or
    /// not). False (no change) when `to` is out of range or in the other run.
    pub fn move_workspace_display(&mut self, workspace: WorkspaceId, to: usize) -> bool {
        let Some(&target) = self.display_order().get(to) else {
            return false;
        };
        let pinned = |id| self.workspace(id).map(|w| w.pinned);
        if pinned(workspace) != pinned(target) {
            return false;
        }
        // Within one run, display order is sidebar order: taking the target's sidebar place
        // lands it before the target (moving up) or after it (moving down), as in display order.
        let Some(at) = self.workspaces.iter().position(|w| w.id == target) else {
            return false;
        };
        self.move_workspace(workspace, at)
    }

    /// Keep each group's workspaces in one run, as the sidebar draws them under one header:
    /// ungrouped ones first, then each group where it first appears; otherwise the order stays.
    /// Pinned workspaces keep their places (the sidebar shows them above the groups, in their own
    /// order), so the unpinned ones are grouped in the places left. True when something moved.
    pub fn group_together<K: PartialEq>(
        &mut self,
        group: impl Fn(&Workspace) -> Option<K>,
    ) -> bool {
        // The unpinned workspaces' places, and their groups.
        let places: Vec<usize> = (0..self.workspaces.len())
            .filter(|&i| !self.workspaces[i].pinned)
            .collect();
        let keys: Vec<Option<K>> = places.iter().map(|&i| group(&self.workspaces[i])).collect();
        // Each one's run: 0 for ungrouped, else 1 + where its group first appears.
        let run = |n: usize| match &keys[n] {
            None => 0,
            Some(k) => 1 + keys.iter().position(|q| q.as_ref() == Some(k)).unwrap_or(n),
        };
        let mut sorted: Vec<usize> = (0..keys.len()).collect();
        sorted.sort_by_key(|&n| run(n)); // stable: the order inside a run stays
        let mut order: Vec<usize> = (0..self.workspaces.len()).collect();
        for (&place, &n) in places.iter().zip(&sorted) {
            order[place] = places[n];
        }
        if order.iter().enumerate().all(|(at, &i)| at == i) {
            return false;
        }
        let mut old: Vec<Option<Workspace>> = std::mem::take(&mut self.workspaces)
            .into_iter()
            .map(Some)
            .collect();
        self.workspaces = order.into_iter().filter_map(|i| old[i].take()).collect();
        true
    }

    /// The next unused id (ids are shared by workspaces, panes and tabs).
    fn next(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id - 1
    }

    /// A pane with one tab showing `kind`.
    fn new_pane(&mut self, kind: TabKind) -> Pane {
        let id = PaneId(self.next());
        let tab = TabId(self.next());
        Pane {
            id,
            tabs: vec![Tab { id: tab, kind }],
            active: tab,
        }
    }

    /// A saved pane's tabs that can come back (their session started, and neither it nor its
    /// terminal is shown yet), marking those used; none when no tab can.
    fn restored_pane(
        &mut self,
        saved: &SavedPane,
        terminals: &[Option<u64>],
        used: &mut [bool],
        shown: &mut HashSet<u64>,
    ) -> Option<Pane> {
        let mut tabs = Vec::new();
        let mut active = None;
        for (i, st) in saved.tabs.iter().enumerate() {
            let kind = match st {
                SavedTab::Terminal { session } => {
                    let Some(Some(terminal)) = terminals.get(*session) else {
                        continue;
                    };
                    if used[*session] || !shown.insert(*terminal) {
                        continue;
                    }
                    used[*session] = true;
                    TabKind::Terminal(*terminal)
                }
                SavedTab::Browser { url } => TabKind::Browser { url: url.clone() },
                // A file gone since comes back too: its tab says so (M11g Task 6).
                SavedTab::File { path, mode, .. } => TabKind::File {
                    path: path.clone(),
                    mode: *mode,
                },
            };
            let id = TabId(self.next());
            if i == saved.active {
                active = Some(id);
            }
            if let SavedTab::File { preview: true, .. } = st {
                self.md_previews.push(id);
            }
            tabs.push(Tab { id, kind });
        }
        let first = tabs.first()?.id;
        Some(Pane {
            id: PaneId(self.next()),
            active: active.unwrap_or(first),
            tabs,
        })
    }

    fn panes(&self) -> impl Iterator<Item = &Pane> {
        self.workspaces.iter().flat_map(|w| &w.panes)
    }

    fn workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.workspaces
            .iter_mut()
            .flat_map(|w| &mut w.panes)
            .find(|p| p.id == id)
    }

    fn index_of_pane(&self, pane: PaneId) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|w| w.panes.iter().any(|p| p.id == pane))
    }

    /// Whether `kind` is a terminal some tab other than `except` already shows (a page or a file
    /// may be open in any number of tabs).
    fn shown_elsewhere(&self, kind: &TabKind, except: Option<TabId>) -> bool {
        matches!(kind, TabKind::Terminal(_))
            && self
                .panes()
                .flat_map(|p| &p.tabs)
                .any(|t| t.kind == *kind && Some(t.id) != except)
    }

    /// Take `tab` out of its pane, closing the pane (and then its workspace) if it is left
    /// empty; the focus moves per `close_tab`.
    fn take_tab(&mut self, tab: TabId) -> Option<(Tab, Closed)> {
        let (_, pane) = self.locate(tab)?;
        // Closed or moved to another pane: no longer a preview.
        self.pin(tab);
        let w = self.index_of_pane(pane)?;
        let mut closed = Closed::default();
        let ws = &mut self.workspaces[w];
        let p = ws.pane_mut(pane)?;
        let at = p.tabs.iter().position(|t| t.id == tab)?;
        let taken = p.tabs.remove(at);
        if let Some(next) = p.tabs.get(at).or_else(|| p.tabs.last()) {
            if p.active == tab {
                p.active = next.id;
            }
            return Some((taken, closed));
        }
        // The pane is empty: it closes.
        ws.panes.retain(|p| p.id != pane);
        closed.panes.push(pane);
        let heir = ws.layout.heir(pane);
        let layout = std::mem::replace(&mut ws.layout, Node::Leaf(pane));
        match layout.remove(pane) {
            Some(rest) => {
                ws.layout = rest;
                if ws.focused == pane {
                    ws.focused = heir.unwrap_or_else(|| ws.layout.leaves()[0]);
                }
            }
            None => {
                closed.workspaces.push(ws.id);
                self.remove_workspace(w);
            }
        }
        Some((taken, closed))
    }

    /// Remove the workspace at `w` in sidebar order; if it was focused, the next in display
    /// order gets the focus, else the one before.
    fn remove_workspace(&mut self, w: usize) -> Workspace {
        let id = self.workspaces[w].id;
        if self.focused == Some(id) {
            let order = self.display_order();
            let at = order.iter().position(|&o| o == id).unwrap_or(0);
            self.focused = order
                .get(at + 1)
                .or_else(|| at.checked_sub(1).and_then(|b| order.get(b)))
                .copied();
        }
        self.workspaces.remove(w)
    }
}

impl Workspace {
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }
}

/// The session the file's focused tab shows (its index in `sessions`): by the focused
/// workspace's focused pane's shown tab (version 2), else the active session (version 1). None
/// when that is out of range.
pub fn saved_focus(state: &SavedState) -> Option<SavedFocus> {
    if state.workspaces.is_empty() {
        return state.active.map(SavedFocus::Terminal);
    }
    let ws = state.workspaces.get(state.active_workspace?)?;
    let pane = nth_leaf(&ws.layout, ws.focused)?;
    Some(match pane.tabs.get(pane.active)? {
        SavedTab::Terminal { session } => SavedFocus::Terminal(*session),
        SavedTab::Browser { .. } => SavedFocus::Browser,
        SavedTab::File { .. } => SavedFocus::File,
    })
}

/// What a file's focused tab showed: a saved session (its index in `sessions`), a page, or a
/// file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedFocus {
    Terminal(usize),
    Browser,
    File,
}

/// `node`'s `n`th leaf in reading order.
fn nth_leaf<T>(node: &Node<T>, n: usize) -> Option<&T> {
    fn walk<'a, T>(node: &'a Node<T>, n: &mut usize) -> Option<&'a T> {
        match node {
            Node::Leaf(leaf) if *n == 0 => Some(leaf),
            Node::Leaf(_) => {
                *n -= 1;
                None
            }
            Node::Split { first, second, .. } => walk(first, n).or_else(|| walk(second, n)),
        }
    }
    walk(node, &mut { n })
}

/// Saved workspaces with each terminal tab's session renumbered by `map` (none: left out, with
/// a pane or workspace it leaves empty); `active` follows its workspace (none if it went).
pub fn renumber(
    saved: &[SavedWorkspace],
    active: Option<usize>,
    map: impl Fn(usize) -> Option<usize>,
) -> (Vec<SavedWorkspace>, Option<usize>) {
    let mut kept = Vec::new();
    let mut kept_active = None;
    for (i, sw) in saved.iter().enumerate() {
        // Each saved pane's new place in reading order, if kept.
        let mut places: Vec<Option<usize>> = Vec::new();
        let mut placed = 0;
        let layout = filter_leaves(&sw.layout, &mut |sp: &SavedPane| {
            let mut tabs = Vec::new();
            let mut active = 0;
            for (t, tab) in sp.tabs.iter().enumerate() {
                let tab = match tab {
                    SavedTab::Terminal { session } => match map(*session) {
                        Some(session) => SavedTab::Terminal { session },
                        None => continue,
                    },
                    page_or_file => page_or_file.clone(),
                };
                if t <= sp.active {
                    active = tabs.len();
                }
                tabs.push(tab);
            }
            let pane = (!tabs.is_empty()).then(|| SavedPane { tabs, active });
            places.push(pane.as_ref().map(|_| placed));
            placed += usize::from(pane.is_some());
            pane
        });
        let Some(layout) = layout else { continue };
        if active == Some(i) {
            kept_active = Some(kept.len());
        }
        kept.push(SavedWorkspace {
            layout,
            focused: places.get(sw.focused).copied().flatten().unwrap_or(0),
            ..sw.clone()
        });
    }
    (kept, kept_active)
}

/// `node` with each leaf mapped by `keep` (in reading order); a leaf it drops gives its space
/// to its sibling, and ratios are clamped. None when every leaf is dropped.
fn filter_leaves<A, B>(node: &Node<A>, keep: &mut impl FnMut(&A) -> Option<B>) -> Option<Node<B>> {
    match node {
        Node::Leaf(a) => keep(a).map(Node::Leaf),
        Node::Split {
            axis,
            ratio,
            first,
            second,
        } => match (filter_leaves(first, keep), filter_leaves(second, keep)) {
            (Some(first), Some(second)) => Some(Node::Split {
                axis: *axis,
                ratio: split_tree::clamp_ratio(*ratio),
                first: Box::new(first),
                second: Box::new(second),
            }),
            (one, other) => one.or(other),
        },
    }
}

fn ratios_in_range<T>(node: &Node<T>) -> bool {
    match node {
        Node::Leaf(_) => true,
        Node::Split {
            ratio,
            first,
            second,
            ..
        } => {
            (split_tree::MIN_RATIO..=split_tree::MAX_RATIO).contains(ratio)
                && ratios_in_range(first)
                && ratios_in_range(second)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::groups;
    use crate::restore::{self, SavedSession};

    fn term(id: u64) -> TabKind {
        TabKind::Terminal(id)
    }
    fn web(url: &str) -> TabKind {
        TabKind::Browser {
            url: Some(url.into()),
        }
    }
    fn ok(m: &Model) {
        m.check().unwrap_or_else(|e| panic!("{e}"));
    }
    /// Every terminal any tab shows.
    fn all_terminals(m: &Model) -> BTreeSet<u64> {
        m.workspaces()
            .iter()
            .flat_map(|w| &w.panes)
            .flat_map(|p| &p.tabs)
            .filter_map(|t| match t.kind {
                TabKind::Terminal(id) => Some(id),
                TabKind::Browser { .. } | TabKind::File { .. } => None,
            })
            .collect()
    }
    fn tab_kinds(m: &Model, pane: PaneId) -> Vec<TabKind> {
        m.pane(pane)
            .unwrap()
            .tabs
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }
    /// Each workspace's terminals by pane in reading order: the shape a migration produced.
    fn shape(m: &Model) -> Vec<Vec<Vec<TabKind>>> {
        m.workspaces()
            .iter()
            .map(|w| {
                w.layout
                    .leaves()
                    .into_iter()
                    .map(|p| tab_kinds(m, p))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_new_workspace_is_one_pane_with_one_tab_and_takes_the_focus() {
        let mut m = Model::new();
        assert_eq!(m.focused_workspace(), None);
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.new_workspace(0, term(2)).unwrap(); // at the top
        ok(&m);
        let ws = m.workspace(b.workspace).unwrap();
        assert_eq!(ws.layout, Node::Leaf(b.pane));
        assert_eq!(tab_kinds(&m, b.pane), [term(2)]);
        assert_eq!(m.focused_workspace(), Some(b.workspace));
        assert_eq!(m.focused_pane(), Some(b.pane));
        assert_eq!(m.focused_tab(), Some(b.tab));
        assert_eq!(
            m.workspaces().iter().map(|w| w.id).collect::<Vec<_>>(),
            [b.workspace, a.workspace]
        );
        // Past the end: clamped to the end.
        let c = m.new_workspace(99, web("https://a.test")).unwrap();
        assert_eq!(m.workspaces().last().unwrap().id, c.workspace);
        // Ids are never handed out twice.
        let ids = [
            a.workspace.0,
            a.pane.0,
            a.tab.0,
            b.workspace.0,
            b.pane.0,
            b.tab.0,
        ];
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
        ok(&m);
    }

    #[test]
    fn a_terminal_is_shown_by_one_tab_only() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        assert_eq!(m.new_workspace(0, term(1)), None);
        assert_eq!(m.split(a.pane, Axis::Row, term(1)), None);
        assert_eq!(m.add_tab(a.pane, term(1)), None);
        let b = m.add_tab(a.pane, term(2)).unwrap();
        assert_eq!(m.set_kind(b.tab, term(1)), None);
        assert_eq!(m.workspaces().len(), 1);
        assert_eq!(tab_kinds(&m, a.pane), [term(1), term(2)]);
        // A tab may take a new terminal (an SSH reconnect) or keep its own.
        assert_eq!(m.set_kind(b.tab, term(2)), Some(term(2)));
        assert_eq!(m.set_kind(b.tab, term(9)), Some(term(2)));
        assert_eq!(m.terminal_tab(9), Some(b.tab));
        assert_eq!(m.terminal_tab(2), None);
        assert_eq!(m.set_kind(TabId(999), term(5)), None);
        ok(&m);
    }

    #[test]
    fn split_puts_a_new_pane_with_one_tab_beside_the_pane() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let right = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let below = m
            .split(a.pane, Axis::Column, web("http://localhost:3000"))
            .unwrap();
        ok(&m);
        assert_eq!(right.workspace, a.workspace);
        let ws = m.workspace(a.workspace).unwrap();
        // (1 over browser) | 2
        assert_eq!(ws.layout.leaves(), [a.pane, below.pane, right.pane]);
        assert_eq!(tab_kinds(&m, below.pane), [web("http://localhost:3000")]);
        assert_eq!(m.focused_pane(), Some(below.pane));
        assert_eq!(m.focused_tab(), Some(below.tab));
        assert_eq!(m.terminals(a.workspace), [1, 2]);
        assert_eq!(m.split(PaneId(999), Axis::Row, term(3)), None);
    }

    #[test]
    fn a_split_in_another_workspace_moves_the_focus_there() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.new_workspace(1, term(2)).unwrap();
        assert_eq!(m.focused_workspace(), Some(b.workspace));
        let c = m.split(a.pane, Axis::Row, term(3)).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        assert_eq!(m.focused_pane(), Some(c.pane));
        ok(&m);
    }

    #[test]
    fn add_tab_appends_to_the_bar_and_shows_it() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.add_tab(a.pane, web("https://b.test")).unwrap();
        let c = m.add_tab(a.pane, term(3)).unwrap();
        ok(&m);
        assert_eq!((b.pane, c.pane), (a.pane, a.pane));
        assert_eq!(
            tab_kinds(&m, a.pane),
            [term(1), web("https://b.test"), term(3)]
        );
        assert_eq!(m.pane(a.pane).unwrap().active, c.tab);
        assert_eq!(m.locate(b.tab), Some((a.workspace, a.pane)));
        assert_eq!(m.add_tab(PaneId(999), term(4)), None);
    }

    #[test]
    fn closing_the_shown_tab_shows_its_right_neighbour_else_its_left() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.add_tab(a.pane, term(2)).unwrap();
        let c = m.add_tab(a.pane, term(3)).unwrap();
        assert!(m.select_tab(b.tab));
        let closed = m.close_tab(b.tab).unwrap();
        assert_eq!(
            closed,
            Closed {
                tabs: vec![Tab {
                    id: b.tab,
                    kind: term(2)
                }],
                ..Closed::default()
            }
        );
        assert_eq!(m.pane(a.pane).unwrap().active, c.tab);
        m.close_tab(c.tab).unwrap();
        assert_eq!(m.pane(a.pane).unwrap().active, a.tab);
        // Closing a tab that isn't shown leaves the shown one.
        let d = m.add_tab(a.pane, term(4)).unwrap();
        m.close_tab(a.tab).unwrap();
        assert_eq!(m.pane(a.pane).unwrap().active, d.tab);
        assert_eq!(m.close_tab(a.tab), None);
        ok(&m);
    }

    #[test]
    fn closing_a_pane_s_last_tab_closes_the_pane() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let c = m.split(b.pane, Axis::Column, term(3)).unwrap();
        // a | (b over c); close a: the (b over c) side takes its space, its first pane the focus.
        assert!(m.focus_pane(a.pane));
        let closed = m.close_tab(a.tab).unwrap();
        assert_eq!(closed.panes, [a.pane]);
        assert_eq!(closed.workspaces, []);
        assert_eq!(m.pane(a.pane), None);
        assert_eq!(m.focused_pane(), Some(b.pane));
        assert_eq!(
            m.workspace(a.workspace).unwrap().layout.leaves(),
            [b.pane, c.pane]
        );
        // Closing a pane that isn't focused keeps the focus where it is.
        let closed = m.close_tab(c.tab).unwrap();
        assert_eq!(closed.panes, [c.pane]);
        assert_eq!(m.focused_pane(), Some(b.pane));
        assert_eq!(m.workspace(a.workspace).unwrap().layout, Node::Leaf(b.pane));
        ok(&m);
    }

    #[test]
    fn closing_a_workspace_s_last_pane_closes_the_workspace() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.new_workspace(1, term(2)).unwrap();
        let c = m.new_workspace(2, term(3)).unwrap();
        // The focused one closes: the next in display order gets the focus.
        m.focus_workspace(b.workspace);
        let closed = m.close_tab(b.tab).unwrap();
        assert_eq!(
            closed,
            Closed {
                tabs: vec![Tab {
                    id: b.tab,
                    kind: term(2)
                }],
                panes: vec![b.pane],
                workspaces: vec![b.workspace],
            }
        );
        assert_eq!(m.focused_workspace(), Some(c.workspace));
        // The last in display order closes: the one before it gets the focus.
        m.close_tab(c.tab).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        ok(&m);
        // The last workspace closes: nothing is focused.
        m.close_tab(a.tab).unwrap();
        assert_eq!(m.focused_workspace(), None);
        assert_eq!(m.focused_tab(), None);
        assert!(m.workspaces().is_empty());
        ok(&m);
    }

    #[test]
    fn the_focus_after_a_close_follows_the_display_order() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.new_workspace(1, term(2)).unwrap();
        let c = m.new_workspace(2, term(3)).unwrap();
        // Display order: c (pinned), a, b. Closing c focuses a, its next in display order.
        m.set_pinned(c.workspace, true);
        m.focus_workspace(c.workspace);
        m.close_tab(c.tab).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        // A workspace that isn't focused closes: the focus stays.
        m.close_tab(b.tab).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        ok(&m);
    }

    #[test]
    fn closing_every_tab_of_a_workspace_closes_it() {
        let mut m = Model::new();
        let keep = m.new_workspace(0, term(9)).unwrap();
        let a = m.new_workspace(1, term(1)).unwrap();
        let b = m.add_tab(a.pane, web("https://x.test")).unwrap();
        let c = m.split(a.pane, Axis::Row, term(3)).unwrap();
        // As closing a workspace does: each of its tabs (`tabs_of`), in turn.
        let closed: Vec<Closed> = m
            .tabs_of(a.workspace)
            .into_iter()
            .map(|t| m.close_tab(t).unwrap())
            .collect();
        let tabs: BTreeSet<TabId> = closed.iter().flat_map(|c| &c.tabs).map(|t| t.id).collect();
        assert_eq!(tabs, BTreeSet::from([a.tab, b.tab, c.tab]));
        let workspaces: Vec<WorkspaceId> =
            closed.iter().flat_map(|c| c.workspaces.clone()).collect();
        assert_eq!(workspaces, [a.workspace], "once, with its last tab");
        assert_eq!(all_terminals(&m), BTreeSet::from([9]));
        assert_eq!(m.focused_workspace(), Some(keep.workspace));
        ok(&m);
    }

    #[test]
    fn a_workspace_s_tabs_are_every_tab_in_reading_order() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let page = m.add_tab(a.pane, web("http://localhost:3000")).unwrap();
        let right = m.split(a.pane, Axis::Row, web("https://docs.rs")).unwrap();
        let other = m.new_workspace(1, term(2)).unwrap();
        assert_eq!(
            m.tabs_of(a.workspace),
            [a.tab, page.tab, right.tab],
            "browser tabs too"
        );
        assert_eq!(m.tabs_of(other.workspace), [other.tab]);
        assert!(m.tabs_of(WorkspaceId(999)).is_empty());
        // Only the terminals for `terminals`.
        assert_eq!(m.terminals(a.workspace), [1]);
    }

    #[test]
    fn reconciling_closes_tabs_of_gone_terminals_and_shows_untabbed_ones() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.split(a.pane, Axis::Row, term(2)).unwrap();
        let b = m.new_workspace(1, term(3)).unwrap();
        m.add_tab(b.pane, web("https://kept.test")).unwrap();
        // Matching: nothing to repair, nothing changes.
        let before = m.clone();
        assert!(m.reconcile(&[1, 2, 3]).is_empty());
        assert_eq!(m, before);
        // A desync: terminal 2 is gone (its tab stayed) and 7 runs with no tab.
        let repaired = m.reconcile(&[1, 3, 7]);
        ok(&m);
        assert_eq!(repaired.len(), 2, "{repaired:?}");
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(1)]],
                vec![vec![term(3), web("https://kept.test")]],
                vec![vec![term(7)]]
            ]
        );
        assert!(m.reconcile(&[1, 3, 7]).is_empty());
    }

    #[test]
    fn move_tab_reorders_within_a_pane() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.add_tab(a.pane, term(2)).unwrap();
        m.add_tab(a.pane, term(3)).unwrap();
        assert_eq!(m.move_tab(a.tab, a.pane, 99), Some(Closed::default()));
        assert_eq!(tab_kinds(&m, a.pane), [term(2), term(3), term(1)]);
        assert_eq!(m.pane(a.pane).unwrap().active, a.tab);
        assert_eq!(m.move_tab(a.tab, a.pane, 1), Some(Closed::default()));
        assert_eq!(tab_kinds(&m, a.pane), [term(2), term(1), term(3)]);
        ok(&m);
    }

    #[test]
    fn move_tab_to_another_pane_closes_the_pane_it_leaves_empty() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let a2 = m.add_tab(a.pane, term(3)).unwrap();
        // a has two tabs: moving one leaves the pane, which shows the other.
        assert_eq!(m.move_tab(a2.tab, b.pane, 0), Some(Closed::default()));
        assert_eq!(tab_kinds(&m, b.pane), [term(3), term(2)]);
        assert_eq!(m.pane(b.pane).unwrap().active, a2.tab);
        assert_eq!(m.pane(a.pane).unwrap().active, a.tab);
        assert_eq!(m.focused_pane(), Some(b.pane));
        // Moving a's only tab closes a.
        let closed = m.move_tab(a.tab, b.pane, 1).unwrap();
        assert_eq!(
            closed,
            Closed {
                panes: vec![a.pane],
                ..Closed::default()
            }
        );
        assert_eq!(tab_kinds(&m, b.pane), [term(3), term(1), term(2)]);
        assert_eq!(m.workspace(a.workspace).unwrap().layout, Node::Leaf(b.pane));
        ok(&m);
        // Moving a pane's only tab into the same pane changes nothing.
        let c = m.new_workspace(1, term(4)).unwrap();
        assert_eq!(m.move_tab(c.tab, c.pane, 0), Some(Closed::default()));
        ok(&m);
        // Into another workspace: the one it leaves empty closes, the focus goes with the tab.
        let closed = m.move_tab(c.tab, b.pane, 0).unwrap();
        assert_eq!(closed.workspaces, [c.workspace]);
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        assert_eq!(m.focused_tab(), Some(c.tab));
        assert_eq!(m.move_tab(TabId(999), b.pane, 0), None);
        assert_eq!(m.move_tab(c.tab, PaneId(999), 0), None);
        ok(&m);
    }

    #[test]
    fn move_tab_across_panes_shows_the_left_pane_s_neighbour_and_selects_the_moved_tab() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let a2 = m.add_tab(a.pane, term(2)).unwrap();
        let a3 = m.add_tab(a.pane, term(3)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(4)).unwrap();
        // a shows its middle tab; moving it away shows its right-hand neighbour.
        m.select_tab(a2.tab);
        assert_eq!(m.move_tab(a2.tab, b.pane, 1), Some(Closed::default()));
        assert_eq!(m.pane(a.pane).unwrap().active, a3.tab);
        assert_eq!(tab_kinds(&m, a.pane), [term(1), term(3)]);
        assert_eq!(tab_kinds(&m, b.pane), [term(4), term(2)]);
        assert_eq!(m.pane(b.pane).unwrap().active, a2.tab);
        assert_eq!(m.focused_pane(), Some(b.pane));
        // a shows its last tab; moving it away shows the one left of it.
        m.select_tab(a3.tab);
        assert_eq!(m.move_tab(a3.tab, b.pane, 0), Some(Closed::default()));
        assert_eq!(m.pane(a.pane).unwrap().active, a.tab);
        // A tab a pane doesn't show moves without changing what that pane shows.
        let a4 = m.add_tab(a.pane, term(5)).unwrap();
        assert_eq!(m.move_tab(a.tab, b.pane, 99), Some(Closed::default()));
        assert_eq!(m.pane(a.pane).unwrap().active, a4.tab);
        assert_eq!(tab_kinds(&m, b.pane), [term(3), term(4), term(2), term(1)]);
        assert_eq!(m.focused_tab(), Some(a.tab));
        ok(&m);
    }

    #[test]
    fn move_tab_into_another_workspace_closes_what_it_leaves_empty() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.new_workspace(1, term(2)).unwrap();
        let b2 = m.split(b.pane, Axis::Column, term(3)).unwrap();
        let c = m.new_workspace(2, term(4)).unwrap();
        // b's lower pane loses its only tab: it closes, b's upper pane takes its place and b's
        // focus; b stays.
        let closed = m.move_tab(b2.tab, a.pane, 1).unwrap();
        assert_eq!(
            closed,
            Closed {
                panes: vec![b2.pane],
                ..Closed::default()
            }
        );
        assert_eq!(m.workspace(b.workspace).unwrap().layout, Node::Leaf(b.pane));
        assert_eq!(m.workspace(b.workspace).unwrap().focused, b.pane);
        // The tab is shown where it went, and that workspace has the focus.
        assert_eq!(tab_kinds(&m, a.pane), [term(1), term(3)]);
        assert_eq!(m.locate(b2.tab), Some((a.workspace, a.pane)));
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        assert_eq!(m.focused_tab(), Some(b2.tab));
        ok(&m);
        // c's only tab leaves: c's pane and c close, nothing ends (no tab in `Closed`).
        let closed = m.move_tab(c.tab, b.pane, 0).unwrap();
        assert_eq!(
            closed,
            Closed {
                tabs: vec![],
                panes: vec![c.pane],
                workspaces: vec![c.workspace],
            }
        );
        assert!(m.workspace(c.workspace).is_none());
        assert_eq!(m.workspaces().len(), 2);
        assert_eq!(tab_kinds(&m, b.pane), [term(4), term(2)]);
        assert_eq!(m.focused_workspace(), Some(b.workspace));
        assert_eq!(m.focused_tab(), Some(c.tab));
        // Every terminal is still shown, once.
        assert_eq!(all_terminals(&m), BTreeSet::from([1, 2, 3, 4]));
        ok(&m);
    }

    #[test]
    fn a_tab_dropped_where_it_is_stays_put() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let a2 = m.add_tab(a.pane, term(2)).unwrap();
        m.add_tab(a.pane, term(3)).unwrap();
        let before = m.clone();
        // At its own index: the same order, nothing closes.
        assert_eq!(m.move_tab(a2.tab, a.pane, 1), Some(Closed::default()));
        assert_eq!(tab_kinds(&m, a.pane), tab_kinds(&before, a.pane));
        // A pane's only tab, into its own pane at any index: no change at all.
        let b = m.new_workspace(1, term(4)).unwrap();
        let before = m.clone();
        for index in [0, 1, 5] {
            assert_eq!(m.move_tab(b.tab, b.pane, index), Some(Closed::default()));
            assert_eq!(m, before);
        }
        ok(&m);
    }

    #[test]
    fn a_drop_in_a_bar_lands_in_the_gap_and_one_where_the_tab_is_does_nothing() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let a2 = m.add_tab(a.pane, term(2)).unwrap();
        let a3 = m.add_tab(a.pane, term(3)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(4)).unwrap();
        let bar = |gap| TabDrop::Bar { pane: a.pane, gap };
        // The middle tab (index 1): the gaps either side of it are its own place.
        assert_eq!(m.drop_place(a2.tab, bar(1)), None);
        assert_eq!(m.drop_place(a2.tab, bar(2)), None);
        assert_eq!(m.drop_place(a2.tab, bar(0)), Some((a.pane, 0)));
        // Past the tab: one less, as it's taken out first.
        assert_eq!(m.drop_place(a2.tab, bar(3)), Some((a.pane, 2)));
        assert_eq!(m.drop_place(a2.tab, bar(99)), Some((a.pane, 2)));
        assert_eq!(m.drop_place(a.tab, bar(3)), Some((a.pane, 2)));
        assert_eq!(m.drop_place(a3.tab, bar(3)), None);
        // Another pane's bar: the gap as it is (clamped).
        let other = |gap| TabDrop::Bar { pane: b.pane, gap };
        assert_eq!(m.drop_place(a2.tab, other(0)), Some((b.pane, 0)));
        assert_eq!(m.drop_place(a2.tab, other(1)), Some((b.pane, 1)));
        assert_eq!(m.drop_place(a2.tab, other(7)), Some((b.pane, 1)));
        // Unknown tab or pane: nowhere.
        assert_eq!(m.drop_place(TabId(999), bar(0)), None);
        let nowhere = TabDrop::Bar {
            pane: PaneId(999),
            gap: 0,
        };
        assert_eq!(m.drop_place(a.tab, nowhere), None);
        // Dropped where it is, the model is left exactly as it was.
        let before = m.clone();
        assert_eq!(m.drop_place(a3.tab, bar(2)), None);
        assert_eq!(m, before);
    }

    #[test]
    fn a_drop_on_a_workspace_s_row_lands_at_the_end_of_its_focused_pane() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let a2 = m.add_tab(a.pane, term(2)).unwrap();
        let b = m.new_workspace(1, term(3)).unwrap();
        let b2 = m.split(b.pane, Axis::Row, term(4)).unwrap();
        m.add_tab(b2.pane, term(5)).unwrap();
        // b last had its right pane focused: there, after its two tabs.
        m.focus_workspace(a.workspace);
        let row = TabDrop::Workspace(b.workspace);
        assert_eq!(m.drop_place(a.tab, row), Some((b2.pane, 2)));
        m.focus_pane(b.pane);
        m.focus_workspace(a.workspace);
        assert_eq!(m.drop_place(a.tab, row), Some((b.pane, 1)));
        // Its own workspace's row: the end of the focused pane, unless it's already last there.
        let own = TabDrop::Workspace(a.workspace);
        assert_eq!(m.drop_place(a.tab, own), Some((a.pane, 1)));
        assert_eq!(m.drop_place(a2.tab, own), None);
        assert_eq!(
            m.drop_place(a.tab, TabDrop::Workspace(WorkspaceId(999))),
            None
        );
        // Dropped there: shown and focused in b, which takes the view.
        let (pane, index) = m.drop_place(a.tab, row).unwrap();
        assert_eq!(m.move_tab(a.tab, pane, index), Some(Closed::default()));
        assert_eq!(tab_kinds(&m, b.pane), [term(3), term(1)]);
        assert_eq!(m.focused_workspace(), Some(b.workspace));
        assert_eq!(m.focused_tab(), Some(a.tab));
        ok(&m);
    }

    #[test]
    fn select_tab_focuses_its_pane_and_workspace() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let a2 = m.add_tab(a.pane, term(2)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(3)).unwrap();
        let c = m.new_workspace(1, term(4)).unwrap();
        assert_eq!(m.focused_tab(), Some(c.tab));
        assert!(m.select_tab(a.tab));
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        assert_eq!(m.focused_pane(), Some(a.pane));
        assert_eq!(m.focused_tab(), Some(a.tab));
        assert!(m.focus_pane(b.pane));
        assert_eq!(m.focused_tab(), Some(b.tab));
        // A workspace keeps its focused pane and each pane its shown tab while away.
        assert!(m.focus_workspace(c.workspace));
        assert!(m.focus_workspace(a.workspace));
        assert_eq!(m.focused_pane(), Some(b.pane));
        assert_eq!(m.pane(a.pane).unwrap().active, a.tab);
        assert!(!m.select_tab(TabId(999)));
        assert!(!m.focus_pane(PaneId(999)));
        assert!(!m.focus_workspace(WorkspaceId(999)));
        let _ = a2;
        ok(&m);
    }

    #[test]
    fn cycle_tab_wraps_within_the_focused_pane() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        assert_eq!(m.cycle_tab(true), None);
        let b = m.add_tab(a.pane, term(2)).unwrap();
        let c = m.add_tab(a.pane, term(3)).unwrap();
        assert_eq!(m.cycle_tab(true), Some(a.tab)); // from c, wrapping
        assert_eq!(m.cycle_tab(true), Some(b.tab));
        assert_eq!(m.cycle_tab(false), Some(a.tab));
        assert_eq!(m.cycle_tab(false), Some(c.tab));
        assert_eq!(m.focused_tab(), Some(c.tab));
        assert_eq!(Model::new().cycle_tab(true), None);
        ok(&m);
    }

    #[test]
    fn rollup_is_the_most_urgent_terminal() {
        use Status::*;
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.add_tab(a.pane, web("https://x.test")).unwrap();
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        m.add_tab(b.pane, term(3)).unwrap();
        let of = |statuses: [Status; 3]| move |id: u64| statuses[(id - 1) as usize];
        assert_eq!(
            m.rollup(a.workspace, of([Idle, Idle, Idle])),
            Some((1, Idle))
        );
        assert_eq!(
            m.rollup(a.workspace, of([Idle, Working, Idle])),
            Some((2, Working))
        );
        assert_eq!(
            m.rollup(a.workspace, of([Done, Working, Idle])),
            Some((1, Done))
        );
        assert_eq!(
            m.rollup(a.workspace, of([Done, Working, Waiting])),
            Some((3, Waiting))
        );
        // A tie goes to the first in reading order.
        assert_eq!(
            m.rollup(a.workspace, of([Idle, Done, Done])),
            Some((2, Done))
        );
        // The same order as a collapsed group's roll-up.
        for s in [
            [Idle, Working, Done],
            [Waiting, Done, Idle],
            [Working, Idle, Idle],
        ] {
            let (_, got) = m.rollup(a.workspace, of(s)).unwrap();
            assert_eq!(got, groups::rolled_up(s));
        }
        // Browser tabs have no ring.
        let w = m.new_workspace(0, web("https://y.test")).unwrap();
        assert_eq!(m.rollup(w.workspace, of([Waiting; 3])), None);
        assert_eq!(m.rollup(WorkspaceId(999), of([Waiting; 3])), None);
    }

    #[test]
    fn a_workspace_put_in_another_group_or_none_stays_there_when_saved_and_loaded() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap().workspace;
        let b = m.new_workspace(1, term(2)).unwrap().workspace;
        assert_eq!(m.workspace(a).unwrap().placed, None);
        assert!(m.set_placed(a, Some(Placed::Ungrouped)));
        assert!(m.set_placed(b, Some(Placed::Folder("/w/api".into()))));
        assert!(!m.set_placed(WorkspaceId(999), None));
        let (saved, _) = m.to_saved(|id| Some(id as usize - 1));
        assert_eq!(
            saved.iter().map(|w| w.placed.clone()).collect::<Vec<_>>(),
            [
                Some(Placed::Ungrouped),
                Some(Placed::Folder("/w/api".into()))
            ]
        );
        assert_saves_and_loads(&m);
        ok(&m);
    }

    #[test]
    fn a_workspace_shows_in_the_group_it_was_put_in_else_its_own() {
        use groups::GroupKey;
        let own = || Some(GroupKey::Folder("/w/web".into()));
        assert_eq!(shown_group(None, own()), own());
        assert_eq!(shown_group(Some(&Placed::Ungrouped), own()), None);
        assert_eq!(
            shown_group(Some(&Placed::Folder("/w/api".into())), own()),
            Some(GroupKey::Folder("/w/api".into()))
        );
        assert_eq!(
            shown_group(Some(&Placed::Host("box".into())), None),
            Some(GroupKey::Host("box".into()))
        );
        // A drop records only a group that isn't its own.
        let api = GroupKey::Folder("/w/api".into());
        assert_eq!(placed_for(own().as_ref(), own().as_ref()), None);
        assert_eq!(placed_for(None, own().as_ref()), Some(Placed::Ungrouped));
        assert_eq!(
            placed_for(None, None),
            None,
            "no group of its own: nothing to record"
        );
        assert_eq!(
            placed_for(Some(&api), own().as_ref()),
            Some(Placed::Folder("/w/api".into()))
        );
    }

    #[test]
    fn pinned_workspaces_come_first_and_unpinning_puts_one_back() {
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..4)
            .map(|i| m.new_workspace(i, term(i as u64 + 1)).unwrap().workspace)
            .collect();
        assert_eq!(m.display_order(), ids);
        assert!(m.set_pinned(ids[2], true));
        assert!(m.set_pinned(ids[3], true));
        assert_eq!(m.display_order(), [ids[2], ids[3], ids[0], ids[1]]);
        // The sidebar order itself didn't change.
        assert_eq!(m.workspaces().iter().map(|w| w.id).collect::<Vec<_>>(), ids);
        assert!(m.set_pinned(ids[2], false));
        assert_eq!(m.display_order(), [ids[3], ids[0], ids[1], ids[2]]);
        assert!(!m.set_pinned(WorkspaceId(999), true));
        ok(&m);
    }

    #[test]
    fn shortcuts_follow_the_display_order_up_to_nine() {
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..11)
            .map(|i| m.new_workspace(i, term(i as u64 + 1)).unwrap().workspace)
            .collect();
        assert_eq!(m.by_shortcut(1), Some(ids[0]));
        assert_eq!(m.by_shortcut(9), Some(ids[8]));
        // ids[9] and on have none.
        m.set_pinned(ids[10], true);
        assert_eq!(m.by_shortcut(1), Some(ids[10]));
        assert_eq!(m.by_shortcut(2), Some(ids[0]));
        assert_eq!(m.by_shortcut(9), Some(ids[7]));
        assert_eq!(m.by_shortcut(0), None);
        assert_eq!(m.by_shortcut(10), None);
        assert_eq!(Model::new().by_shortcut(1), None);
    }

    #[test]
    fn workspaces_move_and_keep_name_colour_and_pin() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap().workspace;
        let b = m.new_workspace(1, term(2)).unwrap().workspace;
        let c = m.new_workspace(2, term(3)).unwrap().workspace;
        assert!(m.move_workspace(c, 0));
        assert_eq!(m.display_order(), [c, a, b]);
        assert!(!m.move_workspace(c, 3));
        assert!(!m.move_workspace(WorkspaceId(999), 0));
        assert!(m.set_name(a, Some("api".into())));
        assert!(m.set_color(a, Some(TabColor::Teal)));
        let ws = m.workspace(a).unwrap();
        assert_eq!(
            (ws.name.as_deref(), ws.color),
            (Some("api"), Some(TabColor::Teal))
        );
        assert!(m.set_name(a, None));
        assert_eq!(m.workspace(a).unwrap().name, None);
        assert!(!m.set_name(WorkspaceId(999), None));
        assert!(!m.set_color(WorkspaceId(999), None));
        ok(&m);
    }

    #[test]
    fn ratios_are_set_by_path_and_clamped() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.split(a.pane, Axis::Row, term(2)).unwrap();
        assert!(m.set_ratio(a.workspace, &[], 0.01));
        assert!(matches!(
            m.workspace(a.workspace).unwrap().layout,
            Node::Split { ratio, .. } if ratio == crate::split_tree::MIN_RATIO
        ));
        assert!(!m.set_ratio(a.workspace, &[true], 0.5));
        assert!(m.equalize(a.workspace));
        assert!(matches!(
            m.workspace(a.workspace).unwrap().layout,
            Node::Split { ratio, .. } if ratio == 0.5
        ));
        assert!(!m.equalize(WorkspaceId(999)));
        ok(&m);
    }

    #[test]
    fn check_reports_a_broken_model() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.split(a.pane, Axis::Row, term(2)).unwrap();
        ok(&m);
        let mut empty_pane = m.clone();
        empty_pane.workspaces[0].panes[0].tabs.clear();
        assert!(empty_pane.check().is_err());
        let mut lost_pane = m.clone();
        lost_pane.workspaces[0].layout = Node::Leaf(a.pane);
        assert!(lost_pane.check().is_err());
        let mut twice = m.clone();
        twice.workspaces[0].panes[1].tabs[0].kind = term(1);
        assert!(twice.check().is_err());
        let mut bad_active = m.clone();
        bad_active.workspaces[0].panes[0].active = TabId(999);
        assert!(bad_active.check().is_err());
        let mut bad_focus = m.clone();
        bad_focus.focused = Some(WorkspaceId(999));
        assert!(bad_focus.check().is_err());
        let mut bad_pane_focus = m.clone();
        bad_pane_focus.workspaces[0].focused = PaneId(999);
        assert!(bad_pane_focus.check().is_err());
        let mut no_workspace = m.clone();
        no_workspace.workspaces.clear();
        assert!(no_workspace.check().is_err()); // focused names a workspace that's gone
        let mut old_id = m.clone();
        old_id.next_id = 0;
        assert!(old_id.check().is_err());
    }

    /// A tiny deterministic generator (no crates): Knuth's MMIX LCG, high bits.
    struct Lcg(u64);
    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) % n.max(1) as u64) as usize
        }
        fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
            (!items.is_empty()).then(|| items[self.below(items.len())])
        }
    }

    /// Saved through the file shape and back: the same model, ids aside.
    fn assert_saves_and_loads(m: &Model) {
        let live: Vec<u64> = all_terminals(m).into_iter().collect();
        let session = |id: u64| live.iter().position(|&t| t == id);
        let (saved, active) = m.to_saved(session);
        let terminals: Vec<Option<u64>> = live.iter().copied().map(Some).collect();
        let back = Model::from_saved(&saved, active, &terminals);
        ok(&back);
        assert_eq!(back.to_saved(session), (saved, active));
        assert_eq!(all_terminals(&back), all_terminals(m));
    }

    #[test]
    fn random_op_sequences_keep_every_rule() {
        for seed in 0..300u64 {
            let mut rng = Lcg(seed);
            let mut m = Model::new();
            let mut live: BTreeSet<u64> = BTreeSet::new();
            let mut next_terminal = 1u64;
            for step in 0..120 {
                let workspaces: Vec<WorkspaceId> = m.workspaces().iter().map(|w| w.id).collect();
                let panes: Vec<PaneId> = m
                    .workspaces()
                    .iter()
                    .flat_map(|w| w.panes.iter().map(|p| p.id))
                    .collect();
                let tabs: Vec<TabId> = m
                    .workspaces()
                    .iter()
                    .flat_map(|w| &w.panes)
                    .flat_map(|p| p.tabs.iter().map(|t| t.id))
                    .collect();
                // A fresh terminal mostly, sometimes a browser or a file, sometimes one already
                // shown.
                let mut kind = |rng: &mut Lcg| match rng.below(7) {
                    0 => web("https://r.test"),
                    6 => file("/r/a.rs"),
                    1 if !live.is_empty() => term(*live.iter().nth(rng.below(live.len())).unwrap()),
                    _ => {
                        next_terminal += 1;
                        term(next_terminal)
                    }
                };
                let note = |opened: Option<Opened>, kind: &TabKind, live: &mut BTreeSet<u64>| {
                    if let (Some(_), TabKind::Terminal(id)) = (opened, kind) {
                        assert!(live.insert(*id), "a terminal shown twice");
                    }
                };
                let forget = |closed: &Closed, live: &mut BTreeSet<u64>| {
                    for t in &closed.tabs {
                        if let TabKind::Terminal(id) = t.kind {
                            assert!(live.remove(&id));
                        }
                    }
                };
                match rng.below(17) {
                    0 | 1 => {
                        let k = kind(&mut rng);
                        let opened = m.new_workspace(rng.below(workspaces.len() + 2), k.clone());
                        note(opened, &k, &mut live);
                    }
                    2 | 3 => {
                        if let Some(p) = rng.pick(&panes) {
                            let k = kind(&mut rng);
                            let axis = if rng.below(2) == 0 {
                                Axis::Row
                            } else {
                                Axis::Column
                            };
                            note(m.split(p, axis, k.clone()), &k, &mut live);
                        }
                    }
                    4 => {
                        if let Some(p) = rng.pick(&panes) {
                            let k = kind(&mut rng);
                            note(m.add_tab(p, k.clone()), &k, &mut live);
                        }
                    }
                    5 | 6 => {
                        if let Some(t) = rng.pick(&tabs) {
                            let closed = m.close_tab(t).unwrap();
                            assert_eq!(closed.tabs.len(), 1);
                            assert_eq!(closed.tabs[0].id, t);
                            forget(&closed, &mut live);
                        }
                    }
                    7 => {
                        if let Some(w) = rng.pick(&workspaces) {
                            for t in m.tabs_of(w) {
                                let closed = m.close_tab(t).unwrap();
                                forget(&closed, &mut live);
                            }
                            assert!(m.workspace(w).is_none());
                        }
                    }
                    8 => {
                        // A drag: dropped in a bar's gap or on a row, as `drop_place` says.
                        if let (Some(t), Some(p), Some(w)) =
                            (rng.pick(&tabs), rng.pick(&panes), rng.pick(&workspaces))
                        {
                            let drop = if rng.below(3) == 0 {
                                TabDrop::Workspace(w)
                            } else {
                                TabDrop::Bar {
                                    pane: p,
                                    gap: rng.below(5),
                                }
                            };
                            match m.drop_place(t, drop) {
                                Some((pane, index)) => {
                                    let closed = m.move_tab(t, pane, index).unwrap();
                                    assert!(closed.tabs.is_empty());
                                    let tabs = &m.pane(pane).unwrap().tabs;
                                    assert_eq!(tabs.get(index).map(|t| t.id), Some(t));
                                    assert_eq!(m.focused_tab(), Some(t));
                                }
                                None => assert!(m.tab(t).is_some()),
                            }
                        }
                    }
                    9 => {
                        if let (Some(t), Some(p)) = (rng.pick(&tabs), rng.pick(&panes)) {
                            let closed = m.move_tab(t, p, rng.below(4)).unwrap();
                            assert!(closed.tabs.is_empty());
                            assert_eq!(m.locate(t).map(|(_, p)| p), Some(p));
                            assert_eq!(m.focused_tab(), Some(t));
                        }
                    }
                    10 => {
                        if let Some(t) = rng.pick(&tabs) {
                            assert!(m.select_tab(t));
                            assert_eq!(m.focused_tab(), Some(t));
                        }
                    }
                    11 => {
                        if let Some(p) = rng.pick(&panes) {
                            assert!(m.focus_pane(p));
                            assert_eq!(m.focused_pane(), Some(p));
                        }
                    }
                    12 => {
                        if let Some(w) = rng.pick(&workspaces) {
                            assert!(m.focus_workspace(w));
                        }
                        m.cycle_tab(rng.below(2) == 0);
                    }
                    13 => {
                        if let Some(w) = rng.pick(&workspaces) {
                            m.set_pinned(w, rng.below(2) == 0);
                            m.move_workspace(w, rng.below(workspaces.len() + 1));
                        }
                    }
                    14 => {
                        if let Some(t) = rng.pick(&tabs) {
                            let k = kind(&mut rng);
                            let before = m.tab(t).unwrap().kind.clone();
                            if let Some(old) = m.set_kind(t, k.clone()) {
                                assert_eq!(old, before);
                                if let TabKind::Terminal(id) = old {
                                    live.remove(&id);
                                }
                                if let TabKind::Terminal(id) = k {
                                    live.insert(id);
                                }
                            }
                        }
                    }
                    15 => {
                        if let Some(w) = rng.pick(&workspaces) {
                            let path: Vec<bool> =
                                (0..rng.below(3)).map(|_| rng.below(2) == 1).collect();
                            m.set_ratio(w, &path, rng.below(120) as f32 / 100.0);
                        }
                    }
                    _ => {
                        if let Some(w) = rng.pick(&workspaces) {
                            assert!(m.equalize(w));
                        }
                    }
                }
                m.check()
                    .unwrap_or_else(|e| panic!("seed {seed}, step {step}: {e}"));
                assert_eq!(
                    all_terminals(&m),
                    live,
                    "seed {seed}, step {step}: terminals"
                );
                if step % 10 == 9 {
                    assert_saves_and_loads(&m);
                }
            }
        }
    }

    #[test]
    fn the_saved_shape_round_trips_through_json() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(11)).unwrap();
        m.add_tab(a.pane, web("http://localhost:3000")).unwrap();
        let b = m.split(a.pane, Axis::Column, term(12)).unwrap();
        m.set_ratio(a.workspace, &[], 0.3);
        m.set_name(a.workspace, Some("api".into()));
        m.set_color(a.workspace, Some(TabColor::Purple));
        let c = m.new_workspace(1, term(13)).unwrap();
        m.set_pinned(c.workspace, true);
        m.select_tab(a.tab);
        let session = |id: u64| Some((id - 11) as usize);
        let (saved, active) = m.to_saved(session);
        assert_eq!(active, Some(0));
        let json = serde_json::to_string(&saved).unwrap();
        assert!(
            json.contains(r#""kind":"browser","url":"http://localhost:3000""#),
            "{json}"
        );
        assert!(json.contains(r#""kind":"terminal","session":0"#), "{json}");
        assert!(json.contains(r#""pinned":true"#), "{json}");
        let back: Vec<SavedWorkspace> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, saved);
        let m2 = Model::from_saved(&back, active, &[Some(11), Some(12), Some(13)]);
        ok(&m2);
        assert_eq!(m2.to_saved(session), (saved, active));
        assert_eq!(
            m2.focused_tab().and_then(|t| m2.tab(t)).unwrap().kind,
            term(11)
        );
        assert_eq!(m2.workspaces()[0].panes.len(), 2);
        let _ = b;
        // A colour this fuxx doesn't know reads as none; missing fields take their defaults.
        let lenient: SavedWorkspace = serde_json::from_str(
            r#"{"color": "chartreuse", "layout": {"leaf": {"tabs": [{"kind": "browser"}]}}}"#,
        )
        .unwrap();
        assert_eq!(lenient.color, None);
        assert!(!lenient.pinned);
        assert_eq!(lenient.focused, 0);
        assert_eq!(SAVED_VERSION, 2);
    }

    #[test]
    fn from_saved_drops_bad_references_without_losing_a_terminal() {
        let pane = |tabs: Vec<SavedTab>, active| SavedPane { tabs, active };
        let t = |session| SavedTab::Terminal { session };
        let saved = vec![
            SavedWorkspace {
                name: Some("w".into()),
                color: None,
                pinned: false,
                placed: None,
                layout: Node::Split {
                    axis: Axis::Row,
                    ratio: 7.0, // clamped
                    first: Box::new(Node::Leaf(pane(vec![t(0), t(9), t(0)], 2))),
                    second: Box::new(Node::Leaf(pane(vec![t(1)], 0))), // didn't start
                },
                focused: 1,
            },
            // Every tab dropped: the workspace goes.
            SavedWorkspace {
                name: None,
                color: None,
                pinned: true,
                placed: None,
                layout: Node::Leaf(pane(vec![t(0), t(1)], 0)),
                focused: 0,
            },
            SavedWorkspace {
                name: None,
                color: None,
                pinned: false,
                placed: None,
                layout: Node::Leaf(pane(vec![SavedTab::Browser { url: None }], 5)),
                focused: 4,
            },
        ];
        // Sessions 2 and 3 started but no tab shows them; 3 shares 0's terminal (a damaged file).
        let m = Model::from_saved(&saved, Some(1), &[Some(10), None, Some(12), Some(10)]);
        ok(&m);
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(10)]],
                vec![vec![TabKind::Browser { url: None }]],
                vec![vec![term(12)]],
            ]
        );
        assert!(matches!(m.workspaces()[0].layout, Node::Leaf(_)));
        // The focused workspace was dropped: the first in display order gets the focus.
        assert_eq!(m.focused_workspace(), Some(m.workspaces()[0].id));
        assert_eq!(Model::from_saved(&[], Some(3), &[]), Model::new());
        let clamped = Model::from_saved(
            &[SavedWorkspace {
                name: None,
                color: None,
                pinned: false,
                placed: None,
                layout: Node::Split {
                    axis: Axis::Row,
                    ratio: f32::NAN,
                    first: Box::new(Node::Leaf(pane(vec![t(0)], 0))),
                    second: Box::new(Node::Leaf(pane(vec![t(1)], 0))),
                },
                focused: 1,
            }],
            None,
            &[Some(1), Some(2)],
        );
        ok(&clamped);
        assert!(
            matches!(clamped.workspaces()[0].layout, Node::Split { ratio, .. } if ratio == 0.5)
        );
        assert_eq!(
            clamped
                .focused_tab()
                .and_then(|t| clamped.tab(t))
                .unwrap()
                .kind,
            term(2)
        );
    }

    #[test]
    fn from_saved_shows_a_shared_terminal_once_within_a_workspace() {
        // A damaged file: sessions 0 and 1 got the same terminal, in two panes of one workspace.
        let pane = |session| {
            Box::new(Node::Leaf(SavedPane {
                tabs: vec![SavedTab::Terminal { session }],
                active: 0,
            }))
        };
        let saved = [SavedWorkspace {
            name: None,
            color: None,
            pinned: false,
            placed: None,
            layout: Node::Split {
                axis: Axis::Row,
                ratio: 0.5,
                first: pane(0),
                second: pane(1),
            },
            focused: 1,
        }];
        let m = Model::from_saved(&saved, Some(0), &[Some(10), Some(10)]);
        ok(&m);
        assert_eq!(shape(&m), [vec![vec![term(10)]]]);
    }

    #[test]
    fn to_saved_leaves_out_only_the_terminals_without_a_session() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let b = m.add_tab(a.pane, web("https://kept.test")).unwrap();
        m.add_tab(a.pane, term(2)).unwrap();
        m.select_tab(b.tab);
        let only = m.new_workspace(1, term(3)).unwrap();
        let (saved, active) = m.to_saved(|id| (id == 2).then_some(0));
        assert_eq!(
            saved,
            [SavedWorkspace {
                name: None,
                color: None,
                pinned: false,
                placed: None,
                layout: Node::Leaf(SavedPane {
                    tabs: vec![
                        SavedTab::Browser {
                            url: Some("https://kept.test".into())
                        },
                        SavedTab::Terminal { session: 0 },
                    ],
                    active: 0,
                }),
                focused: 0,
            }]
        );
        // The focused workspace had nothing to save.
        assert_eq!(m.focused_workspace(), Some(only.workspace));
        assert_eq!(active, None);
    }

    /// A version-1 file as fuxx M11 wrote it: three lone sessions, an M8 layout of four panes
    /// (with a session named in it), and a two-pane layout; the third session is active.
    const RECORDED_V1: &str = r#"{
  "version": 1,
  "active": 7,
  "collapsed": ["/w/api"],
  "collapsed_hosts": [],
  "sessions": [
    {"name": null, "project": "/w/api", "preset": {"name": "claude", "command": "claude"}},
    {"name": "notes", "project": "/w/api", "preset": null},
    {"name": null, "project": "/w/web", "preset": null, "color": "teal"},
    {"name": null, "project": "/w/web", "preset": null},
    {"name": null, "project": null, "preset": null, "ssh": {"host": "devbox", "tmux": true, "tmux_name": "devbox-0a0b0c"}},
    {"name": null, "project": "/w/web", "preset": null},
    {"name": "logs", "project": "/w/web", "preset": null, "color": "purple"},
    {"name": "tests", "project": "/w/web", "preset": null},
    {"name": null, "project": null, "preset": null}
  ],
  "layouts": [
    {"leaf": 0},
    {"leaf": 1},
    {"split": {"axis": "row", "ratio": 0.5,
      "first": {"split": {"axis": "column", "ratio": 0.5, "first": {"leaf": 2}, "second": {"leaf": 7}}},
      "second": {"split": {"axis": "column", "ratio": 0.4, "first": {"leaf": 6}, "second": {"leaf": 3}}}}},
    {"leaf": 4},
    {"split": {"axis": "row", "ratio": 0.5, "first": {"leaf": 5}, "second": {"leaf": 8}}}
  ],
  "sidebar_width": 260.0,
  "sidebar_hidden": false,
  "hint_dismissed": true,
  "browser_data_kept": false
}"#;

    fn started(n: usize) -> Vec<Option<u64>> {
        (0..n as u64).map(|i| Some(100 + i)).collect()
    }

    #[test]
    fn a_recorded_old_file_becomes_workspaces_of_one_tab_panes() {
        let state = restore::decode(RECORDED_V1).expect("the recorded file loads");
        let m = Model::from_old(&state, &started(9));
        ok(&m);
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(100)]],
                vec![vec![term(101)]],
                vec![
                    vec![term(102)],
                    vec![term(107)],
                    vec![term(106)],
                    vec![term(103)]
                ],
                vec![vec![term(104)]],
                vec![vec![term(105)], vec![term(108)]],
            ]
        );
        // The split kept its axes and ratios.
        let ws = &m.workspaces()[2];
        let Node::Split {
            axis,
            ratio,
            second,
            ..
        } = &ws.layout
        else {
            panic!("a split")
        };
        assert_eq!((*axis, *ratio), (Axis::Row, 0.5));
        assert!(matches!(**second, Node::Split { axis: Axis::Column, ratio, .. } if ratio == 0.4));
        // The row's name and colour: the first its sessions had (row order: 2, 7, 6, 3).
        assert_eq!(ws.name.as_deref(), Some("tests"));
        assert_eq!(ws.color, Some(TabColor::Teal));
        assert_eq!(m.workspaces()[1].name.as_deref(), Some("notes"));
        assert_eq!(m.workspaces()[0].name, None);
        assert!(m.workspaces().iter().all(|w| !w.pinned));
        // The active session (7) is focused, in its workspace's pane.
        assert_eq!(m.focused_workspace(), Some(ws.id));
        assert_eq!(m.focused_tab(), m.terminal_tab(107));
        // Every session became exactly one terminal tab.
        assert_eq!(all_terminals(&m), (100..109).collect());
    }

    #[test]
    fn old_lone_sessions_and_files_without_layouts_load() {
        // Before M8 there were no layouts: every session is a workspace of its own.
        let state = restore::decode(
            r#"{"version": 1, "active": 1, "sessions": [{"name": "a"}, {}, {"color": "blue"}]}"#,
        )
        .unwrap();
        let m = Model::from_old(&state, &started(3));
        ok(&m);
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(100)]],
                vec![vec![term(101)]],
                vec![vec![term(102)]]
            ]
        );
        assert_eq!(m.workspaces()[0].name.as_deref(), Some("a"));
        assert_eq!(m.workspaces()[2].color, Some(TabColor::Blue));
        assert_eq!(m.focused_tab(), m.terminal_tab(101));
        // No sessions: no workspaces.
        let empty = Model::from_old(&restore::decode(r#"{"version": 1}"#).unwrap(), &[]);
        ok(&empty);
        assert!(empty.workspaces().is_empty());
        assert_eq!(empty.focused_workspace(), None);
    }

    #[test]
    fn old_layouts_with_bad_indices_are_dropped_safely() {
        let leaf = |i| Box::new(Node::Leaf(i));
        let state = SavedState {
            active: Some(9), // out of range: the first workspace is focused
            sessions: vec![SavedSession::default(); 4],
            layouts: vec![
                Node::Split {
                    axis: Axis::Row,
                    ratio: 0.5,
                    first: leaf(0),
                    second: leaf(7),
                },
                Node::Split {
                    axis: Axis::Column,
                    ratio: 0.5,
                    first: leaf(1),
                    second: leaf(2),
                },
                Node::Split {
                    axis: Axis::Row,
                    ratio: 0.5,
                    first: leaf(2),
                    second: leaf(3),
                },
                Node::Split {
                    axis: Axis::Row,
                    ratio: 0.5,
                    first: leaf(3),
                    second: leaf(3),
                },
            ],
            ..SavedState::default()
        };
        let m = Model::from_old(&state, &started(4));
        ok(&m);
        // Only (1 over 2) is good; 0 and 3 stand alone, as version 1 did. The workspaces are in
        // the old sidebar's order: by the first row each one held.
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(100)]],
                vec![vec![term(101)], vec![term(102)]],
                vec![vec![term(103)]],
            ]
        );
        assert_eq!(m.focused_workspace(), Some(m.workspaces()[0].id));
    }

    #[test]
    fn old_sessions_that_did_not_start_leave_their_neighbours_the_space() {
        let state = restore::decode(RECORDED_V1).unwrap();
        let mut terminals = started(9);
        terminals[7] = None; // the active one
        terminals[4] = None; // a lone one
        let m = Model::from_old(&state, &terminals);
        ok(&m);
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(100)]],
                vec![vec![term(101)]],
                vec![vec![term(102)], vec![term(106)], vec![term(103)]],
                vec![vec![term(105)], vec![term(108)]],
            ]
        );
        // The active session's workspace is still focused, on its first pane.
        assert_eq!(m.focused_workspace(), Some(m.workspaces()[2].id));
        // Fewer terminals than sessions: the rest didn't start.
        let short = Model::from_old(&state, &started(2));
        ok(&short);
        assert_eq!(
            shape(&short),
            [vec![vec![term(100)]], vec![vec![term(101)]]]
        );
    }

    #[test]
    fn old_layouts_come_back_in_the_old_sidebar_order() {
        // Layouts were kept in the order they were made; the sidebar listed sessions in order.
        let state = restore::decode(
            r#"{"version": 1, "active": 0, "sessions": [{}, {}, {}],
                "layouts": [{"leaf": 2}, {"split": {"axis": "row", "ratio": 0.5,
                  "first": {"leaf": 1}, "second": {"leaf": 0}}}]}"#,
        )
        .unwrap();
        let m = Model::from_old(&state, &started(3));
        ok(&m);
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(101)], vec![term(100)]],
                vec![vec![term(102)]]
            ]
        );
    }

    #[test]
    fn closing_a_panes_last_tab_closes_it_and_then_its_workspace() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let web_a = m.add_tab(a.pane, web("https://a.test")).unwrap();
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let three = m.add_tab(b.pane, term(3)).unwrap();
        let other = m.new_workspace(1, term(4)).unwrap();
        m.focus_pane(b.pane);
        let first = m.close_tab(three.tab).unwrap();
        assert_eq!((first.panes, first.workspaces), (vec![], vec![]));
        let closed = m.close_tab(b.tab).unwrap();
        ok(&m);
        assert_eq!((closed.panes, closed.workspaces), (vec![b.pane], vec![]));
        // Its neighbour takes its space and the focus.
        assert_eq!(m.workspace(a.workspace).unwrap().layout, Node::Leaf(a.pane));
        assert_eq!(m.focused_pane(), Some(a.pane));
        // A workspace's last pane: the workspace goes too.
        m.close_tab(web_a.tab).unwrap();
        let last = m.close_tab(a.tab).unwrap();
        ok(&m);
        assert_eq!(last.workspaces, [a.workspace]);
        assert_eq!(m.focused_workspace(), Some(other.workspace));
        assert_eq!(m.close_tab(a.tab), None);
    }

    #[test]
    fn a_drag_moves_within_its_pinned_or_unpinned_run_in_display_order() {
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..5)
            .map(|i| m.new_workspace(i as usize, term(i)).unwrap().workspace)
            .collect();
        m.set_pinned(ids[1], true);
        m.set_pinned(ids[3], true);
        // Display order: 1 3 | 0 2 4.
        assert_eq!(m.display_order(), [ids[1], ids[3], ids[0], ids[2], ids[4]]);
        // 4 to the top of the unpinned run (display place 2).
        assert!(m.move_workspace_display(ids[4], 2));
        assert_eq!(m.display_order(), [ids[1], ids[3], ids[4], ids[0], ids[2]]);
        // 1 below 3 in the pinned run.
        assert!(m.move_workspace_display(ids[1], 1));
        assert_eq!(m.display_order(), [ids[3], ids[1], ids[4], ids[0], ids[2]]);
        // Across the runs, or out of range: refused, nothing moves.
        assert!(!m.move_workspace_display(ids[0], 0));
        assert!(!m.move_workspace_display(ids[3], 4));
        assert!(!m.move_workspace_display(ids[0], 9));
        assert_eq!(m.display_order(), [ids[3], ids[1], ids[4], ids[0], ids[2]]);
        // Unpinning still puts one back where it was in the sidebar order.
        m.set_pinned(ids[1], false);
        ok(&m);
    }

    #[test]
    fn unpinning_puts_a_workspace_back_with_its_group_and_keeps_the_groups_order() {
        let mut m = Model::new();
        // A (pinned, G), B (H), D (G): the groups show H then G.
        let ids: Vec<WorkspaceId> = (0..3)
            .map(|i| m.new_workspace(i as usize, term(i)).unwrap().workspace)
            .collect();
        let (a, b, d) = (ids[0], ids[1], ids[2]);
        m.set_pinned(a, true);
        let group = |w: &Workspace| ["G", "H", "G"].get(m_terminal(w) as usize).copied();
        m.group_together(group);
        assert!(m.unpin(a, group));
        // After D, the last of G: H still before G once grouped.
        m.group_together(group);
        let order: Vec<WorkspaceId> = m.workspaces().iter().map(|w| w.id).collect();
        assert_eq!(order, [b, d, a]);
        assert!(!m.workspace(a).unwrap().pinned);
        // A group of its own: it stays where it is.
        m.set_pinned(b, true);
        assert!(m.unpin(b, group));
        assert_eq!(m.workspaces()[0].id, b);
        assert!(!m.unpin(WorkspaceId(999), group));
        ok(&m);
    }

    #[test]
    fn grouping_leaves_pinned_workspaces_where_they_are() {
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..5)
            .map(|i| m.new_workspace(i as usize, term(i)).unwrap().workspace)
            .collect();
        // Groups by terminal: 0 api, 1 web, 2 api, 3 web, 4 api; 1 and 2 pinned.
        m.set_pinned(ids[1], true);
        m.set_pinned(ids[2], true);
        let group = |w: &Workspace| ["api", "web"][m_terminal(w) as usize % 2];
        // Only the unpinned ones are grouped, in the places they hold: 0 4 api, then 3 web.
        assert!(m.group_together(|w| Some(group(w))));
        let order: Vec<WorkspaceId> = m.workspaces().iter().map(|w| w.id).collect();
        assert_eq!(order, [ids[0], ids[1], ids[2], ids[4], ids[3]]);
        // The pinned ones keep their own order, whatever their groups (a drag among them sticks).
        assert!(m.move_workspace_display(ids[2], 0));
        assert!(!m.group_together(|w| Some(group(w))));
        assert_eq!(m.display_order()[..2], [ids[2], ids[1]]);
        ok(&m);
    }

    #[test]
    fn grouping_keeps_each_group_together_and_ungrouped_ones_first() {
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..6)
            .map(|i| m.new_workspace(i as usize, term(i)).unwrap().workspace)
            .collect();
        // Groups by terminal: 0 api, 1 none, 2 web, 3 api, 4 none, 5 web.
        let group = |w: &Workspace| match m_terminal(w) % 3 {
            0 => Some("api"),
            1 => None,
            _ => Some("web"),
        };
        let mut moved = m.clone();
        assert!(moved.group_together(group));
        ok(&moved);
        let order: Vec<WorkspaceId> = moved.workspaces().iter().map(|w| w.id).collect();
        assert_eq!(order, [ids[1], ids[4], ids[0], ids[3], ids[2], ids[5]]);
        // Already together: nothing moves.
        assert!(!moved.group_together(group));
        let focused = m.focused_workspace();
        m.group_together(group);
        assert_eq!(m.focused_workspace(), focused);
    }

    /// A one-terminal workspace's terminal.
    fn m_terminal(w: &Workspace) -> u64 {
        match w.panes[0].tabs[0].kind {
            TabKind::Terminal(id) => id,
            TabKind::Browser { .. } | TabKind::File { .. } => u64::MAX,
        }
    }

    #[test]
    fn a_file_loads_by_its_workspaces_or_else_its_old_layouts() {
        let old = restore::decode(RECORDED_V1).unwrap();
        assert_eq!(
            Model::from_state(&old, &started(9)),
            Model::from_old(&old, &started(9))
        );
        let mut m = Model::new();
        let a = m.new_workspace(0, term(100)).unwrap();
        m.split(a.pane, Axis::Column, term(101)).unwrap();
        m.new_workspace(1, term(102)).unwrap();
        let (workspaces, active) = m.to_saved(|id| Some((id - 100) as usize));
        let state = SavedState {
            sessions: vec![SavedSession::default(); 3],
            workspaces,
            active_workspace: active,
            ..SavedState::default()
        };
        let back = Model::from_state(
            &restore::decode(&restore::encode(&state)).unwrap(),
            &started(3),
        );
        ok(&back);
        assert_eq!(shape(&back), shape(&m));
        assert_eq!(
            back.focused_workspace(),
            Some(back.workspaces()[1].id),
            "the focused workspace comes back"
        );
    }

    #[test]
    fn renumbering_saved_sessions_drops_the_ones_left_out() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(0)).unwrap();
        let b = m.split(a.pane, Axis::Row, term(1)).unwrap();
        m.add_tab(b.pane, term(2)).unwrap();
        m.new_workspace(1, term(3)).unwrap();
        m.focus_pane(b.pane);
        let (saved, active) = m.to_saved(|id| Some(id as usize));
        // Sessions 1 and 3 are left out; 0 and 2 are now 0 and 1.
        let map = |i: usize| match i {
            0 => Some(0),
            2 => Some(1),
            _ => None,
        };
        let (kept, kept_active) = renumber(&saved, active, map);
        let back = Model::from_saved(&kept, kept_active, &[Some(10), Some(12)]);
        ok(&back);
        assert_eq!(shape(&back), [vec![vec![term(10)], vec![term(12)]]]);
        // The focus stays on the pane that held 1, now showing 2.
        assert_eq!(back.focused_tab(), back.terminal_tab(12));
        // The focused workspace's own sessions all left: no focused one.
        let (_, none) = renumber(&saved, Some(1), map);
        assert_eq!(none, None);
    }

    fn file(path: &str) -> TabKind {
        TabKind::File {
            path: PathBuf::from(path),
            mode: FileMode::Plain,
        }
    }

    #[test]
    fn a_file_tab_round_trips_through_the_saved_shape() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(11)).unwrap();
        let f = m.add_tab(a.pane, file("/w/api/src/main.rs")).unwrap();
        m.split(a.pane, Axis::Row, file("/w/api/Cargo.toml"))
            .unwrap();
        m.select_tab(f.tab);
        let session = |id: u64| Some((id - 11) as usize);
        let (saved, active) = m.to_saved(session);
        assert_eq!(
            nth_leaf(&saved[0].layout, 0).unwrap().tabs,
            [
                SavedTab::Terminal { session: 0 },
                SavedTab::File {
                    path: PathBuf::from("/w/api/src/main.rs"),
                    mode: FileMode::Plain,
                    preview: false,
                },
            ]
        );
        let json = serde_json::to_string(&saved).unwrap();
        assert!(
            json.contains(r#""kind":"file","path":"/w/api/src/main.rs","mode":"plain""#),
            "{json}"
        );
        let back: Vec<SavedWorkspace> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, saved);
        let m2 = Model::from_saved(&back, active, &[Some(11)]);
        ok(&m2);
        assert_eq!(shape(&m2), shape(&m));
        assert_eq!(
            m2.focused_tab().and_then(|t| m2.tab(t)).unwrap().kind,
            file("/w/api/src/main.rs"),
            "the file tab keeps the focus"
        );
        assert_eq!(m2.to_saved(session), (saved, active));
    }

    #[test]
    fn a_saved_file_tab_without_a_mode_loads_as_plain() {
        let tab: SavedTab = serde_json::from_str(r#"{"kind":"file","path":"/x"}"#).unwrap();
        assert_eq!(
            tab,
            SavedTab::File {
                path: PathBuf::from("/x"),
                mode: FileMode::Plain,
                preview: false,
            }
        );
        let diff: SavedTab =
            serde_json::from_str(r#"{"kind":"file","path":"/x","mode":"diff"}"#).unwrap();
        assert_eq!(
            diff,
            SavedTab::File {
                path: PathBuf::from("/x"),
                mode: FileMode::Diff,
                preview: false,
            }
        );
        assert_eq!(FileMode::default(), FileMode::Plain);
    }

    /// M11ac: a file tab's Markdown Preview is saved with it; a file from before has it off,
    /// and a tab with it off is written as before (no `preview` key).
    #[test]
    fn a_saved_file_tab_keeps_its_preview_and_an_old_one_has_it_off() {
        let old: SavedTab = serde_json::from_str(r#"{"kind":"file","path":"/x.md"}"#).unwrap();
        assert_eq!(
            old,
            SavedTab::File {
                path: PathBuf::from("/x.md"),
                mode: FileMode::Plain,
                preview: false,
            }
        );
        let off = serde_json::to_string(&old).unwrap();
        assert!(!off.contains("preview"), "{off}");
        let on = SavedTab::File {
            path: PathBuf::from("/x.md"),
            mode: FileMode::Plain,
            preview: true,
        };
        let json = serde_json::to_string(&on).unwrap();
        assert!(json.contains(r#""preview":true"#), "{json}");
        assert_eq!(serde_json::from_str::<SavedTab>(&json).unwrap(), on);
    }

    #[test]
    fn a_file_tab_s_preview_round_trips_and_follows_a_move() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(11)).unwrap();
        let f = m.add_tab(a.pane, file("/w/README.md")).unwrap();
        let g = m.add_tab(a.pane, file("/w/notes.md")).unwrap();
        assert!(!m.md_preview(f.tab));
        assert!(m.set_md_preview(f.tab, true));
        assert!(!m.set_md_preview(f.tab, true), "already on");
        assert!(!m.set_md_preview(TabId(999), true), "unknown");
        assert!(!m.set_md_preview(a.tab, true), "not a file tab");
        // Moved to its own pane: still on.
        m.split(a.pane, Axis::Row, file("/w/other.md")).unwrap();
        let right = m.focused_pane().unwrap();
        m.move_tab(f.tab, right, 0).unwrap();
        assert!(m.md_preview(f.tab));
        let session = |id: u64| Some((id - 11) as usize);
        let (saved, active) = m.to_saved(session);
        let back = Model::from_saved(&saved, active, &[Some(11)]);
        let previews: Vec<PathBuf> = back
            .panes()
            .flat_map(|p| &p.tabs)
            .filter(|t| back.md_preview(t.id))
            .filter_map(|t| match &t.kind {
                TabKind::File { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(previews, [PathBuf::from("/w/README.md")]);
        assert_eq!(back.to_saved(session), (saved, active));
        // Off again, and closed: forgotten.
        assert!(m.set_md_preview(f.tab, false));
        assert!(!m.md_preview(f.tab));
        m.set_md_preview(g.tab, true);
        m.close_tab(g.tab);
        assert!(!m.md_preview(g.tab));
    }

    /// A single click's preview tab (M11t) showing another file shows it as source.
    #[test]
    fn a_preview_tab_showing_another_file_turns_preview_off() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let first = m
            .preview_file(a.pane, PathBuf::from("/w/a.md"), FileMode::Plain)
            .unwrap();
        m.set_md_preview(first.tab, true);
        let again = m
            .preview_file(a.pane, PathBuf::from("/w/a.md"), FileMode::Plain)
            .unwrap();
        assert!(m.md_preview(again.tab), "the same file keeps it");
        let next = m
            .preview_file(a.pane, PathBuf::from("/w/b.md"), FileMode::Plain)
            .unwrap();
        assert_eq!(next.tab, first.tab);
        assert!(!m.md_preview(next.tab));
    }

    /// A version-2 file as fuxx wrote it before file tabs: a pane of a terminal and a page, and
    /// a pane of one terminal.
    const RECORDED_V2: &str = r#"{
  "version": 2,
  "sessions": [
    {"name": null, "project": "/w/api", "preset": null},
    {"name": "logs", "project": "/w/api", "preset": null}
  ],
  "workspaces": [
    {"name": "api", "layout": {"split": {"axis": "row", "ratio": 0.5,
      "first": {"leaf": {"tabs": [{"kind": "terminal", "session": 0},
                                  {"kind": "browser", "url": "http://localhost:3000"}],
                         "active": 1}},
      "second": {"leaf": {"tabs": [{"kind": "terminal", "session": 1}]}}}},
     "focused": 1}
  ],
  "active_workspace": 0
}"#;

    #[test]
    fn files_from_before_file_tabs_still_load() {
        // Version 1 (sessions and layouts).
        let old = restore::decode(RECORDED_V1).expect("the v1 file loads");
        let m = Model::from_state(&old, &started(9));
        ok(&m);
        assert_eq!(m.workspaces().len(), 5);
        // Version 2 with terminal and browser tabs only.
        let v2 = restore::decode(RECORDED_V2).expect("the v2 file loads");
        let m = Model::from_state(&v2, &started(2));
        ok(&m);
        assert_eq!(
            shape(&m),
            [vec![
                vec![term(100), web("http://localhost:3000")],
                vec![term(101)]
            ]]
        );
        assert_eq!(m.focused_tab(), m.terminal_tab(101));
    }

    #[test]
    fn closing_a_file_tab_leaves_the_terminals_alone() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let f = m.add_tab(a.pane, file("/w/notes.md")).unwrap();
        m.add_tab(a.pane, term(2)).unwrap();
        let closed = m.close_tab(f.tab).unwrap();
        ok(&m);
        assert_eq!(closed.tabs.len(), 1);
        assert_eq!(closed.tabs[0].kind, file("/w/notes.md"));
        assert!(closed.panes.is_empty() && closed.workspaces.is_empty());
        assert_eq!(tab_kinds(&m, a.pane), [term(1), term(2)]);
        assert_eq!(m.terminals(a.workspace), [1, 2]);
        // A file tab alone in a pane: its pane closes with it, the terminal's stays.
        let lone = m.split(a.pane, Axis::Row, file("/w/a.rs")).unwrap();
        let closed = m.close_tab(lone.tab).unwrap();
        ok(&m);
        assert_eq!(closed.panes, [lone.pane]);
        assert_eq!(shape(&m), [vec![vec![term(1), term(2)]]]);
        assert_eq!(m.focused_pane(), Some(a.pane));
    }

    #[test]
    fn a_file_tab_holds_no_terminal() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        m.add_tab(a.pane, file("/w/a.rs")).unwrap();
        // The same file may be open in two panes (it isn't a terminal, which shows once).
        let b = m.split(a.pane, Axis::Row, file("/w/a.rs")).unwrap();
        ok(&m);
        assert_eq!(m.terminals(a.workspace), [1]);
        assert_eq!(m.tabs_of(a.workspace).len(), 3);
        // A workspace of a file only.
        let only = m.new_workspace(1, file("/w/b.rs")).unwrap();
        ok(&m);
        assert!(m.terminals(only.workspace).is_empty());
        let _ = b;
    }

    /// A single click previews a file (M11t, 2026-11-11, your call): the pane's one preview tab
    /// shows it, replaced by the next preview; opening it (a double click) keeps it.
    #[test]
    fn a_preview_tab_is_reused_until_a_file_is_opened_for_good() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let p = |path: &str| PathBuf::from(path);
        let first = m
            .preview_file(a.pane, p("/w/a.rs"), FileMode::Plain)
            .unwrap();
        ok(&m);
        assert!(m.is_preview(first.tab));
        assert_eq!(m.focused_tab(), Some(first.tab));
        // Another preview: the same tab, now showing b.
        let second = m
            .preview_file(a.pane, p("/w/b.rs"), FileMode::Plain)
            .unwrap();
        assert_eq!(second.tab, first.tab);
        assert_eq!(tab_kinds(&m, a.pane), [term(1), file("/w/b.rs")]);
        // Opened for good (the double click): the same tab, no longer a preview.
        let kept = m.open_file(a.pane, p("/w/b.rs"), FileMode::Plain).unwrap();
        assert_eq!(kept.tab, first.tab);
        assert!(!m.is_preview(kept.tab));
        // The next preview is a new tab; a file already open is only selected.
        let third = m
            .preview_file(a.pane, p("/w/c.rs"), FileMode::Plain)
            .unwrap();
        assert_ne!(third.tab, kept.tab);
        assert!(m.is_preview(third.tab));
        let back = m
            .preview_file(a.pane, p("/w/b.rs"), FileMode::Plain)
            .unwrap();
        assert_eq!(back.tab, kept.tab);
        assert!(!m.is_preview(kept.tab), "still kept");
        assert_eq!(
            tab_kinds(&m, a.pane),
            [term(1), file("/w/b.rs"), file("/w/c.rs")]
        );
        // Pinned by hand; a closed preview is forgotten.
        assert!(m.pin(third.tab));
        assert!(!m.is_preview(third.tab));
        let fourth = m
            .preview_file(a.pane, p("/w/d.rs"), FileMode::Diff)
            .unwrap();
        m.close_tab(fourth.tab);
        assert!(!m.is_preview(fourth.tab));
        // Each pane has its own; a preview moved to another pane is kept for good there.
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let here = m
            .preview_file(a.pane, p("/w/e.rs"), FileMode::Plain)
            .unwrap();
        let there = m
            .preview_file(b.pane, p("/w/f.rs"), FileMode::Plain)
            .unwrap();
        assert_ne!(here.tab, there.tab);
        assert!(m.is_preview(here.tab) && m.is_preview(there.tab));
        m.move_tab(here.tab, b.pane, 0);
        assert!(!m.is_preview(here.tab));
        ok(&m);
        // An unknown pane previews nothing.
        assert_eq!(
            m.preview_file(PaneId(999), p("/w/x"), FileMode::Plain),
            None
        );
    }

    #[test]
    fn opening_a_file_twice_in_one_pane_gives_one_tab_selected() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let first = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Plain)
            .unwrap();
        assert_eq!(first.pane, a.pane);
        m.select_tab(a.tab);
        let again = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Plain)
            .unwrap();
        ok(&m);
        assert_eq!(again, first, "the tab it had");
        assert_eq!(tab_kinds(&m, a.pane), [term(1), file("/w/a.rs")]);
        assert_eq!(m.focused_tab(), Some(first.tab));
        // Another file is a new tab; the same file in another pane is that pane's own.
        let other = m
            .open_file(a.pane, PathBuf::from("/w/b.rs"), FileMode::Plain)
            .unwrap();
        assert_ne!(other.tab, first.tab);
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let there = m
            .open_file(b.pane, PathBuf::from("/w/a.rs"), FileMode::Plain)
            .unwrap();
        ok(&m);
        assert_eq!(there.pane, b.pane);
        assert_ne!(there.tab, first.tab);
        assert_eq!(m.focused_tab(), Some(there.tab));
        // An unknown pane opens nothing.
        assert_eq!(
            m.open_file(PaneId(999), PathBuf::from("/w/a.rs"), FileMode::Plain),
            None
        );
    }

    fn diff_file(path: &str) -> TabKind {
        TabKind::File {
            path: PathBuf::from(path),
            mode: FileMode::Diff,
        }
    }

    #[test]
    fn opening_a_file_sets_its_mode_and_keeps_one_tab() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let plain = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Plain)
            .unwrap();
        // The same file in Diff: the same tab, now in Diff.
        let diff = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Diff)
            .unwrap();
        ok(&m);
        assert_eq!(diff, plain, "the tab it had");
        assert_eq!(tab_kinds(&m, a.pane), [term(1), diff_file("/w/a.rs")]);
        assert_eq!(m.focused_tab(), Some(plain.tab));
        // Plain again: the same tab, Plain.
        m.select_tab(a.tab);
        let back = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Plain)
            .unwrap();
        assert_eq!(back, plain);
        assert_eq!(tab_kinds(&m, a.pane), [term(1), file("/w/a.rs")]);
        assert_eq!(m.focused_tab(), Some(plain.tab));
        // Another pane gets its own tab, in its own mode.
        let b = m.split(a.pane, Axis::Row, term(2)).unwrap();
        let there = m
            .open_file(b.pane, PathBuf::from("/w/a.rs"), FileMode::Diff)
            .unwrap();
        ok(&m);
        assert_ne!(there.tab, plain.tab);
        assert_eq!(tab_kinds(&m, b.pane), [term(2), diff_file("/w/a.rs")]);
        assert_eq!(tab_kinds(&m, a.pane), [term(1), file("/w/a.rs")]);
    }

    #[test]
    fn set_file_mode_is_false_for_a_terminal_or_the_same_mode() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        let f = m.add_tab(a.pane, file("/w/a.rs")).unwrap();
        assert!(!m.set_file_mode(a.tab, FileMode::Diff), "a terminal");
        assert!(!m.set_file_mode(TabId(999), FileMode::Diff), "unknown");
        assert!(!m.set_file_mode(f.tab, FileMode::Plain), "the same mode");
        assert!(m.set_file_mode(f.tab, FileMode::Diff));
        assert_eq!(m.tab(f.tab).unwrap().kind, diff_file("/w/a.rs"));
        assert!(!m.set_file_mode(f.tab, FileMode::Diff), "the same mode");
        assert!(m.set_file_mode(f.tab, FileMode::Plain));
        assert_eq!(m.tab(f.tab).unwrap().kind, file("/w/a.rs"));
        ok(&m);
    }

    #[test]
    fn a_diff_tab_round_trips_with_its_mode() {
        let mut m = Model::new();
        let a = m.new_workspace(0, term(11)).unwrap();
        let f = m
            .open_file(a.pane, PathBuf::from("/w/a.rs"), FileMode::Diff)
            .unwrap();
        let session = |id: u64| Some((id - 11) as usize);
        let (saved, active) = m.to_saved(session);
        let json = serde_json::to_string(&saved).unwrap();
        assert!(json.contains(r#""mode":"diff""#), "{json}");
        let back: Vec<SavedWorkspace> = serde_json::from_str(&json).unwrap();
        let m2 = Model::from_saved(&back, active, &[Some(11)]);
        ok(&m2);
        assert_eq!(
            m2.tab(m2.focused_tab().unwrap()).unwrap().kind,
            diff_file("/w/a.rs")
        );
        let _ = f;
    }
}
