//! The status bar (M11i): a `STATUS_BAR` bar along the window's bottom edge, on the canvas, about the
//! followed terminal (`right_sidebar.followed`: the focused one, or the last one shown).
//!
//! Left: its mark and a word (`Working · 4m`, `Needs input · 2m`, `Done · 1m`, `Running`,
//! `Ready`, `Idle`), the branch (an SSH one's host), the folder. Right: the TCP ports its
//! programs listen on as chips (at most 3, then `+N`; a click opens `http://localhost:<port>`),
//! then the update pill (moved here from above the sidebar's footer).
//!
//! Short of room, pieces go in a fixed order (`fit_status_bar`): the update pill's version, the
//! ports into one `N ports` chip, the branch cut to 8 characters, the folder's start, the
//! word, the folder, the branch. The mark and the update pill always show.
//!
//! The ports come from one `lsof` (`procs::listening_ports`) over the shell's descendants in the
//! programs' last `ps` table, off the UI thread, at most every `PORTS_REFRESH`, only while a
//! program runs in a local followed terminal and the window can be seen; a failed `lsof` waits
//! `PORTS_RETRY` (`Ports`).
//!
//! `status_word`, `word_text`, `fit_status_bar`, `port_chips` and `Ports` are pure and tested
//! here; the drawing is checked by the `status-bar` ui-shots.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, ClickEvent, Context, FontWeight, HighlightStyle, MouseButton,
    MouseDownEvent, SharedString, StyledText, TextStyle, Window, div, prelude::*, px,
};

use crate::procs::ProgramState;
use crate::sidebar::tabular;
use crate::sidebar_toggle::Tip;
use crate::status::{Status, age_label};
use crate::status_mark;
use crate::tab_bar::{Glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{STATUS_BAR, STATUS_MARK};
use crate::updater;
use crate::workspace::Workspace;

/// How often the followed terminal's listening ports are looked up again, at most.
pub const PORTS_REFRESH: Duration = Duration::from_secs(5);
/// The wait after a failed (or missing, or slow) `lsof`.
pub const PORTS_RETRY: Duration = Duration::from_secs(30);
/// How often the refresh looks again while a lookup runs (capped at 2 s), so the next lookup
/// lands `PORTS_REFRESH` after its answer.
pub const PORTS_ASKING: Duration = Duration::from_secs(1);

/// Between a status word and its age (`Working · 4m`), and a waiting row's status and question.
pub const SEPARATOR: &str = " · ";

/// The space at each end of the bar, and between its items.
pub const PAD: f32 = 16.0;
pub const ITEM_GAP: f32 = 14.0;
/// The bar's text size (M11al §7; not `text::META`, plan-time call 8).
pub const TEXT: f32 = 12.0;
/// The status mark's box, and the space between it and the word.
pub const MARK: f32 = 10.0;
pub const MARK_GAP: f32 = 6.0;
/// The branch glyph's box, and the space between it and the branch.
pub const BRANCH_GLYPH: f32 = 12.0;
pub const BRANCH_GAP: f32 = 4.0;
/// A cut branch keeps this many characters, then `…`.
pub const BRANCH_SHORT: usize = 8;
/// A port chip: its height, side padding, corners, and the space between chips.
pub const CHIP: f32 = 16.0;
pub const CHIP_PAD: f32 = 6.0;
pub const CHIP_RADIUS: f32 = 4.0;
pub const CHIP_GAP: f32 = 4.0;
/// At most this many port chips, then a `+N` one.
pub const MAX_CHIPS: usize = 3;
/// The update pill: its height, corners, package glyph, the glyph's space before the words,
/// and the room at each end inside it.
pub const PILL: f32 = 16.0;
pub const PILL_RADIUS: f32 = 8.0;
pub const PILL_ICON: f32 = 12.0;
pub const PILL_GAP: f32 = 6.0;
pub const PILL_PAD: f32 = 8.0;
/// The usage strip (M11ap): an agent's logo, the gap between its parts, the gap between agents.
pub const USAGE_LOGO: f32 = 12.0;
pub const USAGE_GAP: f32 = 4.0;
pub const USAGE_AGENT_GAP: f32 = 10.0;
/// The usage popover opens after the pointer rests on the strip this long, and closes after it
/// has left the strip and the popover this long (Orca-style, 2026-11-22).
pub const USAGE_OPEN_DELAY: Duration = Duration::from_millis(150);
pub const USAGE_CLOSE_DELAY: Duration = Duration::from_millis(250);
/// Its colours, fixed whatever the theme (M11e's blue, as asked): the fill, hovered, and its
/// words; the version part (` · v0.3.2`) is the words' white at this opacity.
pub(crate) const UPDATE_FILL: Rgb = Rgb {
    r: 0x1F,
    g: 0x6F,
    b: 0xEB,
};
pub(crate) const UPDATE_HOVER: Rgb = Rgb {
    r: 0x1A,
    g: 0x5F,
    b: 0xCC,
};
pub(crate) const UPDATE_TEXT: Rgb = Rgb {
    r: 0xFF,
    g: 0xFF,
    b: 0xFF,
};
pub(crate) const UPDATE_VERSION_ALPHA: f32 = 0.75;
/// The bar's room is measured this much short, so what the layout rounds up never pushes the
/// update pill past the bar's end.
const SLACK: f32 = 4.0;

/// The word after the status mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusWord {
    Working,
    NeedsInput,
    Done,
    /// Idle, a program other than the shell running there.
    Running,
    /// Idle at the shell's prompt.
    Ready,
    /// Idle, what runs there not known.
    Idle,
}

/// The word for a terminal in `status` running `program`.
pub fn status_word(status: Status, program: &ProgramState) -> StatusWord {
    match (status, program) {
        (Status::Working, _) => StatusWord::Working,
        (Status::Waiting, _) => StatusWord::NeedsInput,
        (Status::Done, _) => StatusWord::Done,
        (Status::Idle, ProgramState::Running(_)) => StatusWord::Running,
        (Status::Idle, ProgramState::Prompt) => StatusWord::Ready,
        (Status::Idle, ProgramState::Unknown) => StatusWord::Idle,
    }
}

/// The word as the bar says it: `Cultivating` (working), `Needs input` and `Done` with how long (`age`, as a
/// sidebar row's: `4m`) after ` · `; the idle words alone.
pub fn word_text(word: StatusWord, age: Option<Duration>) -> String {
    let news = match word {
        StatusWord::Working => "Cultivating",
        StatusWord::NeedsInput => "Needs input",
        StatusWord::Done => "Done",
        StatusWord::Running => return "Running".into(),
        StatusWord::Ready => return "Ready".into(),
        StatusWord::Idle => return "Idle".into(),
    };
    match age {
        Some(age) => format!("{news}{SEPARATOR}{}", age_label(age)),
        None => news.into(),
    }
}

/// How wide each piece is drawn (0: it isn't there), measured as the bar draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measured {
    /// The word after the mark (the mark itself is always `MARK`).
    pub status_word: f32,
    /// The branch with its glyph, whole and cut to `BRANCH_SHORT` characters.
    pub branch: f32,
    pub branch_cut: f32,
    /// The folder, whole and at its shortest (`…` and its own name).
    pub folder: f32,
    pub folder_min: f32,
    /// The port chips (at most 3 and `+N`), and the one `N ports` chip.
    pub ports: f32,
    pub ports_collapsed: f32,
    /// The update pill with its version, and without.
    pub update_wide: f32,
    pub update_narrow: f32,
    /// The usage strip (M11ap): every agent with its reset, without, and the tightest alone.
    pub usage_full: f32,
    pub usage_short: f32,
    pub usage_one: f32,
}

