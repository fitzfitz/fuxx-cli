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

- **Current milestone:** none in progress. **v0.5.0 released 2026-11-10** (M11i–M11n; run 42662570303). Next: your combined on-screen check (129 checks; notes 59–65), then a chores round planned with you (the planning round may also pick up the fuxx-completes repo, `github.com/fitzfitz/fuxx-completes`, design stage). M11n — performance 🟨 built, reviewed, merged, pushed and released 2026-11-10 (every target met). M11m — title bar 🟨 built, reviewed, merged and pushed 2026-11-09; M11l — command blocks 🟨 built, reviewed, merged and pushed 2026-11-09; M11k — first-run screen 🟨 merged and pushed 2026-11-09; M11j — command palette 🟨 merged and pushed 2026-11-09; M11h — Changes, diff view, Activity 🟨 merged and pushed 2026-11-09; M11i — UI uplift 2 🟨 merged and pushed 2026-11-08 (all released in v0.5.0, 2026-11-10). Earlier: M11g released as v0.4.0/v0.4.1. M11e — Update Available 🟨 released as v0.3.0 (2026-11-08), on-screen check of an update pending. M11d — Help menu 🟨 merged and pushed, on-screen check pending. M11c — native notifications and the app icon 🟨 built, on master, on-screen check pending (the banner's icon is still blank). M10b, M10c, M11, the follow-up and M11b 🟨 merged and pushed, on-screen checks pending. Then M12. M0–M9 and M10a ✅ done. See [[00 - Plan Tracker (Start Here)]].
- **Overall phase:** **PIVOTED.** v1 CLI wrapper shipped & archived; now building **fuxx**, a native GPU terminal for AI agents, in Rust (GPUI + `alacritty_terminal` + `portable-pty`). Repo repurposed; vault re-founded.
- **Repo:** `github.com/fitzfitz/fuxx-cli` — public; v1 on `master`. Repo **repurposed** for the terminal app.

---

## Entries

### 2026-11-10 — The grey line above prompts removed; two chores noted
- **Did:** On your screenshot of stacked prompts ("ada garis pemisah abu, jelek") and your choice ("ya hapus"), removed the M11l hairline above each prompt and above the live prompt. First checked it was not a bug: replaying a real zsh + powerlevel10k session (a copy of your `.p10k.zsh` in a scratch HOME) through fuxx's parser showed empty prompts make no blocks; the lines were the design's (one above the live prompt even in a fresh terminal). Gone with it: `block_view::SEPARATOR`, `separator_rows`, the snapshot's `live_prompt` and `history_size`; `blocks::live_prompt` is tests only. TDD: the `command-blocks` ui-shot was changed first to expect no line and failed on the old code (it saw `card_border` above row 3), then passed. 1289 tests pass; the command-blocks scenes pass. Noted two chores candidates in the roadmap at your request: shell choice in Preferences, and naming the finished command in notifications (AI's proposal, not yet agreed).
- **Next:** your look on screen (no grey line above any prompt; the bars unchanged); then merge, and the chores round with you.

