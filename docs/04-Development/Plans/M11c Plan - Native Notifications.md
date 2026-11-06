---
tags: [development, plan, milestone]
milestone: M11c
status: built
updated: 2026-11-06
---

# M11c Plan — Native notifications

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[55 - Milestone 11c - Native Notifications]] (the spec) |
| **Status** | 🟨 Built and reviewed 2026-11-06 (742 tests); on-screen check pending |
| **Tasks done** | 7 / 8 (Tasks 0, 1, 2, 3, 3b, 5, 6); Task 4 is your check |
| **▶ Pick up here** | Task 4: your on-screen check — the list is in [[55 - Milestone 11c - Native Notifications]], "On-screen checklist (pending)" |
| **Decisions used** | Yours (2026-11-06): extend M11 with native notifications; go. Everything else: AI's call, in note 55 "Settled before planning". |
| **Needs your eyes** | 👀 Task 0 (clicks) and Task 4 (on screen) |

**Goal:** fuxx's banners come from fuxx (UserNotifications from the app bundle) and a click shows the terminal that
raised it; anything macOS won't take falls back to today's `osascript` banner.

**Architecture:** A new `native_notify.rs` wraps `UNUserNotificationCenter` (bundle check, permission, post, the
click delegate) behind small pure rules. `notifier::fire` chooses per notification between it and `osascript` and
learns which terminal (`sN`) a banner is about. The window installs the click delegate at launch and turns a click
into `jump_to_notification(sN)`, held while a restore is still running. How a click on a **background-service** banner
reaches the window is decided by Task 0's test.

**Tech Stack:** `objc2-user-notifications` 0.3.2 (new; same family as `objc2-app-kit` / `objc2-web-kit` 0.3.2),
`objc2` 0.6.4, `block2` 0.6.2, GPUI at the pinned rev.

**Spec:** [[55 - Milestone 11c - Native Notifications]]

## Global Constraints

- New dependency, exactly: `objc2-user-notifications = { version = "0.3.2", default-features = false, features = ["std", "block2", "UNNotification", "UNNotificationContent", "UNNotificationRequest", "UNNotificationResponse", "UNNotificationSettings", "UNNotificationTrigger", "UNUserNotificationCenter"] }`. No other new crates.
- The app's identity: `BUNDLE_ID = "io.github.fitzfitz.fuxx"` (as `packaging/macos/Info.plist`). **Never touch
  `UNUserNotificationCenter` unless the main bundle's identifier is `BUNDLE_ID`** — unbundled it throws.
- Banner text: title = today's summary, body = today's body (unchanged wording); no subtitle, **no sound**; permission
  is asked for alerts only.
- Fallback: anything but "allowed" (not answered, denied, unbundled, a rejected or timed-out post) → today's
  `osascript` banner. Never both, never neither.
- Nothing blocks the UI thread or the service's tick thread: permission reads and posts wait off-thread, each at most
  **2 s**, then fall back.
- GPUI's own app delegate is left alone; the notification delegate is its own object, set once.
- Subagents never `cargo run` the app; allowed: `cargo test`, `cargo run --features ui-shots -- __ui-shots [filter]`,
  and building `target/dist/fuxx.app` with `scripts/bundle-mac.sh --app-only`. Each uses its own `CARGO_TARGET_DIR`
  under `/Users/fitzgeral/.claude/jobs/1f9e4e3f/tmp/`.

## Review Focus

1. A notification fires while macOS's permission question is still on screen (status "not determined") → the old
   banner, once; no fuxx banner later for the same event. (Task 1: `route` test for `NotDetermined`.)
2. `cargo run` (unbundled) → never calls into UserNotifications (it would throw) → old banners. (Task 1: `is_bundled_id`
   over `None` and a different id.)
