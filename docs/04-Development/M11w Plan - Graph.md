# M11w Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GRAPH accordion under CHANGES: the commit history with coloured lanes, branch pills, a hover card and header actions, as in Cursor / VS Code.

**Architecture:** `src/git_graph.rs` reads commits (`git log --topo-order`) and a commit's details (`git show`), and builds a GitHub link; `src/graph_layout.rs` (pure) assigns lanes and edges; `src/changes_panel.rs` (or a sibling `graph_panel.rs`) draws the accordion: header buttons, rows (a canvas for the lanes), pills, the hover card.

**Tech Stack:** Rust, GPUI (pinned rev), the system `git` (as every git read in fuxx; no new crate).

**Spec:** [[74 - Milestone 11w - Graph]]

## Global Constraints

- No new dependency. Every git call off the UI thread, capped (`GIT_CAP` for reads), never prompting.
- 200 commits a page; a "Load more" row adds 200.
- Rows 28 pt, text 14 / 12 pt, the 12 pt edge, pill hovers (the right sidebar's system, M11v part 4).
- Lane colours: the theme's ANSI colours, cycled, held to `MARK_MIN` contrast on the panel.
- Reads only while the Changes tab shows and GRAPH is open; no timer of its own.

## Review Focus

1. A repo with no commits / a detached HEAD / a shallow clone (parents missing) → no crash, sensible lanes.
2. Octopus merges (3+ parents) and many simultaneous lanes (cap the drawn columns; the rest share the last).
3. Refs with odd names (`feature/x`, tags, `HEAD -> main`, `origin/HEAD`) parse into the right pills.
4. A hover on a row whose details git can't read (gone after a rebase) → the card shows what the row knows.
5. Non-GitHub / ssh-alias origins → no Open on GitHub, no wrong link.

### Task 1: `graph_layout.rs` — lanes and edges (pure)
- [ ] Failing tests: a straight history (one lane); a fork and a merge (two lanes, a curve in, a curve out); an octopus merge; parents beyond the page (lanes run off the bottom); the lane cap.
- [ ] `pub struct Row { lane: usize, lanes_in: Vec<Option<usize>>, edges: Vec<Edge>, merge: bool }`, `pub fn layout(commits: &[(Oid, Vec<Oid>)], cap: usize) -> Vec<Row>`.

### Task 2: `git_graph.rs` — reading git
- [ ] Failing tests (temp repos): `read(git, root, Scope, limit)` returns commits newest first with parents, author, time, refs, subject; `parse_refs("HEAD -> main, origin/main, tag: v1")`; `details(git, root, hash)` (body, files changed, insertions, deletions); `github_commit_url(remote, hash)` for ssh / https / non-GitHub.
- [ ] `Scope::{Auto, All, Current}`: Auto = `HEAD`, its upstream and origin's default branch (`refs/remotes/origin/HEAD`), as `git log` revisions.

### Task 3: the GRAPH accordion
- [ ] Header (chevron, `GRAPH`, hairline; Auto ▾ menu, ◎, Fetch, Pull, Push, Refresh as icon buttons with tooltips above); rows (lanes canvas, subject, pills, faint author); HEAD bold with a hollow ring; Load more; the hover card (0.4 s; author, relative + full date, message, files changed, hash copy, Open on GitHub); state on `ChangesPanel` (open, scope, page, rows, hovered details), read with the 2 s refresh only when HEAD/refs moved.
- [ ] Tests for the pure parts (pill words, the relative + full date words, which rows to read again).

### Task 4: ui-shots and docs
- [ ] A `changes-graph` scene (a temp repo with a fork and a merge; both themes; the hover card shown); every scene passes; fmt, clippy (no new warnings), tests. Note 74 "as built", Progress Log, Roadmap, Scope.
