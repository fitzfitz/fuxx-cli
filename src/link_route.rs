//! Where a clicked link opens (batch 3, chore 6): "Open links in" (Preferences → Browser) picks a
//! browser split on the right (the default), a new tab in the pane you clicked in, or your
//! default browser. `link_place` is the pure choice; `Workspace::open_link` acts on it. Your
//! focus stays where you clicked; a pane reused shows its new or found tab, whose pill flashes
//! once.
//!
//! A program's own browser opening (an auto-open, through the `auto_open` shims and
//! `fuxx link open URL`: `Workspace::link_open`) goes the same way from the terminal or agent
//! tab it runs in, even in a workspace you aren't looking at; its tab's pill always flashes,
//! and your focus stays where it is.

use serde_json::{Value, json};

use crate::api::{ApiError, Target, parse_target};
use crate::appearance::OpenLinksIn;
use crate::layout_model::{Model, PaneId, TabId, TabKind, WorkspaceId};
use crate::split_tree::Dir;
use crate::workspace_api::{opt_str, required_str};

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

/// Where a link to open came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkFrom {
    /// Clicked (or the menu's Open Link): its tab's pill flashes when a pane is reused.
    Click,
    /// A program opened it by itself (`fuxx link open`): its tab's pill always flashes, since
    /// you didn't ask for it just now.
    AutoOpen,
}

/// The focus before a link opens from workspace `ws`: the focused tab, and `ws`'s own (its
/// focused pane's shown tab), for `keep_focus`.
pub fn focus_before(model: &Model, ws: WorkspaceId) -> (Option<TabId>, Option<TabId>) {
    let ws_tab = model
        .workspace(ws)
        .and_then(|w| model.pane(w.focused))
        .map(|p| p.active);
    (model.focused_tab(), ws_tab)
}

/// After a link opened from pane `from` of workspace `ws` into pane `pane`: the focus as it was
/// (`focus_before`). Into another pane, `ws`'s own focus and then yours go back; into `from`
/// itself (a new tab in this pane), the tab stays shown there, and your focus goes back only
/// when it was in another workspace (an auto-open from one you aren't looking at).
pub fn keep_focus(
    model: &mut Model,
    ws: WorkspaceId,
    from: PaneId,
    pane: PaneId,
    before: Option<TabId>,
    ws_before: Option<TabId>,
) {
    let elsewhere = before
        .and_then(|t| model.locate(t))
        .is_some_and(|(w, _)| w != ws);
    if pane != from
        && let Some(t) = ws_before
    {
        model.select_tab(t);
    }
    if (pane != from || elsewhere)
        && let Some(t) = before
    {
        model.select_tab(t);
    }
}

