---
tags: [business, reference]
---

# 11 - Relationship to cmux

fuxx-cli is inspired by **cmux**, but is deliberately a much smaller thing.

## What cmux is

cmux is a native macOS terminal app (Swift/AppKit) that embeds libghostty's full
rendering engine. On top of that terminal it adds vertical tabs, split panes, an
embedded scriptable browser, a socket/CLI API, and — the part that inspired fuxx-cli —
notification rings that light up when an AI agent finishes or needs attention, driven
by OSC 9 / 99 / 777 escape sequences.

## What fuxx-cli takes from it

Only the notification idea. That is the "interesting 20%" of cmux's value that does not
require building a terminal.

## What fuxx-cli deliberately does NOT do

- No terminal rendering (cmux delegates this to libghostty; fuxx-cli skips it entirely)
- No GPU work, no window, no panes, no tabs
- No embedded browser
- No socket API / daemon (at least not in v1)

## Why the difference matters

cmux is built by an experienced team and its hard parts each represent months of work.
Reimplementing that path as a beginner would stall out. fuxx-cli reaches the same
*felt* benefit (get pinged when the agent needs you) through a process wrapper. See
[[23 - Why Not libghostty]].

## Related

- [[10 - What Is fuxx-cli]]
- [[12 - Scope Boundaries]]
- [[21 - The Wrapper Process Model]]
