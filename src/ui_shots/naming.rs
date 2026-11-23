//! Naming: the default name, names from titles and presets, renames by typing, the rename
//! box, a banner's click and a background `session.new`.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "api-background",
        run: api_background,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/workspace_api.rs",
            "src/workspace_browser.rs",
            "src/browser_tab.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "banner-click",
        run: banner_click,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/native_notify.rs",
            "src/daemon_client.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "default-name",
        run: default_name,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/groups.rs", "src/tab_bar.rs", "src/sidebar.rs"],
    },
    Scene {
        name: "names",
        run: names,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/groups.rs",
            "src/tab_bar.rs",
            "src/sidebar.rs",
            "src/name_input.rs",
            "src/layout_model.rs",
            "src/browser_tab.rs",
            "src/workspace_browser.rs",
            "src/presets.rs",
            "src/ssh_target.rs",
            "src/procs.rs",
        ],
    },
    Scene {
        name: "renames",
        run: renames,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/name_input.rs",
            "src/tab_bar.rs",
            "src/sidebar.rs",
            "src/groups.rs",
            "src/procs.rs",
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "rename-box",
        run: rename_box_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/name_input.rs", "src/sidebar.rs"],
    },
];

/// `fuxx new --background` (and the MCP `background`) while a browser tab has the focus: once
/// the new terminal has opened (taking the focus, as any new session does), the focus goes back
/// to the browser tab, not to the new shell. Panics unless the browser tab is focused again,
/// with GPUI's keyboard.
fn api_background() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| browser_workspace(false, window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let page = cx.read(|cx| {
            let ws = ws.read(cx);
            assert!(ws.focused_browser_tab().is_some(), "api-background: a page");
            ws.model.focused_tab()
        });
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // What `session.new` remembers, then its session opening (beside the page).
                let previous = ws.focus_to_give_back();
                ws.open_shot_tab("helper", std::path::Path::new(API), "", window, cx);
                let id = ws.sessions.active().map(|s| s.id);
                assert!(
                    id.is_some(),
                    "api-background: the new terminal took the focus"
                );
                ws.api_opened(id, previous, None, true, window, cx)
                    .expect("opened");
            })
        })
        .expect("a background session.new");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            let ws = ws.read(cx);
            assert_eq!(
                ws.model.focused_tab(),
                page,
                "api-background: the page has the focus back"
            );
            let view = ws.focused_browser_tab().expect("the page's tab");
            assert!(
                gpui::Focusable::focus_handle(view.read(cx), cx).contains_focused(window, cx),
                "api-background: with the keyboard"
            );
        })
        .expect("read the focus");
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(560.0)), build, act);
}

/// New terminals with no name of yours (M11c Task 3b): **╭∩╮** (`groups::DEFAULT_NAME`), the
/// user's signature, whether in a folder or not (the one shown draws it in its tab and row).
/// Shot in Mocha (both themes with `--all-themes`; `default-name-*.png`) to see that the glyphs
/// draw (no missing-glyph boxes); panics unless both are called that, as `Session::label` (the
/// scripting API's name).
fn default_name() {
    use std::cell::RefCell;
    use std::rc::Rc;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let api = Some(std::path::Path::new(API));
                ws.open_unnamed_shot_session(api, SHELL_TEXT, window, cx);
                ws.open_unnamed_shot_session(None, SHELL_TEXT, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, _window: AnyWindowHandle| {
            let ws = slotted(&acting);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let named = |folder: bool| {
                    let s = ws
                        .sessions
                        .items()
                        .iter()
                        .find(|s| s.project.is_some() == folder);
                    let s = s.expect("both opened");
                    (ws.terminal_label(s.id).to_string(), s.label().to_string())
                };
                let default = crate::groups::DEFAULT_NAME.to_string();
                assert_eq!(
                    named(true),
                    (default.clone(), default.clone()),
                    "default-name: ╭∩╮ for a terminal in a folder too"
                );
                assert_eq!(
                    named(false),
                    (default.clone(), default),
                    "default-name: ╭∩╮ for a terminal with no folder"
                );
            });
            drop(ws);
            acting.borrow_mut().take();
        };
        let path = shoot_with(
            "default-name",
            slug,
            &theme,
            size(px(960.0), px(360.0)),
            build,
            act,
        );
        println!("{}", path.display());
    }
}

/// The folders of `names`'s terminals.
pub(super) const FUXX_CLI: &str = "/Users/me/code/fuxx-cli";

