//! Where a clicked link opens (batch 3, chore 6): "Open links in" (Preferences → Browser) picks a
//! browser split on the right (the default), a new tab in the pane you clicked in, or your
//! default browser. `link_place` is the pure choice; `Workspace::open_link` acts on it. Your
//! focus stays where you clicked; a pane reused shows its new or found tab, whose pill flashes
//! once.

use crate::appearance::OpenLinksIn;
use crate::layout_model::{Model, PaneId, TabId, TabKind, WorkspaceId};
use crate::split_tree::Dir;

/// Below this width (points) the pane you clicked in isn't split: the link opens as a tab in it.
pub const SPLIT_BELOW: f32 = 560.0;

/// Where a link goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkPlace {
    /// The OS opens it: your default browser, or Mail for `mailto:`.
    Os,
    /// That URL is already open in the target pane: its tab is shown.
    Select(TabId),
    /// A new browser tab at the end of this pane's bar.
    TabIn(PaneId),
    /// A new browser pane split right of this one.
    Split(PaneId),
}

/// Whether two addresses are the same page (a trailing `/` aside).
pub fn same_url(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// Where `url`, clicked in pane `from` of workspace `ws`, opens under `mode`.
///
/// - `mailto:` (anything not http or https) and "Your default browser": the OS.
/// - "A new tab in this pane": `from`.
/// - "A browser split on the right": the workspace's last link pane (`last`) while it is
///   still open, else its most recently used browser pane that isn't `from` (`used`: higher is
///   more recent; a tie goes to the right neighbour, else the first in reading order), else a
///   new split right of `from`, or, when `from` is too narrow to split (`from_narrow`), `from`.
///
/// In a pane, a tab already showing that URL is shown instead of a new one.
#[allow(clippy::too_many_arguments)]
pub fn link_place(
    model: &Model,
    ws: WorkspaceId,
    from: PaneId,
    url: &str,
    mode: OpenLinksIn,
    last: Option<PaneId>,
    from_narrow: bool,
    used: impl Fn(TabId) -> u64,
) -> LinkPlace {
    let lower = url.to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    if !web || mode == OpenLinksIn::Browser {
        return LinkPlace::Os;
    }
    let browsers_in = |pane: PaneId| -> Vec<(TabId, Option<&str>)> {
        model
            .pane(pane)
            .map(|p| {
                p.tabs
                    .iter()
                    .filter_map(|t| match &t.kind {
                        TabKind::Browser { url } => Some((t.id, url.as_deref())),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let in_pane = |pane: PaneId| {
        browsers_in(pane)
            .into_iter()
            .find(|(_, u)| u.is_some_and(|u| same_url(u, url)))
            .map_or(LinkPlace::TabIn(pane), |(tab, _)| LinkPlace::Select(tab))
    };
    if mode == OpenLinksIn::ThisPane {
        return in_pane(from);
    }
    let Some(workspace) = model.workspace(ws) else {
        return LinkPlace::Split(from);
    };
    let open = |pane: PaneId| workspace.layout.contains(pane);
    let target = last.filter(|&p| open(p)).or_else(|| {
        let right = workspace
            .layout
            .neighbor(from, Dir::Right)
            .filter(|&p| !browsers_in(p).is_empty());
        let mut best: Option<(PaneId, u64)> = None;
        for pane in workspace.layout.leaves() {
            if pane == from {
                continue;
            }
            for (tab, _) in browsers_in(pane) {
                let u = used(tab);
                if best.is_none_or(|(_, b)| u > b) {
                    best = Some((pane, u));
                }
            }
        }
        // None used yet: the right neighbour's browser first.
        match best {
            Some((_, 0)) if right.is_some() => right,
            other => other.map(|(p, _)| p),
        }
    });
    match target {
        Some(pane) => in_pane(pane),
        None if from_narrow => in_pane(from),
        None => LinkPlace::Split(from),
    }
}

impl crate::workspace::Workspace {
    /// A link clicked (or the menu's Open Link) in tab `from`: where `link_place` says.
    pub(crate) fn open_link(
        &mut self,
        from: TabId,
        url: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Only the schemes a link may have (`links::allowed`): never `file://` or a script.
        if !crate::links::allowed(url) {
            return;
        }
        let mode = cx.global::<crate::appearance::Appearance>().open_links_in;
        let Some((ws, from_pane)) = self.model.locate(from) else {
            self.open_url_outside(url, cx);
            return;
        };
        let narrow = self.pane_width(from_pane).is_some_and(|w| w < SPLIT_BELOW);
        let last = self.link_panes.get(&ws).copied();
        let place = link_place(&self.model, ws, from_pane, url, mode, last, narrow, |t| {
            self.browser_used(t, cx)
        });
        let before = self.model.focused_tab();
        let kind = || TabKind::Browser {
            url: Some(url.to_string()),
        };
        let (tab, pane, reused) = match place {
            LinkPlace::Os => {
                self.open_url_outside(url, cx);
                return;
            }
            LinkPlace::Select(tab) => {
                let Some((_, pane)) = self.model.locate(tab) else {
                    return;
                };
                self.model.select_tab(tab);
                (tab, pane, true)
            }
            LinkPlace::TabIn(pane) => {
                let Some(opened) = self.model.add_tab(pane, kind()) else {
                    return;
                };
                (opened.tab, pane, pane != from_pane)
            }
            LinkPlace::Split(pane) => {
                let Some(opened) = self.model.split(pane, crate::split_tree::Axis::Row, kind())
                else {
                    return;
                };
                (opened.tab, opened.pane, false)
            }
        };
        // Your focus stays where you clicked (unless the link went into that same pane).
        if pane != from_pane
            && let Some(before) = before
        {
            self.model.select_tab(before);
        }
        self.sync();
        if !self.browser_tabs.contains_key(&tab) {
            let view = self.make_browser_tab(tab, None, window, cx);
            view.update(cx, |view, cx| view.load(url, false, cx));
        }
        self.use_browser_tab(tab, cx);
        self.link_panes.insert(ws, pane);
        let still = cx.reduce_motion() || !cx.global::<crate::appearance::Appearance>().animations;
        if reused && !still {
            self.flash_seq += 1;
            self.link_flash = Some((tab, self.flash_seq, std::time::Instant::now()));
        }
        self.save();
        self.activate(window, cx);
        cx.notify();
    }

    /// How wide pane `pane` was last drawn (points); none before the first frame or for a pane
    /// not on screen.
    pub(crate) fn pane_width(&self, pane: PaneId) -> Option<f32> {
        let area = self.pane_area_rect()?;
        self.shown_layout()?
            .rects(area)
            .into_iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, r)| r.w)
    }

    /// The tab whose pill flashes now (a link opened into a pane already there), with its
    /// animation's number; none once the flash is over.
    pub(crate) fn flashing_tab(&self) -> Option<(TabId, u64)> {
        let (tab, seq, at) = self.link_flash?;
        (at.elapsed() < std::time::Duration::from_millis(FLASH_MS)).then_some((tab, seq))
    }
}

/// How long a reused pane's tab pill flashes (as the latest prompt's jump: `agent_chat`).
pub const FLASH_MS: u64 = crate::agent_chat::FLASH_MS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split_tree::Axis;

    const URL: &str = "https://example.com/docs";

    fn browser(url: Option<&str>) -> TabKind {
        TabKind::Browser {
            url: url.map(str::to_string),
        }
    }

    /// A workspace of one terminal pane.
    fn one() -> (Model, WorkspaceId, PaneId) {
        let mut m = Model::new();
        let o = m.new_workspace(0, TabKind::Terminal(1)).unwrap();
        (m, o.workspace, o.pane)
    }

    const NEVER: fn(TabId) -> u64 = |_| 0;

    #[test]
    fn same_url_ignores_a_trailing_slash() {
        assert!(same_url("https://a.dev/", "https://a.dev"));
        assert!(same_url("https://a.dev/x", "https://a.dev/x"));
        assert!(!same_url("https://a.dev/x", "https://a.dev/y"));
    }

    #[test]
    fn mail_and_the_default_browser_go_to_the_os() {
        let (m, ws, p) = one();
        let place = |url, mode| link_place(&m, ws, p, url, mode, None, false, NEVER);
        assert_eq!(place("mailto:a@b.c", OpenLinksIn::Split), LinkPlace::Os);
        assert_eq!(place("MAILTO:a@b.c", OpenLinksIn::ThisPane), LinkPlace::Os);
        assert_eq!(place(URL, OpenLinksIn::Browser), LinkPlace::Os);
    }

    #[test]
    fn the_first_link_splits_right_unless_the_pane_is_narrow() {
        let (m, ws, p) = one();
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::Split, None, false, NEVER),
            LinkPlace::Split(p)
        );
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::Split, None, true, NEVER),
            LinkPlace::TabIn(p)
        );
    }

    #[test]
    fn the_next_link_is_a_tab_in_the_last_link_pane_and_the_same_url_selects() {
        let (mut m, ws, p) = one();
        let right = m.split(p, Axis::Row, browser(Some(URL))).unwrap();
        let place = |m: &Model, url, last| {
            link_place(m, ws, p, url, OpenLinksIn::Split, last, false, NEVER)
        };
        assert_eq!(
            place(&m, "https://other.dev", Some(right.pane)),
            LinkPlace::TabIn(right.pane)
        );
        assert_eq!(
            place(&m, "https://example.com/docs/", Some(right.pane)),
            LinkPlace::Select(right.tab)
        );
        // No last link pane known: the browser pane beside it all the same.
        assert_eq!(
            place(&m, "https://other.dev", None),
            LinkPlace::TabIn(right.pane)
        );
        // The last link pane closed (not in the workspace): found again by its browser.
        assert_eq!(
            place(&m, "https://other.dev", Some(PaneId(999))),
            LinkPlace::TabIn(right.pane)
        );
    }

    #[test]
    fn the_most_recently_used_browser_pane_wins_but_never_the_source_pane() {
        let (mut m, ws, p) = one();
        let a = m.split(p, Axis::Row, browser(None)).unwrap();
        let b = m.split(a.pane, Axis::Column, browser(None)).unwrap();
        // A browser tab in the source pane itself doesn't count.
        let own = m.add_tab(p, browser(None)).unwrap();
        let used = |t: TabId| match t {
            t if t == b.tab => 7,
            t if t == own.tab => 9,
            _ => 1,
        };
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::Split, None, false, used),
            LinkPlace::TabIn(b.pane)
        );
        // A tie: the right neighbour holding a browser.
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::Split, None, false, NEVER),
            LinkPlace::TabIn(a.pane)
        );
    }

    #[test]
    fn a_new_tab_in_this_pane_reuses_its_own_tab_of_that_url() {
        let (mut m, ws, p) = one();
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::ThisPane, None, false, NEVER),
            LinkPlace::TabIn(p)
        );
        let open = m.add_tab(p, browser(Some(URL))).unwrap();
        assert_eq!(
            link_place(&m, ws, p, URL, OpenLinksIn::ThisPane, None, false, NEVER),
            LinkPlace::Select(open.tab)
        );
    }
}
