# Milestone 11t — Preview Tabs

**Status:** 🟨 built on master 2026-11-11 (after v0.7.0); waiting for your check.
**Asked for:** 2026-11-11, by you: "ketika single klik file apakah bisa buka sementara? jika saya single klik file
lain, akan ganti file yang terbuka sementara jadi file tersebut. Jika double klik, akan buka permanen seperti
sekarang?" → "commit, push dan deploy release dulu, lalu kerjakan".

## What it does

- **Files:** a single click on a file opens it in the pane's **preview tab** (its title in *italics*). A single click
  on another file shows that file in the same tab instead of adding one. A **double click** opens it for good (a
  permanent tab, as before): a preview tab showing it becomes permanent.
- **Changes:** the same (single click: preview, in Diff; double click: permanent).
- A preview tab becomes permanent when you **double-click its title** or switch it **File ↔ Diff**.
- One preview tab per pane. A file already open in the pane is just selected (preview or not).
- Enter in the Files tree and ⌘P open for good, as before.

## AI's calls

1. The preview isn't saved: after a restart it is an ordinary tab.
2. A preview tab dragged into another pane becomes permanent there (reordering it within its own bar keeps it a
   preview).

## Not in it

Pinning on scroll or selection, a preview for terminals or browser tabs, a setting to turn it off.

## As built (2026-11-11)

- `layout_model.rs`: `Model::previews` (not saved); `preview_file` (select the tab showing it, else the preview shows
  it, else a new tab becomes the preview); `open_file` keeps the tab it finds; `pin`; `is_preview`; a closed tab or one
  moved to another pane is no longer a preview. Test: `a_preview_tab_is_reused_until_a_file_is_opened_for_good`.
- `file_tab.rs`: `Workspace::preview_file_tab` (a reused tab gets a new view for its new file), `pin_tab`; the
  File ↔ Diff toggle keeps the tab.
- `right_sidebar.rs` (Files) and `changes_panel.rs` (Changes): a single click previews, a double click opens for good;
  Enter in the tree opens for good.
- `tab_bar.rs`: `BarTab::preview`, an italic title; a double click on a file tab's title keeps it.
- Tests: 1354 pass. ui-shots: `files-keys` checks a click previews and the next click reuses the tab.

## On-screen checklist (for you)

1. Single-click a file: it opens in an italic tab.
2. Single-click another: the same tab shows it; no new tab.
3. Double-click a file: a normal tab (the italic one becomes normal if it showed that file).
4. Double-click an italic tab's title, or switch it to Diff: it becomes normal.
5. The same in Changes.
