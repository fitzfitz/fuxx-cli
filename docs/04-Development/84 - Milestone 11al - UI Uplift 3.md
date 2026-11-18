# Milestone 11al — UI Uplift 3: one floating card, roomier sections

**Status:** 📝 direction chosen 2026-11-17 in browser mockups ("looks right, do it right after M11ak"). Design-first: its
own detailed design and plan come after M11ak is built. Not designed or planned yet.

**Asked for:** 2026-11-17, by you: "let's brainstorm on the UI/UX uplift … What do you think we need to improve on the
layout part?" Then, after four structure options and their comfortable versions: "I don't like them all. I love the
current positioning, but each sections feels off and too compact". Then "more options" → "I love ours now the floating
panel on the middle and we need to merge the floating panel on the right with the middle. Left stays unfloating as the
layout is better to stay sticks" → option **1** ("1 is good, but still dont like how the tab presented") → tab style
**E, minimal** ("I like that minimal") → "looks right, do it right after M11ak".

**Mockups:** `.superpowers/brainstorm/34352-1794905233/content/` — `current-refined.html` (R), `floating-merged.html`
(option 1), `tab-styles.html` (E), `combined.html` (the chosen whole). Names in them are made up.

## The direction (yours)

- **Positions stay as today:** workspaces on the left, tabs and terminal in the middle, Files / Changes / Activity on
  the right.
- **Roomier ("comfortable") everywhere:** 13–14 px UI text, ~40–56 px rows and tabs, about double today's padding.
- **Left sidebar stays attached** to the window edge (not floating), on the darkest background.
- **The middle and the right panel are one floating card:** rounded corners, a soft shadow, inset from the window
  edges; inside it the terminal and the right panel sit side by side with a hairline between them (option 1). The
  right panel keeps its own Files / Changes / Activity switch, level with the tab bar.
- **Minimal tabs (E):** quiet text tabs, each with a small icon tile and a status dot, thin separators between them;
  the active tab gets a faint accent wash. `＋ ⋯ ▥` on the right.

## Per section (from R, the refined current layout)

- **Sidebar:** 56 px rows; 14 px names; the second line says who and where ("✳ Claude · api", "›_ zsh · my-app")
  instead of repeating the full path; group headings are quiet text, not cards; the selected row has a soft fill and
  an accent bar on its edge; a status dot on each avatar; ages top-right; a roomy "＋ New Fuxxpace ⌘N" button, 🔔, ⚙.
- **Terminal:** more inner padding (about 28 px sides, 22 px top). Its text size still follows your font setting.
- **Right panel:** Files / Changes / Activity as one segmented switch; a repo card ("my-app · 2 changed · 0 staged");
  Changes open when there are changes, with +/− per file; the branch footer with clear ⟳ and ⇡ buttons.

## Order

**Right after M11ak** (the agent box), before M11ag — your call. The agent box lives inside the card, so they should
match.

## Related

[[40 - Roadmap]] · [[83 - Milestone 11ak - Agent Box]] · [[60 - Milestone 11i - UI Uplift 2]] · [[12 - Scope]]
