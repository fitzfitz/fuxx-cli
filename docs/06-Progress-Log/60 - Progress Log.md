---
tags: [moc, progress, log]
---

# 60 - Progress Log

The running memory of the project. **Add an entry every session.** Newest at the top.
This is what lets any new session — or any AI assistant — get caught up instantly. See
[[54 - Session Startup Checklist]].

## How to write an entry

Copy [[61 - Log Entry Template]] to the top of the "Entries" section below and fill it in.

## Current status snapshot

- **Current milestone:** **M5 — Terminal UX** (see [[40 - Roadmap]]; the **new terminal app**). ⬜ M0–M4 and M4b ✅ done: the first genuinely useful version. Pick up at [[00 - Plan Tracker (Start Here)]].
- **Overall phase:** **PIVOTED.** v1 CLI wrapper shipped & archived; now building **fuxx**, a native GPU terminal for AI agents, in Rust (GPUI + `alacritty_terminal` + `portable-pty`). Repo repurposed; vault re-founded.
- **Repo:** `github.com/fitzfitz/fuxx-cli` — public; v1 on `master`. Repo **repurposed** for the terminal app.

---

## Entries

### 2026-10-26 — Milestone 4b complete: tester build
- **Did:** You installed `fuxx-0.1.0.dmg` like a tester (a copy marked as downloaded): drag to Applications,
  Open Anyway once, then the shell, `claude`, accented text and emoji, rings with the quiet-timer Done and
  banners, reopening without a second prompt, the Intel slice under Rosetta, and version 0.1.0 in Get Info.
  All 7 checks passed. Final review: 0 Critical; its 3 Important findings were fixed or documented (`bc5705b`).
  Updated [[44b - Milestone 4b - Tester Build]], [[40 - Roadmap]], the tracker and `STRUCTURE.md`.
- **Learned:** A Finder-opened fuxx on this Mac behaves like the terminal-launched one, except that Claude
  Code sends no notification (D8, as expected).
- **Blocked:** Nothing.
- **Next task:** Write the M5 plan (Phase 2: Terminal UX).

### 2026-10-26 — M4b Task 1: the tester DMG builds
- **Did:** Added `scripts/bundle-mac.sh` (universal release build → ad-hoc sealed `fuxx.app` →
  `target/dist/fuxx-<version>.dmg`, refuses a version that already has a DMG) and `scripts/check-dmg.sh`
  (the test: contents, both CPU slices, macOS 11.0 floor, version, seal) (`170ec6a`). The final review
  found `check-dmg.sh` silent when the DMG was already open in Finder; fixed with the failure reproduced first.
  Put a "downloaded" copy in `~/Downloads` for your install check. 61 tests pass.
- **Learned:** In bash, `set -e` still applies inside an `EXIT` trap: one failing cleanup command ends the
  trap early. The version guard only sees DMGs still in `target/dist` (known limitation).
- **Blocked:** Your Task 2 install check.
- **Next task:** [[M4b Plan - Tester Build]] → Task 2 (👀).

### 2026-10-26 — M4b plan written; D7 + D8 decided
- **Did:** Compared how other terminals handle both decisions (Ghostty, iTerm2, kitty, Alacritty, WezTerm
  ship universal Mac apps, Zed ships separate Intel and Apple Silicon ones; nearly every terminal sets its
  own `TERM_PROGRAM`, fuxx inherits it). You chose **D7 = universal** (Apple Silicon + Intel) and
  **D8 = accept the weaker detection** when opened from Finder, "for now". Added the `x86_64-apple-darwin`
  Rust target. Wrote [[M4b Plan - Tester Build]] and [[44b - Milestone 4b - Tester Build]] (with tester
  install steps), after a trial run of the plan's scripts: universal DMG, 7.4 MB.
- **Learned:** An Intel release build compiles cleanly at our pinned GPUI rev (Zed ships Intel builds from
  it). macOS 26 is the last release for Intel Macs, but Apple still recommends universal apps. macOS gives
  apps opened from Finder no `LANG`; your shell start-up sets one, a tester's may not (known limitation).
- **Blocked:** Nothing.
- **Next task:** [[M4b Plan - Tester Build]] → Task 1.

