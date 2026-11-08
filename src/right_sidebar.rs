//! The right sidebar (M11g): one per window, on the right of the panes, showing the focused
//! terminal's files (Task 5 fills it; until then its body says "Files").
//!
//! It opens and closes with ⌥⌘\, View → Toggle Right Sidebar, or the icon at the far right of
//! every pane's tab bar (`tab_bar::BarButton::RightSidebar`, lit while it's open), sliding like
//! the left sidebar (`sidebar_toggle::Slide`, not with Reduce motion on). Its left edge resizes
//! it (220–560 pt, 300 by default). Open state and width live in `sessions.json`.
//!
//! It follows the focused terminal (with a browser or file tab focused, the last terminal it
//! followed): that terminal's folder, found like a sidebar row's (`git_branch::refresh_places`:
//! its OSC 7 report, else the kernel's answer, else the folder it started in; an SSH terminal's
//! only as reported). The lookup runs off the UI thread, on a focus change and every 2 s while
//! the sidebar can be seen (`tick_right_sidebar`, which stops once it can't).
//!
//! `clamp`, `width_for_edge`, `resolve_folder` and `lookup_due` are pure and tested.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Window, canvas, div, prelude::*, px,
};

use crate::git_branch::{is_this_mac, process_cwd};
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::sidebar_toggle::{self, Slide};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{
    DIVIDER_ACTIVE, RIGHT_SIDEBAR_DEFAULT_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH, RIGHT_SIDEBAR_MIN_WIDTH,
    TAB_BAR,
};
use crate::ui_tokens::text;
use crate::workspace::Workspace;

/// How often the followed terminal's folder is looked up again while the sidebar can be seen.
pub const REFRESH: Duration = Duration::from_secs(2);
/// The strip along its left edge that resizes it.
const RESIZE_STRIP: f32 = 4.0;
/// The placeholder panel's name (Task 5 puts the Files panel here).
const PANEL: &str = "Files";

/// A width it can take: within 220–560 pt (the default for a missing or odd one).
pub fn clamp(width: Option<f32>) -> f32 {
    match width {
        Some(w) if w.is_finite() => w.clamp(RIGHT_SIDEBAR_MIN_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH),
        _ => RIGHT_SIDEBAR_DEFAULT_WIDTH,
    }
}

/// Its width with its left edge at `x` in a window `window_width` wide (it ends at the
/// window's right).
pub fn width_for_edge(window_width: f32, x: f32) -> f32 {
    clamp(Some(window_width - x))
}

/// A terminal's folder: what it `reported` (OSC 7, already checked to be about this Mac for a
/// local one), else for a local one the kernel's answer (`cwd`, asked only then) and then the
/// folder it started in (`project`). An SSH terminal's is only what it reported: the rest are
/// this Mac's, not the far machine's.
pub fn resolve_folder(
    reported: Option<PathBuf>,
    ssh: bool,
    cwd: impl FnOnce() -> Option<PathBuf>,
    project: Option<PathBuf>,
) -> Option<PathBuf> {
    if ssh {
        return reported;
    }
    reported.or_else(cwd).or(project)
}

/// Whether terminal `id`'s folder is looked up `now`, the last lookup having been `last` (the
/// terminal and when): for a terminal not looked up last, at once; else every `REFRESH`.
pub fn lookup_due(last: Option<(u64, Instant)>, id: u64, now: Instant) -> bool {
    match last {
        Some((of, at)) if of == id => now.saturating_duration_since(at) >= REFRESH,
        _ => true,
    }
}

/// The right sidebar's state: what's saved (closed, width) and what isn't.
pub struct RightSidebarState {
    pub hidden: bool,
    pub width: f32,
    /// Its left edge is being dragged.
    pub resizing: bool,
    /// It is sliding in or out; none when it isn't moving.
    pub slide: Option<Slide>,
    /// The terminal it follows (`sN`): the focused one, or the last one focused.
    pub(crate) followed: Option<u64>,
    /// The last folder found: for which terminal, the folder (none: not found) and whether that
    /// terminal is an SSH one.
    pub(crate) found: Option<(u64, Option<PathBuf>, bool)>,
    /// The last lookup started: for which terminal, and when.
    looked: Option<(u64, Instant)>,
    /// Whether the 2 s refresh is running (only while the sidebar can be seen).
    ticking: bool,
}