/// The tab a `link.open` request runs in: agent tab `agent` (`aN`; `agents` finds its tab by
/// number), else terminal `caller` (`sN`'s number). Neither, or a bad one, is an error.
pub fn caller_tab(
    model: &Model,
    agents: impl Fn(u64) -> Option<TabId>,
    p: &Value,
) -> Result<TabId, ApiError> {
    if let Some(agent) = opt_str(p, "agent")? {
        let Target::Agent(n) = parse_target(agent) else {
            return Err(ApiError::bad_request(format!(
                "agent must be an agent tab like a2, not {agent}"
            )));
        };
        return agents(n).ok_or_else(|| ApiError::not_found(format!("no agent tab a{n}")));
    }
    match p.get("caller").and_then(Value::as_u64) {
        Some(id) => model
            .terminal_tab(id)
            .ok_or_else(|| ApiError::not_found(format!("no session s{id}"))),
        None => Err(ApiError::bad_request(
            "link open runs in a fuxx terminal or agent tab (FUXX_SESSION or FUXX_AGENT_TAB)",
        )),
    }
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
        self.open_link_from(from, url, LinkFrom::Click, window, cx);
    }

    /// `link.open` (`fuxx link open URL`, a shim's): `url`, an http or https address, as a link
    /// clicked in the terminal or agent tab the request names (`caller_tab`). Answered at once:
    /// a page that loads, or doesn't, is the browser tab's business.
    pub(crate) fn link_open(
        &mut self,
        p: &Value,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Value, ApiError> {
        let url = required_str(p, "url")?;
        if !crate::auto_open::is_web(url) {
            return Err(ApiError::bad_request(format!(
                "link open takes an http or https address, not {url}"
            )));
        }
        let agents = |n: u64| {
            self.agents
                .iter()
                .find(|(_, slot)| slot.id == n)
                .map(|(&tab, _)| tab)
        };
        let from = caller_tab(&self.model, agents, p)?;
        self.open_link_from(from, url, LinkFrom::AutoOpen, window, cx);
        Ok(json!({}))
    }

    /// `open_link`, the link from `origin`.
    pub(crate) fn open_link_from(
        &mut self,
        from: TabId,
        url: &str,
        origin: LinkFrom,
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
        let (before, ws_before) = focus_before(&self.model, ws);
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
        keep_focus(&mut self.model, ws, from_pane, pane, before, ws_before);
        self.sync();
        if !self.browser_tabs.contains_key(&tab) {
            let view = self.make_browser_tab(tab, None, window, cx);
            view.update(cx, |view, cx| view.load(url, false, cx));
        }
        self.use_browser_tab(tab, cx);
        self.link_panes.insert(ws, pane);
        let still = cx.reduce_motion() || !cx.global::<crate::appearance::Appearance>().animations;
        if (reused || origin == LinkFrom::AutoOpen) && !still {
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

    /// Two workspaces: `a` (one terminal, focused) and `b` (one terminal), as (model, a's tab,
    /// b's workspace, b's pane, b's tab).
    fn two() -> (Model, TabId, WorkspaceId, PaneId, TabId) {
        let mut m = Model::new();
        let b = m.new_workspace(0, TabKind::Terminal(2)).unwrap();
        let a = m.new_workspace(0, TabKind::Terminal(1)).unwrap();
        (m, a.tab, b.workspace, b.pane, b.tab)
    }

    #[test]
    fn your_focus_stays_after_a_split_in_your_workspace() {
        let (mut m, ws, p) = one();
        let t = m.focused_tab().unwrap();
        let (before, ws_before) = focus_before(&m, ws);
        let right = m.split(p, Axis::Row, browser(Some(URL))).unwrap();
        keep_focus(&mut m, ws, p, right.pane, before, ws_before);
        assert_eq!(m.focused_tab(), Some(t));
    }

    #[test]
    fn an_auto_open_in_another_workspace_moves_nothing_of_yours() {
        let (mut m, yours, b, bp, bt) = two();
        let (before, ws_before) = focus_before(&m, b);
        let right = m.split(bp, Axis::Row, browser(Some(URL))).unwrap();
        keep_focus(&mut m, b, bp, right.pane, before, ws_before);
        assert_eq!(m.focused_tab(), Some(yours), "your tab keeps the focus");
        assert_eq!(
            m.workspace(b).unwrap().focused,
            bp,
            "its terminal stays its focus"
        );
        assert_eq!(m.pane(bp).unwrap().active, bt);
        // "A new tab in this pane": the tab is shown there, but your workspace stays in front.
        let (before, ws_before) = focus_before(&m, b);
        let tab = m.add_tab(bp, browser(Some(URL))).unwrap();
        keep_focus(&mut m, b, bp, bp, before, ws_before);
        assert_eq!(m.focused_tab(), Some(yours));
        assert_eq!(m.pane(bp).unwrap().active, tab.tab);
    }

    #[test]
    fn a_new_tab_in_your_own_pane_is_shown() {
        let (mut m, ws, p) = one();
        let (before, ws_before) = focus_before(&m, ws);
        let tab = m.add_tab(p, browser(Some(URL))).unwrap();
        keep_focus(&mut m, ws, p, p, before, ws_before);
        assert_eq!(m.focused_tab(), Some(tab.tab));
    }

    #[test]
    fn an_auto_open_names_its_terminal_or_agent_tab() {
        let (mut m, ws, p) = one();
        let agent = m
            .add_tab(
                p,
                TabKind::Agent {
                    provider: "claude".into(),
                    cwd: "code/my-app".into(),
                    conversation: None,
                    mode: None,
                    title: None,
                    model: None,
                    effort: None,
                },
            )
            .map(|o| o.tab);
        let _ = ws;
        let terminal = m.terminal_tab(1).unwrap();
        let agents = |n: u64| (n == 2).then_some(agent).flatten();
        let tab = |p: serde_json::Value| caller_tab(&m, agents, &p).map_err(|e| e.code);
        assert_eq!(tab(serde_json::json!({"caller": 1})), Ok(terminal));
        assert_eq!(tab(serde_json::json!({"agent": "a2"})), Ok(agent.unwrap()));
        assert_eq!(tab(serde_json::json!({"caller": 9})), Err("not_found"));
        assert_eq!(tab(serde_json::json!({"agent": "a7"})), Err("not_found"));
        assert_eq!(tab(serde_json::json!({"agent": "s1"})), Err("bad_request"));
        assert_eq!(tab(serde_json::json!({})), Err("bad_request"));
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
