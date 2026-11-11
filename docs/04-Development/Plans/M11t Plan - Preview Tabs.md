# M11t Plan — Preview Tabs

> Executed natively on master. Design: [[71 - Milestone 11t - Preview Tabs]].

1. `layout_model.rs`, test first: `Pane::preview: Option<TabId>`; `Model::preview_file(pane, path, mode)` (select the
   tab showing it; else show it in the preview tab; else a new tab that becomes the preview); `open_file` pins the
   tab it finds; `pin(tab)`; `is_preview(tab)`; a closed or moved preview is forgotten.
2. `file_tab.rs` / `workspace.rs`: `preview_file_tab`; a reused preview tab gets a new view for its new file.
3. `right_sidebar.rs`, `changes_panel.rs`: single click previews, double click opens.
4. `tab_bar.rs`: `BarTab::preview` (italic title); double-click a tab title pins; the File/Diff toggle pins.
5. Verify (tests, fmt, clippy, ui-shots), docs (note 71, Roadmap, Scope, tracker, Progress Log).