3. The terminal closed between the banner and the click → fuxx comes forward, nothing else changes, no panic.
   (Task 2: the `banner-click` ui-shot's closed `sN`.)
4. A click while a restore is still running (fuxx opened by the click) → shown after the restore, not lost, not
   applied to the half-built layout. (Task 2: `PendingShow` tests.)
5. The window is attached and the service also sees the change → still exactly one banner (the service stays quiet
   while a window is attached — unchanged; Task 1 keeps `daemon_should_notify`'s tests green).

---

### Task 0: The click test — where does a background banner's click go? — ✅ (you clicked, 2026-11-06)

A throwaway probe on a throwaway branch (as the first test), run by the controller with you. Not merged.

- Probe modes in the bundled app: `__notify-probe service` (posts like the background service, sets its own click
  delegate, then **stays alive 2 minutes** on a run loop, logging any click), and `__notify-probe window` (the app mode
  of the first test).
- Cases, each logged to `~/Library/Logs/fuxx-notify-probe.log`:
  1. `service` running alone (no fuxx window): you click its banner → does `service` get the click, does macOS open a
     new fuxx, or nothing?
  2. `service` running **and** a `window` probe running: you click the `service` banner → which process gets it?
  3. `service` has exited (after its 2 minutes): you click its banner in Notification Center → what opens?
- **Outcome → ruling in the ledger, choosing Task 3's route:** **Route W** (the window side gets it, or macOS opens
  fuxx and hands it the click) or **Route S** (the service gets it: the service must pass it on).
- Delete the branch afterwards; record the result in note 55 ("Settled before planning", item 1).
- **Result:** (1) no window, service alive → the **service** gets the click, and macOS opens fuxx itself (that new fuxx
  gets no click), seen twice; (2) a window running → the **window** gets the click, even on the service's banner;
  (3) the poster gone → macOS opens fuxx and **the new fuxx** gets the click (delegate set before the app runs).
  **Ruling: both routes** — Task 2 handles any click the window gets (incl. at launch); Task 3 makes the service pass
  its clicks on.

### Task 1: Native banners with fallback — ✅

**Files:**
- Create: `src/native_notify.rs`
- Modify: `src/notifier.rs`, `src/workspace.rs:~1432`, `src/workspace_api.rs:~392`, `src/daemon.rs:~592`,
  `src/main.rs` (`mod`), `Cargo.toml`

**Interfaces:**
- Produces (native_notify): `pub const BUNDLE_ID: &str`; `pub fn is_bundled_id(id: Option<&str>) -> bool` (pure);
  `pub fn is_bundled() -> bool`; `pub enum Permission { Allowed, NotDetermined, Denied }`;
  `pub fn permission() -> Option<Permission>` (blocking ≤ 2 s; `None` on timeout or unbundled);
  `pub fn post(summary: &str, body: &str, terminal: Option<u64>) -> Result<(), PostError>` (blocking ≤ 2 s);
  `pub fn request_id(terminal: Option<u64>, n: u64) -> String` and `pub fn terminal_of(request_id: &str) -> Option<u64>`
  (pure: the `sN` rides in the request identifier, `fuxx-s{N}-{n}` / `fuxx-{n}`; no userInfo dictionary).
- Produces (notifier): `pub enum Route { Native, Script }`;
  `pub fn route(bundled: bool, permission: Option<Permission>) -> Route` (pure: `Native` only for bundled +
  `Some(Allowed)`); `pub fn fire(summary: &str, body: &str, terminal: Option<u64>)` — same blocking, best-effort
  contract as today; a `Native` post error → the `osascript` banner.

- [x] **Step 1: failing tests** — `is_bundled_id` (`Some(BUNDLE_ID)` true; `None`, `Some("com.apple.ScriptEditor2")`,
  `Some("")` false); `route` (bundled × each `Permission` × `None`: only bundled + Allowed → `Native`);
  `request_id` / `terminal_of` round-trip (`Some(3)`, n 7 → `"fuxx-s3-7"` → `Some(3)`; `None` → `"fuxx-7"` → `None`;
  `"fuxx-sx-1"`, `"other"`, `""` → `None`).
- [x] **Step 2: run** `cargo test --bin fuxx native_notify` and `cargo test --bin fuxx notifier` → FAIL (not defined).
- [x] **Step 3: implement** `native_notify.rs` (title and body only; completion handlers waited for on a channel with
  a 2 s timeout) and `notifier::fire(.., terminal)`; callers pass the terminal: workspace.rs the session's id,
  workspace_api.rs the entry's `terminal` (the caller filtered to open ones), daemon.rs `session.id()`.
- [x] **Step 4: run** `cargo test` → all pass (incl. `daemon_should_notify`'s tests unchanged);
  `cargo clippy --all-targets` and with `--features ui-shots`: no new warnings; `cargo fmt --check`.
- [x] **Step 5: commit** `feat(m11c): native banners from the app bundle, osascript fallback`.

### Task 2: Clicks show the terminal — ✅

**Files:**
- Modify: `src/native_notify.rs` (the delegate), `src/main.rs` (install at launch, GUI path only), `src/workspace.rs`
  (`pending_show`, applied in `finish_restoring`), `src/title_area.rs` (`jump_to_notification` reused), `src/ui_shots.rs`

**Interfaces:**
- Consumes: `terminal_of` (Task 1), `Workspace::jump_to_notification(terminal: u64, window, cx)`.
- Produces: `pub fn install(on_click: impl Fn(Option<u64>) + 'static)` (GUI, main thread, once, bundled only: sets
  the center's delegate — banners still shown while fuxx is the active app; a click → `on_click(terminal_of(id))` on
  the main thread — then asks permission for alerts); `pub struct PendingShow` with
  `fn click(&mut self, terminal: Option<u64>, restoring: bool) -> Option<Option<u64>>` (what to show now, or `None`
  while `restoring`, holding it; a later click replaces a held one) and `fn restored(&mut self) -> Option<Option<u64>>`
  (the held click, once) — pure.
- A click: `cx.activate(true)`; then `Some(sN)` → `jump_to_notification(sN)`; `None` (a script's banner from outside
  fuxx's terminals) → fuxx only comes forward.

- [x] **Step 1: failing tests** — `PendingShow`: click while not restoring → shown now; click while restoring → held,
  `restored()` returns it once, then `None`; two clicks while restoring → the later one. ui-shot `banner-click`: two
  workspaces, the second's second tab is a known `sN`; a click for it selects that workspace, pane and tab and gives it
  the keyboard; a click for an `sN` that's closed changes nothing.
- [x] **Step 2: run** the tests and `cargo run --features ui-shots -- __ui-shots banner` → FAIL.
- [x] **Step 3: implement**; the click reaches the `Workspace` through a channel polled by a GPUI task (the scripting
  socket's pattern in `main.rs`).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, full `__ui-shots` → all ok.
- [x] **Step 5: commit** `feat(m11c): a banner click shows its terminal`.

### Task 3: Background-service banners — ✅ (both routes, from Task 0)

**Files:** Modify `src/daemon.rs` (delegate, pending show, main-thread wait), `src/mux_proto.rs` (`Frame::Show`),
`src/workspace_daemon.rs` (apply a `Show`), `src/native_notify.rs` (a delegate usable without the app), `Cargo.toml`
(`objc2-foundation` feature `NSRunLoop`).

- The service (bundled only) sets its own notification delegate. A click → `terminal_of(id)`; with a window attached,
  send it `Frame::Show { id: u64 }`; with none, keep it as the one pending show (a later click replaces it), run
  `open -b io.github.fitzfitz.fuxx` (macOS usually opens fuxx already; this is a harmless backup) and send the `Show`
  right after the next window's `Hello`.
- **The service's main thread must run the main run loop** while it waits for the tick thread (today it blocks in a
  `join`, `daemon.rs:~436`): wait in slices of the main `NSRunLoop` (≤ 0.25 s) until the tick thread has finished, so
  the delegate's main-queue callbacks run (Task 0's probe got clicks only while pumping it).
- The window applies a `Show` through `PendingShow` (Task 2), so a `Show` during the restore lands after it.
- [x] Tests: `Frame::Show` round-trip; the service's pending-show rule (pure: attached → send now; none → hold, later
  click replaces, `Hello` takes it once); the window applying a `Show` via `PendingShow`; the existing daemon tests
  (stop, idle exit, reap) still pass with the run-loop wait. `cargo test`, clippy, fmt, full `__ui-shots`.
- [x] Commit `feat(m11c): background banners land on their terminal`.

### Task 3b: The default name becomes ╭∩╮ — ✅ (added 2026-11-06, your request)

Your words: "Change the new tab, new workspace title from the emoji to ╭∩╮".

- `groups::DEFAULT_NAME` (`src/groups.rs:7`) changes from `"🖕"` to `"╭∩╮"` (U+256D U+2229 U+256E). It is the name a
  terminal tab and its workspace show until you rename them or the program sets a title, so the sidebar row, the tab,
  the window title, ⌘P, `fuxx ls` and notification titles ("fuxx — ╭∩╮") all follow.
- Unchanged (AI's call: not a tab or workspace title): the browser bar's 🖕 mark (`browser/bar.rs` `MARK_TEXT`), and test
  data that merely uses 🖕 as a typed name. Names you gave stay; unnamed tabs and workspaces (restored ones too) show
  ╭∩╮, since the default is applied when shown, not stored.
- Comments naming the default (`workspace.rs` "else 🖕", "often 🖕") follow the new value.
- [x] Tests: `groups` `auto_name(None, None, None) == "╭∩╮"`; the daemon's `notification_summary` default test →
  `"fuxx — ╭∩╮"`; a ui-shot of a new workspace's row and tab showing ╭∩╮ readably (Mocha + Latte); `cargo test`,
  clippy, fmt, full `__ui-shots`.
- [x] Commit `feat(m11c): new tabs and workspaces are named ╭∩╮`.

### Task 6: The app icon — ✅ (added 2026-11-07, your request; after the merge, on master by your word)

Your words: "now we already have icons, should we implement the icon on notif, on app bar, etc?" → "yes go with 3d
cybers. Just update it on master then directly commit on master branch for this work".

- `fuxx.app` gets an icon: the **3D Cyber** logo (`assets/logo/3d-cyber/fuxx-logo-large.png`). macOS then shows it in
  the Dock, ⌘Tab, Finder, Launchpad, the About window, **notification banners** (today a blank square), System
  Settings → Notifications and the permission question. (The menu bar never shows an app icon on macOS.)
- On Apple's icon grid: the logo's visible area (it touches the bottom edge, off-centre) is trimmed, scaled to fit an
  **824 px** body and centred on a transparent **1024 px** canvas, then exported at 16, 32, 128, 256 and 512 px plus
  their @2x versions into `packaging/macos/fuxx.icns` with macOS's `iconutil`.
- `scripts/make-app-icon.py` regenerates the `.icns` (Pillow, as the logo scripts already need); the `.icns` is
  committed, so building the app needs no Python. `scripts/bundle-mac.sh` copies it to `Contents/Resources/` and
  `Info.plist` names it (`CFBundleIconFile` = `fuxx`).
- **The browser bar's mark** (your "we do it on browser bar as well"): the popped-out browser window's toolbar mark
  (`browser/bar.rs` `mark_image`, today the 🖕 emoji drawn as text) becomes the 3D Cyber logo — trimmed and centred
  with no margin (`assets/logo/3d-cyber/fuxx-mark.png`, 64 px, made by the same script), compiled into fuxx and drawn
  at the toolbar's 18 pt. The in-pane browser bar has no mark (unchanged).
- Unchanged: `cargo run` builds keep the generic Dock icon.
- [x] Checks: the mark's embedded PNG is 64 × 64 (a Rust test); the script's fit rule (pure: bbox → scaled size and offset, centred, ≤ 824) by a small test; the `.icns`
  holds all ten sizes (`iconutil -c iconset` round-trip); a built `fuxx.app` has the file and the plist key;
  `cargo test` unchanged. On screen (you): the Dock, ⌘Tab, a notification banner, System Settings and a popped-out browser window's toolbar show the logo.

### Task 4: On screen — 👀 pending (you)

The note's success check plus Review Focus 1–3, as a numbered list in note 55 ("On-screen checklist").

### Task 5: Docs — ✅

Note 55 (what was built, rulings, checklist), roadmap, Plan Tracker, Progress Log, STRUCTURE.md (`native_notify.rs`),
the M4 note's D5 line, and the memory `macos-notify-rust-broken` if the tests taught more.

## Verification log

| Task | Commits | Tests | Review |
|---|---|---|---|
| 0 Click test | spike 368e8c7 (deleted) | you clicked 3 cases | — (result in note 55, item 1) |
| 1 Native banners | b1c3a40..ca12043 | 726 | Approved (sonnet), 4 Minor |
| 2 Clicks show the terminal | ca12043..f1dbfa7 | 729; banner-click ui-shot | Approved (opus), 4 Minor |
| 3 Background-service banners | c39f778..70346c4 | 737 | Approved (opus), 5 Minor |
| 3b Default name ╭∩╮ | 70346c4..d141ca9 | 737; default-name ui-shot | Approved (sonnet), 2 Minor |
| Final whole-branch | 3b80a8d..f855088 | 742; 29 ui-shots scenes | With fixes (opus): 2 Important (banner ids repeated across runs → a click on an old banner showed an unrelated terminal; a new banner could replace an old one), 5 Minor → one fix pass (opus) → re-review 6/6 addressed (sonnet) |
| 6 App icon (on master, your word) | this commit | 743; fit rule 4/4 (Python); `.icns` round-trip: 10 sizes; built app has the icon, plist key, valid seal | Not reviewed by an agent (small, on master at your request) |

## Deferred minors

- A post that times out at 2 s and is delivered later can still show both banners (rare; withdrawn first).
- A banner posted while the background service is still starting carries no terminal: its click only brings fuxx
  forward.
- A held click (background service) never expires; a Show to a window that is closing is dropped.
- Cosmetic: a duplicate log line; `run_loop_slice`'s doc; the ids' parser accepts a leading `+`; the banner-click
  scene's closed-tab id; the ╭∩╮ glyph is checked by eye.

Rulings: in the M11c section of the ledger-derived list in [[55 - Milestone 11c - Native Notifications]] ("What was
built").

## Related

[[55 - Milestone 11c - Native Notifications]] · [[40 - Roadmap]] · [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]]