const INVITEE: &str = "/Users/me/w/invitee";

const SHOT_HOME: &str = "/Users/me";

/// What devbox's remote shell reported (OSC 7).
const DEVBOX_SRC: &str = "/home/me/src";

/// A shell theme's title (OSC 2): the name of a terminal you haven't named.
const SHELL_TITLE: &str = "me@mac:~/code/fuxx-cli";

/// The title Claude Code sets (OSC 2): it names its terminal (the program renames it).
const AGENT_TITLE: &str = "✳ Claude Code";

/// The preset `names`'s at-home terminal was started from.
const SHOT_PRESET: &str = "logs";

/// The terminals of `names`'s shown workspace, by id.
#[derive(Clone, Copy, Default)]
struct NameIds {
    claude: u64,
    prompt: u64,
}

/// `names`'s window. The shown workspace: claude (running `claude` in fuxx-cli, its title
/// Claude Code's, focused), a dev server (`npm run dev` in invitee, no title) and api (yours,
/// running `claude`, with Claude Code's title) in the left pane; the dashboard page, a shell at
/// its prompt in fuxx-cli (with a shell theme's title), one at home started from the `logs`
/// preset and an SSH terminal to devbox in the right. Then a workspace each for: `npm run dev`
/// in invitee, a prompt in fuxx-cli (titled), a prompt at home, api (titled too), and devbox. Programs and
/// folders are set, not looked up.
fn names_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> (Workspace, NameIds) {
    use std::path::Path;
    use std::time::Instant;

    use crate::browser_tab::BarState;
    use crate::layout_model::TabKind;
    use crate::procs::ProgramState::{Prompt, Running};
    use crate::workspace::At;

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let (fuxx, invitee, home) = (
        Some(Path::new(FUXX_CLI)),
        Some(Path::new(INVITEE)),
        Some(Path::new(SHOT_HOME)),
    );
    let running = |name: &str| Running(name.into());
    let claude = ws.open_shot_at(None, fuxx, AGENT_TEXT, At::New, window, cx);
    let left = ws.model.focused_pane().expect("a pane");
    let npm = ws.open_shot_at(None, invitee, SHELL_TEXT, At::Tab(left), window, cx);
    let api = ws.open_shot_at(Some("api"), fuxx, AGENT_TEXT, At::Tab(left), window, cx);
    let url = Some(PAGE_URL.to_string());
    let page = TabKind::Browser { url: url.clone() };
    let opened = ws.model.split(left, Axis::Row, page).expect("split");
    ws.sync();
    let view = ws.make_browser_tab(opened.tab, url, window, cx);
    view.update(cx, |view, _| {
        view.show_state(BarState {
            url: PAGE_URL.into(),
            title: PAGE_TITLE.into(),
            loading: false,
            can_back: false,
            can_forward: false,
        })
    });
    ws.browser_titles.insert(opened.tab, PAGE_TITLE.into());
    let right = At::Tab(opened.pane);
    let prompt = ws.open_shot_at(None, fuxx, SHELL_TEXT, right, window, cx);
    let at_home = ws.open_shot_at(None, home, SHELL_TEXT, right, window, cx);
    let devbox = ws.open_shot_at(None, None, SHELL_TEXT, right, window, cx);
    // A workspace of its own each.
    let rows = [
        (None, invitee, Some(running("npm"))),
        (None, fuxx, Some(Prompt)),
        (None, home, Some(Prompt)),
        (Some("api"), fuxx, Some(running("claude"))),
        (None, None, None),
    ];
    let mut opened_rows = Vec::new();
    for (name, folder, state) in rows {
        let id = ws.open_shot_at(name, folder, SHELL_TEXT, At::New, window, cx);
        if let Some(state) = state {
            ws.programs.set_for_shots(id, state);
        }
        opened_rows.push(id);
    }
    let (row_prompt, row_api, row_devbox) = (opened_rows[1], opened_rows[3], opened_rows[4]);
    for (id, state) in [
        (claude, running("claude")),
        (npm, running("npm")),
        (api, running("claude")),
        (prompt, Prompt),
        (at_home, Prompt),
    ] {
        ws.programs.set_for_shots(id, state);
    }
    for id in [devbox, row_devbox] {
        let i = ws.index_of(id).expect("opened");
        ws.sessions.get_mut(i).expect("opened").ssh = Some(crate::ssh_target::SshTarget {
            host: "devbox".into(),
            tmux: false,
            tmux_name: None,
        });
        ws.places
            .set_folder(id, Some(DEVBOX_SRC.into()), Instant::now());
    }
    for (id, title) in [
        (prompt, SHELL_TITLE),
        (row_prompt, SHELL_TITLE),
        (claude, AGENT_TITLE),
        (api, AGENT_TITLE),
        (row_api, AGENT_TITLE),
    ] {
        let i = ws.index_of(id).expect("opened");
        ws.sessions.get_mut(i).expect("opened").title = Some(title.into());
    }
    let i = ws.index_of(at_home).expect("opened");
    ws.sessions.get_mut(i).expect("opened").preset = Some(crate::presets::Preset {
        name: SHOT_PRESET.into(),
        command: "tail -f log".into(),
    });
    ws.sync();
    ws.select(ws.index_of(claude).expect("claude"), window, cx);
    (ws, NameIds { claude, prompt })
}

