//! M11p: the window chrome as cached parts, and render counters that prove what redraws.
//!
//! **Parts.** The sidebar, the status bar, each pane's tab bar and the title area are each a
//! small view (`ChromePart`) that draws the existing `render_*` code through the `Workspace`.
//! The `Workspace` embeds each with GPUI's `Entity::cached(style)` (pinned rev,
//! `crates/gpui/src/view.rs`), at a definite size: the part's last frame is reused (no render,
//! layout or paint) until the part is notified, its bounds change, or the window refreshes
//! (resize, activation, drag start and end, `refresh_windows`). A breath of the focused
//! terminal redraws the `Workspace` (its ancestor) but not the parts.
//!
//! The rule that keeps every part current: each part observes the `Workspace`, so every
//! `cx.notify()` on the `Workspace` redraws every part. What a part draws that moves on its own
//! (a hover, a `with_animation`, `request_animation_frame`) notifies the view being drawn, which
//! is the part. What the `Workspace` changes while drawing (it shouldn't, M11p Task 2) is set
//! before the parts are drawn in that frame, but only a part drawn again shows it; the two
//! that remain: the drag reset (on a frame GPUI redraws whole, see `Workspace::render`) and
//! the sidebar slide's end (on a frame the workspace asked for, which redraws every part).
//!
//! `FUXX_CHROME_CACHE=off` (read once) draws every part inline, uncached, as before M11p.
//!
//! **Counters.** `bump` is an empty function without the `ui-shots` feature: call sites carry no
//! `cfg` and a release build pays nothing. With the feature each `Counted` part has an atomic
//! counter the `ui-shots` scenes read (`get`) and clear (`reset`).

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;

use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, Render, Subscription, WeakEntity, Window,
    div,
};

use crate::appearance::Appearance;
use crate::layout_model::PaneId;
use crate::workspace::Workspace;

/// Which part of the chrome a `ChromePart` draws.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ChromeKind {
    /// `render_sidebar`.
    Sidebar,
    /// `render_status_bar`.
    StatusBar,
    /// `pane`'s `render_tab_bar`, its bar where `place` says.
    TabBar { pane: PaneId, place: BarPlace },
    /// `render_title`: the collapsed sidebar's buttons.
    Title,
}

/// Where a pane's tab bar is in the window, for what it draws (chores 2026-11-20).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct BarPlace {
    /// At the window's top edge, not in full screen: its empty part drags the window.
    pub drags_window: bool,
    /// The top-right pane's: it has the right sidebar's button (critique issue 4: one per
    /// window, not one per pane).
    pub top_right: bool,
    /// The top-left pane's while the sidebar is hidden: its tabs start after the traffic lights
    /// and the sidebar's buttons (`title_area::band_reserve`; critique issue 5).
    pub under_lights: bool,
}

/// Each of `leaves`' bar's place, `top` being the leaves along the layout's top edge, left to
/// right (`top_leaves`).
pub fn bar_places(
    leaves: &[PaneId],
    top: &[PaneId],
    full_screen: bool,
    sidebar_hidden: bool,
) -> Vec<(PaneId, BarPlace)> {
    leaves
        .iter()
        .map(|&id| {
            let place = BarPlace {
                drags_window: !full_screen && top.contains(&id),
                top_right: top.last() == Some(&id),
                under_lights: sidebar_hidden && top.first() == Some(&id),
            };
            (id, place)
        })
        .collect()
}

/// One cached part of the chrome: draws `kind` through the `Workspace`, and is drawn again
/// whenever the `Workspace` notifies.
pub struct ChromePart {
    workspace: WeakEntity<Workspace>,
    kind: ChromeKind,
    _observe: Subscription,
}

impl ChromePart {
    pub fn new(workspace: &Entity<Workspace>, kind: ChromeKind, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(workspace, |_, _, cx| cx.notify());
        Self {
            workspace: workspace.downgrade(),
            kind,
            _observe: observe,
        }
    }
}

