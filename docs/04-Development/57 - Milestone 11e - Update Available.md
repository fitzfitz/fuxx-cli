---
tags: [development, milestone]
status: built
---

# 57 - Milestone 11e - Update Available

**Status:: 🟨 Built, merged and released as v0.3.0 (2026-11-08); the on-screen check of the update itself (v0.3.1) is pending.**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11e Plan - Update Available]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]]). It comes before M12 and doesn't need it.

## Goal

fuxx tells you when a newer version is out, with a blue **Update Available** button above the sidebar's footer.
One click downloads it, swaps `fuxx.app` and reopens fuxx, with no "Open Anyway" and no paid Apple account. New
versions are published as GitHub Releases, built by GitHub Actions when a version tag is pushed.

## Why now (your words and choices, 2026-11-08)

- "When I open fuxx in Applications, it's still the old version. Can we have 'Update Available', and clicking it
  auto-updates?"
- Without M12 (no $99/year Developer ID). Homebrew is free, but since Homebrew 5.0 the official cask repo takes only
  signed and notarized apps, and `--no-quarantine` is gone, so Homebrew alone wouldn't avoid "Open Anyway".
- You chose **fuxx's own updater** over Sparkle (no new dependency).
- Your additions: the button sits **above the bottom bar** (the `+ 🔔 ⚙` row), reads **"Update Available"**, and
  is **blue and rounded**. Releases are made by **GitHub Actions on a tag**.
- You skipped the questions ("lewati pertanyaan"), so everything else below is AI's call. Correct anything that's
  wrong before the build.

## Why it can work without a Developer ID

macOS asks "Open Anyway" only for apps carrying the **quarantine mark**, which browsers and similar apps put on
files they download. `curl` doesn't add it. So when fuxx downloads the update itself with `/usr/bin/curl`, unpacks it
with `ditto` and swaps it in, the new ad-hoc-signed app opens like a local build. **This is checked on your Mac first
(Task 0)**, together with whether the notification permission survives the swap. If either fails, the design changes
before anything else is built.

**Task 0 result (2026-11-08, on your Mac): both pass.** The M11d build, zipped with `ditto`, served on localhost and
downloaded with `/usr/bin/curl`, carried no `com.apple.quarantine` (only `com.apple.provenance`, which macOS adds to
any created file); `codesign --verify --deep --strict` passed. Swapped into `~/Applications/fuxx.app` (the old 0.2.1
kept as `fuxx.app.previous`) and opened: **no "Open Anyway"**, and Help → Notifications Status… said fuxx sends its
own notifications **without asking again**. Confirmed by you. (Side effect: M11d is now the installed fuxx.)

## Releases (GitHub Actions)

- Workflow `.github/workflows/release.yml`. It runs when a tag `v*` is pushed, on a GitHub-hosted macOS runner
  (free for this public repo).
- Steps:
  1. **Refuses** unless the tag (without the `v`) equals `version` in `Cargo.toml`, so a release can't carry the wrong
     number.
  2. Runs `cargo test`.
  3. Builds the universal app with the existing `scripts/bundle-mac.sh --app-only`.
  4. Zips it with `ditto -c -k --keepParent` as `fuxx-<version>.zip`.
  5. Creates the GitHub Release `v<version>` (title `fuxx <version>`) with that zip and notes made from the commits
     since the last release (`gh release create --generate-notes`).
- **How you release:** bump `version` in `Cargo.toml`, commit, then tag `v<version>` and push the tag. Claude pushes
  a tag only when you say so, because releases are public.
- The first release is **v0.3.0**: everything up to now, M11d included. The version goes from 0.2.0 to 0.3.0 in this
  milestone.

## Checking for updates

- Only a fuxx running from `fuxx.app` checks; `cargo run` never does.
- It checks 10 s after launch, then every 24 h while running. It asks
  `https://api.github.com/repos/fitzfitz/fuxx-cli/releases/latest`, with no token (the repo is public), using `curl`
  on a background thread. Nothing ever waits on it.
- **Newer** means the release's `x.y.z` is greater than the running version, compared as numbers (`0.10.0` >
  `0.9.0`). An older or equal release shows nothing.
- When offline, rate-limited, or when GitHub answers with something unexpected, an automatic check stays **silent**
  and tries again at the next 24 h.