### 2026-11-10 — v0.5.0 released
- **Did:** Released **v0.5.0**: `chore(release)` `7f85fad`, tag `v0.5.0`, GitHub Release run 42662570303 green, asset `fuxx-0.5.0.zip` (https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.5.0). It bundles M11i (UI uplift 2), M11h (Changes, diff view, Activity), the names revert (names are yours, else the program's title, else ╭∩╮), M11j (⌘K palette), M11k (first-run screen), M11l (command blocks), M11m (title bar) and M11n (performance). Notes 59–65, the roadmap, M11n's plan (Tasks 6 and 7, 9/9) and the tracker now say released.
- **Next:** your on-screen check of 0.5.0 (129 checks in one list, as an interactive checklist page; per-note lists in notes 59–65), then plan the chores round with you (it may also pick up fuxx-completes). Found for later, from M11n: cache the window chrome so a breath frame doesn't re-render it all.

### 2026-11-10 — M11n performance built, reviewed, merged and pushed
- **Did:** Built M11n task by task with a fresh builder and reviewer each. The harness came into `scripts/bench/`
  (it backs up, restores and verifies your Ghostty prefs, and kills every process it starts on any exit). The byte
  scanners jump over plain text with `memchr`; a parser thread takes the PTY's reads in 64 KiB batches with one UI
  wake per batch; `Theme::ui()` is computed once per theme; the breath is drawn at level crossings (22 frames per
  breath instead of 32, never a bigger step), sleeps through the hold and rests at full strength after five minutes
  untouched. 1289 tests pass, the full ui-shots set passes (147), clippy and fmt are clean.
- **Measured** (one session, `6fbced0` against the code before M11n, window in front for every CPU run):
  - idle, glow on: 6.99 → **5.35 %** (5 runs each), **0.765 ×**, target ≤ 0.8 ×. The mean ratio is 0.851 × because
    of one unexplained low base run (2.83 %, the first of the session; kept, since nothing showed it invalid); without
    it 0.75 ×, as the frame model predicted;
  - after five minutes untouched: 6.99 → **0.93 %** (glow off 0.75 %), one run per app;
  - glow off 0.75 → 0.74 %;
  - `cat` 100 MiB 0.931 → **0.481 s**, Ghostty 0.338 s = **1.42 ×** (was 2.68–2.93 ×);
  - 200k coloured lines 0.219 → **0.156 s**, Ghostty 0.155 s = **1.01 ×** (up to 1 MiB can be unparsed when `cat`
    returns, so maybe 1.07 ×);
  - launch 163 → 162 ms; RSS 121.1 / 124.6 → 119.2 / 125.0 MB.
- **Rulings beyond the plan:** `MarkScanner` also cuts at CAN/SUB; one buffer per terminal, so a drained terminal
  holds at most 64 KiB; the daemon's subscriber and writer queues got a 1 MiB byte cap each, restoring their real old
  bound (macOS reads were about 1 KiB, so before M11n they held about 1 MiB and 256 KiB; a bigger Snapshot waits and
  goes in alone); turning the glow, Animations or Reduce motion back on ends the rest, and a hidden cursor
  reappearing restarts the breath (0.6 s solid, then a peak); a ui-shots `cursor-flood` scene guards `output_seen`.
  The first Ghostty runs (grid, launch) rewrote your `NSWindowLastPosition` before the restore rule existed; the old
  value is lost, as I told you.
- **Found for later:** the breath still costs about 5.3 %, because every breath frame re-renders the whole window
  chrome and only the panes are cached. Caching the chrome could save more than repainting only the cursor. For the
  chores round.
- **Limits:** nothing verified on the real screen yet; `cpu_rest` is one run per app. Details: note 65, "M11n as
  built".
- **Next:** release v0.5.0, then your combined on-screen check (note 65 has 11 items), then plan the chores round with
  you, and the fuxx-completes repo.

### 2026-11-09 — M11n performance designed and planned
- **Did:** Profiled fuxx (release build, isolated scratch `HOME`, `sample` and `ps`, test variants built from scratch
  copies) and wrote the design ([[65 - Milestone 11n - Performance]]) and the plan ([[M11n Plan - Performance]]: the
  harness and baseline, 4 build tasks, the benchmark, merge, the **v0.5.0** release, docs). Every call is AI's.
- **Found:** **Idle:** each breath tick (20/s) redraws the whole window, because GPUI marks the terminal and its
  ancestor, the workspace, dirty: render, Taffy layout and paint of all the chrome. CPU tracks breath frames (~0.3 %
  per frame per second: 5/10/20 a second read 3.1/~5.0/7.3–8.0 %); `Theme::ui()` recomputes contrast on every render
  (8 % of a frame). **Throughput:** macOS gives ~1 KiB per PTY read (105,515 reads for 100 MB); the single reader
  thread spends 48 % parsing, 29 % in four byte-by-byte scanners, 19 % in `read`; skipping the scanners on plain text
  alone measured −25 %. Also: since M11k the old harness's deleted `sessions.json` shows the Welcome card, so the glow
  never ran in it; fixed in the plan's harness.
- **Plan:** memchr scanners; a parser thread with 64 KiB batches and one UI wake per batch; `Theme::ui()` once per
  theme; the breath drawn at level crossings (same curve, never a bigger step than today, 22 frames per breath
  instead of 32), asleep through the hold, at rest after five minutes untouched. Targets: breathing idle ≤ 0.8 × the
  baseline, ≈ glow-off after 5 min, `cat` ≤ 1.8 × Ghostty (from 2.68), coloured output ≤ 1.4 × (from 1.58).
- **Next:** build M11n (Task 0), then release v0.5.0.

### 2026-11-09 — M11m title bar built, reviewed, merged and pushed
- **Did:** Built the title bar task by task with a fresh builder and reviewer each. Open: the sidebar button at the
  right edge of the sidebar's band (lined up with the footer ⚙) and **"the Fuxx?"** centred there. Collapsed: the
  workspace name centred on the window and **[🔔] [⚙] [sidebar]** at the far right, the 🔔 carrying the unseen dot,
  and its popover dropping down from under the band. Every toggle closes the popover; both ⚙ say "Preferences (⌘,)".
  The reviews led to: the collapsed name waiting for the hide slide (it painted over the outgoing sidebar for up to
  160 ms), the open button clamped to a window narrower than the sidebar, one shared band-button shell and tooltip
  constant, and sharper shots (the title's ink held to a measured −1 pt baseline, the lit 🔔 fill, no band buttons
  while the sidebar shows). The band 🔔 closes the Welcome card, as any overlay does (accepted). The full ui-shots
  run also exposed the M11l `command-blocks-rprompt-tight` failure: a test bug (GPUI rounds the chip's edge and width
  separately; 0.5 pt is half a device pixel at 1x), fixed with a 1-device-pixel tolerance; the product was fine.
  1259 tests pass, clippy (both) adds no warnings, the full ui-shots set passes, built in its own target directory
  (the shared one served another worktree's binary).
- **Limits:** no minimum window size (under about 170 pt the band icons can reach the traffic lights); the Latte
  hover fill is faint (pre-existing); nothing verified at 2x or in the real app. What the build settled: note 64's
  "M11m as built".
- **Next:** design and plan M11n (performance), then the v0.5.0 release, then your combined on-screen check (note 64
  has 20 items), then a chores round planned with you.

### 2026-11-09 — M11m title bar designed and planned
- **Did:** Wrote the design ([[64 - Milestone 11m - Title Bar]]) and the plan ([[M11m Plan - Title Bar]], 3 build
  tasks, merge, the folded release, docs) for your title bar: open, the sidebar button at the right edge of the
  sidebar's band (lined up with the footer ⚙) and **"the Fuxx?"** centred there; collapsed, the workspace name
  centred on the window and **[🔔] [⚙] [sidebar]** at the far right, with the popover dropping down from that 🔔.
  Every detail beyond your layout is AI's call and marked in the note: the title 13 pt semibold dim, never cut,
  hidden on sidebars under 224 pt; the name truncated at a fixed 100 pt inset (hidden under 260 pt); the band 🔔
  carries the unseen dot (until now there was none with the sidebar hidden); every toggle closes the popover; the
  footer ⚙ gets a "Preferences (⌘,)" tooltip to match the band's.
- **Found in the code:** the sidebar button was one element at a fixed place for both states; collapsed with the
  right sidebar open the window's top is two bands and the name was centred on the pane column only; the popover's
  geometry was footer-only (upward). The plan handles each.
- **Next:** build M11m (Task 1), then M11n, then the one combined release.

### 2026-11-09 — M11l command blocks built, reviewed, merged and pushed
- **Did:** Designed (note 63, every call AI's except the chip's shape) and planned M11l, then built it task by task with
  a fresh builder and reviewer each: OSC 133 marks read from the raw bytes and kept as hidden tags in the grid itself
  (so reflow, scrollback and the service's snapshot carry them); a zsh integration injected through `ZDOTDIR` without
  touching your dotfiles, with the `command-blocks` setting as the off switch; a gutter bar per command (red when it
  failed), a hairline between commands and a hover chip with Copy command / Copy output / Rerun (types the command,
  never presses Return); ⇧⌘↑ / ⇧⌘↓ to jump, ⇧⌘C to copy output; palette and Help rows. Your two requests along the
  way: a darker tab bar (the empty tab area was hard to tell from the tab box), and the one-row chip, chosen from
  mockups ("oke begitu saja"), which leaves a right prompt visible. Reviews caught and fixed: prompts joining the
  previous output (PROMPT_SP), a long command never becoming a block, the chip closing on any output, guessable
  Rerun ids (now a random seed), two A marks per prompt with pure, and, in the final review, `exec zsh` leaving a
  block running forever and other precmd hooks' prints landing in the finished command's output. Your powerlevel10k
  bug (only the first command got a block) was root-caused: p10k rebuilds `PROMPT` at every prompt and overwrote the
  marks; the integration now marks at every prompt and uses p10k's own shell integration. The title-area ui-shot
  that failed on master too was M11i's removed sidebar edge that the footer model still subtracted; fixed, and the
  check reads every device pixel. 1251 tests pass (1 ignored: the opt-in real-p10k test, which passed against your
  install in a temp HOME), clippy (both) adds no warnings, the full ui-shots set passes (145). What the build
  settled and its limits: note 63's "M11l as built" and "Limits". **Collapse is not in M11l** (alacritty can't fold
  rows). Not released on its own (one release after M11n).
- **Also (your decisions, 2026-11-09):** renumbered: **M11m = Title bar** (sidebar open: the sidebar icon at the right
  edge of the sidebar's top band and the title "the Fuxx?" centred there, 🔔 ⚙ stay in the footer; collapsed: the
  title hidden, the workspace name centred in the top band, [🔔] [⚙] [sidebar] at the far right; not designed yet)
  and **M11n = Performance**. The one combined release (M11i–M11n) ships after M11n ("push the release after
  performance milestone"); a chores round is planned after it.
- **Next:** M11m (title bar): design it with you first; the on-screen check of note 63's 22 items waits for the
  release.

### 2026-11-09 — M11k first-run screen built, reviewed, merged and pushed
- **Did:** Designed (note 62, every call AI's) and planned M11k, then built it task by task with a fresh builder and
  reviewer each: a `welcomed` record in sessions.json (older files count as welcomed, so upgrading users never see
  it); agent detection on your login shell's PATH (background, capped, nothing installed or run); the Welcome card
  (folder, your presets with "Not found on your PATH", three shortcuts, Skip / Start); starting an agent replaces
  the untouched first terminal only once the new one exists; Help → Welcome to fuxx; "Show Welcome Tips" →
  "Show Tips Card". Reviews caught and fixed: the folder field not opening the panel, the card losing the keyboard
  when a script closed a terminal or anything focused a tab, a dimming seam at the top band, and ⌘W quitting fuxx
  from the card. 1159 tests pass, clippy (both) adds no warnings, the full ui-shots set passes. What the build
  settled: note 62's "M11k as built". Not released on its own (one release after M11l).
- **Next:** M11l (command blocks), the last of the program; then the one release and your combined test list.

### 2026-11-09 — M11j command palette (⌘K) built, reviewed, merged and pushed
- **Did:** Designed (note 61, every call AI's) and planned M11j, then built it task by task with a fresh builder and
  reviewer each: an in-house fuzzy matcher; every action (a parity test against the real key bindings, menus and the
  ⋯ menu), setting and workspace in one list; files from `git ls-files` or a bounded walk, ranked off the UI thread;
  the palette view (⌘K on All, ⌘P on Workspaces, the old ⌘P panel gone, File menu "Command Palette…" and "Go to
  Workspace…"); files and settings. Reviews caught and fixed: ⌘V not pasting, Enter opening an earlier query's
  file, a stuck "Listing files…", a retry storm after a failed listing, and macOS privacy prompts from walking the
  home folder. 1141 tests pass, clippy (both) adds no warnings, the full ui-shots set passes. What the build
  settled: note 61's "M11j as built". Not released on its own (one release after M11l).
- **Also:** a local benchmark (fuxx 0.4.1, fuxx master, Ghostty 1.4) and web research on Warp and cmux: fuxx is on
  par with Ghostty on size and memory, about 35 ms slower to launch, about 2.7× slower on raw output, and uses about
  6 % CPU at idle (the cursor's breathing glow redraws 20×/s; about 0.9 % with it off) against Ghostty's 0.2 %.
- **Next:** M11k (first-run screen); your call on a performance follow-up for the idle CPU.

### 2026-11-09 — Names back to ╭∩╮ (your change to M11i)
- **Did:** On your word ("I still want the ╭∩╮ to be my default terminal name. It's my signature." … "we go back as
  before, unless its a process thing that auto rename the cli"), names are as before M11i: your name, else the
  program's own title, else the preset, else ╭∩╮, for terminals and workspaces everywhere. `smart_name.rs` removed;
  the `ps` look-up stays for the status bar, the port chips and the idle dot, and it and the folder look-up now run
  only for what shows them. Your "I cannot rename them at all": a screenshot test now renames by typing (tab and
  row double-click, ⌘R; idle and running; the program changing its title meanwhile) and the name sticks; no fault
  found in the code, so it's on your on-screen list. Reviewed twice; 1086 tests pass; merged and pushed.
- **Next:** M11j (command palette).

### 2026-11-09 — M11h Changes, diff view, Activity built, reviewed, merged and pushed
- **Did:** Planned M11h from note 59 (17 AI's calls added to the note), then built it task by task with a fresh
  builder and reviewer each: the Changes git core; the diff core; diff mode in file tabs (File | Diff, two number
  columns, red/green tints, change-bar clicks); the right sidebar's Files · Changes · Activity tabs and the Changes
  panel; a per-terminal activity log (in memory, last 200); the Activity panel (header, the waiting question in a
  card, a newest-first timeline). Reviews caught and fixed: non-UTF-8 files never showing a diff, wrong line numbers
  under `diff.suppressBlankEmpty`, size limits that ignored a deleted file's old version, staged deletions and
  deleted folders showing "no longer exists", a static Diff tab re-colouring every 5 s, `+`/`−` signs below 3:1.
  1089 tests pass, clippy (both) adds no warnings, the full ui-shots set passes. What the build settled: note 59's
  "M11h as built". Not released on its own (one release after M11l).
- **Next:** your names change (╭∩╮ back as the default name, branch `m11i-names`), then M11j (command palette).

### 2026-11-08 — M11i UI uplift 2 built, reviewed, merged and pushed
- **Did:** Built M11i autonomously (your "go ahead do them all autonomously"), task by task with a fresh builder
  and a fresh reviewer each: (1) design tokens and 16/12 pt terminal padding; (2) smart names (`claude · fuxx-cli`,
  `npm run dev · api`, the folder at a prompt; flag values and paths never show); (3) compact 22/32 pt rows with a
  tinted selection and an accent bar; (4) panes and the right sidebar as rounded cards with 6 pt gaps and a focus
  ring; (5) a 30 pt tab bar with +, ⋯ (New Terminal Tab, New Browser Tab, Split Right, Split Down) and the sidebar
  toggle; (6) Files type icons, indent guides, a root header and collapse all; (7) a status bar (status, branch,
  folder, listening ports via `lsof`, the update pill moved there). The final whole-branch review found two things,
  both fixed: names were forgotten when a terminal left the screen (names blinked, ⌘P lost them), and a waiting
  row's question wasn't in its tooltip. 1019 tests pass, clippy (both) adds no warnings, the full ui-shots set
  passes (Mocha and Latte). What the build settled is in note 60's "M11i as built". Not released on its own (your
  rule: one release after M11l).
- **Next:** M11h (Changes, diff view, Activity; designed in note 59), then M11j, M11k, M11l, then one release and
  the combined on-screen test list.

### 2026-11-08 — UI uplift requested: M11i–M11l
- **Did:** From your screenshot ("I don't really like the current layout… a modern CLI with good UI/UX"), 11
  suggestions; you chose all of them. Split into four milestones in your agreed order: **M11i UI uplift 2** (1–8),
  then M11h (Changes, diff, Activity, already designed), then **M11j command palette**, **M11k first-run screen**,
  **M11l command blocks**. Scope and roadmap updated.
- **Next:** design M11i.

### 2026-11-08 — v0.4.1 released (download retries)
- **Did:** On your word: merged the retry fix into `master`, version 0.4.1 (953 tests pass), pushed `master` and tag
  `v0.4.1`; the Release run went green (42470892331). GitHub's release downloads were still answering HTTP 504 right
  after (for v0.4.0 and v0.4.1 alike), so updating has to wait until GitHub recovers.
- **Next:** once downloads work, Update and Restart (your 0.3.2 goes straight to 0.4.1); then note 59's M11g checklist.

### 2026-11-08 — Updater: retries on a failed download
- **Did:** You reported "The update couldn't be installed — the download failed" (0.3.2 → 0.4.0). Root cause, checked:
  GitHub's release-download server answered HTTP 504 for every release file (from its edge, `southeastasia`), not a
  fuxx bug. On your "yes", the download now retries 3 times, 2 s apart (`--retry-all-errors`, because curl 8 doesn't
  count a 504 over HTTP/2 with `-f` as transient; verified on the live server). TDD: the args test failed first, then
  passed; 953 tests pass. Branch `m11e-download-retry`. Details in note 57.
- **Next:** merge and ship with the next release (your word); retry the update once GitHub's downloads work again.

### 2026-11-08 — v0.4.0 released (M11g)
- **Did:** On your word ("7-8 go ahead"): merged `m11g-right-sidebar` into `master` (fast-forward; 953 tests pass),
  version 0.4.0, pushed `master` and tag `v0.4.0`; the Release run went green (42469390248), with the new git and
  FIFO tests passing on GitHub's runner. https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.4.0
- **Next:** update with the Update Available button, then note 59's M11g checklist (14 items). Then M11h (Changes,
  diff view, Activity).

### 2026-11-08 — M11g built: the right sidebar, Files and read-only file tabs
- **Did:** On branch `m11g-right-sidebar`, 6 tasks, each reviewed (several after fix rounds), a whole-branch review and
  one final fix round. syntect + two-face highlighting (colours from the fuxx theme, 3:1 floor); git status, ignore
  checks and line changes through the capped runner (a writer/reader thread, the process group stopped at the cap,
  last good state kept with back-off); `TabKind::File`; the right sidebar (⌥⌘\\, a pane icon, resize, remembered in
  sessions.json, follows the focused terminal); the Files panel (lazy tree, marks, filter, keyboard, cached rows); the
  file view (line numbers, change bars, live reload, find off the UI thread with a 10,000 cap, selection and ⌘C).
  The final review caught that "Open in Editor" (plain `open`) could **run** a script or binary; it now opens as text
  (`open -t`), and binaries get Show in Finder. 953 tests pass; ui-shots ok. Calls made while building are in
  note 59 "M11g as built".
- **Next:** on your word (given: "7-8 go ahead"): merge, push, release v0.4.0; then note 59's 14-item checklist.

### 2026-11-08 — v0.3.3 released; the right sidebar designed and approved (M11g–M11h)
- **Did:** On your word, released v0.3.3 (a version bump) to see the new Update Available button with its icon and
  version: "great! nice!". Then designed the right sidebar with you: what cmux (Files/Sessions/Feed) and Warp (code
  review) put there; your file explorer with files opening in the middle; syntect + two-face (your pick); line numbers
  and change bars; an icon on every pane to toggle it. A builder's read raised 9 questions, 8 settled as AI's calls.
  You approved it ("lets go"), which changes the scope: a read-only file viewer is in, an editor stays out
  ([[12 - Scope]]). Design: [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]].
- **Next:** M11g's plan.

### 2026-11-08 — v0.3.2 released (the cursor; the button's icon and version)
- **Did:** On your word: merged `m11f-cursor-style` into `master` (fast-forward), version 0.3.2 (868 tests pass on
  master), pushed `master` and tag `v0.3.2`; the Release run went green (42439235155),
  https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.2. The installed fuxx in `~/Applications` reads 0.3.1, so an
  Update and Restart from 0.3.0 already swapped it in (to be confirmed by you: note 57 items 5–8).
- **Next:** your Update and Restart from 0.3.1 to 0.3.2 (the button now shows the package icon and `v0.3.2`), then
  note 58's checklist (12 items) and the rest of note 57's.

### 2026-11-08 — M11f built: the cursor; M11e's button gets the icon and version
- **Did:** Built on branch `m11f-cursor-style`, 3 tasks, each reviewed (Tasks 2 and 3 after a fix round), then a
  whole-branch review (ready, nothing critical) and one fix round. The Update Available button has a package icon
  and reads `Update Available · v0.3.2` (`Downloading v0.3.2…` while downloading). The cursor: a 2 px bar by
  default, a 90 ms glide, breathing at 20 fps with a 0.6 s solid hold, the agent's status colour, Trail/Sparks as Fun,
  all set in Preferences → Appearance → Cursor and still under Animations / Reduce motion. AI's calls while building
  are in note 58 "As built". 868 tests pass; ui-shots ok.
- **Next:** on your word (given): merge, push, release v0.3.2; then note 58's on-screen checklist (12 items).

### 2026-11-08 — v0.3.1 released; the button seen; M11f planned (cursor style + package icon)
- **Did:** On your word, released v0.3.1 (a version bump; Release run green). On your Mac, fuxx 0.3.0 showed the
  blue **Update Available** button (note 57 item 4). Your asks: a "package" icon on the button (note 57 amendment) and
  a blinking cursor that isn't a full block, then "fun and entertaining" options; you chose the suggested mix ("ya
  pakai cara mu"): Bar + glide + breathing glow + colour following the agent, Trail and Sparks as Fun. Spec
  [[58 - Milestone 11f - Cursor Style]], plan [[M11f Plan - Cursor Style]] (3 build tasks, then v0.3.2). A detail
  found while planning: the background service creates the terminals, so each starts with a "no program has chosen"
  style (`HollowBlock`, which programs can't request) and the window draws your setting there.
- **Next:** your review of note 58 and the plan; note 57's items 5–12 can be tried now (Update and Restart to 0.3.1).

### 2026-11-08 — v0.3.0 released; installed on your Mac
- **Did:** On your word: merged `m11e-update-available` into `master` (fast-forward; 834 tests pass on master), pushed
  `master` (M11d and M11e, 20 commits), and pushed tag `v0.3.0`. The Release workflow went green on its first run
  (run 42429479670): https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.0 with `fuxx-0.3.0.zip` and its digest.
  Downloaded it with curl and checked it the updater's way (digest, one `fuxx.app`, id, version 0.3.0, signature, no
  quarantine), then installed it into `~/Applications` (the last manual install). It opened, and removed
  `fuxx.app.previous` itself once its window was up.
- **Next:** on your word, release v0.3.1 (a version bump) and run note 57's checklist items 4–12 on screen.

### 2026-11-08 — M11e built: Update Available (release and on-screen check pending)
- **Did:** Task 0's spike passed on your Mac (no quarantine via curl, no "Open Anyway", notification permission kept;
  M11d is now the installed fuxx). Built on branch `m11e-update-available`, 5 tasks, each reviewed (Tasks 4 and 5
  after a fix round), then a whole-branch review and one fix (keep `fuxx.app.previous` until the new window opens).
  Version 0.3.0; `.github/workflows/release.yml` publishes a release on a pushed `v*` tag (refusing a tag that
  doesn't match `Cargo.toml`); fuxx checks GitHub 10 s after launch and daily (bundled only), **fuxx → Check for
  Updates…**, a Preferences switch; the blue **Update Available** button above the footer; the install downloads,
  checks (digest, one `fuxx.app`, id, version, signature), swaps via a detached script and reopens. AI's calls while
  building are in note 57 "As built". 834 tests pass; ui-shots ok; no new clippy warnings.
- **Next:** on your word, merge and push (Task 6), then tag v0.3.0 (Task 7), then v0.3.1 and the on-screen check
  (Task 8).

### 2026-11-08 — M11e planned: Update Available
- **Did:** You asked for an "Update Available" that updates fuxx itself, without M12. Findings: Homebrew is free, but
  since Homebrew 5.0 its official casks need a Developer ID and notarization, and `--no-quarantine` is gone, so it
  wouldn't avoid "Open Anyway". A download made by fuxx itself (curl) carries no quarantine mark, so a self-made
  updater can work with the ad-hoc signature. You chose fuxx's own updater (not Sparkle), a blue rounded "Update
  Available" above the bottom bar, and releases by GitHub Actions on a tag. You skipped the questions; the other
  choices are AI's, in [[57 - Milestone 11e - Update Available]]. Plan: [[M11e Plan - Update Available]]: a spike on
  your Mac first (no "Open Anyway", notification permission kept), then the workflow and v0.3.0, the check, the
  button, the install. Scope updated ([[12 - Scope]]).
- **Next:** your review of note 57 and the plan; then Task 0.

### 2026-11-08 — M11d built: the Help menu (on-screen check pending)
- **Did:** Built on branch `m11d-help-menu` (worktree `.worktrees/m11d-help-menu`), 5 tasks, each reviewed, then a
  whole-branch review ("ready to merge", no must-fix issues) and one polish round. Help after Window, marked as macOS's
  Help menu (search box). Keyboard Shortcuts (⌘/) and the fuxx Command Reference share one panel, built from the real
  key bindings and from `fuxx help`'s text; tests fail if either drifts. Notifications Status… (a sheet from macOS's
  answer), Show Welcome Tips, Report an Issue… (prefilled GitHub issue), Show Logs (`~/Library/Logs/fuxx/`), and the
  window now writes `fuxx.log` there when run from fuxx.app. AI's calls while building, now in note 56 "As built": ⌘/
  from Preferences or a popped-out page shows the panel rather than closing it; ⌥⌘Q is listed when it is bound; the
  MCP row quotes a path with spaces. The review also confirmed the screenshot runs never touch your saved sessions.
  786 tests pass, ui-shots ok, fmt and clippy clean; `target/dist/fuxx.app` built in the worktree.
- **Merged** into `master` locally on your word (2026-11-08, fast-forward; 786 tests pass on master); not pushed.
- **Next:** your on-screen check (17 items in note 56). The M11c check is still open.

### 2026-11-07 — M11d planned: the Help menu
- **Did:** You asked to plan the Help menu (requested 2026-11-06) and said "go" to skip the questions, so every
  design choice is AI's call, written down in [[56 - Milestone 11d - Help Menu]] for you to correct. The Help menu
  comes after Window and gets macOS's search box (fuxx marks it as the Help menu itself, since GPUI doesn't). It
  holds Keyboard Shortcuts (⌘/, a ⌘P-style searchable panel built from the real key-binding table, so a test fails if
  a new shortcut isn't listed), the fuxx Command Reference (the same panel, built from `fuxx help`'s text, plus the
  MCP setup line with the running fuxx's path), Notifications Status… (a sheet from macOS's current answer, with
  Open System Settings), Show Welcome Tips (the sidebar comes back with the card), Report an Issue… (a prefilled
  GitHub issue with fuxx's and macOS's versions) and Show Logs (`~/Library/Logs/fuxx/`, where the window now writes
  `fuxx.log` when run from the app). No new crates; `objc2-app-kit` gains `NSMenu` and `NSMenuItem`.
  Plan: [[M11d Plan - Help Menu]], 5 build tasks, then your check and the docs. The status snapshot above was out of
  date (it still said M11b) and is corrected.
- **Next:** your review of note 56 and the plan, then the build. The M11c on-screen check is still open.

### 2026-11-07 — M11c Task 6: the app icon (on master, on-screen check pending)
- **Did:** Your request after the merge: the 3D Cyber logo (from your logo session) is fuxx's app icon — the Dock,
  ⌘Tab, Finder, notification banners (no more blank square) and System Settings — and the popped-out browser window's
  toolbar mark instead of 🖕; a `cargo run` build sets the logo as its Dock icon at launch (your follow-up). The logo touched the canvas' bottom edge off-centre, so `scripts/make_app_icon.py` trims
  it onto Apple's icon grid (824 px body in 1024 px) and writes `packaging/macos/fuxx.icns` (10 sizes) and a 64 px mark;
  `bundle-mac.sh` copies the icon in and `Info.plist` names it. Committed straight on master, as you asked. 743 tests
  pass; app rebuilt.
- **Next:** your on-screen check (note 55's list now 19 items: 16–18 are the icon); then M11d.
- **Seen on your Mac (2026-11-07):** fuxx banners ("#10 proj — Finished · Worked for 13s") and a click jumping to the
  tab work. **Open:** the banner's icon is still blank after three ruled-out causes — see note 55 "Known limitation";
  next try needs Xcode (macOS 26's `Assets.car` icon). Pushed to origin on your word.

### 2026-11-06 — M11c built: native notifications (on-screen check pending)
- **Did:** Your request to "extend M11" with the notification part of M12. A first test on your Mac showed the ad-hoc
  signed `fuxx.app` can post banners titled fuxx and get the click back — no paid Apple account needed. A second
  (Task 0) showed where a click goes: to an open fuxx window; with none, to the background service (macOS opens fuxx
  itself); with neither, to the fuxx macOS opens. Built on branch `m11c-native-notifications`: banners from fuxx with
  the old `osascript` banner as fallback (never silent), a click shows the terminal that raised it (also from a
  closed window via the background service), and — your request — new tabs and workspaces are named ╭∩╮. The final
  review caught that banner ids repeated across runs, so a click on an old banner could show an unrelated terminal;
  fixed (an id names the run that numbered its terminal). 742 tests pass; app rebuilt at `target/dist/fuxx.app`.
  Your other session's logo commits ride on the same branch, as you asked. M11d (Help menu) requested next.
- **Next:** your M11c on-screen check (15 items in note 55), then merge on your word.
- **Open:** banners show a blank icon until fuxx has an app icon (done in Task 6, 2026-11-07).

### 2026-11-06 — M11b Task 6c: footer icons bigger, matching, symmetric (on-screen check pending)
- **Did:** From your screenshot: the sidebar footer's +, 🔔 and gear were different sizes (11, 13 and 9 px; the gear was
  a text character) and the + sat 12 px from the left while the gear sat 14 px from the right. Now all three are 16 px
  line icons drawn like the tab bars' icons, in 28 px buttons, centred at one height, with the + and the gear 13 px
  from their edges. ui-shots measure this in both themes at two sidebar widths. Review approved; 723 tests pass. App
  rebuilt at `target/dist/fuxx.app`.
- **Next:** Task 8, your on-screen check (note 54's list: 68 items; 47a is new).

### 2026-11-06 — M11b Task 7b: drags feel like picking the tab up (on-screen check pending)
- **Did:** Your request after the build: while a pane tab or a sidebar row is dragged the pointer is a closed hand, the
  copy stays exactly where you grabbed it, and instead of an insertion line the other tabs (or rows, within their
  group) slide aside over 0.15 s to open a gap where it will land, as in Ghostty. Dragging into another pane opens the
  gap there and closes up the bar it left; over a place that wouldn't take it the gap goes home. New pure module
  `drag_gap.rs`. The review caught that my first landing rule was lopsided (a 1 px move right swapped tabs, so a shaky
  click could reorder tabs or workspaces) and that a Finder drag after a row drag hid that row; both fixed, re-review
  clean. 723 tests pass; every ui-shots scene passes. App rebuilt at `target/dist/fuxx.app`.
- **Next:** Task 8, your batched on-screen check — note 54's list now has 67 items (22 and 27 changed; 29a–c new).
- **Open:** the cursor and how smooth the slides feel can only be checked on screen.

### 2026-11-06 — M11b built: workspaces, pane tabs and the browser in a pane (on-screen check pending)
- **Did:** Built and reviewed [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]] on branch `m11b-workspaces`
  (Tasks 0–7, 4b, 4c, 4d, 4e, 6b; Task 8 is your check, so **nothing is marked done**). The sidebar now lists
  workspaces; a workspace is a split layout of panes; each pane has a tab bar of terminal and browser tabs. Rows show the
  bold name, `branch • ~/folder`, and a 🔔 Needs input pill with the question; selected rows fill with the accent;
  workspaces can be pinned. The browser is a real web view placed over a pane, with a fuxx bar (address field, history
  suggestions, "Search Google for …") and it hides whenever a fuxx menu, box or drag is in front of it. Agents'
  `fuxx browser …` drive a browser tab beside their terminal; Pop Out moves the page to a window. Workspaces, panes, tabs
  and page addresses come back after a restart. Your requests during the build: resizing no longer makes a terminal look
  busy (your bug report); an empty tab bar drags the window; ⌘T is a new tab and ⌘N a new workspace; ⌘R, ⌘[ / ⌘] and
  ⌘= / ⌘- / ⌘0 act on a page; drag tabs anywhere (within a pane, into another pane, onto a sidebar row); the 🔔 list and
  + moved into the sidebar footer. Reviews found real problems and each was fixed: a restore that lost its fallback
  focus, Pop Out of the last tab quitting fuxx, a streaming terminal settling early to Done during a resize drag, a
  rename box unreadable on the selected row, `fuxx notify` missing from the 🔔 list, a shaky tab click selecting nothing,
  and (final review) a shortcut leaving the 🔔 popover open, `fuxx new --background` taking the keyboard from a page, and
  a workspace of pages lost on relaunch. 715 tests pass. A fragile `splits` screenshot check was also fixed. The
  64-item on-screen checklist, the verification log and the deferred minors are in note 54 and the plan; every ruling
  is in [[Rulings - M10b M10c M11 (autonomous run)]].
- **Learned:** A native web view sits above everything GPUI draws and takes its clicks, so the rule has to be "hide the
  page whenever anything of ours is in front", checked at one place every frame. A screenshot check that samples one
  pixel row can pass or fail with the screen's scale; sample where a line is plain. A resize makes a shell redraw, and
  that redraw looks exactly like work unless the status tracker is told a resize just happened.
- **Blocked:** On your batched on-screen check (M10b, M10c, M11, the follow-up, M11b). Please also confirm the rulings
  listed in note 54: the workspace name shows at the top only while the sidebar is hidden; page shortcuts act whenever a
  browser tab has fuxx's focus, including its address field; the 🔔 is hidden with the sidebar. Branch `m11b-workspaces`
  stacks on `m11b-browser-bar-prefs`, which stacks on `m11-ui-ux`, M10c and M10b; nothing is merged.
- **Next task:** Your on-screen check ([[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]], "On-screen
  checklist (pending)"); then merge, then M12.

### 2026-11-06 — M10c/M11 follow-up built: browser bar, keep-data setting, tabbed Preferences, sidebar button, tab-like rows (on-screen check pending)
- **Did:** Built and reviewed [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]] on branch
  `m11b-browser-bar-prefs` (Tasks 1, 2, 2b, 3, 3b, 3c; Task 3d moved to M11b; Task 4 is your check, batched with M10b,
  M10c and M11, so **nothing is marked done**). A Safari-like compact bar on browser windows (back, forward,
  reload/stop, an address field, a loading line, light or dark with the fuxx theme); two Browser settings (Show address
  bar, Keep history and data, Off by default); Preferences as vertical tabs (General, Appearance, Sessions, Browser;
  920 × 452); then your in-session requests: the fuxx logo (you chose the 🖕 mark, right of the address bar); a sidebar
  button beside the window buttons (with a slide); no number on sidebar rows (⌘N on the right instead); and tab-like
  rows (× on hover, middle-click closes, right-click menu with Tab Color, bulk closes always ask). Your cmux layout
  request (tabs holding terminals, workspaces, pane tabs, the in-pane browser) became its own milestone, **M11b**, so
  Task 3d moved there. Reviews found real problems: web pages followed the fuxx theme instead of macOS (fixed: pages
  follow macOS, the bar follows fuxx); the × hid the focused session's status mark; Close Tabs to the Right closed
  sessions folded in a collapsed group; and keep-data Off could leave saved data on disk after a hand edit or a quit
  during the delete (fixed with a flag in the sessions file; launch and the last On window closing wipe, and the flag
  clears only when WebKit says the deletion is done). 577 tests. The 22-item on-screen list, the verification log and
  the deferred minors are in the plan; one known limitation (a lost background service can stop the flag being saved)
  is documented there.
- **Learned:** Off-screen capture can't render macOS's glass toolbar items, so the bar's look is only checkable on
  screen; `NSToolbar` needs a titled window (a borderless one gets no items). A "when switched" delete isn't enough for
  data on disk: write down that data may exist, and wipe whenever it may be left. Hover-only controls must not hide
  status the person needs; ask what the control replaces.
- **Blocked:** On your batched on-screen check (M10b, M10c, M11, follow-up). Branch `m11b-browser-bar-prefs` stacks on
  `m11-ui-ux`, which stacks on M10c and M10b; nothing is merged. After the check: merge them into `master`, then
  M11b (input note: [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]]).

### 2026-11-06 — Milestone 11 built: UI/UX uplift (on-screen check pending)
- **Did:** Same arrangement as M10b and M10c: built and reviewed, **not** marked done until you run the on-screen
  checks (batched with M10b and M10c). Built [[M11 Plan - UI-UX Uplift]] (10 of 11 tasks; the eleventh is your check),
  with off-screen screenshots in a dark and a light theme looked at for every UI task: an integrated titlebar and
  padding; design tokens with a contrast floor (light themes too); a sidebar with a question line under a waiting row,
  ages, headers, scrolling, resizing and a hint card; shaped status marks (ring, spinning arc, pulsing dot, dot with
  a check) with an Animations switch and macOS Reduce motion; pane headers and quiet dividers; ⇧⌘U and ⌘P; a rename
  that selects the old name; Preferences with swatches and a live preview. Reviews found real problems: the spinner
  kept waking while the window was hidden and every tick repainted the terminals (fixed: marks stop when the window
  can't be seen, terminal views are cached); ⌘P over a rename closed itself; the notification body repeated its
  summary; the Preferences preview was cut off at 16 pt. The final review said Ready to merge, then a polish pass
  fixed hover text contrast, the host picker's and address box's look, and ⌘= ⌘− ⌘0 in a browser window. 546 tests.
  The 21-item on-screen list is in [[53 - Milestone 11 - UI-UX Uplift]]; the minor findings left on purpose are in
  the plan. Visible change to know: notification titles are now `#3 api — Needs your input`, with the question (or
  "Worked for 3m") as the body, and `fuxx events` carries the same text.
- **Learned:** GPUI's off-screen renderer (behind the `ui-shots` feature) lets reviewers see every scene without a
  display, but AppKit needs the main thread, so the scenes run from a hidden `__ui-shots` entry rather than from
  tests. A 10 fps ticker is cheap only if it stops when nothing visible moves and doesn't repaint the terminals.
  Contrast has to be checked on every surface that is actually drawn, hover fills included. GPUI has no letter
  spacing at this rev, so headers use small caps.
- **Blocked:** On your batched on-screen check for M10b, M10c and M11. Branch `m11-ui-ux` is not merged (it stacks on
  M10c, which stacks on M10b). After the check: merge M10b, M10c and M11 into `master`, then M12.

### 2026-11-06 — Milestone 10c built: agent browser (on-screen check pending)
- **Did:** Same arrangement as M10b: built and reviewed, **not** marked done until you run the on-screen checks
  (batched with M10b and M11). Built [[M10c Plan - Agent Browser]] (7 of 8 tasks; the eighth is your check): a WebKit
  window per session, opened with ⇧⌘B or `fuxx browser open`, and driven by `fuxx browser
  open|snapshot|click|fill|press|read|eval|wait|screenshot|url|back|reload|close` and 13 matching MCP tools. Every
  task was reviewed. Reviews found real gaps: the browser dropped on an SSH reconnect, and the spec's ⌘C/⌘V/⌘A/⌘Z/⌘W
  did not reach the browser (so fuxx got an Edit menu). The whole-branch review found an agent's `open` took your
  keyboard, and page text could reach your terminal as escape sequences (retitle a session, fire a notification);
  both fixed and re-reviewed. 501 tests; a hidden `fuxx __browser-probe` drives a real off-screen web view end to end
  and is kept for future GPUI/WebKit updates. The 22-item on-screen list is in [[52 - Milestone 10c - Agent Browser]];
  the minor findings left on purpose are in the plan.
- **Learned:** A WebKit view can live in its own `NSWindow` inside a GPUI app with no GPUI cooperation, but GPUI owns
  the menu bar, so editing keys and ⌘W only reach the page through an Edit menu and a small window subclass; with the
  browser in front, fuxx's other menu shortcuts do nothing. Pages with a strict CSP block `eval`, so agent code runs as
  an expression first. Text from a page must be stripped of control characters before it reaches a terminal.
- **Blocked:** Nothing. Branch `m10c-agent-browser` is not merged (it stacks on M10b); M11 is next.

### 2026-11-06 — Milestone 10b built: SSH workspaces (on-screen check pending)
- **Did:** You asked me to run M10b, then M10c, then M11 on my own, with all the on-screen checks saved for the
  end. So M10b is built and reviewed but **not** marked done: no ✅ until you run the checks. Built
  [[M10b Plan - SSH Workspaces]] (8 of 9 tasks; the ninth is your check): a host picker (⇧⌘S) fed by
  `~/.ssh/config` and recent hosts, panes that run your own `ssh`, one shared connection per host, sidebar groups by
  host, a "press Enter to reconnect" line when the link drops (automatic with tmux), restore and background-service
  support, and `fuxx new --ssh`. Every task was reviewed, then a whole-branch review found three real problems
  (SSH panes missed your login-shell settings when opened from Finder; early ssh errors vanished; retries stayed
  noisy), plus focus, `#` comments and a hung shared connection. All fixed and re-reviewed. 457 tests. The merged
  21-item on-screen list is in [[51 - Milestone 10b - SSH Workspaces]]; the minor findings left on purpose are in
  the plan.
- **Learned:** An app opened from Finder does not get your `.zshrc`, so anything ssh needs from it (agent socket,
  ProxyCommand tools) has to be read from a login shell once. Also: two tmux panes to one host mirrored each other
  until each got its own tmux session name, and a connection that drops right after connecting must not count as
  "was working", or the retries restart at 3 s forever.
- **Blocked:** Nothing. Branch `m10b-ssh-workspaces` is not merged; the next milestones stack on it until you check.

### 2026-11-05 — Milestone 10b planned: SSH workspaces
- **Did:** You said "m10b go" and to skip the questions, so the design is the feasibility study's Option A, AI's
  call throughout. Wrote [[51 - Milestone 10b - SSH Workspaces]] and [[M10b Plan - SSH Workspaces]] (9 tasks): a
  host picker (⇧⌘S) fed by `~/.ssh/config` and recent hosts, panes that run your own `ssh`, one shared connection
  per host, sidebar groups by host, a "press Enter to reconnect" line when the link drops (automatic with the tmux
  option), restore and background-service support, and `fuxx new --ssh`.
- **Learned:** Today every session is a login shell with the agent typed into it, and a session that exits just
  disappears with no exit code. SSH needs both changed: ssh runs as the program itself, and its exit code (255 =
  connection failed) decides whether the pane stays. Also, macOS caps socket paths at 104 bytes, so the shared
  ssh connection lives under `/tmp/fuxx-<uid>`, not Application Support. Your `~/.ssh/config` currently lists only
  git hosts, which the picker hides, so you'd type a host the first time.

### 2026-11-05 — Milestone 10a complete: persistent sessions
- **Did:** Built [[M10a Plan - Persistent Sessions]] (12 tasks, each reviewed, plus a whole-branch review). With
  `persistent-sessions = on` (or Preferences → Sessions), a small background service (`fuxx daemon`) owns your
  sessions. Quitting leaves them running; ⌥⌘Q ends them all; reopening puts every live session back in its place
  with its screen, scrollback and status; notifications still arrive while the window is closed. You ran the
  privacy-folder spike (a background process keeps access to Documents, Desktop and Downloads, no private API
  needed) and then all 21 on-screen checks, which passed. The final review found one real problem (starting with
  the setting off could restart sessions the service still held, giving duplicate agents); fixed and re-reviewed.
  Also removed the spike code, keeping the plain spawn. 399 tests, 0 warnings.
- **Learned:** A terminal can be rebuilt from bytes alone (a "snapshot" the service writes), as long as the
  alt screen's hidden main screen is handled; a few settings (scroll region, tab stops, character sets) are
  private to the terminal library and can't be copied, so programs have to redraw them. Also, the service's ids
  restart each time it starts, so fuxx must check it is talking to the same service before trusting saved ids.
- **Blocked:** Nothing.
- **Next task:** Decide whether to plan M10b (SSH workspaces); see [[40 - Roadmap]].

### 2026-10-27 — Milestone 9 complete: scripting API (+ Caps Lock fix)
- **Did:** You skipped the design questions and asked for more than cmux / Warp / iTerm2. Built
  [[M9 Plan - Scripting API]] (10 tasks, every one reviewed; security review of the socket): a same-user socket,
  the `fuxx` command (ls/new/close/focus/rename/send/key/paste/read/status/question/answer/wait/events/note/
  progress/notify/equalize/presets), agent-aware `wait`/`question`/`answer`, a live event stream, sidebar note and
  progress bar, and `fuxx mcp` so an agent can drive other agents. A whole-branch review found two real problems
  (a nested fuxx inheriting the outer one's session, and `wait --text` matching text already on screen), fixed
  before your check. All on-screen checks passed. Also fixed: **Caps Lock** now capitalises letters (GPUI's
  macOS key events ignore it). Saved a survey of multi-agent tools as input ([[Input - Agent Sessions Research]]);
  not on the roadmap yet (your call). 243 tests, 0 warnings.
- **Learned:** GPUI's macOS `key_char` only applies Shift/Option, never Caps Lock (`window.capslock()` has it).
  Claude Code has grown its own agent view (`claude agents`), so fuxx's edge is cross-vendor exact state and
  answering in place, not copying single-vendor orchestrators.
- **Blocked:** Nothing.
- **Next task:** Decide the next milestone (M10, or the agent-sessions input).

### 2026-10-27 — Milestones 6b, 7 and 8 complete
- **Did:** You ran the combined on-screen check (M6b Task 5, M7 Task 7, M8 Task 6): all good, including JetBrains
  Mono ligatures, groups, rename, presets, restore and split panes. All three milestones flipped to ✅.
- **Learned:** Nothing new.
- **Blocked:** Nothing.
- **Next task:** Brainstorm **M9 — Scripting API** with you, then its spec and plan.

### 2026-10-27 — Roadmap renumbered: M11 is the UI/UX uplift
- **Did:** On request, a new **M11 — UI/UX uplift** before shipping; the signed `.app` (+ notification click) moved
  M11 → **M12**, Homebrew M12 → **M13**. M9 (scripting API) and M10 (cmux extras) unchanged. Every current note,
  plan, STRUCTURE.md and the bundle script now use the new numbers; older log entries keep the old ones.
- **Learned:** Nothing new.
- **Blocked:** Nothing.
- **Next task:** Your M6b/M7/M8 on-screen check; then M9.

### 2026-10-27 — Tester build 0.2.0
- **Did:** Bumped to **0.2.0** and built `target/dist/fuxx-0.2.0.dmg` (universal: arm64 + x86_64, ad-hoc signed, 13 MB)
  with the M4b script. The script now copies the bundled fonts' and themes' licence texts into
  `fuxx.app/Contents/Resources/Licenses/`, as their licences require for every copy. Checked by mounting the DMG.
- **Learned:** Nothing new.
- **Blocked:** Nothing. The M6b/M7/M8 on-screen check is still yours to do (the DMG is a way to do it).
- **Next task:** Combined 👀 check; then plan M9.

### 2026-10-27 — M6b, M7 and M8 built, reviewed and merged (on-screen check pending)
- **Did:** On request, plan changes first: the default session name is 🖕; **M7 and M8 built before the M6b check**,
  with all three checks done together; **split panes moved into scope** ([[12 - Scope]], [[40 - Roadmap]], AGENTS.md);
  **JetBrains Mono bundled as the default font** (M6b Task 4b) after the ligature check showed none of the installed
  fonts have ligatures. Wrote [[47 - Milestone 7 - Session Ergonomics]] / [[M7 Plan - Session Ergonomics]] and
  [[48 - Milestone 8 - Split Panes]] / [[M8 Plan - Split Panes]] (your choices: 3 built-in presets + your own;
  restore = layout + re-run presets, as a setting; nested splits). Every task was built by one agent and reviewed
  by another; each milestone had a whole-branch review. Fixes from review: restore uses today's presets (M7),
  focus after closing goes to the pane that takes the space (M8). Then, on request, merged all three to `master`
  before the on-screen check. 150 tests, 0 warnings.
- **Learned:** A process-registered font shows up in GPUI's font list, so bundling a text font needs no extra API.
  GPUI's capture-phase mouse listener lets a pane switch focus without stealing the terminal's own click; a div with
  no listeners gets no hitbox, so an overlay can dim a pane without blocking it.
- **Blocked:** Nothing. Your on-screen checks for M6b, M7 and M8.
- **Next task:** Combined 👀 check (M6b Task 5, M7 Task 7, M8 Task 6); fix what fails; then a tester DMG with the font licences.

### 2026-10-27 — M6b plan written and approved
- **Did:** D9 decided (theme list, pick only). You skipped the design questions ("go"); researched GPUI menus, second
  windows, lists and live global updates at the pinned rev; wrote [[46b - Milestone 6b - Settings Window]] and
  [[M6b Plan - Settings Window]] (6 tasks, 106 → 116 tests). You approved it and per-task agents ("right on").
- **Learned:** GPUI takes menu shortcuts from context-free key bindings at `set_menus` time, so bindings must come
  first; with `QuitMode::LastWindowClosed`, fuxx only quits when every window is closed.
- **Blocked:** Nothing.
- **Next task:** [[M6b Plan - Settings Window]] → Task 1.

### 2026-10-27 — Milestone 6 complete: fonts and config
- **Did:** Built [[M6 Plan - Fonts and Config]] on branch `m6-fonts-config`, one agent per task, each reviewed:
  settings reader with includes (`ead1c28`, loop fix `3c66150`), themes (`354190e`), theme-driven colors
  (`31ca371`), fonts + bundled icon font (`5719329`, fixes `b331aa5` `e5fdb29`), startup wiring (`476574f`).
  Mid-milestone you changed the plan to **fuxx's own settings file** with six built-in themes (`529c0a5`, fixes
  `1b9887e` `40efd80`), and added **Mac line/word keys** after the on-screen check (`924f45a`). All checks passed.
  0 warnings, 106 tests.
- **Learned:** macOS ignores fonts only loaded in an app's memory for missing glyphs; registering the font file
  with CoreText fixes prompt icons. ctrl-arrows belong to macOS (Spaces) and plain zsh doesn't bind Home/End, in
  any terminal. Bundled fonts/themes carry several licences: notices ship in `assets/`.
- **Blocked:** **D9** before the M6b plan.
- **Next task:** Decide D9 → write the M6b plan (settings window).

### 2026-10-27 — Plan changes during M6: own settings file, built-in themes, settings window
- **Did:** On request, three plan changes, each written into the vault before building:
  1. fuxx reads **its own settings file** (`~/.config/fuxx/config`, Ghostty-style `key = value`), not Ghostty's config (option 1). [[12 - Scope]], [[11 - Features]], [[40 - Roadmap]] and [[46 - Milestone 6 - Fonts and Config]] updated; new Task 6 in [[M6 Plan - Fonts and Config]] (built, 105 tests).
  2. Themes come from your `~/.config/fuxx/themes` folder **and** six built-in ones (Catppuccin Mocha/Latte, Dracula, Nord, Gruvbox Dark, TokyoNight), kept with their credits and licence texts (you chose to keep all six).
  3. A **settings window** — File → Preferences… and cmd-, — added as milestone **M6b** after M6; its plan is written when M6 is done. Open decision **D9**: pick-only vs typing hex colors.
- **Learned:** The bundled Nerd Font and themes carry more than one licence (CC BY 4.0, Apache-2.0, OFL besides MIT); notices now ship next to them in `assets/`.
- **Blocked:** Your M6 on-screen check (Task 7).
- **Next task:** [[M6 Plan - Fonts and Config]] → Task 7.

### 2026-10-27 — M6 plan written and approved
- **Did:** Pushed M5. You skipped the design questions and asked to follow the mockup (Screen 2, #5 font and
  #6 theme from Ghostty), so the AI made the calls in [[46 - Milestone 6 - Fonts and Config]]. Research: Ghostty
  1.3.1's config rules from its bundled docs and tests; GPUI font fallbacks/features at the pinned rev. Your Ghostty
  sets only `font-family = "MesloLGS NF"`. A headless spike proved how prompt icons can work for everyone: a font
  only loaded in fuxx's memory is ignored by macOS, but registering a bundled Symbols Nerd Font file with CoreText
  works. Wrote [[M6 Plan - Fonts and Config]] (7 tasks, 85 → 101 tests); you approved it ("right").
- **Learned:** Ghostty 1.3 reads four config files (`config` and `config.ghostty`, in `~/.config/ghostty` and App
  Support), later winning. GPUI silently swaps a missing font family for Helvetica, so fuxx must check installed families.
- **Blocked:** Nothing.
- **Next task:** [[M6 Plan - Fonts and Config]] → Task 1.

### 2026-10-27 — Milestone 5 complete: terminal UX
- **Did:** Built [[M5 Plan - Terminal UX]] on branch `m5-terminal-ux`, one fresh agent per task, each reviewed:
  scrollback core and live-screen status (`5cd0c02`), selection/copy/paste bytes (`f4998a2`), mouse
  positions and reports (`13cc465`), the link finder with an http/https/mailto allowlist (`55e5781`), real
  pixel sizes on resize (`d4b2b77`), and the view wiring (`1bb7742`). Reviews added a crash fix (stale points
  clamped under the Term lock), links opening by url, and shift-drag starting a selection (`35c55e6`, `6ee41a9`).
  Whole-branch review: ready to merge, 0 Critical/Important. All 8 on-screen checks passed. 0 warnings, 85 tests.
- **Learned:** Claude Code turns on mouse tracking, focus reporting and asks about the kitty keyboard protocol
  at startup. A test binary can go stale when a scratch copy shares `target/`: give probes their own target dir.
  Tab not accepting Claude Code's grey example/suggestion turned out to predate M5 (same in the M4b app);
  debugged and logged as **tech debt** in [[00 - Plan Tracker (Start Here)]].
- **Blocked:** Nothing.
- **Next task:** Write the M6 plan (fonts & config, incl. prompt icons).

### 2026-10-26 — M5 plan written and approved
- **Did:** You chose to skip the design questions ("go"), so the AI made the behaviour calls, modelled on
  Zed's terminal, and listed them in [[45 - Milestone 5 - Terminal UX]] for you to veto. Wrote
  [[M5 Plan - Terminal UX]] (8 tasks, 61 → 83 tests) after reading the exact APIs at our pinned GPUI rev
  and `alacritty_terminal 0.26.0`, and a throwaway spike of the scroll/selection/link calls. You approved
  it and the one-agent-per-task execution ("right").
- **Learned:** GPUI runs key bindings before a view's own key-down handler, so terminal shortcuts can be
  plain bindings. When scrolled back, the visible rows are history: status detection must read the live screen.
- **Blocked:** Nothing.
- **Next task:** [[M5 Plan - Terminal UX]] → Task 1.

### 2026-10-26 — Prompt icons show as `?`: kept for M6
- **Did:** You checked the app's look against the M4 mockup: layout, numbered sessions, Idle rings and the
  window title are as planned. The `?` boxes in your powerlevel10k prompt are Nerd Font icons that Menlo
  (the only font fuxx draws) lacks; Ghostty has them built in. On request (option 1), kept the fix for
  **M6** and wrote it into its roadmap line and [[11 - Features]], plus a known limitation in
  [[M4b Plan - Tester Build]] and a line in the tester install steps. No code changed.
- **Learned:** Your Ghostty config names no font, so M6's "read Ghostty's config" alone wouldn't bring the icons.
- **Blocked:** Nothing.
- **Next task:** Write the M5 plan (Phase 2: Terminal UX).

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
