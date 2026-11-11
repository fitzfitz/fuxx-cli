# Milestone 11t — Preview Tabs

**Status:** 🟨 asked for and approved 2026-11-11; being built on master, after v0.7.0.
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
2. A preview tab dragged into another pane stays a preview there only if that pane has none (else permanent).
   (Kept simple: a moved tab becomes permanent.)

## Not in it

Pinning on scroll or selection, a preview for terminals or browser tabs, a setting to turn it off.

## On-screen checklist (for you)

1. Single-click a file: it opens in an italic tab.
2. Single-click another: the same tab shows it; no new tab.
3. Double-click a file: a normal tab (the italic one becomes normal if it showed that file).
4. Double-click an italic tab's title, or switch it to Diff: it becomes normal.
5. The same in Changes.