/// The usage strip as shown (M11ap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageShown {
    Full,
    /// No resets.
    Short,
    /// The tightest agent alone, its percent only.
    One,
    Hidden,
}

/// The ports as shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortsShown {
    /// A chip each (at most 3, then `+N`).
    All,
    /// One `N ports` chip.
    Collapsed,
    None,
}

/// A piece of text as shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    Full,
    /// The branch cut to `BRANCH_SHORT` characters; the folder its start cut to the room left.
    Short,
    Hidden,
}

/// What the bar shows, fitted to its room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub update_wide: bool,
    pub ports: PortsShown,
    pub branch: Cut,
    pub folder: Cut,
    pub status_word: bool,
    pub usage: UsageShown,
}

/// Everything shown, nothing cut.
const ALL: Shown = Shown {
    update_wide: true,
    ports: PortsShown::All,
    branch: Cut::Full,
    folder: Cut::Full,
    status_word: true,
    usage: UsageShown::Full,
};

/// How wide the bar's content is with `shown` (its end padding not included): the mark, then
/// each piece there with `ITEM_GAP` before it (the word `MARK_GAP`), at least `ITEM_GAP`
/// between the left side and the right. A cut folder counts at its shortest.
pub fn needed(m: &Measured, shown: &Shown) -> f32 {
    // Pieces not there (0 wide) take no gap either.
    let after = |gap: f32, width: f32| if width > 0.0 { gap + width } else { 0.0 };
    let word = if shown.status_word {
        m.status_word
    } else {
        0.0
    };
    let branch = match shown.branch {
        Cut::Full => m.branch,
        Cut::Short => m.branch_cut,
        Cut::Hidden => 0.0,
    };
    let folder = match shown.folder {
        Cut::Full => m.folder,
        Cut::Short => m.folder_min.min(m.folder),
        Cut::Hidden => 0.0,
    };
    let left = MARK + after(MARK_GAP, word) + after(ITEM_GAP, branch) + after(ITEM_GAP, folder);
    let ports = match shown.ports {
        PortsShown::All => m.ports,
        PortsShown::Collapsed => m.ports_collapsed,
        PortsShown::None => 0.0,
    };
    let update = if shown.update_wide {
        m.update_wide
    } else {
        m.update_narrow
    };
    let usage = match shown.usage {
        UsageShown::Full => m.usage_full,
        UsageShown::Short => m.usage_short,
        UsageShown::One => m.usage_one,
        UsageShown::Hidden => 0.0,
    };
    // The right side's pieces there, `ITEM_GAP` apart: the usage strip, the ports, the update.
    let parts = [usage, ports, update];
    let there = parts.iter().filter(|w| **w > 0.0).count();
    let right = parts.iter().sum::<f32>() + ITEM_GAP * there.saturating_sub(1) as f32;
    left + after(ITEM_GAP, right)
}

/// What fits in `room` (the bar's width less its end padding): everything if it can; else
/// pieces go in this order until the rest fits: the update pill's version, the ports into one
/// `N ports` chip, the usage strip's resets (M11ap), the branch cut to 8 characters, the
/// folder's start (down to its own name), the usage strip down to its tightest agent, the word,
/// the folder, the branch, the usage strip, and last the ports chip. The mark and the update
/// pill (its short form) always show.
pub fn fit_status_bar(m: &Measured, room: f32) -> Shown {
    let mut shown = ALL;
    let steps: [fn(&mut Shown); 11] = [
        |s| s.update_wide = false,
        |s| s.ports = PortsShown::Collapsed,
        |s| s.usage = UsageShown::Short,
        |s| s.branch = Cut::Short,
        |s| s.folder = Cut::Short,
        |s| s.usage = UsageShown::One,
        |s| s.status_word = false,
        |s| s.folder = Cut::Hidden,
        |s| s.branch = Cut::Hidden,
        |s| s.usage = UsageShown::Hidden,
        |s| s.ports = PortsShown::None,
    ];
    // With no usage strip (nothing read), its steps change nothing: skipped.
    let usage_steps = [2, 5, 9];
    for (i, step) in steps.into_iter().enumerate() {
        if needed(m, &shown) <= room {
            break;
        }
        if m.usage_full == 0.0 && usage_steps.contains(&i) {
            continue;
        }
        step(&mut shown);
    }
    shown
}

/// The room a cut folder (`shown.folder` is `Short`) has in `room`: what the rest leaves.
pub fn folder_room(m: &Measured, shown: &Shown, room: f32) -> f32 {
    let rest = Shown {
        folder: Cut::Hidden,
        ..*shown
    };
    room - needed(m, &rest) - ITEM_GAP
}

/// The port chips' words (`:3000`), at most `MAX_CHIPS`, and how many more there are (the `+N`
/// chip) if any.
pub fn port_chips(ports: &[u16]) -> (Vec<String>, Option<usize>) {
    let chips = ports
        .iter()
        .take(MAX_CHIPS)
        .map(|p| format!(":{p}"))
        .collect();
    let more = ports.len().checked_sub(MAX_CHIPS).filter(|&n| n > 0);
    (chips, more)
}

/// The one chip all the ports collapse into: `6 ports` (`1 port`).
pub fn ports_collapsed(count: usize) -> String {
    match count {
        1 => "1 port".into(),
        n => format!("{n} ports"),
    }
}

/// The wait before the next ports lookup after one that went `ok` (`PORTS_REFRESH`) or failed
/// (`PORTS_RETRY`).
pub fn ports_wait(ok: bool) -> Duration {
    if ok { PORTS_REFRESH } else { PORTS_RETRY }
}

/// The followed terminal's listening ports, as last looked up, and when they're next due.
#[derive(Default)]
pub struct Ports {
    /// The terminal they're for (another followed: none shown until its own lookup).
    of: Option<u64>,
    found: Vec<u16>,
    /// When the next lookup for `of` is due.
    next: Option<Instant>,
    /// The terminal a lookup is running for.
    asking: Option<u64>,
    /// Nothing is looked up (the UI screenshots set what they show).
    pub frozen: bool,
    /// Whether the 5 s refresh is running (`Workspace::tick_ports`).
    pub ticking: bool,
}

impl Ports {
    /// Terminal `id`'s ports, as last found (none for another terminal's).
    pub fn of(&self, id: u64) -> &[u16] {
        if self.of == Some(id) {
            &self.found
        } else {
            &[]
        }
    }

    /// Whether terminal `id`'s ports are looked up `now`: at once for a newly followed
    /// terminal, else when the wait after the last lookup is over; never while one runs or
    /// when frozen.
    pub fn due(&self, id: u64, now: Instant) -> bool {
        let waited = self.of != Some(id) || self.next.is_none_or(|next| now >= next);
        !self.frozen && self.asking.is_none() && waited
    }

    /// A lookup for `id` starts (another terminal's ports are let go).
    pub fn ask(&mut self, id: u64) {
        self.asking = Some(id);
        if self.of != Some(id) {
            self.of = Some(id);
            self.found.clear();
            self.next = None;
        }
    }

    /// The lookup for `id` answered `found` (none: `lsof` failed) at `now`; true when what is
    /// shown changed.
    pub fn take(&mut self, id: u64, found: Option<Vec<u16>>, now: Instant) -> bool {
        self.asking = None;
        if self.of != Some(id) {
            return false;
        }
        self.next = Some(now + ports_wait(found.is_some()));
        let found = found.unwrap_or_default();
        let changed = self.found != found;
        self.found = found;
        changed
    }

    /// Forget the ports (no program runs in the followed terminal any more); true when some
    /// were shown.
    pub fn clear(&mut self) -> bool {
        let shown = !self.found.is_empty();
        self.of = None;
        self.found.clear();
        self.next = None;
        shown
    }

