---
tags: [development, decisions]
---

# Rulings — M10b, M10c, M11 (autonomous run, 2026-11-05 → 2026-11-06)

You asked to run M10b → M10c → M11 autonomously ("do it autonomously until M11 done"). Every decision the AI took on your behalf during the build is listed here, copied from the build ledgers, each with what it costs if wrong. Rework anything you disagree with. The user-visible ones are also summarised in each milestone note's "Decisions made along the way".

## [[51 - Milestone 10b - SSH Workspaces]]

- Place variants undefined → Ruling A 
- Place for splits → Ruling A 
- env vars are process-wide/unsafe in parallel tests → Ruling B 
- Ruling A: T5 defines `pub enum Place { New, Split { of: usize, dir: SplitDir }, Replace(usize) }` (use the existing split-direction type; adapt names to the code) — T6 uses Place::New, T7 uses New/Split, reconnect uses Replace — cost if wrong: a rename.
- Ruling B: no FUXX_SSH env var. `ssh_hosts::resolve_with(program: &OsStr, host, timeout)` and `ssh_control::ensure_healthy_with(program, host, dir, timeout)` take the program; `resolve` / `ensure_healthy` call them with "ssh". Tests pass a temp script — cost if wrong: none (tests only).
- Ruling C: fold two Task 1 minors into Task 2 (it edits the same main.rs lines and uses is_git_host): move the misplaced dead-code comment above `mod ssh_target;`, and make is_git_host's hostname match ASCII case-insensitive (+ one test line) — cost if wrong: a few lines.
- User (2026-11-05): "do it autonomously until M11 done". Ruling D: run M10b → M10c → M11 back to back without check-ins. Each milestone gets spec + plan (AI's calls, marked as such; M10c from study §4 + user's "agents automate it"; M11 from the M11 Input note + "fun, not boring, but professional" + animations toggle), SDD with reviews, docs marked "built and reviewed; on-screen check pending". Branches stack (m10c from m10b, m11 from m10c); no merge/push to master and no ✅ until the user runs the batched on-screen checks — cost if wrong: the user waits for merges until they check.
- Ruling E: fold two test-reliability fixes into Task 5 — (a) the ETXTBSY flake in ssh_hosts/ssh_control fake-script tests: retry the spawn in run_capped on ETXTBSY (raw os error 26) a few times with a short sleep (also protects real use); (b) make terminal.rs `a_daemon_session_s_exit_code_reaches_the_replica` deterministic with `Tuning { reap: false, .. }` — cost if wrong: a few lines.
- Ruling F (re-graded Important, to enter T5's fix round): every tmux pane to one host shares `fuxx-<host>` and mirrors the others. Give each SSH session its own tmux name, made once at open (`fuxx-<sanitized host>-<6 hex random>`, ≤ 32 chars after `fuxx-` per the constraint), kept on the session and saved with it (e.g. `SshTarget.tmux_name: Option<String>`, serde default/skip-none), reused on reconnect and restore, and never stored in the recent-hosts file — cost if wrong: one field + a test.
- Task 5: Ruling: re-grade Minor 3 (failed reconnect start leaves "reconnecting in Ns" with no timer — auto-reconnect silently stops) to Important by effect — cost if wrong: a few lines.
- Task 5: Ruling: re-grade Minor 5 (banner printed over the remote's alt screen / scroll region / colours — every tmux or vim disconnect looks garbled) to Important by effect; fix: write a reset (SGR 0, leave alt screen ?1049l, full scroll region) before the banner — cost if wrong: the pre-ssh main screen shows under the banner (as iTerm does).
- Task 5: Ruling: "name = host" in the brief is satisfied by the host group header; Session.name stays None so rows show the remote title — accepted (implementer's reading) — cost if wrong: one line.
- Task 5: Ruling: workspace_api.rs refusal of scripted input to disconnected sessions was the controller's dispatch decision — accepted.
- Ruling G: move the SSH block out of workspace.rs into a new src/workspace_ssh.rs (pure move, same pattern as workspace_daemon.rs) in this fix round, before Tasks 6–7 add more; and replace `let ssh = program.is_some()` with a check on the session's ssh field — cost if wrong: churn only.
- Task 5: fix round 1/5 dispatched (resume implementer): Important 1, 2, re-graded 3 and 5, Ruling F (per-session tmux name), Ruling G (move).
- Task 8: deferred to the batched on-screen check (Ruling D); checklist = plan Task 8 + task 5/6/7 reports' on-screen checks + "typing a substring of a recent host selects the recent one first".
- Final: Ruling (I1): SSH launches get the user's shell environment by capturing it once — `$SHELL -ilc 'command env -0'`, stdin null, 3 s cap, off the UI thread, cached — as the base env for SSH programs (Local and daemon Create env); fall back to the current env. ssh stays the direct program (exit codes preserved, argv-only) — cost if wrong: a slow/noisy shell rc delays the first SSH open ≤3 s.
- Final: Ruling (I2): hold an SSH pane when code == 255, or when code is non-zero and the session lasted < 10 s; other exits close as today. Non-255 holds use "ssh ended (exit N) — press Enter to reconnect" and never auto-reconnect — cost if wrong: a quick `exit 1` after login holds instead of closing (⌘W closes).
- Final: Ruling (I3): a connection counts as "was up" only after it produced output AND lasted ≥ 60 s; only then do retries reset and a drop count as news — cost if wrong: none seen.
- Final: Ruling: re-grade to fix-pass — `Host foo # comment` junk rows (reviewer: fix before merge); auto-reconnect stealing focus (closes picker/rename box); a hung shared connection never cleared (Review Focus 3: after a timed-out -O check, remove that host's own control socket — path from `ssh -G -o ControlPath=<dir>/%C host` → `controlpath`).
- Final: Ruling: `ssh -G` / `ssh -O check|exit` keep fuxx's own environment (only the pane's ssh gets the shell env) — accepted for M10b: a -G failure only drops fuxx's reuse/keepalive options — cost if wrong: from Finder, reuse checks may use a different ssh binary than the pane.
- Final: Ruling: accepted side changes — TerminalView::new no longer takes focus itself (callers do); a rename box open on a reconnecting session carries over to the new pane — cost if wrong: on-screen focus check catches it.
- Final review clean. Task 9: dispatched (sonnet) — docs marked "built and reviewed; on-screen check pending" (Ruling D), batched checklist in note 51, deferred minors copied into the plan, commit the M11 input note too.

## [[52 - Milestone 10c - Agent Browser]]

- Spec: docs/04-Development/52 - Milestone 10c - Agent Browser.md. Branch m10c-agent-browser from m10b-ssh-workspaces 793dd76; plan commit ad26086. Autonomous run (user, 2026-11-05); Ruling D in the M10b ledger applies (on-screen checks batched; no merge until checked).
- Ruling H: Browser::eval/screenshot return a `futures::channel::oneshot::Receiver` if `futures` is already a dependency, else `std::sync::mpsc` polled from a GPUI timer, or `smol`/GPUI's own channel if GPUI re-exports one — implementer picks what the codebase already uses; no new crate for channels — cost if wrong: a type rename.
- Ruling I: fix the M10b timing flake (a_hung_master_s_socket_is_removed) in M10c Task 1's dispatch — widen its time margins the way 335d1a4 did for finish-expecting tests, without weakening what it asserts — cost if wrong: none.
- Task 0: minor (deferred): probe PNG path fixed per run; Dock icon may flash ms before Accessory; watchdog exits from a non-main thread; eval accepts only string results (Task 3 decides; Ruling J).
- Ruling J: Task 3's Browser::eval returns any JS value as serde_json::Value by having the wrapper script return JSON.stringify(result) (undefined → null) and parsing it — cost if wrong: values JSON can't hold (functions, DOM nodes) come back as null/{}.
- Task 1: dispatched (sonnet), BASE e5767f7 (+ Ruling I flake fix).
- Task 1: implementer DONE_WITH_CONCERNS fde2ba0 (Ruling I flake fix), 785f9c6 (463 tests ×3 green; RED 4/5). Review dispatched (sonnet).
- Task 1: minor (deferred): `http://a@evil` credentials kept (in spec); Ruling I fix not reproducible (watch full-suite runs).
- Ruling K: fold three Task 1 minors into Task 4 (which first uses normalize_url): refuse an empty port (`javascript:`, `about:`, `data:`, `http:/x` alone must be refused); take the explicit-scheme branch only when the text before `://` has no `/`, `?` or `#`; add the named-risk test cases (`JaVaScRiPt:alert(1)`, `JAVASCRIPT://x`, `//host`, `https:\\x`, `http:/x`, leading tab, `http://a@evil`) — cost if wrong: a few lines.
- Task 2: Ruling: render_outline's `title:` / `url:` header lines above the nodes are accepted (useful context for agents) — cost if wrong: two lines of output.
- Ruling L: fold two Task 2 minors into Task 3 (it edits browser_js for the eval wrapper): escape U+2028/2029 in literal(); `press` uses an own-property check (Object.hasOwn / Object.create(null)) so `constructor`/`toString` are unknown keys — cost if wrong: two lines.
- Task 3: Ruling: agent `eval` code runs as an expression first (CSP-safe) and falls back to `eval` for statements — accepted; the JSON-literal rule covers data arguments, and `eval` runs arbitrary JS by design — cost if wrong: an expression that breaks out of the wrapper only harms the agent's own result.
- Task 3: Ruling: WKNavigationDelegate failure callbacks added (plan said no delegate) — accepted: the blank-page rule alone misses failures when a page is already showing; errors read "the page failed to load: <WebKit reason>" — cost if wrong: one more objc2 class.
- Task 3: Ruling: target=_blank handled by a WKUIDelegate that loads in the same view (shim kept for window.open) — accepted.
- Task 3: Ruling: re-grade Minor 2 to Important by effect — the statements retry on any NSError can run a click/fill twice (double form submit) on pages that patch JSON.stringify; fix: retry only on a SyntaxError exception, and fuxx's own scripts never fall back — cost if wrong: a few lines.
- Task 3: Ruling: fold Minor 3 (an address NSURL can't read is silently "loaded" over a showing page → set failure "not a valid address") and Minor 5 (move the probe to src/browser/probe.rs as a child module, pure move) into the fix round — cost if wrong: churn.
- Task 4: dispatched (opus), BASE 5b4d6d5 (+ Ruling K).
- Task 4: implementer DONE_WITH_CONCERNS 9150805 (484 tests; RED 9/14 incl. 3 Ruling K defects). Review dispatched (sonnet).
- Task 4: Ruling: accepted — ⌘V bound for the address box only; `//host` and `/x` refused with "an address needs a host"; browser_open(index, url, cx) without window; 2 lines in workspace_daemon end_all_and_quit close browsers.
- Ruling M (re-graded Important, spec says ⌘C/⌘V/⌘A/⌘Z work in the browser and ⌘W closes it): add an Edit menu built from GPUI `MenuItem::os_action` (Undo, Cut, Copy, Paste, Select All — verified OsAction exists at the pinned rev, mapping to copy:/paste:/… selectors) wired to fuxx's existing terminal copy/paste actions where they exist (no new terminal behaviour; items without a fuxx action route only to the OS selector), so the browser window's WKWebView gets the standard selectors; and make ⌘W close the browser window when it is key (e.g. an NSWindow subclass handling the key equivalent, or a File-menu "Close Window" os path) — cost if wrong: a new visible Edit menu in fuxx (standard on macOS).
- Task 4: Ruling: on an SSH reconnect the browser follows the session (re-key old id → new id in replace_session; it keeps its row and name) + a Browsers rekey test — cost if wrong: one method.
- Task 4: fix round 1/5 dispatched (resume implementer): Important 1 + Ruling M.
- Ruling N: fold into Task 5 — the browser window's ⌘W/⌘Z match by keyCode (physical key), not charactersIgnoringModifiers, so on non-Latin layouts ⌘W can't fall through to the menu's Close Session and end a session — cost if wrong: a few lines.
- Task 5: implementer DONE_WITH_CONCERNS 042c928 (Ruling N), 6f82aa6 (499 tests; RED 11+1; probe ×2). Review dispatched (sonnet).
- Task 5: Ruling: Ruling N implemented as "the key's own letter on Latin layouts, US key position on other scripts" (as menus match) instead of keyCode-only, which would swap ⌘W/⌘Z on AZERTY — accepted — cost if wrong: none seen.
- Task 5: Ruling: browser.wait timeout keeps M9's "timed out waiting" text and exit 124 (the condition never held; the page answered) — accepted.
- Task 5: Ruling: browser methods listed in api::BROWSER_METHODS so M9's methods test stays untouched; MCP tool count 14 → 27 — accepted.
- Task 6: deferred to the batched on-screen check (Ruling D).
- Final: Ruling (I1): agent `browser open` (socket/CLI/MCP) brings the window forward without taking the keyboard (orderFront); only the ⇧⌘B address box uses makeKeyAndOrderFront — cost if wrong: an agent-opened browser may sit behind other apps.
- Final: Ruling: re-grade "read output and page titles reach the terminal with raw escape sequences (a page can retitle a session or fire a notification)" to Important by effect; fix: strip control characters (keep \n and \t) from page-derived text in the CLI's human-readable output (read, url/title, snapshot outline, eval pretty print) — JSON output stays exact — cost if wrong: a page's odd characters are dropped from human output.

## [[53 - Milestone 11 - UI-UX Uplift]]

- Spec: docs/04-Development/53 - Milestone 11 - UI-UX Uplift.md. Branch m11-ui-ux from m10c-agent-browser 1164ede; plan commit 54780eb. Autonomous run; Ruling D (M10b ledger) applies.
- Ruling O 
- Ruling O: when M10a's hold_saving is active, sidebar width/hidden/hint changes still apply in memory and are saved with the next allowed save (no special path) — cost if wrong: a resized sidebar isn't remembered in that rare run.
- Task 0: implementer DONE_WITH_CONCERNS d559c32 (501 tests; smoke PNGs ok). Ruling: scenes run via a hidden `__ui-shots` entry in main.rs behind the feature (`cargo run --features ui-shots -- __ui-shots [filter]`) because AppKit needs the main thread and libtest runs tests off it — accepted; instructions updated. Hand-written stored-deflate PNG encoder (no new crate) — accepted. Review dispatched (sonnet).
- Ruling P: fold into Task 1 — `ui_shots::render` registers fuxx's bundled fonts (fonts::register_bundled_fonts as appearance.rs:129 does) and scenes use fonts::gpui_font, so text in screenshots is real; fix the stale Cargo.toml comment — cost if wrong: one line.
- Task 1: Ruling: plain 1 px divider / sidebar border / panel border held to 1.4:1 (spec's Splits section asks for "quiet"); accent/focus lines and marks stay ≥ 3:1 — accepted — cost if wrong: dividers slightly faint on some themes.
- Task 1: Ruling: waiting colour held to 4.5:1 (also used as error text) — accepted.
- Task 2: implementer DONE_WITH_CONCERNS 3e8b3c5 (511 tests; RED on assertions). Controller viewed window-split-latte: clean. Ruling: `inset` does padding only; new GridLayout::fit shares the leftover (needs cell size) — accepted. Browser windows (raw NSWindow) aren't acted on by Window › Minimize/Zoom/⌘M — accepted (their own buttons work). Review dispatched (sonnet).
- Task 2: review Approved (sonnet). Ruling: band double-click follows the macOS title-bar setting (spec: "as macOS does") — accepted. Task 2: complete (commits 42c8b44..3e8b3c5, review clean).
- Ruling Q (re-graded by effect — agents like Claude Code draw prompts inside box borders, so the headline question would read "│ Do you want… │"): fold into Task 5 — waiting_line trims box-drawing characters (U+2500–U+257F) and surrounding spaces, maps tabs to a space, and when the matched line is only the pattern (e.g. "(y/n)") prefixes the previous non-empty line; + tests incl. "last matching line wins" and "a secret on the newest line gives None" — cost if wrong: a few lines.
- Task 4: implementer DONE_WITH_CONCERNS 40d0650 (518 tests; RED on assertions). Controller viewed marks-mocha: distinct shapes, crisp. Ruling: the `animations` setting and macOS Reduce motion both drive GPUI's App::reduce_motion (so all with_animation go still) — accepted; Working arc drawn as a filled outline (no round caps without lyon) — accepted. Review dispatched (sonnet).
- Task 4: Ruling (I2): cache the terminal pane views (GPUI AnyView::cached at the pinned rev, if present) so a sidebar/mark tick doesn't repaint grids; if caching isn't available or unsafe at this rev, report and keep the ticker but document the cost — cost if wrong: a terminal that misses a repaint (on-screen check).
- Task 4: Ruling: fold the minor — Reduce motion via NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification instead of the 5 s poll — into the fix round.
- Task 5: dispatched (opus), BASE 195a35f (+ Ruling Q).
- Task 5: implementer DONE_WITH_CONCERNS ef78bc2 (Ruling Q), 516eeba (sidebar) — 530 tests. Controller viewed sidebar-mocha/latte: designed, clear hierarchy.
- Task 5: Ruling: a coarse age-label redraw timer (about once a second for a Done row's first minute, then per minute/hour, never while hidden) is accepted alongside the 10 fps ticker — it's not continuous animation; frozen "12s" labels would mislead — cost if wrong: a wake per second during a Done row's first minute.
- Task 5: Ruling: group headers use OpenType small caps (no letter-spacing in GPUI at this rev) — accepted; amend spec wording in T10. 12 px footer/hint title off the 13/11 scale — accepted.
- Task 5: Ruling: any row with a sub-line (note/progress/"not live"/question) is 42 px, not only waiting rows — accepted (keeps the old note line); amend the constraint wording in T10.
- Ruling R: fold into Task 6 — notify the Workspace from the sidebar scroll handler (so a Working row scrolled into view starts the ticker) and clear `new_menu` in toggle_sidebar — cost if wrong: two lines.
- Task 7: implementer DONE_WITH_CONCERNS 5870777 (538 tests; RED 4). Ruling: daemon notification text unchanged (it doesn't know GUI numbers/groups) — accepted; "Go to Session…" / "Go to What Needs You" File menu items — accepted.
- Ruling S (queued for the fix round): notification body no longer repeats the summary — Waiting: body = the question (fallback "Needs your input"); Done: body = "Worked for <age_label of the Working stretch>" (fallback "Finished") — cost if wrong: a different notification body.
- Task 7: review Needs fixes (sonnet). Important: (1) overlay focus fight — opening ⌘P over rename/host picker/address box (or vice versa) closes the new one; (2) Ruling S.
- Task 7: Ruling: re-grade the rename selection being barely visible in Latte (216,219,226 on 232,235,239) to Important by effect — the point of the rename fix is that you see the old name is selected; use a selection colour with real contrast (e.g. accent at ~30 % on the field) — cost if wrong: a stronger highlight.
- Task 7: Ruling: fold the switcher look minors — filter box inset/softer focus border; placeholder clear of the caret — into the fix round.
- Task 8: implementer DONE_WITH_CONCERNS 980c01e (544 tests; RED 4). Ruling: window 760 × 646 (computed) — accepted; Sessions label wording change — docs in T10. Review dispatched (sonnet).
- Task 9: deferred to the batched on-screen check (Ruling D).
- Final: Ruling: re-grade three minors to a fix pass by effect — (1) text on hover backgrounds 4.03–4.23:1 breaks the 4.5:1 Global Constraint: add hover pairs to the contrast test and fix the hover tokens; (2) host picker and browser address box still use the old overlay styling (inconsistent with ⌘P in the same spot): share the switcher's panel styling + add their screenshots; (3) ⌘=/⌘−/⌘0 pressed in an agent-browser window change the terminal font size: the browser window swallows them (no-op) and the stale browser.rs comment is fixed — cost if wrong: small.
- Final: Ruling: residual — the address-box error text (ring_waiting) now sits on `panel`, where it's only guaranteed 3:1 on dark themes (4.5:1 is guaranteed on the sidebar) — parked as a deferred minor (no second fix wave): add ring_waiting on panel/panel_hover at 4.5:1 to drawn_pairs later — cost if wrong: a slightly dim error line in the address box on some dark themes.


## [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]] (2026-11-06)

- Spec: the 2026-11-06 amendments in notes 52 and 53. Branch m11b-browser-bar-prefs from m11-ui-ux fb94b14. User chose: build now before the merge; address-bar setting; bar-off keeps buttons; keep-data Off by default, On→Off deletes; sections General/Appearance/Sessions/Browser. Ruling D applies (on-screen batched).
- Task 2: implementer DONE_WITH_CONCERNS 0839ef9 (557 tests; RED 9; probe ok). Ruling: off-screen capture can't render Liquid Glass toolbar items (layout only) — the look is an on-screen check; the probe window is titled-but-hidden (borderless gets no toolbar); the page holds the keyboard from the start (field only on ⌘L/click) — accepted. Review dispatched (opus).
- Task 2: Ruling (re-graded Important — Global Constraint "Agents unchanged"): pages must keep following macOS light/dark for prefers-color-scheme (as before), not the fuxx theme; only the bar/window chrome follows the theme — give the WKWebView its own appearance tracking the system — cost if wrong: pages don't match the bar's light/dark.
- Task 2: Ruling: fold Minor 1 (honour willBeInsertedIntoToolbar) and Minor 2 (the 100 ms loading tick only for browsers whose window can be seen) into the same fix round — cost if wrong: a few lines.
- Ruling T: fold Task 2b Minor 1–2 into Task 3 — mark NSImageView setEditable(false) + the probe asserts it — cost if wrong: one line.
- Task 3: implementer DONE_WITH_CONCERNS b010aa3 (Ruling T), f806edc (561 tests; RED 4). Window 920 × 452. Review dispatched (sonnet).
- Task 3b: implementer DONE_WITH_CONCERNS abf0ddb (564 tests; RED 3). Ruling: full screen keeps an empty 38 px band so the button doesn't sit on the first row — accepted. Review dispatched (sonnet).
- Ruling U: fold the 3b re-toggle-mid-slide jump into Task 3c (store the current shown fraction as the slide's `from`) — cost if wrong: a few lines.
- Task 3c: Ruling (correcting my own dispatch decision, by effect): the active row must NOT always show × — it hid the focused session's status mark; × shows on hover only, for every row — queued for the fix round.
- Task 3c: Ruling: "Close Tabs to the Right" closes only rows visible below the clicked one (not sessions folded inside a collapsed group) — queued for the fix round.
- Final: Ruling (I1): keep-data Off must not leave saved data on disk — persist a flag (`SavedState.browser_data_kept`) set whenever a browser opens with keep-data On; when the setting is Off and the flag is set, wipe at launch and when the last keep-data-On window closes; clear the flag only after WebKit reports the deletion done — cost if wrong: one extra wipe call at launch.
- Final: Ruling: fold Minor 1 — the right-click tab menu closes when the keyboard moves to a terminal / the active session changes (⌘1–9, ⌘T, ⇧⌘U) — into the fix pass.
- Final: Ruling: parked residual N1 (no second fix wave) — while M10a's hold_saving is active (background service lost / failed List), opening a keep-data-On browser can't persist browser_data_kept, so a later Off might not wipe that data — rare (needs a lost service and keep-data On in the same run); document as a known limitation — cost if wrong: saved browser data may survive a later Off in that rare run.

## [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]] (2026-11-06)

Branch `m11b-workspaces` from `m11b-browser-bar-prefs` (3dc3d27). You said "just continue", so it was built before your batched check (Ruling D applies: the on-screen check comes after). Same layout as above: each ruling, then what it costs if wrong, in plain words. The three that change what you see most are repeated in note [[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]] under "Rulings you should confirm".

**Task 1: the layout model**

- Task 1: Ruling accepted: the loader takes the saved state plus the terminal numbers (`from_old` / `from_saved`), because tabs need real `sN` ids and sessions open first. Cost if wrong: a signature change.
- Task 1: Ruling accepted: the saved-file version becomes 2 (`SAVED_VERSION`), kept in the layout model. Old files still load. Cost if wrong: one number.
- Task 1: Ruling accepted: an old layout that becomes a workspace takes its first session's name and colour. Cost if wrong: an old workspace may get a different name than you expect.
- Task 1: Ruling accepted: creating something focuses it; a new tab goes at the end of the bar; moving a tab may cross workspaces; positions follow the sidebar order. Cost if wrong: one-line changes each.

**Task 2: the workspace on the model**

- Task 2: Ruling accepted: renaming a sidebar row names the workspace (`session.rename` still names terminals); colours belong to workspaces (pane headers lose the dot). Cost if wrong: names or colours attach to the wrong thing.
- Task 2: Ruling accepted: the row menu says "…Workspace…" (Close Workspace, Close Other Workspaces, Close Workspaces Below, Rename Workspace…, Workspace Color); version 1 files load in the old sidebar order; `fuxx ls` text is unchanged (the new fields are in `--json`). Cost if wrong: wording, or a script that parses `fuxx ls` text would break (it doesn't).
- Task 2: Ruling accepted: `#N` is a terminal's place; ⌘N and the window title are a workspace's place; ⇧⌘] / ⇧⌘[ cycle workspaces. Cost if wrong: numbers mean something different from what you expect.
- Task 2: Ruling (review, item 5): restore keeps M10a's fallback: if the saved focus didn't come back, focus the workspace of the first live or adopted terminal, not a freshly restarted shell, with its original test. Cost if wrong: after a restart you land in a new shell instead of your live agent.
- Task 2: Ruling (review, item 9): menus and hints follow the new model: "Rename Workspace…", "Move Workspace Up/Down", ⌘W says "Close Tab", the hint says "⌘T new workspace" (changed again by Task 4d). Cost if wrong: wording.
- Task 2: Ruling (review, item 10): keep split coverage in the sidebar and tab screenshots (a split workspace in those scenes or its own scene). Cost if wrong: a split bug could slip past the screenshot checks.
- Task 2: Ruling (re-graded Important, a risk for later tasks): a runtime cross-check in `sync()`: every session has exactly one terminal tab and every terminal tab has a live session; if not, log it, assert in debug builds, and repair an untabbed session. Cost if wrong: a terminal with no tab (or a tab with a dead terminal) would go unnoticed.
- Task 2: Ruling (re-graded Important, a risk for browser tabs): sidebar rows, the active row and "row on screen" are keyed by workspace, not by position in a filtered list, and the dead `Model::close_workspace` is deleted. Cost if wrong: one workspace with no terminal would shift every row below it.

**Task 3: pane tab bars**

- Task 3: Ruling accepted: a tab's age shows in its tooltip (no room in a 90 px tab); separators sit only between unfilled tabs (contrast floor); Close Other Tabs in Pane always asks. Cost if wrong: small look and wording changes.

**Task 4: the browser in a pane**

- Task 4: Ruling accepted: agents drive only tabs in a pane, never popped-out windows. Cost if wrong: an agent can't reach a page you popped out.
- Task 4: Ruling accepted: Pop Out moves the live page (no reload, so scroll position and login stay). Cost if wrong: a reload would lose page state.
- Task 4: Ruling accepted: ⇧⌘B opens its address in the focused workspace's browser tab (or splits right to make one). Cost if wrong: it would open a separate window as before.
- Task 4: Ruling accepted: closing a workspace closes its browser tabs; fuxx quits only when no terminals and no workspaces remain; the per-session browser tests were replaced; the ⌘P switcher's entry id is a workspace id. Cost if wrong: a workspace of pages could be left behind, or fuxx could quit too early.
- Task 4: Ruling: re-grade review #15 to Important: popping out the last tab used to quit fuxx and close the window just opened. Popped-out windows now count in the quit rule. Cost if wrong: you lose a page you just popped out.
- Task 4: Ruling: fold five cheap minors into fix round 1: #4 (suggestions panel misaligned or clipped), #5 (enabled and disabled buttons looked the same), #8 (give the keyboard back before clearing the GPUI link), #9 (a failed Pop Out keeps the tab), #13 (pages follow live light/dark changes). Cost if wrong: a few lines each.
- Task 4: Ruling: #14 (⌘R / ⌘[ / ⌘] / ⌘= ⌘- ⌘0 do fuxx's thing inside a focused page) was a plan question, left as is and raised with you. You answered yes: those keys act on the page (Task 4e). Cost if wrong: none now.

**Task 4b: resizing doesn't look busy** (your bug report)

- Ruling: output in the 500 ms after a resize changes no status (no Working, no Done, no notification), but a real notification during a resize still counts, so it isn't lost. Cost if wrong: a working agent's first 500 ms of output after a resize doesn't refresh its Working age.
- Task 4b: Ruling accepted: `TerminalCore::resize` returns whether a new size was sent, and the view calls `on_resize` only then. Cost if wrong: a no-op resize would open the quiet window for nothing.
- Task 4b: Ruling (review): while a terminal is Working, output inside the window still counts as output (it can't change the status), and the settle-to-Done check waits while a resize window is open. Cost if wrong: a just-finished agent's Done comes up to 500 ms later after a resize.
- Task 4b: Ruling: the scoped re-review was done by the controller (an 8.7 KB diff), not a fresh reviewer. Cost if wrong: a missed edge in about 20 lines.

**Task 4c: empty tab bar drags the window**

- Task 4c: Ruling (re-graded by effect): a stale "band pressed" flag could make the window jump on a later plain hover over a top bar, so a one-line guard (start the move only while the left button is held) went into Task 4d. Cost if wrong: a rare window jump until 4d landed.

**Task 4d: ⌘T and ⌘N**

- Ruling: ⌘T with a browser tab focused still opens a terminal tab in that pane; with no workspace open it makes one. Cost if wrong: one condition.
- Task 4d: Ruling: ⌘T's new tab goes at the end of the pane's bar (like the bar's new-terminal button), overriding the brief's "after the active tab". Cost if wrong: a one-line index change.
- Task 4d: Ruling (review): rename the siblings to say what they make ("New Workspace in Folder…", "New SSH Workspace…", "New Agent Workspace") and pin the hint line and menu labels in tests. Cost if wrong: wording.

**Task 4e: page shortcuts**

- Task 4e: Ruling accepted: the page keys act whenever the browser tab holds fuxx's focus (the page, its bar or its address field), not only while the page itself has the keyboard. On a browser tab you mean the page. Cost if wrong: one `has_keyboard()` check.
- Task 4e: Ruling accepted: ⌘R / ⌘[ / ⌘] in the address field close it first (as a bar-button click does), and a disabled action does nothing (no fallback to fuxx's meaning). Cost if wrong: a key press that seems to do nothing.

**Task 5: sidebar rows, pinning, git branch**

- Task 5: Ruling accepted: background-service terminals use OSC 7, else their start folder (the service protocol has no pid); SSH rows show the host and no branch; grouping leaves pinned workspaces in place. Cost if wrong: a service terminal's row may show an old folder after a `cd` if the shell doesn't report it.
- Task 5: Ruling accepted: on the selected row, status marks are drawn in `on_accent` (the status colours fail 3:1 on the accent fill); their shapes still differ. Cost if wrong: the marks lose their colour on that one row.
- Task 5: Ruling accepted: `/usr/bin/git` runs only when the developer tools are installed, so there is no install prompt every 5 s. Cost if wrong: without the tools, rows show only the folder.
- Task 5: Ruling accepted: rows grow from 28 to 42 px (waiting 42 to 56), and the screenshot checks were updated. Cost if wrong: fewer rows fit on screen.
- Task 5: Ruling (review #1): the rename box draws its own text colour; on the selected row it was unreadable. Cost if wrong: every rename would be blind on the selected row.
- Task 5: Ruling (review #2): a local terminal accepts the OSC 7 folder only from this Mac's hostname (or empty or `localhost`); otherwise the kernel's cwd is used. Cost if wrong: after `ssh`, the remote's folder could stick and even show this Mac's branch for it.
- Task 5: Ruling: fold the visible, cheap minors into fix round 1: #3 (folder name lost at 180 px, a gap before •), #4 (narrow waiting question showed "Do…"), #5 (per-frame row rebuild), #8 (unpinning reordered groups), #9 (pinned rows ran into ungrouped rows: separate them), #10 (row drag preview was a 28 px name chip), #11 (prune `Places.branches`), #12 (stale dead-code allow). Cost if wrong: small look changes.
- Task 5: Ruling accepted (fix round): a background-service terminal's OSC 7 also needs this Mac's host; bidi-override characters are rejected like control characters; a row's place check runs at most once a second. Cost if wrong: a new row may show its start folder for up to a second.

**Task 6: title area and the 🔔**

- Task 6: Ruling accepted: the workspace name shows in the 38 px band only while the sidebar is hidden. With the sidebar shown there is no band over the content, and the accent-filled row names the workspace (a band would cost about two rows of every pane). Flagged for you at the on-screen check. Cost if wrong: about ten lines and re-checking the window drag from 4c.
- Task 6: Ruling accepted: closed terminals' entries stay greyed and unclickable; entries store only `sN`, so the workspace name is looked up live and survives tab moves. Cost if wrong: an entry could name an old workspace.
- Task 6: Ruling (review #1, Important): `fuxx notify` is recorded in the 🔔 list too. Cost if wrong: scripts' notifications show a banner but aren't in the list.
- Task 6: Ruling (review #3): draw an outline bell like the sidebar button and + (hover dims or brightens), instead of the colour emoji. Cost if wrong: swap back to the emoji glyph.
- Task 6: Ruling (review #5): an exhaustive match, so a new overlay kind can't miss the "hide the page" list. Cost if wrong: none (compile-time).
- Task 6: Ruling accepted (fix round): a script's notify entry targets its caller (`FUXX_SESSION`) while that terminal is open; from outside fuxx it has no terminal and isn't clickable. No new API parameter (outside the plan). Cost if wrong: one `target` parameter.

**Task 7: drag tabs anywhere**

- Task 7: Ruling accepted: the drag chip hangs 6 px below the pointer (so the insertion line stays visible); Esc cancels tab drags only; dropping on the tab's own workspace row moves it to the end of the focused pane; the page already hides during any GPUI drag. Cost if wrong: small look and behaviour tweaks.
- Task 7: Ruling (the implementer's, pending review, then overturned by the reviewer): "a drop at the tab's own place does nothing" became "a drop at the tab's own place selects it", because a shaky click over 2 px is a GPUI drag with no click. Selection happens in the bar's drop, not on press. Cost if wrong: a shaky click would not select the tab.
- Task 7: Ruling accepted (fix round): the broken `splits` screenshot check ("1 px plain divider") failed since before M11b, because it sampled y = 300 where another divider sits and passed or failed with the screen's scale. It now samples the vertical divider at y = 450. Cost if wrong: none to the app.

**Task 6b: the 🔔 and + in the footer** (your request)

- Task 6b: Ruling: with the sidebar hidden the 🔔 is hidden too (like the gear), and its popover opens upward from the footer. Cost if wrong: the unseen dot is invisible while the sidebar is hidden.
- Task 6b: Ruling (review): carry #1 (popover anchored to the bell's left, straddling the sidebar edge: anchor it to the sidebar's right edge and gear) and #2 (the bell had no "open" state while its popover showed: mirror the +) into the final fix pass. Cost if wrong: a popover that overhangs and a bell that doesn't show it is open.

**Final review**

- Final: Ruling: re-grade m1 (a workspace of pages was dropped on relaunch when no terminal was left) to Important, because the spec says workspaces and browser addresses are restored. Cost if wrong: you lose a pages-only workspace on relaunch.
- Final: Ruling: fold m2 (dead `Model::close_pane`, `workspace_of`, `Browser::open`), m3 (stale ⌘N comments) and Task 6b #1 and #2 into the one fix pass. Cost if wrong: none.
- Final: Ruling accepted: the + menu stays out of the overlay list (it holds no keyboard), but `close_overlays` and a new `close_menus()` (called from `activate()` and from ⇧⌘U's already-active path) close it, and it hides pages. Cost if wrong: a shortcut could leave the + menu open.
- Final: Ruling accepted: the popover's width follows the sidebar (228–340 px) with its right edge at the gear. Cost if wrong: look only.
- Final: Ruling accepted: pages-only workspaces come back with the background service whatever session-restore says (as browser tabs beside live terminals already did); without the service, session-restore Off still starts fresh. Cost if wrong: a pages-only workspace appears after a restart even though restore is Off, while the service runs.
