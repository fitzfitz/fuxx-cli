---
tags: [development, plan, milestone]
milestone: M4b
status: in-progress
updated: 2026-10-26
---

# M4b Plan — Tester Build (unsigned universal `.app` in a DMG)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[44b - Milestone 4b - Tester Build]] |
| **Status** | 🟨 In progress |
| **Tasks done** | 1 / 4 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | ✅ **D7** = universal: Apple Silicon + Intel (decided 2026-10-26) · ✅ **D8** = accept the weaker detection when opened from Finder (decided 2026-10-26) |
| **Needs your eyes** | 👀 Task 2: install from the DMG like a tester, on both CPU slices |

**Goal:** One command turns the current code into `target/dist/fuxx-<version>.dmg`. A tester opens
it, drags **fuxx** to Applications, clears Gatekeeper once, and gets the M4 app (sessions, rings,
banners) on an **Apple Silicon or Intel** Mac.

**Architecture:**

```
 scripts/bundle-mac.sh
   version ◄── cargo pkgid (Cargo.toml)          refuses if fuxx-<version>.dmg already exists
   cargo build --release --target aarch64-apple-darwin ┐
   cargo build --release --target x86_64-apple-darwin  ┴─► lipo -create ─► fuxx.app/Contents/MacOS/fuxx
   packaging/macos/Info.plist (@VERSION@ → version) ──────────────────► fuxx.app/Contents/Info.plist
   codesign --sign - (ad-hoc seal) ─► + Applications symlink ─► hdiutil (UDZO) ─► target/dist/fuxx-<version>.dmg
 scripts/check-dmg.sh <dmg>   mounts it read-only and asserts every property above
```

**Updates for testers** (as you described, 2026-10-26): bump `version` in `Cargo.toml`, run
`scripts/bundle-mac.sh`, send the new `fuxx-<version>.dmg`. The tester drags it over the old copy.

**Tech Stack:** No new crates. macOS tools from the Xcode Command Line Tools: `lipo`, `codesign`,
`hdiutil`, `plutil`, `vtool` (all present on this machine). One extra Rust target,
`x86_64-apple-darwin`, added with `rustup target add` on 2026-10-26.

**Spec:** [[44b - Milestone 4b - Tester Build]] · [[40 - Roadmap]] (M4b line) · [[12 - Scope]] · D7, D8 in [[00 - Plan Tracker (Start Here)]]

## Global Constraints

- Everything from the M0–M4 plans. **No new crates, no Rust source changes.** `cargo test` stays at **61 passed**.
- **D7:** the binary in the app is **universal**: `arm64` + `x86_64` slices, both with minimum macOS **11.0** (`MACOSX_DEPLOYMENT_TARGET=11.0`, matching `LSMinimumSystemVersion`).
- **Unsigned for Apple:** ad-hoc seal only (`codesign --sign -`). **No Developer ID, no notarization** (M11). Testers click "Open Anyway" once per version.
- **D8:** the shell's environment stays inherited as-is. Opened from Finder, Claude Code sends no notifications; rings rely on the 3 s quiet timer and the screen's question patterns. Nothing is built for this.
- Version comes **only** from `Cargo.toml`'s `version`. One DMG per version; the script never overwrites one.
- Bundle identifier: **`io.github.fitzfitz.fuxx`** (from the GitHub repo owner). M11 signs with it, so it shouldn't change later.
- Banners still say **"Script Editor"** (osascript; fixed in M11). Expected.
- Scripts: `#!/usr/bin/env bash`, `set -euo pipefail`, executable. Never `cmd | grep -q` (under `pipefail` an early-exiting `grep` can fail the pipe): capture the output first.

## Review Focus