/// Names (as before M11i, the user's call of 2026-11-09): every tab, row and the window title
/// say a terminal's `Session::label`: your name, else the program's own title (a program that
/// sets one renames its terminal), else its preset's name, else ╭∩╮, whatever runs there and
/// wherever it is. In Mocha (both themes with `--all-themes`; `names-*.png`). Panics unless:
/// - the tabs read Claude Code's title, ╭∩╮ (the dev server: no title), `api` (yours wins
///   over the title), the page's title, the shell theme's title, `logs` (the preset) and ╭∩╮
///   (devbox); the rows read the shown workspace's focused terminal's name and ╭∩╮, the shell
///   theme's title, ╭∩╮, `api` and ╭∩╮; the window title the shown workspace's;
/// - each terminal's name is its `Session::label` (the scripting API's);
/// - only a name that isn't its title has the title under it in a tooltip (api's tab and row);
/// - Enter on an untouched automatic name, in the tab's rename box and the row's, keeps the
///   terminal and the workspace automatic, also when the title changes while the box is open
///   (the name then follows the new title).
fn names() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    use gpui::KeyBinding;

    use crate::groups::DEFAULT_NAME;
    use crate::sidebar::SidebarRow;
    use crate::workspace::window_title;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let ids = Rc::new(Cell::new(NameIds::default()));
        let set_ids = ids.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let (ws, ids) = names_workspace(window, cx);
                set_ids.set(ids);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = ids.get();
            let n = format!("names-{slug}");
            cx.read(|cx| {
                let ws = ws.read(cx);
                let now = Instant::now();
                let panes = ws.shown_layout().expect("a layout").leaves();
                let bars: Vec<Vec<crate::tab_bar::BarTab>> = panes
                    .iter()
                    .map(|&p| ws.pane_bar_tabs(ws.model.pane(p).expect("its pane"), now))
                    .collect();
                let titles = |bar: &[crate::tab_bar::BarTab]| -> Vec<String> {
                    bar.iter().map(|t| t.title.to_string()).collect()
                };
                assert_eq!(
                    titles(&bars[0]),
                    [AGENT_TITLE, DEFAULT_NAME, "api"],
                    "{n}: the left pane's tabs"
                );
                assert_eq!(
                    titles(&bars[1]),
                    [PAGE_TITLE, SHELL_TITLE, SHOT_PRESET, DEFAULT_NAME],
                    "{n}: the right pane's tabs"
                );
                let tips: Vec<&str> = bars
                    .iter()
                    .flatten()
                    .filter(|t| t.terminal.is_some())
                    .map(|t| t.tip.as_str())
                    .collect();
                assert_eq!(
                    tips,
                    [
                        AGENT_TITLE,
                        DEFAULT_NAME,
                        &format!("api\n{AGENT_TITLE}"),
                        SHELL_TITLE,
                        SHOT_PRESET,
                        DEFAULT_NAME,
                    ],
                    "{n}: the title is under a name only when it isn't the name"
                );
                // Every terminal's name is the scripting API's, whatever runs there.
                for s in ws.sessions.items() {
                    assert_eq!(ws.terminal_label(s.id), s.label(), "{n}: s{}", s.id);
                }
                let rows: Vec<(String, Option<String>)> = ws
                    .rows_in_view(now)
                    .into_iter()
                    .filter_map(|row| match row {
                        SidebarRow::Workspace(w) => Some((w.label, ws.workspace_tip(w.workspace))),
                        SidebarRow::Header(_) => None,
                    })
                    .collect();
                let mut labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
                labels.sort();
                // A row whose terminal has only the default name is "New Fuxxpace" (2026-11-10).
                let new = crate::groups::NEW_WORKSPACE_NAME;
                let mut want = [AGENT_TITLE, new, SHELL_TITLE, new, "api", new];
                want.sort();
                assert_eq!(labels, want, "{n}: the rows");
                let tips: Vec<&str> = rows.iter().filter_map(|(_, t)| t.as_deref()).collect();
                assert_eq!(
                    tips,
                    [format!("api\n{AGENT_TITLE}").as_str()],
                    "{n}: only api's row has a tooltip, the title under the name"
                );
                let shown = ws.model.focused_workspace().expect("shown");
                let number = ws.workspace_number(shown).expect("numbered");
                assert_eq!(
                    window_title(number, &ws.workspace_label(shown)),
                    window_title(number, AGENT_TITLE),
                    "{n}: the window title"
                );
                let i = ws.index_of(ids.prompt).expect("open");
                assert_eq!(ws.sessions.items()[i].label(), SHELL_TITLE, "{n}: API");
            });
            // Enter on an untouched automatic name: the tab's rename box, then the row's.
            cx.update(|cx| {
                cx.bind_keys([KeyBinding::new(
                    "enter",
                    crate::name_input::Confirm,
                    Some("NameInput"),
                )])
            });
            let shown_ws = cx.read(|cx| ws.read(cx).model.focused_workspace().expect("shown"));
            // Each box twice: untouched; then with the title changing while it's open (claude
            // says what it's at): Enter on the text it opened with still stores no name
            // (Settled #14), and the name follows the title.
            const NEW_TITLE: &str = "✳ Fix the tests";
            let set_title = |cx: &mut VisualTestAppContext, title: &str| {
                cx.update(|cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.index_of(ids.claude).expect("open");
                        ws.sessions.get_mut(i).expect("open").title =
                            Some(title.to_string().into());
                        cx.notify();
                    })
                });
                cx.run_until_parked();
            };
            for (tab, changes) in [(true, false), (false, false), (true, true), (false, true)] {
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        if tab {
                            ws.start_tab_rename(ids.claude, window, cx)
                        } else {
                            ws.start_rename(shown_ws, window, cx)
                        }
                    })
                })
                .expect("open the rename box");
                cx.run_until_parked();
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (_, input) = ws.renaming.as_ref().expect("the rename box is open");
                    assert_eq!(
                        input.read(cx).text(),
                        AGENT_TITLE,
                        "{n}: the box starts with the name"
                    );
                });
                if changes {
                    set_title(cx, NEW_TITLE);
                }
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let m = format!("{n} (tab: {tab}, changed while open: {changes})");
                    assert!(ws.renaming.is_none(), "{m}: Enter closed the box");
                    let i = ws.index_of(ids.claude).expect("open");
                    assert_eq!(ws.sessions.items()[i].name, None, "{m}: still automatic");
                    let w = ws.model.workspace(shown_ws).expect("shown");
                    assert_eq!(w.name, None, "{m}: the workspace still automatic");
                    let now = if changes { NEW_TITLE } else { AGENT_TITLE };
                    assert_eq!(ws.terminal_label(ids.claude).as_ref(), now, "{m}");
                    assert_eq!(ws.workspace_label(shown_ws).as_ref(), now, "{m}");
                });
                set_title(cx, AGENT_TITLE);
            }
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "names",
            slug,
            &theme,
            size(px(1440.0), px(680.0)), // every row two lines (M11r): all six in view
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// A made-up shell pid for terminal `id` in `renames`'s `ps` tables.
fn shot_shell_pid(id: u64) -> u32 {
    1000 + id as u32
}

