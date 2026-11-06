//! Workspaces → panes → tabs (M11b): the sidebar lists workspaces, a workspace is a split layout
//! of panes, a pane holds an ordered row of terminal and browser tabs. Every rule lives here
//! (new, split, add tab, close-last cascades, move, select, roll-up, pinned order, ⌘1–⌘9, the
//! saved shape and the loader for older files). Pure and GPUI-free, so the rules are unit-tested.

use std::collections::HashSet;

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

/// What a tab shows: a terminal (by its scripting id, `sN`) or a web page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabKind {
    Terminal(u64),
    Browser { url: Option<String> },
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

/// One sidebar row: a split layout of panes (never empty) and the pane with the keyboard.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
    /// Your name for it; none: the caller names it after its first terminal.
    pub name: Option<String>,
    pub color: Option<TabColor>,
    pub pinned: bool,
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

/// The workspaces in sidebar order, which one is focused, and the next id to hand out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    workspaces: Vec<Workspace>,
    focused: Option<WorkspaceId>,
    next_id: u64,
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
/// keeps its page.
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

    pub fn workspace_of(&self, pane: PaneId) -> Option<WorkspaceId> {
        self.workspaces
            .iter()
            .find(|w| w.panes.iter().any(|p| p.id == pane))
            .map(|w| w.id)
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
                TabKind::Browser { .. } => None,
            })
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

    /// Its ⌘N digit: 1–9 for the first nine in display order.
    pub fn shortcut(&self, workspace: WorkspaceId) -> Option<u8> {
        let at = self.display_order().iter().position(|&w| w == workspace)?;
        (at < 9).then(|| at as u8 + 1)
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

    /// Show something else in `tab` (a reconnected terminal, a browser's new page); returns
    /// what it showed. None when `tab` is unknown or another tab already shows the terminal.
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
        closed.tabs.insert(0, taken);
        Some(closed)
    }

    /// Close a workspace and every tab in it; the focus moves as for `close_tab`.
    pub fn close_workspace(&mut self, workspace: WorkspaceId) -> Option<Closed> {
        let w = self.workspaces.iter().position(|w| w.id == workspace)?;
        let ws = self.remove_workspace(w);
        let order = ws.layout.leaves();
        let mut panes = ws.panes;
        panes.sort_by_key(|p| order.iter().position(|&id| id == p.id));
        Some(Closed {
            panes: panes.iter().map(|p| p.id).collect(),
            tabs: panes.into_iter().flat_map(|p| p.tabs).collect(),
            workspaces: vec![workspace],
        })
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

    pub fn set_pinned(&mut self, workspace: WorkspaceId, pinned: bool) -> bool {
        self.workspace_mut(workspace)
            .map(|w| w.pinned = pinned)
            .is_some()
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
        let (trees, _) = split_tree::sanitize(state.layouts.clone(), state.sessions.len());
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
            };
            let id = TabId(self.next());
            if i == saved.active {
                active = Some(id);
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

    /// Whether `kind` is a terminal some tab other than `except` already shows.
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
                TabKind::Browser { .. } => None,
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
        assert_eq!(m.workspace_of(a.pane), Some(a.workspace));
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
        m.close_workspace(c.workspace).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        // A workspace that isn't focused closes: the focus stays.
        m.close_workspace(b.workspace).unwrap();
        assert_eq!(m.focused_workspace(), Some(a.workspace));
        ok(&m);
    }

    #[test]
    fn close_workspace_ends_every_tab_in_it() {
        let mut m = Model::new();
        let keep = m.new_workspace(0, term(9)).unwrap();
        let a = m.new_workspace(1, term(1)).unwrap();
        let b = m.add_tab(a.pane, web("https://x.test")).unwrap();
        let c = m.split(a.pane, Axis::Row, term(3)).unwrap();
        let closed = m.close_workspace(a.workspace).unwrap();
        let tabs: BTreeSet<TabId> = closed.tabs.iter().map(|t| t.id).collect();
        assert_eq!(tabs, BTreeSet::from([a.tab, b.tab, c.tab]));
        assert_eq!(
            closed.panes.iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([&a.pane, &c.pane])
        );
        assert_eq!(closed.workspaces, [a.workspace]);
        assert_eq!(all_terminals(&m), BTreeSet::from([9]));
        assert_eq!(m.focused_workspace(), Some(keep.workspace));
        assert_eq!(m.close_workspace(a.workspace), None);
        ok(&m);
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
        assert_eq!(m.shortcut(ids[0]), Some(1));
        assert_eq!(m.shortcut(ids[8]), Some(9));
        assert_eq!(m.shortcut(ids[9]), None);
        m.set_pinned(ids[10], true);
        assert_eq!(m.shortcut(ids[10]), Some(1));
        assert_eq!(m.shortcut(ids[0]), Some(2));
        assert_eq!(m.shortcut(ids[8]), None);
        assert_eq!(m.by_shortcut(1), Some(ids[10]));
        assert_eq!(m.by_shortcut(9), Some(ids[7]));
        assert_eq!(m.by_shortcut(0), None);
        assert_eq!(m.by_shortcut(10), None);
        assert_eq!(Model::new().by_shortcut(1), None);
        assert_eq!(m.shortcut(WorkspaceId(999)), None);
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
                // A fresh terminal mostly, sometimes a browser, sometimes one already shown.
                let mut kind = |rng: &mut Lcg| match rng.below(6) {
                    0 => web("https://r.test"),
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
                            let closed = m.close_workspace(w).unwrap();
                            assert_eq!(closed.workspaces, [w]);
                            forget(&closed, &mut live);
                        }
                    }
                    8 | 9 => {
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
                layout: Node::Leaf(pane(vec![t(0), t(1)], 0)),
                focused: 0,
            },
            SavedWorkspace {
                name: None,
                color: None,
                pinned: false,
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
        // Only (1 over 2) is good; 0 and 3 stand alone, as version 1 did.
        assert_eq!(
            shape(&m),
            [
                vec![vec![term(101)], vec![term(102)]],
                vec![vec![term(100)]],
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
}
