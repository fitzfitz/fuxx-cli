---
tags: [workflow, release, checklist]
updated: 2026-11-10
---

# 55 - Releasing

How a fuxx release is made. A release is a **GitHub Release** `v<version>` carrying `fuxx-<version>.zip` (the
universal, ad-hoc-signed `fuxx.app`). Every installed fuxx finds it through **Update Available** (M11e, note
[[57 - Milestone 11e - Update Available]]), so a release reaches everyone who has fuxx: it is public.

## The rule

- **Claude releases only when you say so** ("deploy", "release", "rilis"). Pushing a `v*` tag publishes; a release
  can be deleted but people may already have downloaded it.
- Release from **master**, after the work is merged and pushed.

## Choosing the number

`0.x.y` until M12. A release with a new feature or milestone raises `x` (0.5.0 → 0.6.0, `y` back to 0); one with only
fixes or tiny changes raises `y` (0.4.0 → 0.4.1). Past releases: `git log --oneline --grep "chore(release)"`.

## Steps

1. **Bump** `version` in `Cargo.toml` (the `[package]` one), then refresh `Cargo.lock` without building:
   `cargo metadata --format-version 1 --offline > /dev/null`.
2. **Check** the tag you will push matches: `scripts/check-release-tag.sh v<version>` (the workflow runs the same
   check and refuses a mismatch).
3. **Commit** only `Cargo.toml` and `Cargo.lock`:
   `chore(release): <version> — <what's in it, in a few words>`.
4. **Tag and push**: `git tag v<version>`, `git push origin master`, then `git push origin v<version>`.
5. **Watch** the run: `gh run list --workflow release.yml --limit 1`, then `gh run watch <id> --exit-status`.
   When it is green the release is at `https://github.com/fitzfitz/fuxx-cli/releases/tag/v<version>` with
   `fuxx-<version>.zip`.
6. **Docs** (part of the release): the milestone notes and their plans say "released in v<version>"; the
   [[40 - Roadmap]] and [[00 - Plan Tracker (Start Here)]] lines; a [[60 - Progress Log]] entry with the release
   commit, the tag, the run id and the asset link. Commit and push those as `docs: … released in v<version>`.

## What the workflow does

`.github/workflows/release.yml`, on GitHub-hosted `macos-15` runners (free: the repo is public).

```
           ┌─ test ─────────────────┐  cargo test (and, on a tag, the tag-matches-Cargo.toml check)
 push ─────┼─ slice: Apple Silicon ─┼─▶ release: lipo the two slices → fuxx.app → zip → publish (tags only)
           └─ slice: Intel ─────────┘  scripts/bundle-mac.sh --build <target>, uploaded as an artifact
```

- **When it runs:**
  - a pushed `v*` tag: everything, and **publishes** the GitHub Release (`gh release create --generate-notes`, notes
    made from the commits since the last release);
  - a push to **master that touches code** (`src/`, `assets/`, `packaging/`, `Cargo.toml`, `Cargo.lock`, the two
    release scripts, the workflow): test, both slices and the bundle, **no publish**. A newer master push cancels a
    master run still going; a tag's run is never cancelled;
  - by hand: `gh workflow run release.yml --ref master` (same as a master push).
- **The cache** (`Swatinem/rust-cache@v2`, one per job): it keeps cargo's downloads and compiled dependencies; fuxx
  itself is always compiled fresh. GitHub lets a run restore only caches saved on **its own ref or master**, and a
  tag is its own ref, so caches are **saved only by master runs** and every tag restores master's. Unused caches
  expire after **7 days**: a release after a quiet week starts cold (still in parallel). That is why master pushes run
  the workflow too.
- **Speed** (2026-11-10): one job compiling both slices one after the other, no cache: about **16 min** (v0.5.0 and
  v0.6.0). Three parallel jobs, cold cache: **9 m 47 s** (run 42755633352). Warm cache: **2 m 20 s** (run 42763987270: test 1 m 55 s, each slice about 2 m, the bundle 13 s).
- `scripts/bundle-mac.sh` options: none = both slices and the tester DMG; `--app-only` = just `target/dist/fuxx.app`;
  `--build <target>` = one slice only; `--prebuilt` = bundle slices already built (the workflow's last job).

## When something goes wrong

- **"tag … doesn't match Cargo.toml version":** the bump wasn't committed, or the tag has a typo. Nothing was
  published. Remove the tag (`git push origin --delete v<version>`, `git tag -d v<version>`), fix, tag again.
- **A failed test or build** on a tag: nothing was published. Fix on master, push, then move the tag only on your
  word (delete and re-push it as above).
- **A bad published release:** `gh release delete v<version>` plus deleting the tag removes it, but installed fuxx
  copies may already have offered or downloaded it; usually release a fixed `y+1` instead. Only on your word.
- **A broken cache:** `gh cache list`, `gh cache delete <id>` (or `--all`); the next master run rebuilds it.

## Local builds

- A tester DMG: bump the version, `scripts/bundle-mac.sh`, then `scripts/check-dmg.sh target/dist/fuxx-<version>.dmg`
  (M4b, note [[44b - Milestone 4b - Tester Build]]).
- Builds go to `target/<target>/release/` and `target/dist/`; they never touch the dev build in `target/debug/`.

## Links

- [[57 - Milestone 11e - Update Available]] (the release workflow's origin and how fuxx finds updates) ·
  [[44b - Milestone 4b - Tester Build]] · [[12 - Scope]] ("Small changes after v0.5.0") · [[60 - Progress Log]]