/// What the programs' lookup does with each `ps` table (`Programs::take` and a redraw; the
/// `ps` itself, and the 3 s timer that runs it, are left out: `for_shots` freezes the lookups):
/// each of `states`' terminals at its prompt (`false`) or running `make all` (`true`), offered
/// twice so the steadying takes it.
fn programs_lookup(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    states: &[(u64, bool)],
) {
    use crate::procs::Proc;
    let proc = |pid: u32, pgid: u32, tpgid: u32, args: &str| Proc {
        pid,
        ppid: 1,
        pgid,
        tpgid,
        stat: "S".into(),
        args: args.into(),
    };
    let mut table = Vec::new();
    let mut shells = Vec::new();
    for &(id, running) in states {
        let shell = shot_shell_pid(id);
        let job = shell + 500;
        table.push(proc(
            shell,
            shell,
            if running { job } else { shell },
            "-zsh",
        ));
        if running {
            table.push(proc(job, job, job, "make all"));
        }
        shells.push((id, shell));
    }
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            ws.programs.take(Some(table.clone()), &shells);
            ws.programs.take(Some(table), &shells);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

/// The ids of `renames`'s terminals.
#[derive(Clone, Copy, Default)]
struct RenameIds {
    prompt: u64,
    running: u64,
}

/// Renaming by typing (the user's report, 2026-11-09: "I cannot rename them at all"), for a
/// terminal at its prompt and one running a program, each in a workspace of its own. Each box
/// is opened by a real gesture; a programs lookup (`programs_lookup`) runs while it's open and
/// after Enter, and the program sets a new title (OSC 2, through the terminal's own parser and
/// `TerminalEvent::TitleChanged`) between the typing and Enter:
/// - a double-click on its tab: the tab's box opens with the keyboard (with ╭∩╮: no title, no
///   preset), stays open and focused through the lookup and the title, and the typed name
///   names the terminal: the tab, the row (its workspace has no name) and the window title
///   show it, not the title;
/// - a double-click on its row (the sidebar takes the keyboard back on each press first): the
///   row's box opens with the keyboard, holds, and the typed name names the workspace: the row
///   and the window title show it, the tab the terminal's;
/// - ⌘R: the same, a second name for the workspace.
///
/// In Mocha (both themes with `--all-themes`; `renames-*.png`, the second workspace renamed).
/// Panics on the first that doesn't hold.
fn renames() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    use gpui::{
        Focusable, KeyBinding, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Point,
    };

    use crate::sidebar::SidebarRow;
    use crate::workspace::{At, RenameSession, Renaming, window_title};

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let ids = Rc::new(Cell::new(RenameIds::default()));
        let set_ids = ids.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let fuxx = Some(std::path::Path::new(FUXX_CLI));
                let prompt = ws.open_shot_at(None, fuxx, SHELL_TEXT, At::New, window, cx);
                let running = ws.open_shot_at(None, fuxx, AGENT_TEXT, At::New, window, cx);
                ws.programs.set_for_shots(prompt, ProgramState::Prompt);
                ws.programs
                    .set_for_shots(running, ProgramState::Running("claude".into()));
                ws.sync();
                set_ids.set(RenameIds { prompt, running });
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = ids.get();
            cx.update(|cx| {
                cx.bind_keys([
                    KeyBinding::new("enter", crate::name_input::Confirm, Some("NameInput")),
                    KeyBinding::new("cmd-r", RenameSession, None),
                ])
            });
            let none = Modifiers::none();
            let double_click = |cx: &mut VisualTestAppContext, at: Point<Pixels>| {
                for count in [1, 2] {
                    cx.simulate_event(
                        window,
                        MouseDownEvent {
                            position: at,
                            modifiers: none,
                            button: MouseButton::Left,
                            click_count: count,
                            first_mouse: false,
                        },
                    );
                    cx.simulate_event(
                        window,
                        MouseUpEvent {
                            position: at,
                            modifiers: none,
                            button: MouseButton::Left,
                            click_count: count,
                        },
                    );
                }
                cx.run_until_parked();
            };
            // Whether the rename box is open on `on`, with the keyboard, and its text.
            let open_on = |cx: &mut VisualTestAppContext, on: Renaming| {
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    ws.renaming
                        .as_ref()
                        .filter(|(r, _)| *r == on)
                        .map(|(_, input)| {
                            let input = input.read(cx);
                            (
                                input.focus_handle(cx).is_focused(window),
                                input.text().to_string(),
                            )
                        })
                })
                .expect("read the box")
            };
            // The program sets terminal `id`'s title (OSC 2), through the terminal's parser.
            let set_title = |cx: &mut VisualTestAppContext, id: u64, title: &str| {
                cx.update(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    let view = ws.sessions.items()[i].view.read(cx);
                    view.show_text(&format!("\x1b]2;{title}\x07"));
                });
                cx.run_until_parked();
                let got = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    ws.sessions.items()[i].title.as_ref().map(|t| t.to_string())
                });
                assert_eq!(got.as_deref(), Some(title), "the title reached the session");
            };
            // What the tab, the row and the window title say about terminal `id`'s workspace.
            let shown = |cx: &mut VisualTestAppContext, id: u64| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let now = Instant::now();
                    let pane = ws.model.focused_pane().expect("a pane");
                    let tabs = ws.pane_bar_tabs(ws.model.pane(pane).expect("its pane"), now);
                    let tab = tabs
                        .iter()
                        .find(|t| t.terminal == Some(id))
                        .expect("its tab");
                    let w = ws.model.focused_workspace().expect("shown");
                    let row = ws.rows_in_view(now).into_iter().find_map(|row| match row {
                        SidebarRow::Workspace(r) if r.workspace == w => Some(r.label),
                        _ => None,
                    });
                    let number = ws.workspace_number(w).expect("numbered");
                    (
                        tab.title.to_string(),
                        row.expect("its row"),
                        window_title(number, &ws.workspace_label(w)),
                        number,
                    )
                })
            };
            // Type `typed` into the box open on `on` (whose text is `from`), with a lookup
            // before and the title changing before Enter, then a lookup after.
            let rename = |cx: &mut VisualTestAppContext,
                          id: u64,
                          running: bool,
                          on: Renaming,
                          from: &str,
                          typed: &str,
                          m: &str| {
                assert_eq!(
                    open_on(cx, on),
                    Some((true, from.to_string())),
                    "{m}: the box is open with the keyboard, with the name"
                );
                programs_lookup(cx, &ws, &[(id, !running)]);
                assert_eq!(
                    open_on(cx, on).map(|(focused, _)| focused),
                    Some(true),
                    "{m}: still open with the keyboard after a lookup"
                );
                cx.simulate_input(window, typed);
                set_title(cx, id, &format!("✳ {typed} title"));
                assert_eq!(
                    open_on(cx, on),
                    Some((true, typed.to_string())),
                    "{m}: still open with the keyboard and the typing after a new title"
                );
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
                programs_lookup(cx, &ws, &[(id, running)]);
                assert!(
                    cx.read(|cx| ws.read(cx).renaming.is_none()),
                    "{m}: Enter closed the box"
                );
            };
            let default = crate::groups::DEFAULT_NAME;
            for (id, running, typed, ws_clicked, ws_keyed) in [
                (ids.prompt, false, "my-shell", "shell-work", "shell-2"),
                (ids.running, true, "my-agent", "agent-work", "agent-2"),
            ] {
                let n = format!("renames-{slug} (running: {running})");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.index_of(id).expect("open");
                        ws.select(i, window, cx);
                    })
                })
                .expect("show it");
                cx.run_until_parked();
                // The tab: a double-click on it.
                let pane = shown_panes(cx, &ws)[0];
                let at = tab_bounds(cx, &ws, pane, 0).expect("laid out").center();
                double_click(cx, at);
                let m = format!("{n}, the tab");
                rename(cx, id, running, Renaming::Terminal(id), default, typed, &m);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    let name = ws.sessions.items()[i].name.as_deref();
                    assert_eq!(name, Some(typed), "{m}: the terminal is named");
                });
                let (tab, row, title, number) = shown(cx, id);
                assert_eq!(tab, typed, "{m}: the tab, not the title");
                assert_eq!(row, typed, "{m}: the row (its workspace has no name)");
                assert_eq!(title, window_title(number, typed), "{m}: the window title");

                // The row: a double-click on it, then ⌘R.
                let (w, at) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let w = ws.model.focused_workspace().expect("shown");
                    let item = ws
                        .rows_in_view(Instant::now())
                        .iter()
                        .position(|row| matches!(row, SidebarRow::Workspace(r) if r.workspace == w))
                        .expect("its row");
                    let b = ws.sidebar.scroll.bounds_for_item(item).expect("laid out");
                    (w, b.center())
                });
                for (gesture, from, typed_ws) in [
                    ("double-click", typed, ws_clicked),
                    ("cmd-r", ws_clicked, ws_keyed),
                ] {
                    let m = format!("{n}, the row by {gesture}");
                    match gesture {
                        "double-click" => double_click(cx, at),
                        _ => {
                            cx.simulate_keystrokes(window, "cmd-r");
                            cx.run_until_parked();
                        }
                    }
                    rename(cx, id, running, Renaming::Workspace(w), from, typed_ws, &m);
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let name = ws.model.workspace(w).and_then(|w| w.name.clone());
                        assert_eq!(name.as_deref(), Some(typed_ws), "{m}: the workspace");
                    });
                    let (tab, row, title, number) = shown(cx, id);
                    assert_eq!(tab, typed, "{m}: the tab keeps the terminal's name");
                    assert_eq!(row, typed_ws, "{m}: the row");
                    assert_eq!(
                        title,
                        window_title(number, typed_ws),
                        "{m}: the window title"
                    );
                }
            }
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "renames",
            slug,
            &theme,
            size(px(960.0), px(360.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Two workspaces for `banner_click`: api's, and docs's split in two: docs and notes in the left
/// pane (docs shown), tests in the right (focused). gone, a tab of the left pane, was closed.
/// api's workspace is the one shown.
fn banner_click_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
    ws.open_shot_session("docs", project, SHELL_TEXT, None, window, cx);
    ws.open_shot_tab("notes", project, AGENT_TEXT, window, cx);
    ws.open_shot_tab("gone", project, SHELL_TEXT, window, cx);
    let at = |ws: &Workspace, name: &str| ws.sessions.position(|x| x.label() == name);
    let gone = at(&ws, "gone").expect("gone");
    ws.close_at(gone, window, cx);
    ws.select(at(&ws, "docs").expect("docs"), window, cx);
    ws.open_shot_session("tests", project, SHELL_TEXT, Some(Axis::Row), window, cx);
    ws.select(at(&ws, "api").expect("api"), window, cx);
    ws
}

/// A click on one of fuxx's banners (M11c Task 2), fed in the way the click delegate feeds it:
/// through the clicks channel the workspace listens to (the delegate itself only runs from the
/// app bundle, so it can't run here). From api's workspace:
/// - a click for gone (closed since its banner), one for no terminal (a script's banner) and
///   one for notes's number from an earlier run of fuxx (numbers start again each run)
///   change nothing;
/// - a click for notes while the saved sessions are reopening shows nothing until they have,
///   then (as one with no restore running does at once) docs's workspace, its left pane and
///   notes's tab, with the keyboard;
/// - a click on a background-service banner, which the service passes on as `Show` (fed as
///   `DaemonClient::shows` hands it over), is handled the same way: held while the saved
///   sessions reopen (it opened fuxx), then notes is shown.
///
/// Shot once notes is shown (`banner-click-mocha.png`). Panics unless all of that holds.
fn banner_click() {
    use crate::layout_model::{PaneId, TabId, WorkspaceId};
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| banner_click_workspace(window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    type Shown = (Option<WorkspaceId>, Option<PaneId>, Option<TabId>);
    let shown = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| -> Shown {
        cx.read(|cx| {
            let model = &ws.read(cx).model;
            (
                model.focused_workspace(),
                model.focused_pane(),
                model.focused_tab(),
            )
        })
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let (clicks, heard) = futures::channel::mpsc::unbounded();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.listen_for_clicks(heard, window, cx))
        })
        .expect("listen for clicks");
        let (notes, gone, api) = cx.read(|cx| {
            let ws = ws.read(cx);
            let id = |name: &str| {
                ws.sessions
                    .items()
                    .iter()
                    .find(|s| s.label() == name)
                    .map(|s| s.id)
            };
            let notes = id("notes").expect("notes");
            // gone opened right after notes, so it had the next number.
            (notes, notes + 1, id("api"))
        });
        let before = shown(cx, &ws);
        assert_eq!(
            cx.read(|cx| ws.read(cx).focused_terminal()),
            api,
            "banner-click: api's workspace is shown first"
        );
        assert!(
            cx.read(|cx| ws.read(cx).index_of(gone).is_none()),
            "banner-click: gone (s{gone}) is closed"
        );

        // This workspace runs its sessions itself: this run numbers them.
        let run = crate::native_notify::this_run();
        use crate::native_notify::{About, Terminal};
        let ours = |id: u64| Some(About::Terminal(Terminal { run, id }));
        let earlier = Some(About::Terminal(Terminal {
            run: run.wrapping_add(1),
            id: notes,
        }));
        for (click, what) in [
            (ours(gone), "a closed terminal"),
            (None, "no terminal"),
            (earlier, "notes's number from an earlier run"),
        ] {
            clicks.unbounded_send(click).expect("send a click");
            cx.run_until_parked();
            assert_eq!(
                shown(cx, &ws),
                before,
                "banner-click: a click for {what} changes nothing"
            );
        }

        // Where notes is, and a check that it is what's shown, with the keyboard.
        let (want_tab, want_ws, want_pane) = cx.read(|cx| {
            let model = &ws.read(cx).model;
            let tab = model.terminal_tab(notes).expect("notes's tab");
            let (w, p) = model.locate(tab).expect("notes's place");
            (tab, w, p)
        });
        let notes_shown = |cx: &mut VisualTestAppContext, how: &str| {
            let (workspace, pane, tab) = shown(cx, &ws);
            assert_ne!(workspace, before.0, "banner-click {how}: another workspace");
            assert_eq!(
                workspace,
                Some(want_ws),
                "banner-click {how}: notes's workspace"
            );
            assert_eq!(pane, Some(want_pane), "banner-click {how}: notes's pane");
            assert_ne!(
                pane, before.1,
                "banner-click {how}: not the pane focused before"
            );
            assert_eq!(tab, Some(want_tab), "banner-click {how}: notes's tab");
            cx.update_window(window, |_, window, cx| {
                let ws = ws.read(cx);
                let index = ws.index_of(notes).expect("notes");
                let view = &ws.sessions.items()[index].view;
                assert!(
                    gpui::Focusable::focus_handle(view.read(cx), cx).contains_focused(window, cx),
                    "banner-click {how}: notes has the keyboard"
                );
            })
            .expect("read the focus");
        };

        // While the saved sessions are reopening, the click waits for them to finish.
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, _| ws.restoring = true)
        })
        .expect("restoring");
        clicks.unbounded_send(ours(notes)).expect("send a click");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: nothing is shown while restoring"
        );
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.finish_restoring(window, cx))
        })
        .expect("finish restoring");
        cx.run_until_parked();
        notes_shown(cx, "after the restore");

        // Back on api's workspace, a click with no restore running shows notes at once.
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                ws.select(api, window, cx);
            })
        })
        .expect("back to api");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: api's workspace again"
        );
        clicks.unbounded_send(ours(notes)).expect("send a click");
        cx.run_until_parked();
        notes_shown(cx, "at once");

        // A `Show` from the background service, while the saved sessions are reopening.
        let (shows, heard_shows) = futures::channel::mpsc::unbounded();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // Numbered by the run the workspace numbers by, as the service's are when it
                // runs them.
                ws.listen_for_shows(heard_shows, Some(run), window, cx);
                let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                ws.select(api, window, cx);
                ws.restoring = true;
            })
        })
        .expect("back to api, restoring");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: api's workspace again"
        );
        shows.unbounded_send(notes).expect("send a Show");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: a Show waits while restoring"
        );
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.finish_restoring(window, cx))
        })
        .expect("finish restoring");
        cx.run_until_parked();
        notes_shown(cx, "from the service, after the restore");
        slot.borrow_mut().take();
    };
    let path = shoot_with(
        "banner-click",
        "mocha",
        &theme,
        size(px(1280.0), px(600.0)),
        build,
        act,
    );
    println!("ui-shots: {}", path.display());
}