1. **A tester on an Intel Mac** gets an app that won't start → `check-dmg.sh` asserts both slices and their macOS floor (Task 1); Task 2 runs the Intel slice under Rosetta.
2. **A downloaded copy won't open** (no "Open Anyway", or reported "damaged") → `check-dmg.sh` asserts a strict, valid seal (Task 1); Task 2 opens a quarantined copy, as a download would be.
3. **Two different builds share a version**, so nobody knows which one a tester has → `bundle-mac.sh` refuses an existing DMG (Task 1, Step 6).
4. **Opened from Finder, the shell is broken**: wrong folder, `claude` not found, accented text garbled → Task 2 checks each in a Finder-launched copy.
5. **Rings look dead to testers** because Claude Code sends nothing (D8) → Task 2 runs real `claude` in the Finder-launched app and expects Done from the quiet timer.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `packaging/macos/Info.plist` | Create | Bundle metadata; `@VERSION@` placeholder | 1 |
| `scripts/check-dmg.sh` | Create | The test: asserts a DMG is installable (contents, universal, macOS floor, version, seal) | 1 |
| `scripts/bundle-mac.sh` | Create | Builds both slices, assembles + seals `fuxx.app`, writes the DMG | 1 |
| `STRUCTURE.md` | Modify | Map the new `scripts/` and `packaging/` folders | 3 |

---

### Task 0: Record D7 + D8 — ✅

- [x] [[00 - Plan Tracker (Start Here)]]: **D7** ✅ "universal (Apple Silicon + Intel), decided 2026-10-26; affects M4b, M11, M12". **D8** ✅ "accept the weaker detection, decided 2026-10-26". M4b row → this plan, ⬜ 0/4. **Where we are** → M4b, next action Task 1. Fix the stale "merged into `master` locally, not pushed" (M4 is pushed).
- [x] [[40 - Roadmap]]: M4b line → universal + D8 outcome, link this plan. M11 line: drop "Open decision D7" (decided).
- [x] Create [[44b - Milestone 4b - Tester Build]] (goal, checklist, success check, tester install steps).
- [x] [[60 - Progress Log]]: entry "M4b plan written; D7 + D8 decided".
- [x] Commit *(when you say so)*: `git add docs && git commit -m "docs(m4b): D7 universal, D8 accept weaker detection, M4b plan"`

### Task 1: Bundle + check scripts — ⬜

**Files:** Create `packaging/macos/Info.plist`, `scripts/check-dmg.sh`, `scripts/bundle-mac.sh`.

**Interfaces:** Produces `scripts/bundle-mac.sh` (no arguments; prints the DMG path on success)
and `scripts/check-dmg.sh <dmg>` (prints `OK: <dmg> (fuxx <version>, <archs>)` and exits 0, or
`FAIL: <reason>` on stderr and exits 1). Task 2 installs from that DMG.

> **Why a script is the test.** There's no Rust to unit-test here. The deliverable is a file a
> tester installs, so `check-dmg.sh` asserts its properties, and it is re-run on every future version.

- [ ] **Step 1: Write the test.** `scripts/check-dmg.sh <dmg>` takes the version from the file name
  (`fuxx-<version>.dmg`) and fails with a message for each of these, in order:
  1. the file doesn't exist → `FAIL: <dmg> not found`; `hdiutil verify -quiet` fails;
  2. after `hdiutil attach -quiet -readonly -nobrowse -mountpoint <mktemp -d>` (detach via `trap … EXIT`):
     no `fuxx.app`; `Applications` isn't a symlink to `/Applications`;
  3. `lipo -archs fuxx.app/Contents/MacOS/fuxx` lacks `arm64` or `x86_64` → `FAIL: not universal: <archs>`;
  4. for `arm64` and `x86_64`, `vtool -arch <a> -show-build` lacks `minos 11.0`;
  5. `plutil -lint -s` fails; `CFBundleShortVersionString` or `CFBundleVersion` (`plutil -extract <key> raw`) ≠ the version;
  6. `codesign --verify --strict fuxx.app` fails.