- **fuxx → Check for Updates…** (in the fuxx menu, above Hide fuxx) checks now and always answers with a sheet: the
  update offer, "fuxx 0.3.0 is the newest version." (up to date), or "Couldn't check for updates" with the reason
  (offline, GitHub's error).
- **Preferences → General → "Check for updates"** switch, on by default (settings line `check-updates = off` turns
  it off). Under it: "Asks GitHub once a day whether a newer fuxx is out." Off stops the automatic checks; the menu
  item still works.

## The Update Available button

```
│   ~/Kerja/proj                │
│  ┌─────────────────────────┐  │
│  │    Update Available     │  │   blue, rounded, clickable
│  └─────────────────────────┘  │
│  ─────────────────────────────│
│  +                     🔔  ⚙  │
```

- It sits just above the footer row, inset like the footer (`ROW_INSET` each side), 8 px above the footer's top
  edge. It is 28 px tall with an 8 px radius.
- Colours: fill **`#1F6FEB`** (blue), hovered **`#1A5FCC`**, text white, centred, medium weight, the sidebar's
  name size. These are fixed rather than themed, as you asked for blue. White on `#1F6FEB` meets the 4.5:1
  contrast floor in both light and dark themes.
- It shows only while a newer version is known. While the update downloads it reads **"Downloading v0.3.2…"** (see the Amendment) and
  can't be clicked. With the sidebar hidden, it shows once the sidebar is open again.
- **Clicking it** opens a sheet on the window:
  - Title: **"fuxx 0.3.0 is available"**.
  - Text: "You have 0.2.0." Then the release notes' first 6 lines. Then, only if restarting ends sessions (the
    background service is off and sessions are open): "Restarting ends 4 sessions."
  - Buttons: **Update and Restart** · **Later**.
- With the background service on, the sessions keep running, and the new fuxx reattaches to them after the restart.

## Installing the update

1. Download `fuxx-<version>.zip` (the release's asset with exactly that name) with `curl -fL` into
   `~/Library/Caches/fuxx/update/`, which is emptied first.
2. **Check it.** All four must hold, or nothing is swapped:
   - its SHA-256 equals the asset's `digest` from GitHub;
   - it unzips (with `ditto -x -k`) to exactly one `fuxx.app`;
   - that app's bundle identifier is `io.github.fitzfitz.fuxx` and its version is the release's;
   - `codesign --verify --deep --strict` passes.
3. **Swap and reopen.** fuxx writes a small shell script to the cache folder, starts it detached and quits. The
   script:
   - waits until that fuxx's process has ended (at most 60 s);
   - renames the running app to `fuxx.app.previous` beside it, and moves the new app into place;
   - if the move fails, puts the previous app back;
   - removes any quarantine mark (`xattr -dr com.apple.quarantine`, a no-op normally), then opens fuxx.
4. The new fuxx, at launch, deletes `fuxx.app.previous` if it finds it. Until then the previous version stays beside
   it, and you can open it by hand if the new one won't start.
5. **Where it can't update itself**, fuxx opens the release page in your browser instead (the button and the sheet
   say "Download from GitHub…" instead of Update and Restart). That covers a folder it can't write, running from a
   disk image, or a release with no matching zip or no digest.
6. **When the download or a check fails**, a sheet says "The update couldn't be installed", with the reason and
   **Open Release Page** · **OK**. The installed fuxx is untouched.

## As built (2026-11-08)

Where the build settled something the plan left open, or changed it:

- **The digest must be `sha256:` plus exactly 64 hex characters**; anything else counts as no digest (the release
  page fallback). Release links fuxx opens are used only under `https://github.com/fitzfitz/fuxx-cli/`.
- **"Restarting ends N sessions"** follows the background service as it is running (on, off, failed, starting), not
  only the setting fuxx started with.
- **`install` acts only if the same version is still on offer** when the sheet's answer arrives.
- **The swap script** uses absolute tool paths and its own `PATH`. It refuses to move the new app over anything at
  `fuxx.app`, and never opens a half-moved app: if the old one can't be put back, it opens nothing and the old copy
  stays as `fuxx.app.previous`. If the old fuxx hasn't quit within 60 s, it changes nothing.