/// The rename box over a real workspace, in both themes:
/// - `rename`: the rename box just opened on shell (the selected row, tinted with the accent),
///   its old name selected; `rename-typed`: "api-2" typed over it.
///
/// Panics unless the rename box has the old (or typed) name, and the box's text is drawn in
/// `text` (it reads on the box, whatever the row under it is filled with).
fn rename_box_scenes() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["rename", "rename-typed"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| switcher_workspace(window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The renamed row's bounds: left, right, top.
            let renamed = Rc::new(Cell::new(None));
            let found = renamed.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.start_rename(ws.model.display_order()[3], window, cx)
                    })
                })
                .expect("open it");
                cx.run_until_parked();
                if scene == "rename-typed" {
                    cx.simulate_input(window, "api-2");
                }
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (_, input) = ws.renaming.as_ref().expect("the rename box is open");
                    let typed = if scene == "rename" { "shell" } else { "api-2" };
                    assert_eq!(input.read(cx).text(), typed, "{scene}-{slug}");
                    // shell's row: under api's header, after claude, codex and tests.
                    let b = ws.sidebar.scroll.bounds_for_item(4).expect("laid out");
                    found.set(Some((
                        f32::from(b.left()),
                        f32::from(b.right()),
                        f32::from(b.top()),
                    )));
                });
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            {
                let shot = render_with(&theme, dims, build, act);
                let (left, right, top) = renamed.get().expect("the renamed row");
                // The box, right of the mark, along the name line.
                let x0 = left + 8.0 + 14.0 + 8.0;
                let ink: usize = (0..30)
                    .map(|n| top + 4.0 + n as f32 * 0.5)
                    .map(|y| count_along(&shot, dims, ui.text, y, x0, right - 40.0))
                    .sum();
                assert!(
                    ink >= 20,
                    "{scene}-{slug}: the box's text is in `text` ({ink} pixels)"
                );
            }
        }
    }
}
