---
tags: [tech, git]
---

# 35 - Repo and Git

Version control and where the code lives.

## The repo

Target repository: `https://github.com/fitzfitz/fuxx-cli`
(At vault creation time this returned 404 — create it, or make it public, before
pushing. This vault itself is delivered as files for you to commit.)

## Minimum Git workflow

```
git init                       # once, if starting fresh
git add .
git commit -m "message"
git branch -M main
git remote add origin https://github.com/fitzfitz/fuxx-cli.git
git push -u origin main
```

## What to commit

- All source (`src/`), `Cargo.toml`, `Cargo.lock`
- This Obsidian vault (e.g. under a `docs/` or `vault/` folder)
- A `.gitignore` that excludes `/target` (Rust's build output — large, regenerable)

## Suggested .gitignore

```
/target
**/*.rs.bk
.DS_Store
```

## Related

- [[32 - Cargo and Project Layout]]
- [[34 - Homebrew Distribution]]
- [[60 - Progress Log]]