### 2026-10-26 — Milestone 4 complete: first genuinely useful version
- **Did:** Decided **D5** with you after comparing how Ghostty, iTerm2, Warp and Zed handle notification
  clicks: "click a notification → jump to the session" moves to **M11** (every terminal that does it is a
  real `.app` using Apple's notification system). Built [[M4 Plan - Notification Rings]] on branch
  `m4-notification-rings`, one fresh agent per task, each reviewed: a focus-aware `osascript` notifier with
  AppleScript escaping for untrusted titles (`5af9f85`), then status rings in the sidebar and background
  notifications fired off the UI thread (`a0990fc`). The plan's code compiled unchanged. All on-screen checks
  passed, including real `claude` runs. 0 warnings, 61 tests pass.
- **Learned:** Zed (same GPUI) shows "agent needs you" in its own pop-up window rather than a system
  notification. Noted as an option, not adopted (outside the plan).
- **Blocked:** **D7** and **D8** before the M4b plan can be written.
- **Next task:** Decide D7 + D8 → write the M4b plan (unsigned `.app` in a DMG for testers).

### 2026-10-26 — Milestone 3 complete: many sessions
- **Did:** Built [[M3 Plan - Many Sessions]] on branch `m3-many-sessions`, one fresh agent per task, each
  reviewed: `SessionList` (`31b9a32`), embeddable `TerminalView` + `Workspace` with sidebar and the D6
  shortcuts (`0350c50`), PTY writes on a per-session writer thread (`9326082`). Your on-screen check
  found that **Working** never showed in the status log; root cause: `StatusTracker::on_output` set it
  but never reported the change (latent since M2). Fixed with a test first (`6973016`). All checks then
  passed on screen, plus an input-flood check (3000 cursor queries to a program that isn't reading):
  the app stays responsive. 0 warnings, 55 tests pass.
- **Learned:** M2's Task 6 "Working" checks couldn't have caught the bug, because the log line was never
  printed and nothing asserted it. A status change must be *reported*, not only stored; the new test pins it.
  The `1R;` text after the flood is zsh reading the queued replies, not an app bug.
- **Blocked:** **D5** (notification click with `osascript`) before M4 starts.
- **Next task:** Decide D5 → [[M4 Plan - Notification Rings]] → Task 1. M4's expected count is now 61.

### 2026-10-26 — M3 plan changes: D6, PTY writer thread; one proposal dropped
- **Did:** On request ("recommended for all three"): **D6** = the recommended shortcuts. Added M3 **Task 4**:
  PTY writes move to a per-session writer thread, so `TerminalCore::write` never blocks the UI. A new test
  proved the freeze first (`write blocked` after 2 s against M2 code). Dropped the third proposal
  (end a session when the shell exits even if a background job holds the PTY): a probe showed macOS
  already ends it. Re-verified the whole M3 plan from its text in a scratch worktree: 0 warnings, 54 passed.
  M4's expected count is now 60.
- **Learned:** On macOS, when the shell (session leader) exits, the tty is revoked and the reader gets
  EOF at once, even with a disowned job (`&!`). With `nohup … &`, zsh refuses the first `exit` itself.
  The M1 review's concern holds only on Linux.
- **Blocked:** Nothing.
- **Next task:** [[M3 Plan - Many Sessions]] → Task 1.

### 2026-10-26 — Milestone 2 complete: agent status detected
- **Did:** Built [[M2 Plan - Agent State Detection]] on branch `m2-agent-state`, one fresh agent per task,
  each reviewed: notification detector for OSC 9 / 99 / 777 that skips the `9;4` progress bar (`a2da2e2`),
  status heuristics with Idle / Working / Waiting / Done (`92fead3`), screen tail text (`ed7a4cb`),
  notification counting in the reader thread (`1e05b1e`), and the tracker wired into the view with BEL
  from the parser (`1843452`). Final review (opus): ready to merge; its one Important item (a settled
  session re-read the whole screen every 50 ms after a notification) was fixed with your OK (`316c884`).
  Build: 0 warnings from our code; `cargo test`: 47 passed. **You confirmed
  on screen** every Task 6 check, including all four notification types, the progress bar not ending
  "working", the tab beep staying silent, and a real `claude` run launched from Ghostty.
- **Learned:** The first-guess constants (150 ms echo window, 3 s quiet, 8 tail lines) needed no tuning.
  D4 = Idle (recommended option) is now decided.
- **Blocked:** D6 (shortcuts) and the two M3 proposals from the M1 review are yours to decide before M3.
- **Next task:** Decide those, then [[M3 Plan - Many Sessions]] → Task 1.

### 2026-10-26 — Plan change: M4b tester build (unsigned .app in a DMG)
- **Did:** On request, added **M4b** right after M4 in [[40 - Roadmap]], [[00 - Plan Tracker (Start Here)]]
  and [[12 - Scope]]: wrap the release binary in an unsigned `.app` and ship it in a DMG for
  drag-and-drop install. M11 now signs/notarizes that bundle. Added open decision **D8**: opened from
  Finder, the shell inherits no terminal identity, so Claude Code sends no notifications. No code
  changed; M4b's detailed plan is written when M4 is done.
- **Learned:** An unsigned DMG still needs the tester to clear Gatekeeper once ("Open Anyway").
  The binary needs nothing else installed: it links only macOS system frameworks (checked with
  `otool -L`), arm64 only, declared minimum macOS 11.0 (untested below the dev machine).
- **Blocked:** Nothing.
- **Next task:** [[M2 Plan - Agent State Detection]] → Task 6 (your on-screen check).

### 2026-10-26 — Plan change: M2 detects OSC 99, OSC 777 and BEL too
- **Did:** On request (option b), amended [[M2 Plan - Agent State Detection]]: the detector now counts
  OSC 9, OSC 99 (kitty) and OSC 777 `notify` (Ghostty), and skips `OSC 9;4;` (a progress bar). A real
  BEL from the parser counts while the session is Working. The shell's environment stays inherited,
  and no Claude Code setting is documented: both were kept out on request. Updated [[12 - Scope]],
  [[11 - Features]], [[42 - Milestone 2 - Agent State Detection]], and the M3/M4 plans that consume
  the renamed `TermEvent::Output { notifications }`. Fixed the M2 plan's `cargo test grid` count (7 → 8).
  Re-verified Tasks 1–5 from the amended plan text in a scratch worktree: 0 warnings, 46 passed.
- **Learned:** The installed Claude Code (2.1.310) picks its notification from the inherited
  `TERM_PROGRAM`: iTerm2 → OSC 9, kitty → OSC 99, Ghostty → OSC 777, Terminal.app → BEL, anything
  else → nothing. Under Ghostty ≥ 1.2 / iTerm2 ≥ 3.6.6 it also sends `OSC 9;4;` progress bars while
  working, which v1's detector would have read as "done". You launch from Ghostty 1.3.1, so this path
  is the one that matters.
- **Blocked:** Nothing.
- **Next task:** [[M2 Plan - Agent State Detection]] → Task 1.

### 2026-10-26 — Milestone 1 complete: a real shell in the GPUI window
- **Did:** Built [[M1 Plan - One Terminal Pane]] task by task (one fresh agent per task, each
  reviewed) on branch `m1-terminal-pane`: keystroke → bytes (`d4b972f`), color palette (`522122f`),
  screen snapshot + paint batching (`adc3283`), live terminal core with PTY + `alacritty_terminal`
  + reader thread (`d86e2b6`), and the GPUI `TerminalView` (`4a359c3`). Build: 0 warnings from our
  code; `cargo test`: 21 passed. **You confirmed on screen** all 12 checks: prompt + block cursor,
  typing, `ls -G` colors, `TERM=xterm-256color`, `vim`, ctrl-c/ctrl-d, resize + `tput`, `界🙂x`
  without overlap, `seq 1 200000`, `claude`, `exit` quits, cmd-q quits.
- **Learned:** GPUI's own text system (D2) is enough to draw a terminal grid; no `gpui-ghostty`
  fallback needed. Programs like Claude Code send terminal queries, so replies must be written
  back to the PTY, and synchronized updates need a periodic flush (50 ms timer).
- **Known gaps (by plan):** no scrollback/selection/mouse (M5), fixed Menlo 14 px (M6), no IME/dead
  keys, the cursor covers only half of a wide glyph, shell exit quits the app (changes in M3).
- **Final review (opus):** ready to merge; 0 Critical. Three forward-looking items raised as plan
  questions: the shell inherits the launcher's terminal-identity env (decides whether Claude Code
  sends OSC 9 → M2), PTY writes can block the UI thread (→ M3), and `exit` with a disowned job
  holding the tty doesn't end the session (→ M3).
- **Blocked:** M2 Task 1 waited on the env / OSC 9 question (settled the same day, see the entry above). Merged into `master` locally, then pushed.
- **Next task:** Decide the OSC 9 question, then [[M2 Plan - Agent State Detection]] → Task 1 (D4 still open; recommended option used by default).

### 2026-10-26 — Open decision D7 added (target CPUs for M11)
- **Did:** On request, added **D7** to [[00 - Plan Tracker (Start Here)]] and the M11 line of
  [[40 - Roadmap]]: which CPUs the shipped `.app` runs on. Options: Apple Silicon only (what builds
  today, `aarch64-apple-darwin`) or universal (Apple Silicon + Intel). Not chosen yet; decide when
  M11 gets its detailed plan. No code changed.
- **Learned:** The vault named target OSes but never CPU architecture; GPUI on Intel Macs at our
  pinned rev is unverified.
- **Blocked:** Nothing.
- **Next task:** [[M1 Plan - One Terminal Pane]] → Task 1.

### 2026-10-26 — M0 merged; D2 + D3 decided
- **Did:** Merged `m0-gpui-window` into `master` locally (fast-forward; `cargo test` ok on the merged
  result; not pushed). Decided **D2 = GPUI's own text system** and **D3 = `futures` crate**
  (`futures::channel::mpsc`), both the recommended options. Started branch `m1-terminal-pane`.
- **Learned:** D3 adds the first new *direct* dependency since M0 (`futures`), but it's already in
  `Cargo.lock` through GPUI, so nothing new is downloaded.
- **Blocked:** Nothing.
- **Next task:** [[M1 Plan - One Terminal Pane]] → Task 1.

### 2026-10-26 — Milestone 0 complete: GPUI window launches without Xcode
- **Did:** Decided **D1 = `runtime_shaders`** (2026-10-25). Created branch `m0-gpui-window` and committed the
  pending vault/plans/mockups work (`9da5ab4`) and the crate scaffold (`71221cb`). Pinned `gpui` +
  `gpui_platform` to rev `1d2a4b3f` with `font-kit` + `runtime_shaders` (`b8602af`; `Cargo.lock`
  records the rev). `main.rs` now titles the window "fuxx", quits when the last window closes and on
  cmd-q (`d0d92a0`). Build: 0 warnings from our code; `cargo test` ok (no tests yet). **You confirmed
  on screen:** the window shows the placeholder text, and both the close button and cmd-q quit.
- **Learned:** Full Xcode isn't needed: `runtime_shaders` compiles the Metal shaders at launch, so the
  Command Line Tools are enough. GPUI ships no key bindings (cmd-q must be bound to an `actions!`
  action), and on macOS it keeps the app alive with no windows unless `QuitMode::LastWindowClosed`.
- **Blocked:** Decisions **D2** (how to draw glyphs) and **D3** (PTY → UI channel) before M1. The
  branch isn't merged into `master` yet.
- **Next task:** Decide D2 + D3, then [[M1 Plan - One Terminal Pane]] → Task 1.

### 2026-10-12 — Plan change: project groups added to M7
- **Did:** On request, added **project groups** to the plan: [[11 - Features]] (B, `P1`) and
  [[40 - Roadmap]] (M7, alongside presets and restore, which already deal with working folders).
  Scope kept small: you pick the project when creating a session (no tracking of `cd`), each group
  header shows its most urgent ring, collapse/expand, and "new session in this project". Not
  included: per-project settings, nested groups, auto-detection. Updated mockup screen 2 (grouped
  sidebar, legend #13), the mockup catalog and `STRUCTURE.md`. M0–M4 plans unchanged.
- **Learned:** A rolled-up ring per project keeps the payoff even when a project is collapsed.
- **Blocked:** Still waiting on decisions **D1–D6**.
- **Next task:** Decide **D1**, then [[M0 Plan - Scaffold and GPUI Window]] → Task 1.

### 2026-10-12 — Repo layout doc + HTML mockups of the finished app
- **Did:** On request, added `STRUCTURE.md` at the repo root: today's tree, the full `src/` tree after M4 with the
  milestone that adds each file (names taken from the M0–M4 plans' Files tables), a data-flow sketch,
  and what M5–M12 add (no file names, since those aren't planned yet). Added `mockups/`: four static
  HTML pages (overview catalog + M4 screen + Phase 2 screen + Phase 3–4 screen) covering every
  feature in [[11 - Features]], each tagged with priority and milestone. The M4 screen uses the
  plans' palette, sidebar size, shortcuts and notification text; its buttons simulate the
  focus-aware notification rule and Waiting → Idle on typing. Linked from README and [[00 - Home]].
  Checked in Chrome: renders, no console errors, interactions behave. No code or `Cargo` changes.
- **Learned:** Two catalog items have no milestone yet (scrollback search, notification history),
  and "tabs vs sidebar-only" is still open. The mockups mark them that way instead of guessing.
  The socket env-var name and the Homebrew cask name are marked illustrative until M9 / M12.
- **Blocked:** Still waiting on decisions **D1–D6**.
- **Next task:** Decide **D1**, then [[M0 Plan - Scaffold and GPUI Window]] → Task 1.

### 2026-10-12 — Detailed, compile-verified plans for M0–M4 + plan tracker
- **Did:** Wrote a step-by-step implementation plan for every Phase 0–1 milestone
  (`04-Development/Plans/`: M0–M4), each with a status indicator per task, checkbox steps, a
  **▶ Pick up here** pointer, review focus, a manual 👀 checklist and a verification log. Added
  [[00 - Plan Tracker (Start Here)]] (where we are, next action, open decisions D1–D6) and
  [[31 - API Fact Sheet (pinned rev)]]. All plan code was compiled and tested stage by stage in a
  scratch crate against the pinned GPUI rev: 0 warnings; 21 / 38 / 44 / 50 unit tests passing for
  M1 / M2 / M3 / M4. Linked the plans from the roadmap, milestone notes, Home and the startup checklist.
  No repo code changed.
- **Learned:** M0 is **not** stuck on Xcode: GPUI's `runtime_shaders` feature compiles Metal shaders
  at startup (builds here with only CLT; launch unverified), and `font-kit` is required or no text
  renders. At this GPUI rev the APIs differ from most tutorials (`gpui_platform::application()`,
  6-arg `ShapedLine::paint`, `resolve_font`). Terminal query replies (`PtyWrite`) must be written back,
  or vim and Claude Code hang. Synchronized updates need a timeout flush. `osascript` notifications
  can't report clicks, so "click → jump to session" can't be done in M4 (D5).
- **Blocked:** Waiting on decisions **D1–D6** (recommended options are already written into the plans).
- **Next task:** Decide **D1**, then [[M0 Plan - Scaffold and GPUI Window]] → Task 1.

### 2026-07-17 — Pivot to terminal app: repo repurposed, vault re-founded
- **Did:** After shipping v1, pivoted to a new product — a native GPU terminal for running AI agents (cmux-like), in **Rust** (GPUI app shell + `alacritty_terminal` VT core + `portable-pty`, reused from v1). Chose the pure-Rust `alacritty_terminal` renderer over `gpui-ghostty` to avoid the Zig toolchain. **Repurposed this repo.** Archived the v1 wrapper docs → `docs/90-Archive-v1-CLI-Wrapper/`; wrote a fresh forward vault (Home / What Is / Scope [reverses v1] / Architecture / Tech Stack / Roadmap M0–M4 / M0 note); rewrote `AGENTS.md` for the new project.
- **Learned:** libghostty *rendering*-embedding is Swift-only; the Rust path (GPUI + `alacritty_terminal`) is viable with real precedents (tty7, Paneflow, zTerm). `gpui-ghostty` exists (Ghostty rendering in GPUI) but needs Zig 0.14.1.
- **Blocked:** Nothing. (Heads-up: GPUI's first build is heavy; it's a pinned git dep, pre-1.0.)
- **Next task:** [[40a - Milestone 0 - Scaffold and GPUI Window]] — repurpose the crate + get a blank GPUI window launching.

### 2026-07-17 — Milestone 3 complete → v1 DONE (notify-rust dropped for osascript)
- **Did:** Added a decoupled `notifier` module and wired detection → desktop notification, retiring M2's stderr signal (removed its two integration tests). **Discovered during manual verification that `notify-rust` never displays on macOS 26** (its deprecated `NSUserNotification` backend accepts the call — `show()` = `Ok` — but shows nothing, regardless of bundle-id attribution). Pivoted the notifier to shell out to **`osascript`**, which works; dropped the `notify-rust` dependency. `notifier::fire`'s signature was unchanged, so no other code moved. Confirmed a real banner fires end-to-end. 12 tests pass. **This completes v1.**
- **Learned:** Systematic debugging to a real root cause: `show() = Ok` ≠ "displayed"; modern macOS neutered `NSUserNotification` for unbundled binaries, while `osascript`'s `display notification` (entitled system path) works. Decoupling paid off — swapping the whole notification backend touched only `notifier.rs` + `Cargo.toml`.
- **Blocked:** Nothing. (Cosmetic: banner is attributed to "Script Editor"; a real identity needs an `.app` bundle — M4+.)
- **Next task:** v1 is complete — **stop adding features.** Optional: [[44 - Milestone 4 - Ship via Homebrew]].

### 2026-07-17 — Milestone 2 complete: OSC 9 detection end-to-end
- **Did:** Completed Tasks 1–3: stateful `OscDetector` module + unit tests (byte pattern scanning for ESC `]9`), wired into the pump with stderr signal, 14 tests pass (8 unit + 6 integration). Live validation confirmed detection works (`cargo run -- printf '\033]9;hi\007'` prints NOTIFICATION DETECTED to stderr). Vault updated: Milestone 2 marked done; deferred payload extraction, OSC 99/777, and ConEmu; refreshed repo status.
- **Learned:** Stateful pattern scanning via feed() loop and state machine; cross-chunk robustness (target sequence split between reads); integration testing a PTY wrapper; detecting escape codes in real-time output streams.
- **Blocked:** Nothing
- **Next task:** [[43 - Milestone 3 - Fire a Notification]] — fire a native desktop notification (via `notify-rust` crate) when a detection occurs.

### 2026-07-17 — Milestone 1 complete: PTY wrapper with raw-mode I/O
- **Did:** Implemented Tasks 1–3: arg parsing (cli.rs), PTY spawn + output passthrough + exit code, stdin forwarding + raw-mode guard (Drop). All 6 tests pass (2 unit, 4 integration). Manual validation with vim/bash confirmed child sees real TTY, colors render, keystrokes forward, terminal restores cleanly. Updated vault to record Milestone 1 done and crossterm dependency.
- **Learned:** `portable-pty` provides cross-platform PTY spawning; `crossterm` raw mode prevents shell line-buffering/echo so stdin forwards instantly; Drop guards ensure cleanup even on panic; `is_terminal()` guards Drop to avoid breaking piped I/O.
- **Blocked:** Nothing
- **Next task:** [[42 - Milestone 2 - Detect the Signal]] — scan the captured output stream for OSC 9 notification escape sequences (ESC `]9`) and log a detection.

### 2026-07-17 — Milestone 0 done + AGENTS.md operating rules
- **Did:** Installed Rust via `rustup` (rustc/cargo 1.97.1, stable, aarch64). Scaffolded
  the crate at the repo root with `cargo init --name fuxx-cli --bin`; `cargo run` prints
  `fuxx-cli v0.1.0 — toolchain works.` Added `AGENTS.md` (system prompt: Superpowers
  workflow, milestone-only discipline, no out-of-plan suggestions, always update docs)
  and a `CLAUDE.md` that imports it. Marked [[40a - Milestone 0 - Toolchain and Hello World]] ✅.
- **Learned:** `Cargo.toml` is the package manifest (name/version/edition/deps);
  `src/main.rs`'s `fn main()` is the entry point; `println!`/`env!` are macros (the `!`),
  and Cargo injects `CARGO_PKG_VERSION` at compile time.
- **Blocked:** GitHub repo still 404 — create it before the first push.
- **Next task:** [[41 - Milestone 1 - Launch and Capture]] — spawn an agent as a child
  process and pass its output through to the terminal.

### YYYY-MM-DD — Vault created (planning)
- **Did:** Created this Obsidian knowledge vault — architecture, tech stack, roadmap,
  agentic workflow, and this log.
- **Learned:** Project scope locked to the wrapper-process model; libghostty and
  terminal emulation are explicitly out of scope. v1 = wrap one agent + notify.
- **Blocked:** GitHub repo returns 404 — needs to be created/made public.
- **Next task:** [[40a - Milestone 0 - Toolchain and Hello World]] — install Rust,
  `cargo new`, hello world.

<!-- Add new entries ABOVE this line, newest first. -->
