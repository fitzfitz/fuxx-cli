---
tags: [development, milestone]
status: done
---

# 44b - Milestone 4b - Tester Build

**Status:: ✅ Done (2026-10-26). Installed from a "downloaded" DMG and confirmed on screen, incl. the Intel slice under Rosetta.**  ·  **Lets testers try the MVP.**
**Plan:** [[M4b Plan - Tester Build]] (step-by-step, with status per task)

## Goal

Wrap the M4 app in an **unsigned `.app`** and ship it in a **DMG**, so a tester can drag fuxx to
Applications and run it, on an **Apple Silicon or Intel** Mac (D7). Each update is a new DMG with a
new version number. Signing, notarization and Homebrew stay later ([[40 - Roadmap]]: M11, M12).

## Decisions (2026-10-26)

- **D7 — universal.** The app runs on Apple Silicon (M-series) and Intel Macs.
- **D8 — accept the weaker detection.** Opened from Finder, Claude Code doesn't know which terminal
  it's in and sends no notifications. Rings then rely on the 3 s quiet timer and the screen's
  question patterns. Revisit if testers report missed rings.

## Rough scope / checklist

- [x] One command builds `target/dist/fuxx-<version>.dmg` (version from `Cargo.toml`), and refuses to reuse a version.
- [x] The app inside is universal (`arm64` + `x86_64`), minimum macOS 11.0, ad-hoc sealed.
- [x] A check script confirms a DMG is installable before it goes to testers.
- [x] Installed like a tester (a "downloaded" copy, Open Anyway), the M4 app works when opened from Finder.
- [x] The Intel slice runs (under Rosetta on this Mac).

## Success check

A tester opens the DMG, drags fuxx to Applications, clicks "Open Anyway" once, and gets working
sessions, rings and banners.

## Tester install steps (send these with the DMG)

1. Open `fuxx-<version>.dmg` and drag **fuxx** onto **Applications**. For an update, replace the old copy.
2. Open fuxx from Applications. macOS says it can't verify the app (it isn't signed yet).
3. Open **System Settings → Privacy & Security**, scroll down, click **Open Anyway** next to fuxx, and confirm.
   This is needed once per new version.
4. Banners appear as "Script Editor" for now. That's expected.

## Related

- [[40 - Roadmap]] · [[12 - Scope]] · [[00 - Plan Tracker (Start Here)]] · [[44 - Milestone 4 - Notification Rings]]
