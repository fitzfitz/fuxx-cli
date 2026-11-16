//! The scene registry: every area's scenes, their run order, and the startup check that they
//! agree. To add a scene: put it in its area file's `SCENES` and its name in `ORDER` below (an
//! edit to this file runs no scene by itself; the area file is what `--changed` follows).

use super::{
    Scene, branch, changes, chrome, chrome_parts, file_tab, file_tab_md, naming, overlays, palette,
    panes, right_sidebar, sidebar, status_bar, terminal, welcome, window,
};

/// Files that run or declare the suite but change no scene's picture: an edit to one of these
/// alone runs nothing under `--changed` (they are neither shared nor unclaimed). `mod.rs`
/// (the runner) and `harness.rs` stay in `SHARED`.
pub(super) const RUNNER_ONLY: &[&str] = &[
    "src/ui_shots/registry.rs",
    "src/ui_shots/args.rs",
    "src/ui_shots/changed.rs",
];

/// Each area's file and scenes. A new area is added here (and as a `mod` in `mod.rs`); its file
/// is claimed by its own scenes (`claiming`).
pub(super) const AREAS: &[(&str, &[Scene])] = &[
    ("src/ui_shots/window.rs", window::SCENES),
    ("src/ui_shots/sidebar.rs", sidebar::SCENES),
    ("src/ui_shots/panes.rs", panes::SCENES),
    ("src/ui_shots/chrome_parts.rs", chrome_parts::SCENES),
    ("src/ui_shots/chrome.rs", chrome::SCENES),
    ("src/ui_shots/file_tab.rs", file_tab::SCENES),
    ("src/ui_shots/file_tab_md.rs", file_tab_md::SCENES),
    ("src/ui_shots/right_sidebar.rs", right_sidebar::SCENES),
    ("src/ui_shots/changes.rs", changes::SCENES),
    ("src/ui_shots/branch.rs", branch::SCENES),
    ("src/ui_shots/naming.rs", naming::SCENES),
    ("src/ui_shots/palette.rs", palette::SCENES),
    ("src/ui_shots/welcome.rs", welcome::SCENES),
    ("src/ui_shots/overlays.rs", overlays::SCENES),
    ("src/ui_shots/status_bar.rs", status_bar::SCENES),
    ("src/ui_shots/terminal.rs", terminal::SCENES),
];

/// The scenes with their own area file added to `sources`, as `--changed` reads them, so an edit
/// to a scene's file runs that area's scenes. Built once per run (the extended lists live for the
/// rest of it).
pub(super) fn claiming() -> Vec<Scene> {
    AREAS
        .iter()
        .flat_map(|(file, area)| {
            area.iter().map(move |scene| {
                let mut sources = vec![*file];
                sources.extend_from_slice(scene.sources);
                Scene {
                    sources: Box::leak(sources.into_boxed_slice()),
                    ..*scene
                }
            })
        })
        .collect()
}

/// Every scene's name, in run order (the order of a plain run, of each shard and of the serial
/// run).
pub(super) const ORDER: &[&str] = &[
    "smoke",
    "tokens",
    "window",
    "marks",
    "sidebar-marks",
    "cached-panes",
    "chrome-breath-skips-parts",
    "chrome-notify-redraws-parts",
    "chrome-pane-churn",
    "chrome-kill-switch",
    "chrome-pixels-same",
    "chrome-ages-current",
    "chrome-working-marks",
    "chrome-folder-polled",
    "chrome-followed",
    "chrome-title-once",
    "chrome-rows-once",
    "chrome-status-elsewhere",
    "chrome-model-edits",
    "chrome-notifications",
    "chrome-places-programs-ports",
    "chrome-globals",
    "chrome-resize-title",
    "chrome-drag-cancel",
    "chrome-renaming",
    "chrome-new-bar-reveal",
    "chrome-session-facts",
    "chrome-bars-and-edges",
    "chrome-sidebar-scroll",
    "chrome-sidebar-header-mark",
    "chrome-row-sources",
    "chrome-file-tab-toolbar",
    "sidebar",
    "sidebar-rows",
    "sidebar-selection",
    "sidebar-button",
    "title-area",
    "title-bar",
    "tabs",
    "splits",
    "cards",
    "cards-split",
    "cards-corners",
    "workspaces",
    "tab-bars",
    "tab-drag",
    "browser-tab",
    "file-tab",
    "file-diff",
    "file-tab-md",
    "file-tab-md-mermaid",
    "file-tab-scrollbar",
    "right-sidebar",
    "files",
    "files-follow",
    "right-panels",
    "changes",
    "changes-bar",
    "changes-staging",
    "changes-graph",
    "changes-sticky",
    "activity",
    "api-background",
    "banner-click",
    "default-name",
    "names",
    "renames",
    "rename-box",
    "palette",
    "palette-workspaces",
    "palette-actions",
    "palette-keys",
    "palette-run",
    "palette-refresh",
    "palette-recent",
    "palette-files",
    "palette-relist",
    "palette-settings",
    "palette-narrow",
    "palette-motion",
    "branch-switcher",
    "branch-switcher-confirm",
    "branch-switcher-real",
    "branch-switcher-open",
    "welcome",
    "welcome-checking",
    "welcome-keys",
    "welcome-narrow",
    "welcome-start",
    "welcome-folder",
    "welcome-first-run",
    "welcome-reopen",
    "overlays",
    "help",
    "panels",
    "preferences",
    "status-bar",
    "status-bar-hidden",
    "status-bar-ports",
    "status-bar-narrow",
    "update-button",
    "cursor",
    "cursor-flood",
    "command-blocks",
    "command-blocks-alt-screen",
    "command-blocks-narrow",
    "command-blocks-rprompt",
    "command-blocks-small-font",
    "command-blocks-off",
    "command-blocks-jump",
    "command-blocks-copy",
    "command-blocks-rerun",
];

/// Every scene, in `ORDER`. Panics, naming the scene, if one is in no area or in two, or if
/// `ORDER` misses or repeats one.
pub(super) fn scenes() -> Vec<&'static Scene> {
    let all: Vec<&'static Scene> = AREAS.iter().flat_map(|(_, area)| area.iter()).collect();
    for scene in &all {
        let declared = all.iter().filter(|s| s.name == scene.name).count();
        assert!(
            declared == 1,
            "ui-shots: scene {} is declared {declared} times",
            scene.name
        );
        let listed = ORDER.iter().filter(|&&n| n == scene.name).count();
        assert!(
            listed == 1,
            "ui-shots: scene {} is in ORDER {listed} times",
            scene.name
        );
    }
    ORDER
        .iter()
        .map(|&name| {
            *all.iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("ui-shots: ORDER names {name}, which no area declares"))
        })
        .collect()
}
