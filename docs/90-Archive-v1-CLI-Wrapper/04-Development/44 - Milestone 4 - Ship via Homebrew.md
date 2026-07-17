---
tags: [development, milestone]
status: not-started
---

# 44 - Milestone 4 - Ship via Homebrew

**Status:: ⬜ Not started**

## Goal

Make fuxx-cli installable with `brew install`.

## Checklist

- [ ] `cargo build --release`
- [ ] Push the code to `github.com/fitzfitz/fuxx-cli` (create/repair the repo first)
- [ ] Create a GitHub release with the artifact (or a source tarball) + checksum
- [ ] Create the `homebrew-fuxx-cli` tap repo
- [ ] Write the formula pointing at the release
- [ ] Test: `brew tap fitzfitz/fuxx-cli && brew install fuxx-cli`

## Success check

A clean `brew install` puts a working `fuxx` on the PATH.

## What you'll learn

Release builds, GitHub releases, Homebrew taps/formulae. Independent of the Rust work.

## Related

- [[34 - Homebrew Distribution]] · [[35 - Repo and Git]]
