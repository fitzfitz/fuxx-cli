# Proposal — File View Upgrades (M11af–M11ai)

**Status:** 📝 directions chosen in browser mockups 2026-11-16; each milestone is **design-first** (its own detailed
design, then a plan), not designed or planned yet.

**Asked for:** 2026-11-16, by you, while M11ae was being built: "can we make the UI great on the file preview? right now
its very basic. Make me options on the UI and UX" → "everything" (both the code view and the Markdown Preview) →
"everything, all layout good. but I want what future holds. Give me some futuristic layout of file code preview. The
future I want is realistic not some sci-fi, fantasy future type of futuristic" → "you only showing smaller part. make it
show full window" → picked **"1 and 3"** of the near-future ideas → order **"your order is fine, go ahead"**.

**Order (yours):** M11ae (file tab chores) → M11aa's remaining UI tasks (agent tabs) → **M11af** → **M11ag** → **M11ah**
→ **M11ai**.

All of it stays a **read-only viewer** ([[12 - Scope]]: no editing, no language servers), offline, no new services.

## M11af — Live changes, with who wrote them (near-future idea 1)

- While a file tab is shown and the file changes on disk, the lines that changed **glow and cool down** over about a
  minute (the newest brightest), with a small "just now / 12 s ago" at the line's end while warm.
- A margin mark per line says **who changed it this session**: the agent (its avatar letter) or not the agent (you, or
  your editor).
  - **Exact** for an agent in an agent tab (M11aa): fuxx sees its file-edit tool calls.
  - **A guess** for an agent in a plain terminal: a change while that workspace's agent shows Working is marked as the
    agent's; any other change as not the agent's. (AI's proposal, told to you 2026-11-16; to confirm in the design.)
- **Follow** (a pill in the bar, "Following claude"): keeps the agent's newest edit in view; scrolling yourself stops it.

## M11ag — Ask the agent from the code (near-future idea 3)

- Select lines in a file tab → **Ask claude** (and ⌘↵) puts them into that workspace's agent tab box as a **context
  chip** (`workspace.rs:43–45`) beside what you type; **Copy with path** beside it.
- The agent's own explanation for an edit (from its reply) can show as a **note under the lines it changed**.
- Needs M11aa's agent tabs.

## M11ah — Code view upgrade (directions A + B + C, all chosen)

- **A · Calm and clear:** the clicked line highlighted (its number bright), quieter other numbers, faint indent guides,
  rounded change bars, more spacing.
- **B · Find your way:** a sticky line with the function or heading you're in, a breadcrumb, folding by indent (▸ / ▾),
  marks on the scrollbar for changes and find matches, ↑ / ↓ to jump between changes.
- **C · Review-first diffs:** each change a card titled with its function and its +/− counts, the exact changed words
  marked inside a line, unchanged stretches folded ("⋯ 36 unchanged lines — click to show"), a side-by-side option.

## M11ai — Markdown reader upgrade (directions A + B + C, merged)

- **A · Comfortable reader:** a centred column (about 70 characters), bigger type and spacing, coloured code blocks with
  their language and a **Copy** button, GitHub-style callouts (Note, Tip, Important, Warning, Caution), heading anchors.
- **B · Docs site:** an "On this page" outline that follows where you are (click to jump), a thin reading-progress line.
- **C · Looks like GitHub:** striped tables, front-matter shown as a small table, badges drawn as pills from their alt
  text (offline).
- A's centred column and C's full width don't fit together: the AI's call is A's column with C's tables, badges and
  front-matter (told to you 2026-11-16).

## Not in them (for now)

Editing; language servers; the other near-future ideas shown and not picked (2 rewind this session, 4 outline and
peek, 5 focus and age lens).

## Related

[[40 - Roadmap]] · [[12 - Scope]] · [[80 - Milestone 11ae - File Tab Chores]] ·
[[Proposal - Chat View and Agent Tabs]]