- [ ] **Step 2: Run** `scripts/check-dmg.sh target/dist/fuxx-0.1.0.dmg`. Expected: `FAIL: target/dist/fuxx-0.1.0.dmg not found`, exit 1.
- [ ] **Step 3: Write `packaging/macos/Info.plist`** (XML plist) with exactly: `CFBundleName` and
  `CFBundleDisplayName` = `fuxx`, `CFBundleIdentifier` = `io.github.fitzfitz.fuxx`,
  `CFBundleExecutable` = `fuxx`, `CFBundlePackageType` = `APPL`,
  `CFBundleShortVersionString` and `CFBundleVersion` = `@VERSION@`,
  `LSMinimumSystemVersion` = `11.0`, `NSHighResolutionCapable` = true.
- [ ] **Step 4: Write `scripts/bundle-mac.sh`**, runnable from any folder (`cd "$(dirname "$0")/.."` first):
  1. `version` = `cargo pkgid`'s text after the last `#` or `@` (`path+file:///…#fuxx@0.1.0` → `0.1.0`);
     `dmg=target/dist/fuxx-$version.dmg`. If it exists: `<dmg> already exists: bump version in Cargo.toml` on stderr, exit 1.
  2. Export `MACOSX_DEPLOYMENT_TARGET=11.0`. For `aarch64-apple-darwin` and `x86_64-apple-darwin`:
     if `rustup target list --installed` lacks it, `missing Rust target: rustup target add <target>` on stderr, exit 1;
     else `cargo build --release --target <target>`.
  3. In a `mktemp -d` stage (removed on exit): `fuxx.app/Contents/MacOS/fuxx` =
     `lipo -create` of `target/<target>/release/fuxx` for both; `Contents/Info.plist` = the
     template with `@VERSION@` replaced (`sed`); `codesign --force --sign - fuxx.app`;
     `ln -s /Applications Applications`.
  4. `mkdir -p target/dist`; `hdiutil create -quiet -volname "fuxx $version" -srcfolder <stage> -format UDZO <dmg>`; print the DMG path.