- **The zip must hold exactly one app, named `fuxx.app`.**
- **`fuxx.app.previous` is removed once the new fuxx's window has opened** (not before), so a build that can't open
  its window leaves the previous version in place.
- Four failure reasons have their own wording: "the download failed", "the download folder couldn't be emptied",
  "the swap couldn't be started", "fuxx can't update itself here".
- Known limits, left as they are: the daily timer counts only time the Mac is awake (a laptop asleep overnight checks
  less often); turning the setting on waits for the next daily tick; the background service keeps running the old
  version until it stops by itself (as planned). A future release that changes the protocol's minimum would show
  the existing protocol message after updating, until the old service stops.

## Amendment (2026-11-08, your request after seeing the button)

"It needs a 'package' icon in the button." The button gets a line-drawn package icon (an isometric box with a tape
line, like fuxx's other footer icons) in a 16 px box, white, 6 px left of the text, the pair centred. Then, your
second ask: "also add the version it will update to". The button reads **`Update Available · v0.3.2`**, the
` · v0.3.2` part in white at 75% opacity; while downloading it reads **`Downloading v0.3.2…`**. When the sidebar is
too narrow for all of it, the version part is dropped first; the icon and "Update Available" always show. Built in
[[M11f Plan - Cursor Style]] Task 1 and shipped in v0.3.2. On-screen item 4 was seen on your Mac before this: the
button appears above the footer (2026-11-08).

## Fix: retries on a failed download (2026-11-08, your "yes")

Your update from 0.3.2 to 0.4.0 showed "The update couldn't be installed — the download failed". Cause (checked): GitHub's
release-download server answered **HTTP 504** for every release file (also v0.3.3's), from its own edge
(`x-github-edge-region: southeastasia`); github.com and the API were fine, no proxy. fuxx behaved as designed (the sheet,
nothing touched). The download now retries: `--retry 3 --retry-delay 2 --retry-all-errors` (plain `--retry` didn't
help: with `-f` over HTTP/2, curl 8 reports a 504 as error 56, not a transient one; checked against the live server).
It still fails after 3 retries if GitHub keeps failing; a half-written zip never gets past the checksum. Ships with
the next release (your installed 0.3.2 still has the old code).

## Not in M11e

- Installing without a click, background installs, or installing at quit.
- Beta or nightly channels, skipping a version, release notes inside the app beyond the sheet's lines.
- Developer ID signing and notarization (M12), and Homebrew (M13).
- Delta updates; Intel- or Apple-Silicon-only downloads (the zip stays universal).
- Updating the background service separately: it restarts from the new app the next time it's started, as today.

## On-screen checklist (pending)

1. ✅ (2026-11-08) **Task 0 (spike):** the M11d build, downloaded by `curl` and swapped into `~/Applications/fuxx.app` by the spike
   script, opens with **no "Open Anyway"**.
2. ✅ (2026-11-08) **Task 0 (spike):** Help → Notifications Status… says fuxx can send its own notifications without asking again
   (the permission survived the swap).
3. ✅ (2026-11-08, run 42429479670, green on the first run) Pushing tag `v0.3.0` runs the Release workflow to green, and the release page shows `fuxx-0.3.0.zip`.
4. ✅ (2026-11-08, seen by you) With 0.3.0 installed and `v0.3.1` released, within about 10 s of opening fuxx the blue **Update Available** button
   is above the footer.
5. Clicking it shows "fuxx 0.3.1 is available", with "You have 0.3.0." and the notes.
6. **Later** closes the sheet; the button stays.
7. **Update and Restart:** the button reads "Downloading v…" (the target version), then fuxx closes and reopens as 0.3.1 with no
   "Open Anyway". `fuxx.app.previous` is gone afterwards.
8. With the background service on, the sessions are still there after the restart. With it off, the sheet said how
   many sessions would end.
9. fuxx → Check for Updates… on 0.3.1 says "fuxx 0.3.1 is the newest version."
10. Wi-Fi off, then fuxx → Check for Updates… → "Couldn't check for updates" with a reason. No button appears.
11. Preferences → General → Check for updates off: no automatic check (no button for a newer release); the menu
    item still works. After turning it on again, restart fuxx to check at once (otherwise the next daily tick).
12. Light and dark themes: the button is readable (the ui-shots cover this too).