impl Render for ChromePart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        // Listeners made in there (`cx.listener`) still go to the workspace; a hover, an
        // animation or `request_animation_frame` in there notifies the view being drawn: this.
        self.workspace
            .update(cx, |ws, cx| ws.render_chrome(kind, window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// The parts, made as they're first drawn (`Workspace::sync_chrome`): field `chrome` on
/// `Workspace`.
#[derive(Default)]
pub struct ChromeParts {
    pub sidebar: Option<Entity<ChromePart>>,
    pub status_bar: Option<Entity<ChromePart>>,
    pub title: Option<Entity<ChromePart>>,
    /// One per pane on screen, by (pane, its bar's place): a pane whose place changes (to or
    /// from the window's top edge, its corners, the sidebar hidden) gets a new part (the old one
    /// is dropped).
    pub tab_bars: HashMap<(PaneId, BarPlace), Entity<ChromePart>>,
}

impl Workspace {
    /// Part `kind` as its `ChromePart` draws it. Two things a part draws move with no notify
    /// of the workspace, so the part asks for its own next frame (a request made here notifies
    /// the view being drawn, the part):
    /// - tabs or rows sliding out of a drag's way (`drag_slides`), every frame until there.
    ///   (`Workspace::render` asks too, which redraws every part, but it looks before the parts
    ///   are drawn, so it misses a slide that starts in this frame.)
    /// - a new tab bar's first frame, where its row isn't laid out yet and can't scroll to its
    ///   shown tab (`tab_row_scroll`): once more, laid out.
    ///
    /// The sidebar's slide needs nothing here: `Workspace::render` asks for a frame while it
    /// lasts (a request from the workspace notifies it, so every part is drawn again), and ends
    /// it before the parts are drawn, so the title area shows the name on the frame it ends.
    pub(crate) fn render_chrome(
        &mut self,
        kind: ChromeKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        match kind {
            ChromeKind::Sidebar => {
                let sidebar = self.render_sidebar(ui, window, cx);
                if self.drag_slides.borrow().moving(Instant::now()) {
                    window.request_animation_frame();
                }
                sidebar
            }
            ChromeKind::StatusBar => self.render_status_bar(ui, window, cx),
            ChromeKind::TabBar { pane, place } => {
                let new = !self.tab_scroll.borrow().contains_key(&pane);
                let bar = self
                    .render_tab_bar(pane, ui, place, window, cx)
                    .unwrap_or_else(|| div().into_any_element());
                let unrevealed = || {
                    let shown = self.model.pane(pane).map(|p| p.active);
                    self.tab_scroll
                        .borrow()
                        .get(&pane)
                        .is_some_and(|(_, revealed)| shown.is_some() && *revealed != shown)
                };
                if self.drag_slides.borrow().moving(Instant::now()) || (new && unrevealed()) {
                    window.request_animation_frame();
                }
                bar
            }
            ChromeKind::Title => self.render_title(ui, window, cx),
        }
    }

    /// Make the parts not made yet, and the tab bars' for the panes on screen (each pane's
    /// key as `tab_bar_keys` says now); drop the tab-bar parts no longer wanted.
    pub(crate) fn sync_chrome(&mut self, window: &Window, cx: &mut Context<Self>) {
        let me = cx.entity();
        let part = |kind, cx: &mut Context<Self>| cx.new(|cx| ChromePart::new(&me, kind, cx));
        let chrome = &mut self.chrome;
        for (slot, kind) in [
            (&mut chrome.sidebar, ChromeKind::Sidebar),
            (&mut chrome.status_bar, ChromeKind::StatusBar),
            (&mut chrome.title, ChromeKind::Title),
        ] {
            if slot.is_none() {
                *slot = Some(part(kind, cx));
            }
        }
        let keys = self.tab_bar_keys(window);
        let bars = &mut self.chrome.tab_bars;
        bars.retain(|key, _| keys.contains(key));
        for (pane, place) in keys {
            bars.entry((pane, place))
                .or_insert_with(|| part(ChromeKind::TabBar { pane, place }, cx));
        }
        // Each part read in every frame, so the window keeps it among the entities it shows:
        // GPUI drops a notify of an entity the last frame didn't touch, and a cached part's
        // reused frame doesn't touch it (nor does its first, if made in that same frame: a
        // view only counts the entities first touched while it renders).
        let chrome = &self.chrome;
        for part in [&chrome.sidebar, &chrome.status_bar, &chrome.title]
            .into_iter()
            .flatten()
            .chain(chrome.tab_bars.values())
        {
            part.read(cx);
        }
        // The same for the marks' views on screen (M11p Task 6), which their clock's ticks
        // notify, and for the clock. A view no longer drawn (its tab gone idle, the status bar
        // following another terminal) is left out: kept, but its notifies redraw nothing.
        self.clock.read(cx);
        let marks = self.marks.borrow();
        for (key, _) in self.marks_on_screen() {
            if let Some(mark) = marks.get(&key) {
                mark.read(cx);
            }
        }
        // And the agent tabs the panes show (M11aa): a view `ensure_agent_tabs` made in this
        // `render` is first drawn (cached) in the frame it is made, which doesn't count it, so
        // without this its own notifies (a streamed reply) would redraw nothing until a refresh.
        if let Some(tree) = self.shown_layout() {
            for pane in tree.leaves() {
                if let Some(view) = self.pane_agent_tab(pane) {
                    view.read(cx);
                }
            }
        }
    }

    /// The tab-bar parts the screen wants: each pane of the shown layout, and its bar's place.
    pub(crate) fn tab_bar_keys(&self, window: &Window) -> Vec<(PaneId, BarPlace)> {
        let Some(tree) = self.shown_layout() else {
            return Vec::new();
        };
        let leaves: Vec<PaneId> = tree
            .leaves()
            .into_iter()
            .filter(|id| self.model.pane(*id).is_some())
            .collect();
        bar_places(
            &leaves,
            &tree.top_leaves(),
            window.is_fullscreen(),
            self.sidebar.hidden,
        )
    }
}

/// Whether the chrome is drawn as cached parts: unless `FUXX_CHROME_CACHE=off` (read once).
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    let on = *ON.get_or_init(|| on_for(std::env::var("FUXX_CHROME_CACHE").ok().as_deref()));
    #[cfg(feature = "ui-shots")]
    let on = on && !shots_off::get();
    on
}

/// What `FUXX_CHROME_CACHE` = `value` says: cached unless it is `off`.
pub fn on_for(value: Option<&str>) -> bool {
    value != Some("off")
}

/// ui-shots only: draw the chrome uncached in this process from now on (`true`) or as the
/// environment says (`false`), to compare the two in one window.
#[cfg(feature = "ui-shots")]
pub mod shots_off {
    use std::sync::atomic::{AtomicBool, Ordering};

    static OFF: AtomicBool = AtomicBool::new(false);

    pub fn set(off: bool) {
        OFF.store(off, Ordering::Relaxed);
    }

    pub fn get() -> bool {
        OFF.load(Ordering::Relaxed)
    }
}

/// A chrome part whose render passes are counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Counted {
    /// `Workspace::render`.
    Workspace,
    /// `render_sidebar`.
    Sidebar,
    /// `render_status_bar`.
    StatusBar,
    /// `render_tab_bar`, once per pane's bar.
    TabBar,
    /// `render_title`: the collapsed sidebar's buttons (empty while the sidebar is shown: its
    /// band is the sidebar's, counted as `Sidebar`).
    Title,
    /// `sidebar_rows`, the sidebar's row model (also called outside `render_sidebar`).
    SidebarRows,
    /// `Window::set_window_title`, from `Workspace::render`.
    WindowTitle,
    /// A `MarkView` (M11p Task 6): one status mark, every one counted together.
    Mark,
}

