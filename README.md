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

## Built with

Rust · [GPUI](https://github.com/zed-industries/zed).

## Status

Early and in active development. macOS first (Linux-capable via the Rust stack). The `docs/` folder
is an Obsidian vault with the vision, architecture, and roadmap.

## History

This repository previously shipped a small Rust CLI that hosted a single agent and fired a
notification on completion. It has since been repurposed into the terminal app; that earlier code
remains in the git history, with its docs under `docs/90-Archive-v1-CLI-Wrapper/`.