    /// How long until the refresh should look again, from `now`: until the next lookup is due
    /// (`PORTS_ASKING` while one runs, so it wakes soon after the answer).
    pub fn wait(&self, now: Instant) -> Duration {
        if self.asking.is_some() {
            return PORTS_ASKING;
        }
        self.next
            .map_or(Duration::ZERO, |next| next.saturating_duration_since(now))
    }

    /// How long the refresh (`Workspace::tick_ports`) sleeps from `now`: `wait`, never sooner
    /// than a second nor later than `PORTS_RETRY`.
    pub fn sleep(&self, now: Instant) -> Duration {
        self.wait(now).clamp(Duration::from_secs(1), PORTS_RETRY)
    }

    /// Whether an answer (`take`, which said whether what is shown `changed`) redraws at once:
    /// what is shown changed, or the terminal followed `now` (`followed`) isn't the one
    /// answered and its own lookup is due, so the redraw asks it at once (one lookup runs at a
    /// time, so it waited for this answer).
    pub fn redraw_after_answer(&self, changed: bool, followed: Option<u64>, now: Instant) -> bool {
        changed || followed.is_some_and(|id| self.due(id, now))
    }

    /// Show `found` for terminal `id` (the screenshots; nothing is looked up there).
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(&mut self, id: u64, found: Vec<u16>) {
        self.of = Some(id);
        self.found = found;
    }
}

/// What the status bar says about its terminal (`Workspace::status_bar_facts`).
#[derive(Clone, Debug, PartialEq)]
pub struct BarFacts {
    pub status: Status,
    pub word: StatusWord,
    /// The word as said (`Working · 4m`).
    pub text: String,
    /// A program other than the shell runs there (an idle mark's dot, not its ring).
    pub running: bool,
    /// The branch checked out in its folder; for an SSH terminal, `ssh <host>`.
    pub branch: Option<String>,
    pub ssh: bool,
    /// Its folder (an SSH one's only as its shell reported it).
    pub folder: Option<PathBuf>,
    /// The TCP ports its programs listen on.
    pub ports: Vec<u16>,
}

/// The status bar's mark for a followed agent tab (its `aN`), apart from its tab's.
pub(crate) const STATUS_BAR_AGENT_MARK: &str = "status-bar-agent";

/// `branch` as a cut branch shows it: its first `BRANCH_SHORT` characters, then `…` (an SSH
/// terminal's `ssh` kept before its cut host).
pub fn cut_branch(branch: &str) -> String {
    let (lead, name) = branch
        .strip_prefix("ssh ")
        .map_or(("", branch), |host| ("ssh ", host));
    if name.chars().count() <= BRANCH_SHORT {
        return branch.to_string();
    }
    let kept: String = name.chars().take(BRANCH_SHORT).collect();
    format!("{lead}{kept}\u{2026}")
}

/// How wide `text` is drawn at the bar's size (the UI font, `weight`, tabular figures when
/// `tabular`).
pub(crate) fn text_width(
    text: &str,
    weight: FontWeight,
    tabular_figures: bool,
    window: &Window,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let mut font = TextStyle::default().font();
    font.weight = weight;
    if tabular_figures {
        font.features = tabular();
    }
    let run = gpui::TextRun {
        len: text.len(),
        font,
        color: hsla(UPDATE_TEXT),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(TEXT),
        &[run],
        None,
    );
    f32::from(line.width())
}

/// The tooltip for the `+N` and `N ports` chips: every port.
fn all_ports_tip(ports: &[u16]) -> String {
    let list: Vec<String> = ports.iter().map(|p| format!(":{p}")).collect();
    format!("Listening on {}", list.join(", "))
}

/// The room the status bar takes at the window's bottom: `STATUS_BAR`, or none while
/// Settings → General → Status Bar is off (M11v part 3). The footer's popovers sit above it.
pub(crate) fn status_bar_room(cx: &gpui::App) -> f32 {
    if cx.global::<crate::appearance::Appearance>().status_bar {
        STATUS_BAR
    } else {
        0.0
    }
}

impl Workspace {
    /// What the status bar says about the followed terminal (`right_sidebar.followed`): none
    /// with no terminal followed.
    pub(crate) fn status_bar_facts(&self) -> Option<BarFacts> {
        // An agent tab followed: its status, its folder and the branch there.
        if let Some(slot) = self
            .right_sidebar
            .followed_agent
            .and_then(|tab| self.agents.get(&tab))
        {
            let word = status_word(slot.status, &ProgramState::Prompt);
            let news = matches!(
                word,
                StatusWord::Working | StatusWord::NeedsInput | StatusWord::Done
            );
            let folder = self.right_sidebar.agent_folder.as_deref();
            return Some(BarFacts {
                status: slot.status,
                word,
                text: word_text(word, news.then(|| slot.since.elapsed())),
                running: false,
                branch: folder.and_then(|f| self.places.branch(f)).map(String::from),
                ssh: false,
                folder: folder.map(Path::to_path_buf),
                ports: Vec::new(),
            });
        }
        let id = self.right_sidebar.followed?;
        let s = self.sessions.items().iter().find(|s| s.id == id)?;
        let host = s.ssh.as_ref().map(|t| t.host.as_str());
        // What runs over SSH is on the other machine: not looked up.
        let program = match host {
            Some(_) => ProgramState::Unknown,
            None => self.programs.state(id),
        };
        let word = status_word(s.status, &program);
        let news = matches!(
            word,
            StatusWord::Working | StatusWord::NeedsInput | StatusWord::Done
        );
        let age = news.then(|| s.status_since.elapsed());
        let found = self.places.folder(id);
        let folder = match host {
            Some(_) => found,
            None => found.or(s.project.as_deref()),
        };
        let branch = match host {
            Some(host) => Some(format!("ssh {host}")),
            None => folder.and_then(|f| self.places.branch(f)).map(String::from),
        };
        Some(BarFacts {
            status: s.status,
            word,
            text: word_text(word, age),
            running: matches!(program, ProgramState::Running(_)),
            branch,
            ssh: host.is_some(),
            folder: folder.map(Path::to_path_buf),
            ports: self.ports.of(id).to_vec(),
        })
    }

    /// Open `http://localhost:<port>` in the followed terminal's workspace's browser tab, or a
    /// new one split right of its pane (`browser_in`, as you asked for it).
    fn open_port(&mut self, port: u16, window: &mut Window, cx: &mut Context<Self>) {
        let place = self
            .right_sidebar
            .followed
            .and_then(|id| self.model.terminal_tab(id))
            .and_then(|tab| self.model.locate(tab));
        let url = format!("http://localhost:{port}");
        let opened = place.and_then(|(ws, pane)| self.browser_in(ws, pane, &url, true, window, cx));
        if opened.is_none() {
            eprintln!("[fuxx] status bar: no browser tab could be opened for {url}");
        }
    }