#[cfg(feature = "ui-shots")]
mod counters {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::Counted;

    static COUNTS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

    pub fn bump(c: Counted) {
        COUNTS[c as usize].fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(c: Counted) -> u64 {
        COUNTS[c as usize].load(Ordering::Relaxed)
    }

    pub fn reset() {
        for count in &COUNTS {
            count.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(feature = "ui-shots")]
pub use counters::{bump, get, reset};

/// Count one render pass of `c` (nothing without the `ui-shots` feature).
#[cfg(not(feature = "ui-shots"))]
#[inline(always)]
pub fn bump(_: Counted) {}

#[cfg(test)]
mod tests {
    use super::{BarPlace, bar_places, on_for};
    use crate::layout_model::PaneId;

    /// Critique issues 4 and 5 (2026-11-20): one right-sidebar button per window, on the
    /// top-right pane's bar; with the sidebar hidden the top-left pane's bar makes room for the
    /// lights and the sidebar's buttons; only the top row drags the window (not in full screen).
    #[test]
    fn each_bar_knows_its_place_in_the_window() {
        let p = PaneId;
        // 1 | (2 over 3): leaves 1, 2, 3; top 1, 2.
        let leaves = [p(1), p(2), p(3)];
        let top = [p(1), p(2)];
        let place = |drags_window, top_right, under_lights| BarPlace {
            drags_window,
            top_right,
            under_lights,
        };
        assert_eq!(
            bar_places(&leaves, &top, false, false),
            vec![
                (p(1), place(true, false, false)),
                (p(2), place(true, true, false)),
                (p(3), place(false, false, false)),
            ]
        );
        // Hidden sidebar: the top-left one is under the lights; full screen drags nothing.
        assert_eq!(
            bar_places(&leaves, &top, true, true),
            vec![
                (p(1), place(false, false, true)),
                (p(2), place(false, true, false)),
                (p(3), place(false, false, false)),
            ]
        );
        // One pane: it is both.
        assert_eq!(
            bar_places(&[p(7)], &[p(7)], false, true),
            vec![(p(7), place(true, true, true))]
        );
    }

    #[test]
    fn the_cache_is_off_only_when_the_variable_says_off() {
        assert!(on_for(None));
        assert!(on_for(Some("")));
        assert!(on_for(Some("on")));
        assert!(on_for(Some("OFF")), "exactly `off`, as the spike read it");
        assert!(!on_for(Some("off")));
    }
}
