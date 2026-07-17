---
tags: [architecture, decision, scope]
---

# 23 - Why Not libghostty

A recorded decision. This is the scope boundary that keeps the project finishable.

## The temptation

cmux embeds libghostty to render a real terminal. The instinct is "fuxx-cli should do
what cmux does, so it should use libghostty too."

## Why it's rejected for this project

- **libghostty's full embedding API is not stabilized for general use.** Only
  libghostty-vt (the parsing engine, no rendering) is generally available, and its API
  is explicitly still unstable with breaking changes expected.
- **The rendering/embedding path is currently macOS + Swift**, used by native apps. The
  existing Rust binding is for the parser only, not the renderer.
- **fuxx-cli does not need to render anything.** The user already has a terminal. We
  only need to *listen* to output. Rendering is not our problem to solve.
- **Beginner scope.** Building a terminal from an unstable engine is a multi-month
  effort for an experienced team. It would stall a first project.

## The decision

fuxx-cli uses the [[21 - The Wrapper Process Model|wrapper process model]] instead. No
libghostty, no emulation. If terminal-level features are ever truly needed, revisit —
but only after v1 ships.

## Related

- [[12 - Scope Boundaries]]
- [[11 - Relationship to cmux]]
