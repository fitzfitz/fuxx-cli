# fuxx

A native, GPU-accelerated terminal for running AI coding agents.

Run several coding agents at once in a single window. Each session shows a live status —
**working**, **done**, or **waiting for input** — so you can step away and get pulled back exactly
when an agent needs you, instead of babysitting a wall of terminal windows.

## What it does

- **A real terminal** — GPU-rendered, runs any shell or program.
- **Many agent sessions in one window** — a session sidebar with quick-switch.
- **Notification rings** — a per-session status badge plus desktop notifications when a background
  session finishes or needs input, and never a nag about the session you're already looking at.
- **Groups, presets, restore, split panes** — sessions per project folder, one-key agent presets,
  your layout back after a restart, nested splits.
- **Scriptable, and agent-aware** — the `fuxx` command and an MCP server (see below).

## Scripting fuxx

Inside any fuxx session, the `fuxx` command talks to the running app (outside, put the binary on your `PATH`):

```sh
fuxx ls                                    # sessions: id, number, status, name …
fuxx new --preset claude --split right --background   # prints the new id, e.g. s4
fuxx wait s4                               # until the agent needs you (also --for done|exit, --text, --any)
fuxx question s4                           # what it's asking
fuxx answer s4 "yes"                       # answer it (refused unless it's waiting)
fuxx events                                # live stream of status changes, opens, closes …
fuxx read s4 --lines 20 · fuxx send s4 "make test" --enter · fuxx note "building" · fuxx progress 40
```

Agents can drive fuxx too: `claude mcp add fuxx -- fuxx mcp` (use the full path to the binary if `fuxx`
isn't on your `PATH`). `fuxx help` lists every command; `scripting = off` in `~/.config/fuxx/config`
turns it off. Details: `docs/04-Development/49 - Milestone 9 - Scripting API.md`.

## See it

Open [`mockups/index.html`](mockups/index.html) in a browser for illustrative mockups of the finished
app: the first usable version (M4), the daily driver (Phase 2) and the full vision (Phase 3–4), with
every planned feature labeled by milestone. [`STRUCTURE.md`](STRUCTURE.md) maps the repo layout.

## Built with

Rust · [GPUI](https://github.com/zed-industries/zed).

## Status

Early and in active development. macOS first (Linux-capable via the Rust stack). The `docs/` folder
is an Obsidian vault with the vision, architecture, and roadmap.

## History

This repository previously shipped a small Rust CLI that hosted a single agent and fired a
notification on completion. It has since been repurposed into the terminal app; that earlier code
remains in the git history, with its docs under `docs/90-Archive-v1-CLI-Wrapper/`.
