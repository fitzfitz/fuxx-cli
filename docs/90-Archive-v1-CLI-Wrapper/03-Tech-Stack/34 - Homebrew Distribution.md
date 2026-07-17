---
tags: [tech, distribution]
---

# 34 - Homebrew Distribution

How users install fuxx-cli. The target experience:

```
brew tap fitzfitz/fuxx-cli
brew install fuxx-cli
```

## The beginner-appropriate path: a tap

A "tap" is your own small Homebrew repository (a GitHub repo named
`homebrew-fuxx-cli`). It contains a **formula** — a small Ruby file describing where to
download the release binary/source and how to install it. This is exactly how tools like
cmux distribute (`brew tap manaflow-ai/cmux`).

## The rough sequence (details in [[44 - Milestone 4 - Ship via Homebrew]])

1. `cargo build --release` to produce a binary.
2. Create a GitHub **release** with the built artifact (or source tarball).
3. Create the `homebrew-fuxx-cli` tap repo.
4. Write the formula pointing at that release, with its checksum.
5. Test locally with `brew install --build-from-source`.

## Note

Homebrew formulae are Ruby, but you don't need to *learn* Ruby — you fill in a small
template. Treat this as its own milestone; it's unrelated to the Rust learning.

## Related

- [[35 - Repo and Git]]
- [[44 - Milestone 4 - Ship via Homebrew]]