impl RightSidebarState {
    /// Closed, at the default width.
    pub fn new() -> Self {
        Self {
            hidden: true,
            width: RIGHT_SIDEBAR_DEFAULT_WIDTH,
            resizing: false,
            slide: None,
            followed: None,
            found: None,
            looked: None,
            ticking: false,
        }
    }

    /// Take its open state and width from the sessions file.
    pub fn load(&mut self, saved: &SavedState) {
        self.hidden = saved.right_sidebar_hidden;
        self.width = clamp(saved.right_sidebar_width);
    }
}

impl Workspace {
    /// ⌥⌘\, View → Toggle Right Sidebar, a pane's icon: open or close the right sidebar. It
    /// slides unless Reduce motion is on (a slide under way turns back from where it got to).
    /// The keyboard stays where it is, unless what had it went with the sidebar: then the
    /// focused tab takes it.
    pub(crate) fn toggle_right_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.right_sidebar;
        state.hidden = !state.hidden;
        state.resizing = false;
        let current = state.slide;
        state.slide = (!cx.reduce_motion()).then(|| Slide::start(!state.hidden, current));
        if self.right_sidebar.hidden && window.focused(cx).is_none() {
            self.focus_active(window, cx);
        }
        self.save();
        cx.notify();
    }

    /// A pane's right-sidebar icon: that pane takes the focus, then the sidebar opens or closes.
    pub(crate) fn right_sidebar_button(
        &mut self,
        pane: crate::layout_model::PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.focus_pane(pane) {
            self.sync();
        }
        self.activate(window, cx);
        self.toggle_right_sidebar(window, cx);
    }

    /// The folder the right sidebar shows, and whether its terminal is an SSH one: the followed
    /// terminal's, as last looked up; before the first lookup for it, what is known without
    /// asking the kernel (its report, else the folder it started in). None with no terminal
    /// followed yet, or none found.
    // The Files panel (M11g Task 5) reads it; until then only the UI screenshots do.
    #[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
    pub(crate) fn right_sidebar_folder(&self, cx: &App) -> Option<(PathBuf, bool)> {
        let id = self.right_sidebar.followed?;
        if let Some((of, folder, ssh)) = &self.right_sidebar.found
            && *of == id
        {
            return folder.clone().map(|f| (f, *ssh));
        }
        let session = self.sessions.items().iter().find(|s| s.id == id)?;
        let ssh = session.ssh.is_some();
        let (reported, _) = self.reported_folder(id, cx)?;
        resolve_folder(reported, ssh, || None, session.project.clone()).map(|f| (f, ssh))
    }

    /// Terminal `id`'s OSC 7 report (counted for a local terminal only when it is about this
    /// Mac) and its shell's pid; none for an unknown terminal.
    fn reported_folder(&self, id: u64, cx: &App) -> Option<(Option<PathBuf>, Option<u32>)> {
        let session = self.sessions.items().iter().find(|s| s.id == id)?;
        let ssh = session.ssh.is_some();
        let (reported, pid) = session.view.read(cx).working_folder();
        let hostname = self.places.hostname.as_deref();
        let reported = reported
            .filter(|r| ssh || is_this_mac(&r.host, hostname))
            .map(|r| r.path);
        Some((reported, pid))
    }

    /// Whether the followed terminal's folder can be looked up: the sidebar is open, the window
    /// can be seen, and nothing is frozen (the UI screenshots).
    fn right_sidebar_wanted(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        !self.right_sidebar.hidden && seen && !self.places.frozen
    }

    /// Follow the focused terminal (a browser or file tab focused: keep the last one) and look
    /// its folder up where due (`lookup_due`): at once for a newly followed terminal, else every
    /// 2 s. `render` calls it on every frame, the 2 s refresh between frames.
    pub(crate) fn refresh_right_sidebar(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_terminal() {
            self.right_sidebar.followed = Some(id);
        }
        let Some(id) = self.right_sidebar.followed else {
            return;
        };
        let now = Instant::now();
        if !self.right_sidebar_wanted(cx) || !lookup_due(self.right_sidebar.looked, id, now) {
            return;
        }
        let Some((reported, pid)) = self.reported_folder(id, cx) else {
            return;
        };
        let Some(session) = self.sessions.items().iter().find(|s| s.id == id) else {
            return;
        };
        let (ssh, project) = (session.ssh.is_some(), session.project.clone());
        self.right_sidebar.looked = Some((id, now));
        cx.spawn(async move |this, cx| {
            // The kernel lookup (`proc_pidinfo`) off the UI thread.
            let folder = cx
                .background_executor()
                .spawn(async move {
                    resolve_folder(reported, ssh, || pid.and_then(process_cwd), project)
                })
                .await;
            this.update(cx, |this, cx| {
                // An answer for a terminal no longer followed is dropped.
                let found = Some((id, folder, ssh));
                if this.right_sidebar.followed == Some(id) && this.right_sidebar.found != found {
                    this.right_sidebar.found = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Start the 2 s refresh unless it's running or not wanted (`right_sidebar_wanted`); it
    /// stops once it isn't, so a closed sidebar or a hidden window is never woken. `render`
    /// calls it, so opening the sidebar or showing the window again starts it.
    pub(crate) fn tick_right_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.right_sidebar.ticking || !self.right_sidebar_wanted(cx) {
            return;
        }
        self.right_sidebar.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                let wanted = this
                    .update(cx, |this, cx| {
                        let wanted = this.right_sidebar_wanted(cx);
                        this.right_sidebar.ticking = wanted;
                        if wanted {
                            this.refresh_right_sidebar(cx);
                        }
                        wanted
                    })
                    .unwrap_or(false);
                if !wanted {
                    break;
                }
            }
        })
        .detach();
    }

    /// The right sidebar and how it sits this frame: laid out beside the pane column, or (while
    /// sliding) laid over it from the right; none when it's closed and still. Ends a slide past
    /// its time; until then every frame asks for the next. Also: how much of it shows while
    /// sliding.
    pub(crate) fn right_sidebar_layout(
        &mut self,
        ui: UiColors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        self.right_sidebar.slide = self
            .right_sidebar
            .slide
            .filter(|s| s.since.elapsed() < sidebar_toggle::SLIDE);
        let slide = self.right_sidebar.slide.map(|s| s.shown());
        if slide.is_some() {
            window.request_animation_frame();
        }
        if self.right_sidebar.hidden && slide.is_none() {
            return (None, None);
        }
        let node = self.render_right_sidebar(ui, window, cx);
        match slide {
            Some(shown) => {
                let width = self.right_sidebar.width;
                let over = div()
                    .absolute()
                    .top_0()
                    .right(px(-(1.0 - shown) * width))
                    .h_full()
                    .child(node)
                    .into_any_element();
                (None, Some(over))
            }
            None => (Some(node), None),
        }
    }

    /// The right sidebar: its panel's name on a strip as tall as the tab bars (the window can be
    /// dragged by it), the panel (a placeholder until Task 5), and its left edge, which resizes it.
    fn render_right_sidebar(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let strip = div()
            .id("right-sidebar-band")
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(TAB_BAR))
            .px(px(10.0))
            .border_b_1()
            .border_color(hsla(ui.sidebar_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(PANEL);
        let strip = if window.is_fullscreen() {
            strip
        } else {
            self.drag_window_by(strip, cx)
        };
        div()
            .id("right-sidebar")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.right_sidebar.width))
            .h_full()
            .bg(hsla(ui.sidebar_background))
            .border_l_1()
            .border_color(hsla(ui.sidebar_border))
            .child(strip)
            .child(div().flex_1().min_h(px(0.0)))
            .child(self.render_right_resize_edge(ui, cx))
            .into_any_element()
    }

    /// The strip along the right sidebar's left edge: drag it to resize the sidebar. A 2 px
    /// accent line shows on hover and while dragging; while dragging, the whole window follows
    /// the mouse until release (as the left sidebar's edge does).
    fn render_right_resize_edge(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let resizing = self.right_sidebar.resizing;
        let this = cx.entity().downgrade();
        let follow = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if !resizing {
                    return;
                }
                // Window-wide, not tied to the strip, so a fast move off it keeps resizing.
                let moved = this.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let window_width = f32::from(window.viewport_size().width);
                    moved
                        .update(cx, |this, cx| {
                            // Released where no mouse-up reached us (e.g. outside the window).
                            if event.dragging() {
                                let x = f32::from(event.position.x);
                                this.resize_right_sidebar(window_width, x, cx)
                            } else {
                                this.end_right_resize(cx)
                            }
                        })
                        .ok();
                });
                let released = this.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released
                            .update(cx, |this, cx| this.end_right_resize(cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_0();
        div()
            .id("right-sidebar-resize")
            .group("right-sidebar-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-1.0))
            .w(px(RESIZE_STRIP))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation(); // not a click in the sidebar
                    this.right_sidebar.resizing = true;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(1.0))
                    .w(px(DIVIDER_ACTIVE))
                    .bg(hsla(ui.accent))
                    .when(!resizing, |line| {
                        line.invisible()
                            .group_hover("right-sidebar-edge", |style| style.visible())
                    }),
            )
            .child(follow)
            .into_any_element()
    }

    /// The right sidebar's left edge follows the mouse (it ends at the window's right).
    pub(crate) fn resize_right_sidebar(
        &mut self,
        window_width: f32,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        let width = width_for_edge(window_width, x);
        if width != self.right_sidebar.width {
            self.right_sidebar.width = width;
            cx.notify();
        }
    }

    fn end_right_resize(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.right_sidebar.resizing) {
            self.save(); // once per drag, not on every move
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::decode;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_file_without_the_right_sidebar_opens_it_closed_and_300_wide() {
        let saved = decode(r#"{"version":2,"sessions":[{}]}"#).expect("loads");
        let mut state = RightSidebarState::new();
        state.hidden = false;
        state.width = 410.0;
        state.load(&saved);
        assert!(state.hidden);
        assert_eq!(state.width, 300.0);
        // As saved: shown, and a width out of range comes back in it.
        let saved =
            decode(r#"{"version":2,"right_sidebar_hidden":false,"right_sidebar_width":1200.0}"#)
                .expect("loads");
        state.load(&saved);
        assert!(!state.hidden);
        assert_eq!(state.width, 560.0);
    }

    #[test]
    fn its_width_stays_between_220_and_560() {
        assert_eq!(clamp(Some(100.0)), 220.0);
        assert_eq!(clamp(Some(900.0)), 560.0);
        assert_eq!(clamp(Some(333.0)), 333.0);
        assert_eq!(clamp(None), 300.0);
        assert_eq!(clamp(Some(f32::NAN)), 300.0);
    }

    #[test]
    fn its_left_edge_follows_the_pointer() {
        // In a 1280 pt window, the edge at x = 980 makes it 300 wide; 80 left, 80 wider.
        assert_eq!(width_for_edge(1280.0, 980.0), 300.0);
        assert_eq!(width_for_edge(1280.0, 900.0), 380.0);
        // Never past its bounds.
        assert_eq!(width_for_edge(1280.0, 1270.0), 220.0);
        assert_eq!(width_for_edge(1280.0, 10.0), 560.0);
    }

    #[test]
    fn a_local_terminal_folder_is_its_report_else_the_kernels_else_where_it_started() {
        let report = Some(PathBuf::from("/a/reported"));
        let cwd = || Some(PathBuf::from("/a/cwd"));
        let project = Some(PathBuf::from("/a/project"));
        let got = resolve_folder(report.clone(), false, cwd, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/reported")));
        let got = resolve_folder(None, false, cwd, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/cwd")));
        let got = resolve_folder(None, false, || None, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/project")));
        assert_eq!(resolve_folder(None, false, || None, None), None);
    }

    #[test]
    fn an_ssh_terminal_folder_is_only_what_it_reported() {
        // The kernel's answer and the start folder are this Mac's, not the far machine's.
        let asked = std::cell::Cell::new(false);
        let cwd = || {
            asked.set(true);
            Some(PathBuf::from("/local/cwd"))
        };
        let project = Some(PathBuf::from("/local/project"));
        assert_eq!(resolve_folder(None, true, cwd, project.clone()), None);
        assert!(!asked.get(), "no kernel lookup for an ssh terminal");
        let got = resolve_folder(Some(PathBuf::from("/srv/app")), true, || None, project);
        assert_eq!(got.as_deref(), Some(Path::new("/srv/app")));
    }

    #[test]
    fn it_refreshes_on_a_new_terminal_or_every_two_seconds() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // Never looked up, or for another terminal: now.
        assert!(lookup_due(None, 7, now));
        assert!(lookup_due(Some((6, now)), 7, now));
        // The same terminal: once 2 s have passed.
        assert!(!lookup_due(
            Some((7, now)),
            7,
            now + Duration::from_millis(1999)
        ));
        assert!(lookup_due(Some((7, now)), 7, now + REFRESH));
    }
}
