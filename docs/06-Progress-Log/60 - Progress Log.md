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

- **Current milestone:** [[41 - Milestone 1 - Launch and Capture]]
- **Overall phase:** Milestone 0 done (toolchain + hello world); Milestone 1 not started
- **Repo:** `github.com/fitzfitz/fuxx-cli` (was 404 at vault creation — create it)

---

## Entries

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
