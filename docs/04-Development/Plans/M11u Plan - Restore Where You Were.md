# M11u Plan — Restore Where You Were

> Executed natively on master. Design: [[72 - Milestone 11u - Restore Where You Were]].

1. `restore.rs`, test first: `state_path_for(home, dev)` (debug: `sessions-dev.json`); `SavedSession::{cwd, agent}`
   (serde default, skipped when none); `launch` runs the saved preset, else the preset named by `agent` (Agents only);
   `presets::agent_for(program, presets)` (the preset whose name, or command's first word, is the program).
2. `workspace.rs`: `saved_state` fills `cwd` (reported folder on this Mac, else `places.folder`) and `agent`
   (`programs` → `agent_for`); `start_in` starts in `cwd` when it exists; `save_soon` (at most once a second) on a
   folder move and on a program change.
3. Verify (tests, fmt, clippy), docs (note 72, Roadmap, Scope, tracker, Progress Log).
