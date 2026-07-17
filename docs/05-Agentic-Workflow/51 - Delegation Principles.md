---
tags: [workflow, ai, principles]
---

# 51 - Delegation Principles

The rules that keep AI assistance from turning into a black-box codebase.

## 1. One task at a time

Delegate a single milestone task, not "build the whole thing." Small tasks are
reviewable; large ones aren't. Mirrors [[40 - Roadmap]]'s one-milestone rule.

## 2. You must be able to explain every merged line

If you can't explain what a line does, don't commit it — ask the AI to explain it first.
This is the single rule that separates learning from cargo-culting.

## 3. Understand the error before accepting the fix

When Rust's compiler complains, read the message, form a guess, *then* ask. The compiler
is the best Rust teacher you have; don't skip past it.

## 4. Verify, don't trust

Run the success check yourself. AI can be confidently wrong. A milestone isn't done
because the AI says so — it's done when its check passes on your machine.

## 5. Keep the vault and log current

Update statuses in the milestone notes and add a [[60 - Progress Log]] entry every
session. The docs are part of the deliverable, not an afterthought.

## 6. Guard the scope boundary

Before accepting any suggestion that adds a feature, check it against [[12 - Scope]]. The pull
now is the opposite of v1's: to turn fuxx into a general IDE, or to drag cmux-parity extras
(split panes, in-app browser, SSH, daemon) into the core early. Build the terminal + the
multi-agent rings first; resist the rest until the core is solid.

## Related

- [[50 - Agentic Workflow]] · [[52 - Known Risks and Gotchas]]