    /// The pointer came onto or left the strip or its popover (Orca-style, your word,
    /// 2026-11-22): on the strip a moment (`USAGE_OPEN_DELAY`) opens the list; off both a moment
    /// (`USAGE_CLOSE_DELAY`) closes it — the gap between them never closes it on the way.
    pub(crate) fn usage_hovered(
        &mut self,
        on_strip: Option<bool>,
        on_popover: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        if let Some(on) = on_strip {
            self.usage_hover.0 = on;
        }
        if let Some(on) = on_popover {
            self.usage_hover.1 = on;
        }
        self.usage_hover_gen += 1;
        let generation = self.usage_hover_gen;
        let (strip, popover) = self.usage_hover;
        let (wait, opening) = if strip && !self.usage_open {
            (USAGE_OPEN_DELAY, true)
        } else if !strip && !popover && self.usage_open {
            (USAGE_CLOSE_DELAY, false)
        } else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                if this.usage_hover_gen != generation {
                    return;
                }
                this.usage_open = opening;
                if !opening {
                    this.usage_detail = None;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The usage popover (M11ap; Orca-style since 2026-11-22): above the strip, its right edge on
    /// the strip's. A short list — "Usage", when it was last read and a refresh (turning while a
    /// read runs), then a row per agent: its logo, name, a small bar and the tightest limit's
    /// percent and reset. Hovering a row shows that agent's limits beside the list, in the box
    /// panel's rows. It closes as the pointer leaves it and the strip, on Esc, or a press outside.
    fn usage_popover(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        use crate::agent_catalog::{Catalogs, Lookup};
        use crate::icons::{Icon, icon};
        use crate::usage_strip::StripValue;
        use crate::usage_view::{UsageInk, heading, limit_row, quiet, top_row};
        use gpui::{
            Animation, AnimationExt, Transformation, anchored, deferred, percentage, point,
        };
        let theme = cx.global::<crate::appearance::Appearance>().theme;
        let ink = UsageInk::of(&theme);
        let catalogs = cx.try_global::<Catalogs>();
        let now_unix = crate::agent_box::now_unix();
        let offset = crate::cli::local_utc_offset(now_unix as u64 * 1000);
        let agents = crate::usage_strip::strip_agents(catalogs, now_unix);
        let reading = agents.iter().any(|a| {
            catalogs.is_some_and(|c| matches!(c.usage_last(a.name), Some(Lookup::Loading)))
        });
        let updated = agents
            .iter()
            .filter_map(|a| catalogs.and_then(|c| c.usage_ready_at(a.name)))
            .max()
            .map(|at| {
                let ago = std::time::Instant::now()
                    .saturating_duration_since(at)
                    .as_secs();
                if ago < 60 {
                    "Updated just now".to_string()
                } else {
                    format!("Updated {}m ago", ago / 60)
                }
            });
        let spin = icon(Icon::Reload, px(14.), hsla(ui.text_dim));
        let refresh_icon: AnyElement = if reading {
            spin.with_animation(
                "usage-refresh-spin",
                Animation::new(std::time::Duration::from_millis(800)).repeat(),
                |svg, t| svg.with_transformation(Transformation::rotate(percentage(t))),
            )
            .into_any_element()
        } else {
            spin.into_any_element()
        };
        let hover = ui.panel_hover;
        let header = div()
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(ui.text))
                    .child("Usage"),
            )
            .children(updated.map(|u| {
                div()
                    .text_size(px(crate::ui_tokens::text::META))
                    .text_color(hsla(ui.text_dim))
                    .child(u)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .id("usage-refresh")
                    .size(px(22.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!reading, |d| {
                        d.cursor_pointer()
                            .hover(move |s| s.bg(hsla(hover)))
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                let home = crate::workspace::home_dir().unwrap_or_default();
                                for &p in crate::providers::all() {
                                    crate::agent_catalog::load_usage(p, home.clone(), cx);
                                }
                            }))
                    })
                    .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip("Refresh usage".into())).into() })
                    .child(refresh_icon),
            );
        // A row per agent: the tightest limit at a glance; hovering it shows the detail.
        self.usage_rows_at.borrow_mut().clear();
        let rows: Vec<AnyElement> = agents
            .iter()
            .map(|agent| {
                let name = agent.name;
                let (pct, reset, used) = match agent.value {
                    StripValue::Used { used, reset_in } => (
                        format!("{used}%"),
                        reset_in.map(crate::usage_strip::short_reset),
                        Some(used),
                    ),
                    StripValue::Reading => ("\u{2026}".into(), None, None),
                };
                let (bar, pct_ink) = used.map_or((ink.bar_ok, ui.text_dim), |u| ink.band(u));
                let shown = self.usage_detail == Some(name);
                let at = self.usage_rows_at.clone();
                let measure = gpui::canvas(
                    move |b, _, _| at.borrow_mut().push((name, b)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full();
                div()
                    .id(SharedString::from(format!("usage-row-{name}")))
                    .relative()
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .font_features(tabular())
                    .when(shown, |d| d.bg(hsla(hover)))
                    .hover(move |s| s.bg(hsla(hover)))
                    .child(measure)
                    .children(
                        agent
                            .logo
                            .and_then(|l| crate::logos::image(l, 14., cx))
                            .map(|img| gpui::img(img).size(px(14.))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(ui.text))
                            .child(agent.label),
                    )
                    .children(reset.map(|r| {
                        div()
                            .text_size(px(crate::ui_tokens::text::META))
                            .text_color(hsla(ui.text_dim))
                            .child(r)
                    }))
                    .children(used.map(|u| {
                        div()
                            .w(px(40.))
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(hsla(ink.track))
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(f32::from(u.min(100)) / 100.))
                                    .rounded(px(2.))
                                    .bg(hsla(bar)),
                            )
                    }))
                    .child(
                        div()
                            .min_w(px(36.))
                            .flex()
                            .justify_end()
                            .text_color(hsla(pct_ink))
                            .child(pct),
                    )
                    .child(icon(Icon::ChevronRight, px(12.), hsla(ui.text_dim)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.usage_detail != Some(name) {
                            this.usage_detail = Some(name);
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            })
            .collect();
        let list = crate::overlay_panel::shell(&ui, crate::agent_box::USAGE_WIDTH)
            .id("usage-popover")
            .p(px(4.))
            .gap(px(0.))
            .child(header)
            .children(rows);
        // The hovered agent's limits beside the list.
        let detail = self
            .usage_detail
            .and_then(|name| agents.iter().find(|a| a.name == name))
            .map(|agent| {
                let failed = catalogs
                    .is_some_and(|c| matches!(c.usage_last(agent.name), Some(Lookup::Failed(_))));
                let mut body: Vec<AnyElement> = vec![
                    div()
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .children(
                            agent
                                .logo
                                .and_then(|l| crate::logos::image(l, 14., cx))
                                .map(|img| gpui::img(img).size(px(14.))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(ui.text))
                                .child(agent.label),
                        )
                        .when(failed, |d| {
                            d.child(
                                div()
                                    .text_size(px(crate::ui_tokens::text::META))
                                    .text_color(hsla(ui.text_dim))
                                    .child("Couldn't refresh"),
                            )
                        })
                        .into_any_element(),
                ];
                let live = catalogs
                    .and_then(|c| c.live_usage(agent.name))
                    .map(Lookup::Ready);
                let found = live
                    .as_ref()
                    .or_else(|| catalogs.and_then(|c| c.usage_last(agent.name)));
                let limits = catalogs.map_or(&[][..], |c| c.limits(agent.name));
                for row in crate::agent_box::usage_rows(found, limits, None, now_unix, offset) {
                    use crate::agent_box::UsageRow;
                    body.push(match row {
                        UsageRow::Heading(name) => heading(name, &ui),
                        UsageRow::Quiet(text) => quiet(text, &ui),
                        UsageRow::Text(text) => match crate::agent_box::top_list(&text) {
                            Some((title, list)) => div()
                                .flex()
                                .flex_col()
                                .child(heading(title, &ui))
                                .children(list.into_iter().map(|(n, p)| top_row(n, p, &ui)))
                                .into_any_element(),
                            None => quiet(text, &ui),
                        },
                        UsageRow::Limit {
                            name,
                            used_pct,
                            resets,
                            note,
                        } => limit_row(name, used_pct, resets, note, &ui, &ink).into_any_element(),
                    });
                }
                crate::overlay_panel::shell(&ui, crate::agent_box::USAGE_WIDTH)
                    .id("usage-detail")
                    .p(px(4.))
                    .gap(px(0.))
                    .child(
                        div()
                            .id("usage-detail-body")
                            .flex()
                            .flex_col()
                            .max_h(px(crate::agent_box::USAGE_MAX_H))
                            .overflow_y_scroll()
                            .children(body),
                    )
            });
        // The panels' box blocks the mouse, not each panel: a panel that did would hide the
        // pointer from this box's hover, and the popover closed under a resting pointer (your
        // report, 2026-11-22).
        let panels = div()
            .id("usage-panels")
            .occlude()
            .flex()
            .items_end()
            .gap(px(6.))
            .children(detail)
            .child(list)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.usage_hovered(None, Some(*hovered), cx)
            }))
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // A press on the strip is its own, not "outside".
                if this
                    .usage_strip_at
                    .get()
                    .is_some_and(|b| b.contains(&event.position))
                {
                    return;
                }
                this.usage_open = false;
                this.usage_detail = None;
                cx.notify();
            }));
        div()
            .absolute()
            .top_0()
            .right_0()
            .child(deferred(
                anchored()
                    .anchor(gpui::Anchor::BottomRight)
                    .offset(point(px(0.), px(-6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(panels),
            ))
            .into_any_element()
    }

    /// The status bar (see the module): 22 pt along the window's bottom, on the canvas. Each
    /// piece is measured as drawn, then fitted to the window's width (`fit_status_bar`).
    pub(crate) fn render_status_bar(
        &mut self,
        ui: UiColors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        crate::chrome_cache::bump(crate::chrome_cache::Counted::StatusBar);
        let facts = self.status_bar_facts();
        let update = cx.try_global::<updater::Updates>().map(|u| u.state.clone());
        let wide = update.as_ref().and_then(|s| updater::button(s, true));
        let narrow = update.as_ref().and_then(|s| updater::button(s, false));
        let home = self.places.home.clone();

        // Measured as drawn.
        let plain = |t: &str| text_width(t, FontWeight::NORMAL, false, window);
        let pill = |b: &Option<updater::UpdateButton>| {
            b.as_ref().map_or(0.0, |b| {
                let words = text_width(&b.text(), FontWeight::MEDIUM, false, window);
                PILL_PAD + PILL_ICON + PILL_GAP + words + PILL_PAD
            })
        };
        let chip = |t: &str| CHIP_PAD + text_width(t, FontWeight::NORMAL, true, window) + CHIP_PAD;
        let ports = facts.as_ref().map_or(&[][..], |f| &f.ports[..]);
        let (chips, more) = port_chips(ports);
        let more_text = more.map(|n| format!("+{n}"));
        let chip_words: Vec<&str> = chips
            .iter()
            .map(String::as_str)
            .chain(more_text.as_deref())
            .collect();
        let chips_width = chip_words.iter().map(|t| chip(t)).sum::<f32>()
            + CHIP_GAP * chip_words.len().saturating_sub(1) as f32;
        let collapsed_text = ports_collapsed(ports.len());
        let branch_text = facts.as_ref().and_then(|f| f.branch.clone());
        let branch_lead = |ssh: bool| if ssh { 0.0 } else { BRANCH_GLYPH + BRANCH_GAP };
        let ssh = facts.as_ref().is_some_and(|f| f.ssh);
        let branch_width = |t: &str| branch_lead(ssh) + plain(t);
        let folder_path = facts.as_ref().and_then(|f| f.folder.clone());
        let folder_home = if ssh { None } else { home.as_deref() };
        let folder_full = folder_path
            .as_deref()
            .map(|f| crate::sidebar::home_relative(f, folder_home));
        // At its shortest: `…` and its own name (`root_path_line` never goes shorter).
        let folder_least = folder_path
            .as_deref()
            .map(|f| crate::right_sidebar::root_path_line(f, folder_home, |_| false));
        // The usage strip (M11ap): each agent's logo, percent and reset, measured as drawn.
        let now_unix = crate::agent_box::now_unix();
        let agents = crate::usage_strip::strip_agents(
            cx.try_global::<crate::agent_catalog::Catalogs>(),
            now_unix,
        );
        let texts: Vec<(String, Option<String>, Option<u8>)> = agents
            .iter()
            .map(|a| match a.value {
                crate::usage_strip::StripValue::Used { used, reset_in } => (
                    format!("{used}%"),
                    reset_in.map(crate::usage_strip::short_reset),
                    Some(used),
                ),
                crate::usage_strip::StripValue::Reading => ("\u{2026}".into(), None, None),
            })
            .collect();
        let tab_w = |t: &str| text_width(t, FontWeight::NORMAL, true, window);
        let agent_w = |(pct, reset, _): &(String, Option<String>, Option<u8>), resets: bool| {
            USAGE_LOGO
                + USAGE_GAP
                + tab_w(pct)
                + reset
                    .as_deref()
                    .filter(|_| resets)
                    .map_or(0.0, |r| USAGE_GAP + tab_w(r))
        };
        let strip_w = |resets: bool| {
            if texts.is_empty() {
                return 0.0;
            }
            2.0 * CHIP_PAD
                + texts.iter().map(|t| agent_w(t, resets)).sum::<f32>()
                + USAGE_AGENT_GAP * (texts.len() - 1) as f32
        };
        // The tightest agent alone: its percent only.
        let tightest_ix = texts
            .iter()
            .enumerate()
            .max_by_key(|(_, t)| t.2.unwrap_or(0))
            .map(|(i, _)| i);
        let usage_widths = (
            strip_w(true),
            strip_w(false),
            tightest_ix.map_or(0.0, |i| 2.0 * CHIP_PAD + agent_w(&texts[i], false)),
        );
        let m = Measured {
            status_word: facts.as_ref().map_or(0.0, |f| plain(&f.text)),
            branch: branch_text.as_deref().map_or(0.0, &branch_width),
            branch_cut: branch_text
                .as_deref()
                .map_or(0.0, |b| branch_width(&cut_branch(b))),
            folder: folder_full.as_deref().map_or(0.0, plain),
            folder_min: folder_least.as_deref().map_or(0.0, plain),
            ports: if ports.is_empty() { 0.0 } else { chips_width },
            // (A lone port's chip is narrower than `1 port`: it never collapses.)
            ports_collapsed: if ports.is_empty() {
                0.0
            } else {
                chip(&collapsed_text).min(chips_width)
            },
            update_wide: pill(&wide),
            update_narrow: pill(&narrow),
            usage_full: usage_widths.0,
            usage_short: usage_widths.1,
            usage_one: usage_widths.2,
        };
        let bar_width = f32::from(window.viewport_size().width);
        let room = bar_width - 2.0 * PAD - SLACK;
        let shown = fit_status_bar(&m, room);

        // The left side: the mark and the word, the branch, the folder.
        let left = facts.as_ref().map(|f| {
            let id = self.right_sidebar.followed;
            let session = id.and_then(|id| self.sessions.items().iter().find(|s| s.id == id));
            let agent = self
                .right_sidebar
                .followed_agent
                .and_then(|tab| self.agents.get(&tab));
            let mark = match (agent, session) {
                (Some(slot), _) if f.status != Status::Idle => {
                    let size = MARK * crate::sidebar::mark_size(f.status) / STATUS_MARK;
                    self.status_mark(
                        STATUS_BAR_AGENT_MARK,
                        slot.id,
                        slot.status,
                        None,
                        ui,
                        ui.sidebar_background,
                        size,
                        cx,
                    )
                }
                (Some(_), _) => status_mark::idle_mark(false, &ui).into_any_element(),
                (None, Some(session)) if f.status != Status::Idle => {
                    // Its halo's box scaled as the sidebar's is to its mark.
                    let size = MARK * crate::sidebar::mark_size(f.status) / STATUS_MARK;
                    self.session_mark_sized(
                        "status-bar",
                        session,
                        ui,
                        ui.sidebar_background,
                        size,
                        cx,
                    )
                }
                (None, _) => status_mark::idle_mark(f.running, &ui).into_any_element(),
            };
            let word_color = if f.word == StatusWord::NeedsInput {
                ui.ring_waiting
            } else {
                ui.text_dim
            };
            let branch = match shown.branch {
                Cut::Full => branch_text.clone(),
                Cut::Short => branch_text.as_deref().map(cut_branch),
                Cut::Hidden => None,
            };
            let folder = match shown.folder {
                Cut::Full => folder_full.clone(),
                Cut::Short => folder_path.as_deref().map(|path| {
                    let fits_in = folder_room(&m, &shown, room);
                    crate::right_sidebar::root_path_line(path, folder_home, |t| plain(t) <= fits_in)
                }),
                Cut::Hidden => None,
            };
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(ITEM_GAP))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(MARK_GAP))
                        .child(
                            div()
                                .flex_none()
                                .size(px(MARK))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(mark),
                        )
                        .when(shown.status_word, |line| {
                            line.child(
                                div()
                                    .flex_none()
                                    .whitespace_nowrap()
                                    .text_color(hsla(word_color))
                                    .child(f.text.clone()),
                            )
                        }),
                )
                .when_some(branch, |line, branch| {
                    let ssh = f.ssh;
                    line.child(
                        div()
                            .id("status-branch")
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(BRANCH_GAP))
                            .text_color(hsla(ui.text_dim))
                            .when(!ssh, |b| b.child(glyph(Glyph::Branch, ui.text_dim)))
                            .child(div().flex_none().whitespace_nowrap().child(branch))
                            // A local branch opens the branch switcher (an SSH host does nothing).
                            .when(!ssh, |b| {
                                b.cursor_pointer()
                                    .hover(move |s| s.text_color(hsla(ui.text)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        |_: &MouseDownEvent, _, cx| cx.stop_propagation(),
                                    )
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(terminal) = this.right_sidebar.followed {
                                            this.open_branch_switcher(terminal, window, cx);
                                        }
                                    }))
                            }),
                    )
                })
                .when_some(folder, |line, folder| {
                    line.child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(hsla(ui.text_faint))
                            .child(folder),
                    )
                })
        });

        // The right side: the port chips, then the update pill.
        let chip_el = |id: SharedString, words: String, tip: String| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .h(px(CHIP))
                .px(px(CHIP_PAD))
                .rounded(px(CHIP_RADIUS))
                .bg(hsla(ui.row_hover))
                .font_features(tabular())
                .text_color(hsla(ui.text_dim))
                .whitespace_nowrap()
                .child(words)
                .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() })
        };
        let port_list: Vec<u16> = ports.to_vec();
        let collapses = chip(&collapsed_text) < chips_width;
        let ports_el = match shown.ports {
            PortsShown::None => None,
            _ if ports.is_empty() => None,
            PortsShown::Collapsed if collapses => Some(
                chip_el(
                    "status-ports".into(),
                    collapsed_text,
                    all_ports_tip(&port_list),
                )
                .into_any_element(),
            ),
            PortsShown::All | PortsShown::Collapsed => {
                let mut row = div().flex_none().flex().items_center().gap(px(CHIP_GAP));
                for (&port, words) in port_list.iter().zip(&chips) {
                    let id: SharedString = format!("status-port-{port}").into();
                    row = row.child(
                        chip_el(id, words.clone(), format!("Open localhost:{port}"))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hsla(ui.row_active)))
                            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open_port(port, window, cx)
                            })),
                    );
                }
                if let Some(more) = more_text {
                    row = row.child(chip_el(
                        "status-ports-more".into(),
                        more,
                        all_ports_tip(&port_list),
                    ));
                }
                Some(row.into_any_element())
            }
        };
        let label = if shown.update_wide { wide } else { narrow };
        let pill_el = label.map(|label| {
            let clickable = label.clickable;
            let words = label.text();
            // The version in the same white, faded (a highlight's colour is blended over the
            // text's, so white at 75% over white would still be white).
            let faint = label.version.as_ref().map(|v| {
                let start = label.words.len();
                let style = HighlightStyle {
                    fade_out: Some(1.0 - UPDATE_VERSION_ALPHA),
                    ..Default::default()
                };
                (start..start + v.len(), style)
            });
            div()
                .id("status-update")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(PILL_GAP))
                .h(px(PILL))
                .px(px(PILL_PAD))
                .rounded(px(PILL_RADIUS))
                .bg(hsla(UPDATE_FILL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(UPDATE_TEXT))
                .child(glyph(Glyph::Package, UPDATE_TEXT))
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .child(StyledText::new(words).with_highlights(faint)),
                )
                .when(clickable, |pill| {
                    pill.cursor_pointer()
                        .hover(|style| style.bg(hsla(UPDATE_HOVER)))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            updater::show_offer(this, window, cx)
                        }))
                })
                .into_any_element()
        });
        let usage_el = (shown.usage != UsageShown::Hidden && !agents.is_empty()).then(|| {
            let theme = cx.global::<crate::appearance::Appearance>().theme;
            let on = [ui.sidebar_background, ui.row_hover, ui.row_active];
            let pct_ink = |used: Option<u8>| match used.map(crate::agent_top::band) {
                Some(crate::agent_top::Band::Caution) => {
                    crate::theme::readable(theme.palette[3], &on, crate::ui_tokens::TEXT_MIN)
                }
                Some(crate::agent_top::Band::Critical) => {
                    crate::theme::readable(theme.palette[1], &on, crate::ui_tokens::TEXT_MIN)
                }
                _ => ui.text_dim,
            };
            let shown_ix: Vec<usize> = match shown.usage {
                UsageShown::One => tightest_ix.into_iter().collect(),
                _ => (0..agents.len()).collect(),
            };
            let resets = shown.usage == UsageShown::Full;
            let open = self.usage_open;
            let at = self.usage_strip_at.clone();
            let measure = gpui::canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .size_full();
            let strip = div()
                .id("status-usage")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(USAGE_AGENT_GAP))
                .h(px(CHIP))
                .px(px(CHIP_PAD))
                .rounded(px(CHIP_RADIUS))
                .relative()
                .font_features(tabular())
                .whitespace_nowrap()
                .child(measure)
                .when(open, |d| d.bg(hsla(ui.row_active)))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(if open { ui.row_active } else { ui.row_hover })))
                .children(shown_ix.into_iter().map(|i| {
                    let (pct, reset, used) = texts[i].clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(USAGE_GAP))
                        .children(
                            agents[i]
                                .logo
                                .and_then(|l| crate::logos::image(l, USAGE_LOGO, cx))
                                .map(|img| gpui::img(img).size(px(USAGE_LOGO))),
                        )
                        .child(div().text_color(hsla(pct_ink(used))).child(pct))
                        .when_some(reset.filter(|_| resets), |d, r| {
                            d.child(div().text_color(hsla(ui.text_dim)).child(r))
                        })
                }))
                // Orca-style (2026-11-22): the pointer on it opens the list after a moment; a
                // press opens it at once.
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.usage_hovered(Some(*hovered), None, cx)
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.usage_open = true;
                        cx.notify();
                    }),
                );
            // No tooltip: the list says it (your words, 2026-11-22: "remove the tooltip").
            div()
                .relative()
                .flex_none()
                .child(strip)
                .when(open, |d| d.child(self.usage_popover(ui, cx)))
                .into_any_element()
        });
        let right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(ITEM_GAP))
            .children(usage_el)
            .children(ports_el)
            .children(pill_el);

        div()
            .id("status-bar")
            .flex_none()
            .flex()
            .items_center()
            .w_full()
            .h(px(STATUS_BAR))
            .px(px(PAD))
            .overflow_hidden()
            .text_size(px(TEXT))
            .children(left)
            // At least `ITEM_GAP` between the sides (`fit_status_bar` keeps it).
            .child(div().flex_1().min_w(px(ITEM_GAP)))
            .child(right)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_update_pill_is_blue_rounded_and_readable() {
        let rgb = |r, g, b| Rgb { r, g, b };
        assert_eq!(UPDATE_FILL, rgb(0x1F, 0x6F, 0xEB));
        assert_eq!(UPDATE_HOVER, rgb(0x1A, 0x5F, 0xCC));
        assert_eq!(UPDATE_TEXT, rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(UPDATE_VERSION_ALPHA, 0.75);
        assert_eq!(
            (PILL, PILL_RADIUS, PILL_ICON, PILL_GAP),
            (16.0, 8.0, 12.0, 6.0)
        );
        // White text meets the 4.5:1 floor on both fills, whatever the theme.
        for fill in [UPDATE_FILL, UPDATE_HOVER] {
            let ratio = crate::ui_tokens::contrast(UPDATE_TEXT, fill);
            assert!(ratio >= 4.5, "white on {fill:?} is {ratio:.2}");
        }
    }

    #[test]
    fn a_long_branch_is_cut_to_eight_characters() {
        assert_eq!(cut_branch("main"), "main");
        assert_eq!(cut_branch("feature/"), "feature/");
        assert_eq!(cut_branch("feature/rows"), "feature/\u{2026}");
        // An SSH terminal's host is cut, `ssh` kept.
        assert_eq!(cut_branch("ssh build-box"), "ssh build-bo\u{2026}");
        assert_eq!(cut_branch("ssh pi"), "ssh pi");
    }

    #[test]
    fn the_word_follows_the_status_and_when_idle_what_runs() {
        let running = ProgramState::Running("npm run dev".into());
        for program in [&running, &ProgramState::Prompt, &ProgramState::Unknown] {
            assert_eq!(status_word(Status::Working, program), StatusWord::Working);
            assert_eq!(
                status_word(Status::Waiting, program),
                StatusWord::NeedsInput
            );
            assert_eq!(status_word(Status::Done, program), StatusWord::Done);
        }
        assert_eq!(status_word(Status::Idle, &running), StatusWord::Running);
        assert_eq!(
            status_word(Status::Idle, &ProgramState::Prompt),
            StatusWord::Ready
        );
        assert_eq!(
            status_word(Status::Idle, &ProgramState::Unknown),
            StatusWord::Idle
        );
    }

    #[test]
    fn the_words_say_how_long_only_while_there_is_news() {
        let min = |m: u64| Some(Duration::from_secs(m * 60));
        assert_eq!(
            word_text(StatusWord::Working, min(4)),
            "Cultivating \u{b7} 4m"
        );
        assert_eq!(
            word_text(StatusWord::NeedsInput, min(2)),
            "Needs input \u{b7} 2m"
        );
        assert_eq!(word_text(StatusWord::Done, min(1)), "Done \u{b7} 1m");
        assert_eq!(
            word_text(StatusWord::Working, Some(Duration::from_secs(12))),
            "Cultivating \u{b7} 12s"
        );
        assert_eq!(word_text(StatusWord::Working, None), "Cultivating");
        assert_eq!(word_text(StatusWord::Running, min(9)), "Running");
        assert_eq!(word_text(StatusWord::Ready, min(9)), "Ready");
        assert_eq!(word_text(StatusWord::Idle, None), "Idle");
    }

    /// A long branch, a deep folder, five ports and an update with its version.
    const LONG: Measured = Measured {
        status_word: 80.0,
        branch: 260.0,
        branch_cut: 70.0,
        folder: 240.0,
        folder_min: 60.0,
        ports: 160.0,
        ports_collapsed: 50.0,
        update_wide: 170.0,
        update_narrow: 120.0,
        usage_full: 0.0,
        usage_short: 0.0,
        usage_one: 0.0,
    };

    /// M11ap: the usage strip gives way in the lead designer's order — its resets after the
    /// ports collapse, one agent after the folder is cut, gone just before the ports.
    #[test]
    fn the_usage_strip_gives_way_in_its_order() {
        let m = Measured {
            usage_full: 200.0,
            usage_short: 120.0,
            usage_one: 50.0,
            ..LONG
        };
        let at = |room: f32| fit_status_bar(&m, room);
        // Everything: the strip whole, before the ports (+ 14).
        assert_eq!(needed(&m, &ALL), 982.0 + 200.0 + 14.0);
        assert_eq!(at(1196.0).usage, UsageShown::Full);
        // Short of room: the version, the ports collapse, then the strip's resets.
        let s = at(1196.0 - 50.0 - 110.0 - 1.0);
        assert_eq!(
            (s.update_wide, s.ports, s.usage),
            (false, PortsShown::Collapsed, UsageShown::Short)
        );
        assert_eq!(s.branch, Cut::Full, "the branch waits");
        // Much less: one agent once the folder is cut.
        let tight = at(400.0);
        assert!(matches!(tight.usage, UsageShown::One | UsageShown::Hidden));
        assert_eq!(tight.folder, Cut::Hidden);
        // Least of all: gone before the ports.
        let least = at(10.0);
        assert_eq!(
            (least.usage, least.ports),
            (UsageShown::Hidden, PortsShown::None)
        );
    }

    #[test]
    fn with_room_for_everything_everything_shows() {
        // mark 10 + 6 + word 80, + 14 + branch 260, + 14 + folder 240 = 624 on the left;
        // ports 160 + 14 + update 170 = 344 on the right; 14 between: 982.
        assert_eq!(needed(&LONG, &ALL), 982.0);
        assert_eq!(fit_status_bar(&LONG, 982.0), ALL);
        assert_eq!(fit_status_bar(&LONG, 5000.0), ALL);
    }

    #[test]
    fn short_of_room_pieces_go_in_the_stated_order() {
        let narrow = Shown {
            update_wide: false,
            ..ALL
        };
        let collapsed = Shown {
            ports: PortsShown::Collapsed,
            ..narrow
        };
        let branch_cut = Shown {
            branch: Cut::Short,
            ..collapsed
        };
        let folder_cut = Shown {
            folder: Cut::Short,
            ..branch_cut
        };
        let no_word = Shown {
            status_word: false,
            ..folder_cut
        };
        let no_folder = Shown {
            folder: Cut::Hidden,
            ..no_word
        };
        let no_branch = Shown {
            branch: Cut::Hidden,
            ..no_folder
        };
        let no_ports = Shown {
            ports: PortsShown::None,
            ..no_branch
        };
        // Each step's width: the one before less what it saves.
        let steps = [
            (narrow, 932.0),     // the version: 170 → 120
            (collapsed, 822.0),  // the ports: 160 → 50
            (branch_cut, 632.0), // the branch: 260 → 70
            (folder_cut, 452.0), // the folder: 240 → its least, 60
            (no_word, 366.0),    // the word and its gap: 86
            (no_folder, 292.0),  // the cut folder and its gap: 74
            (no_branch, 208.0),  // the cut branch and its gap: 84
            (no_ports, 144.0),   // the ports chip and its gap: 64
        ];
        let mut above = 982.0;
        for (shown, width) in steps {
            assert_eq!(needed(&LONG, &shown), width, "{shown:?}");
            // Just short of the step before: this step.
            assert_eq!(
                fit_status_bar(&LONG, above - 1.0),
                shown,
                "room {}",
                above - 1.0
            );
            assert_eq!(fit_status_bar(&LONG, width), shown, "room {width}");
            above = width;
        }
        // The mark and the short update pill always show, however little room there is.
        assert_eq!(
            needed(&LONG, &no_ports),
            MARK + ITEM_GAP + LONG.update_narrow
        );
        assert_eq!(fit_status_bar(&LONG, 50.0), no_ports);
        assert_eq!(fit_status_bar(&LONG, 0.0), no_ports);
    }

    #[test]
    fn pieces_that_are_not_there_take_no_room() {
        // A terminal at a prompt outside a repo, no ports, nothing to update.
        let m = Measured {
            status_word: 40.0,
            folder: 100.0,
            folder_min: 50.0,
            ..Measured::default()
        };
        assert_eq!(needed(&m, &ALL), MARK + MARK_GAP + 40.0 + ITEM_GAP + 100.0);
        assert_eq!(fit_status_bar(&m, 170.0), ALL);
        // No terminal at all: the update pill alone (and the mark's room, kept).
        let m = Measured {
            update_wide: 170.0,
            update_narrow: 120.0,
            ..Measured::default()
        };
        assert_eq!(needed(&m, &ALL), MARK + ITEM_GAP + 170.0);
    }

    #[test]
    fn a_cut_folder_gets_the_room_the_rest_leaves() {
        let shown = fit_status_bar(&LONG, 500.0);
        assert_eq!(shown.folder, Cut::Short);
        // 500 less the rest (452 less the folder's least 60 and its gap 14 = 378) and its gap.
        assert_eq!(folder_room(&LONG, &shown, 500.0), 500.0 - 378.0 - ITEM_GAP);
        assert!(folder_room(&LONG, &shown, 500.0) >= LONG.folder_min);
    }

    #[test]
    fn at_most_three_port_chips_then_how_many_more() {
        assert_eq!(
            port_chips(&[3000, 5173, 8080, 9000, 9229]),
            (
                vec![":3000".into(), ":5173".into(), ":8080".into()],
                Some(2)
            )
        );
        assert_eq!(
            port_chips(&[3000, 5173, 8080]),
            (vec![":3000".into(), ":5173".into(), ":8080".into()], None)
        );
        assert_eq!(port_chips(&[4000]), (vec![":4000".into()], None));
        assert_eq!(port_chips(&[]), (Vec::<String>::new(), None));
        assert_eq!(ports_collapsed(6), "6 ports");
        assert_eq!(ports_collapsed(1), "1 port");
    }

    #[test]
    fn a_failed_lsof_waits_30_s_and_a_good_one_5_s() {
        assert_eq!(ports_wait(false), Duration::from_secs(30));
        assert_eq!(ports_wait(true), Duration::from_secs(5));
        assert_eq!(ports_wait(true), PORTS_REFRESH);

        let t = Instant::now();
        let s = Duration::from_secs;
        let mut ports = Ports::default();
        assert!(ports.due(7, t), "never looked up: at once");
        ports.ask(7);
        assert!(!ports.due(7, t), "not while it runs");
        assert_eq!(ports.wait(t), PORTS_ASKING, "a short sleep while it runs");
        // It fails: nothing shown, and the next try 30 s on.
        assert!(!ports.take(7, None, t));
        assert_eq!(ports.of(7), &[] as &[u16]);
        assert!(!ports.due(7, t + s(5)));
        assert!(!ports.due(7, t + s(29)));
        assert!(ports.due(7, t + s(30)));
        assert_eq!(ports.wait(t + s(10)), s(20));
        // It works again: shown, and back to every 5 s.
        ports.ask(7);
        assert!(ports.take(7, Some(vec![3000]), t + s(30)));
        assert_eq!(ports.of(7), &[3000]);
        assert!(!ports.due(7, t + s(34)));
        assert!(ports.due(7, t + s(35)));
        // The same answer again: nothing changed.
        ports.ask(7);
        assert!(!ports.take(7, Some(vec![3000]), t + s(35)));
    }

    #[test]
    fn another_terminal_is_looked_up_at_once_and_shows_only_its_own_ports() {
        let t = Instant::now();
        let mut ports = Ports::default();
        ports.ask(7);
        ports.take(7, Some(vec![3000]), t);
        assert_eq!(ports.of(8), &[] as &[u16], "not 7's");
        assert!(ports.due(8, t), "a newly followed terminal: at once");
        ports.ask(8);
        assert_eq!(ports.of(7), &[] as &[u16], "7's are let go");
        ports.take(8, Some(vec![5173]), t);
        assert_eq!(ports.of(8), &[5173]);
        // Cleared (its program ended): nothing shown, and it's looked up at once again.
        assert!(ports.clear());
        assert!(!ports.clear());
        assert_eq!(ports.of(8), &[] as &[u16]);
        assert!(ports.due(8, t));
        // Frozen (the screenshots): never.
        ports.frozen = true;
        assert!(!ports.due(9, t));
    }

    #[test]
    fn an_answer_for_a_terminal_no_longer_asked_about_is_dropped() {
        let t = Instant::now();
        let mut ports = Ports::default();
        ports.ask(7);
        ports.clear();
        assert!(!ports.take(7, Some(vec![3000]), t));
        assert_eq!(ports.of(7), &[] as &[u16]);
    }

    /// The refresh (`Workspace::tick_ports`) sleeps `Ports::sleep` between looks: the next
    /// lookup lands 5 s after the last answer, not a clamp's second later.
    #[test]
    fn the_next_lookup_lands_5_s_after_the_answer() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        assert!(PORTS_ASKING <= Duration::from_secs(1));
        let mut ports = Ports::default();
        ports.ask(7);
        // Asleep while lsof runs; it answers 300 ms in.
        let first = ports.sleep(t);
        assert!(ports.take(7, Some(vec![3000]), t + ms(300)));
        let mut now = t + first;
        while !ports.due(7, now) {
            now += ports.sleep(now);
        }
        assert_eq!(now, t + ms(300) + PORTS_REFRESH);
        // Never sooner than a second, never later than the retry.
        let mut idle = Ports::default();
        assert_eq!(idle.sleep(t), Duration::from_secs(1));
        idle.ask(7);
        idle.take(7, None, t);
        assert_eq!(idle.sleep(t), PORTS_RETRY);
    }

    /// Note 60 §7: the followed terminal's ports are looked up at once when it changes. One
    /// lookup runs at a time, so a terminal followed while another's runs is asked when that
    /// answer comes: the answer redraws, and the redraw asks.
    #[test]
    fn an_answer_for_a_terminal_no_longer_followed_asks_the_followed_one_at_once() {
        let t = Instant::now();
        let mut ports = Ports::default();
        ports.ask(7);
        // Terminal 8 is followed while 7's lookup runs: not asked yet.
        assert!(!ports.due(8, t));
        let changed = ports.take(7, Some(vec![]), t);
        assert!(!changed, "nothing shown changed");
        assert!(ports.due(8, t));
        assert!(
            ports.redraw_after_answer(changed, Some(8), t),
            "8 is asked at once"
        );
        // Still 7 (or nothing followed): nothing to redraw for.
        assert!(!ports.redraw_after_answer(changed, Some(7), t));
        assert!(!ports.redraw_after_answer(changed, None, t));
        assert!(
            ports.redraw_after_answer(true, Some(7), t),
            "what's shown changed"
        );
    }
}