- [ ] **Step 5: Run** `scripts/bundle-mac.sh && scripts/check-dmg.sh target/dist/fuxx-0.1.0.dmg`.
  Expected: `codesign` notes `replacing existing signature` (the linker's), then
  `target/dist/fuxx-0.1.0.dmg`, then `OK: target/dist/fuxx-0.1.0.dmg (fuxx 0.1.0, x86_64 arm64)`.
  The first run compiles both slices (about 1–2 min each here).
- [ ] **Step 6: Run** `scripts/bundle-mac.sh` again. Expected: `target/dist/fuxx-0.1.0.dmg already exists: bump version in Cargo.toml`, exit 1, the DMG unchanged.
- [ ] **Step 7: No warnings of ours.** `grep -E "^(warning|error)"` over Step 5's output, minus the GPUI `future version` note. Expected: **no output**. `cargo test`: `test result: ok. 61 passed`.
- [ ] **Step 8: Commit** *(when you say so)*: `git add packaging scripts && git commit -m "feat(m4b): universal tester DMG (bundle-mac.sh) with check-dmg.sh"`

### Task 2: Install it like a tester — 👀 ⬜

The AI prepares a "downloaded" copy; **you** do the rest and report what you see.

- [ ] *(AI)* Copy the DMG to `~/Downloads/fuxx-0.1.0.dmg` and mark it as downloaded:
  `xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" ~/Downloads/fuxx-0.1.0.dmg`.
- [ ] Double-click it. A window shows **fuxx** and an **Applications** folder. Drag fuxx onto Applications.
- [ ] Open fuxx from **Applications** (double-click). macOS blocks it ("Apple could not verify…").
  System Settings → Privacy & Security → **Open Anyway** → it opens. *(Expected for an unsigned build.)*
- [ ] Opened from Finder, the first session starts in your **home folder**, `claude --version` works,
  and `echo "héllo 🚀"` prints correctly.
- [ ] M4 still works: cmd-t sessions, rings, a background `claude` turns 🟢 about 3 s after it
  finishes and a banner fires. *(D8: Claude Code sends no notification here, so Done comes from the quiet timer. Expected.)*
- [ ] cmd-q quits. Opening it again from Applications needs **no** second "Open Anyway".
- [ ] **Intel slice (Rosetta):** in Terminal, `arch -x86_64 /Applications/fuxx.app/Contents/MacOS/fuxx`.
  The window opens and a shell works. *(This proves the Intel code runs. The graphics are still this Mac's GPU, so a real Intel Mac stays untested.)*
- [ ] Finder → fuxx.app → Get Info shows version **0.1.0**.

### Task 3: Update the docs — ⬜

- [ ] [[44b - Milestone 4b - Tester Build]]: tick the checklist; **Status:: ✅ Done**.
- [ ] [[40 - Roadmap]]: M4b → ✅.
- [ ] [[00 - Plan Tracker (Start Here)]]: M4b → ✅ 4/4; **Where we are** → next is M5 (Phase 2), plan to be written.
- [ ] `STRUCTURE.md`: add `scripts/` (M4b: `bundle-mac.sh`, `check-dmg.sh`) and `packaging/macos/Info.plist`.
- [ ] [[60 - Progress Log]]: entry "M4b complete: tester DMG".
- [ ] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-26 | Release build, Apple Silicon (`MACOSX_DEPLOYMENT_TARGET=11.0`) | ✅ 0 warnings of ours, `minos 11.0` | AI build (~68 s), `vtool -show-build` |
| 2026-10-26 | Release build, Intel (`x86_64-apple-darwin`) at the pinned GPUI rev | ✅ 0 warnings of ours | AI build (~42 s after deps) |
| 2026-10-26 | Task 1's scripts, drafted from this plan's steps, in the repo (then removed so Task 1 starts clean) | ✅ Step 2 FAIL (not found) → Step 5 `OK: … (fuxx 0.1.0, x86_64 arm64)` → Step 6 refused; DMG 7.4 MB | AI run |
| 2026-10-26 | `check-dmg.sh` rejects an Apple Silicon-only image | ✅ `FAIL: not universal: arm64` | AI mutation check |
| 2026-10-26 | Gatekeeper on the trial app | ✅ `rejected` (unsigned, as expected → "Open Anyway"); `Signature=adhoc`, `Format=app bundle with Mach-O universal (x86_64 arm64)` | `spctl --assess`, `codesign -dv` |
| 2026-10-26 | A Finder-like environment (`env -i`, launchd sets no `LANG`): login shell finds `claude`; text is UTF-8 | ✅ on this Mac (something in your shell start-up sets `LANG=C.UTF-8`) | AI `env -i … zsh -l -i` |
| 2026-10-26 | `cargo test` | ✅ 61 passed | AI run |

## Known limitations

| Limitation | Lands in |
|---|---|
| Testers click "Open Anyway" once per new version (ad-hoc seal, not Developer ID) | M11 (signing + notarization) |
| Opened from Finder, Claude Code sends no notifications; Done/Waiting come from the quiet timer and screen patterns (D8) | Revisit if testers report missed rings |
| Banners say "Script Editor"; clicking one opens Script Editor | M11 (D5) |
| No app icon (macOS shows the generic one) | Not in the plan; ask if wanted |
| The Intel slice is only checked under Rosetta, not on a real Intel Mac | A tester with an Intel Mac |
| A tester whose shell config sets no locale may see accented text garbled when fuxx is opened from Finder (macOS sets no `LANG` for apps; yours is fine) | Revisit if a tester reports it |
| No auto-update: testers install each new DMG by hand | Not in the plan |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M4 Plan - Notification Rings]] · [[40 - Roadmap]] · [[12 - Scope]]
