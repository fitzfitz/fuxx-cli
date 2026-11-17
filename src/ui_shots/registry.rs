//! The scene registry: every area's scenes, their run order, and the startup check that they
//! agree.
//!
//! Adding a scene:
//! 1. write its function in its area file (`window.rs`, `terminal.rs`, ...);
//! 2. add a `Scene` row to that file's `SCENES`: its `themes` (`Both` if it checks colours or a
//!    theme's look), `serial` (`true` only for one of the reasons on `Scene::serial`, named in a
//!    comment beside it), its `kind` (`Visual` if its checks read pixels) and its `sources` (the
//!    app files its views come from);
//! 3. add its name to `ORDER` below.
//!
//! A new area file also goes in `AREAS` (with its `FILE`) and as a `mod` in `mod.rs`; a test
//! fails until it does. An edit to this file runs no scene by itself; the area file is what
//! `--changed` follows.

use super::{
    Scene, agent, branch, changes, chrome, chrome_parts, file_tab, file_tab_md, naming, overlays,
    palette, panes, right_sidebar, sidebar, status_bar, terminal, welcome, window,
};

/// Files that run or declare the suite but change no scene's picture: an edit to one of these
/// alone runs nothing under `--changed` (they are neither shared nor unclaimed). `mod.rs`
/// (the runner) and `harness.rs` stay in `SHARED`.
pub(super) const RUNNER_ONLY: &[&str] = &[
    "src/ui_shots/registry.rs",
    "src/ui_shots/args.rs",
    "src/ui_shots/changed.rs",
];

/// Each area's file (its `FILE`, from `file!()`, so it can't name another area's file) and
/// scenes. A new area is added here (and as a `mod` in `mod.rs`); its file is claimed by its own
/// scenes (`claiming`).
pub(super) const AREAS: &[(&str, &[Scene])] = &[
    (window::FILE, window::SCENES),
    (sidebar::FILE, sidebar::SCENES),
    (panes::FILE, panes::SCENES),
    (chrome_parts::FILE, chrome_parts::SCENES),
    (chrome::FILE, chrome::SCENES),
    (file_tab::FILE, file_tab::SCENES),
    (file_tab_md::FILE, file_tab_md::SCENES),
    (right_sidebar::FILE, right_sidebar::SCENES),
    (changes::FILE, changes::SCENES),
    (branch::FILE, branch::SCENES),
    (naming::FILE, naming::SCENES),
    (palette::FILE, palette::SCENES),
    (welcome::FILE, welcome::SCENES),
    (overlays::FILE, overlays::SCENES),
    (status_bar::FILE, status_bar::SCENES),
    (terminal::FILE, terminal::SCENES),
    (agent::FILE, agent::SCENES),
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
    "chrome-agent-tabs",
    "chrome-agent-status",
    "chrome-agent-age",
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
    "file-live-glow",
    "file-live-marks",
    "file-live-none",
    "file-live-tooltip",
    "file-live-modes",
    "file-follow-pill",
    "file-follow-scrolls",
    "file-follow-wheel-stops",
    "file-follow-mode",
    "file-code-calm",
    "file-code-guides",
    "file-code-bars",
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
    "agent-tab-empty",
    "agent-tab-working",
    "agent-tab-permission",
    "agent-tab-done",
    "agent-tab-failed",
    "agent-tab-bad-resume",
    "agent-tab-long",
    "agent-tab-markdown",
    "agent-tab-cached-current",
    "agent-tab-at-files",
    "agent-tab-close",
    "agent-tab-keys",
    "agent-tab-review",
    "agent-tab-held",
    "agent-tab-runs",
    "agent-tab-agy",
    "agent-workspace-split",
    "agent-preset-menu",
    "agent-pane-menu",
    "agent-sidebar-waiting",
    "agent-tab-composer-wrap",
    "agent-tab-attachment",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `src/ui_shots/` files that hold no scenes: the runner and its parts.
    const NOT_AREAS: &[&str] = &["mod", "harness", "registry", "args", "changed"];

    #[test]
    fn each_area_file_is_repo_relative_and_declares_its_own_scenes() {
        // `file!()` is what git's paths are compared with (`--changed`): it must be the
        // repo-relative path, and the file must be the one that declares these scenes.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for (file, area) in AREAS {
            assert!(
                file.starts_with("src/ui_shots/") && file.ends_with(".rs"),
                "{file} is not a repo-relative src/ui_shots/ path"
            );
            let text = std::fs::read_to_string(root.join(file))
                .unwrap_or_else(|e| panic!("{file} does not read: {e}"));
            for scene in *area {
                assert!(
                    text.contains(&format!("name: \"{}\"", scene.name)),
                    "{file} does not declare scene {}",
                    scene.name
                );
            }
        }
    }

    #[test]
    fn every_area_file_in_src_ui_shots_is_in_areas() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui_shots");
        let mut missing: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("src/ui_shots/ reads") {
            let path = entry.expect("an entry").path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).expect("a name");
            if NOT_AREAS.contains(&stem) {
                continue;
            }
            let file = format!("src/ui_shots/{stem}.rs");
            if !AREAS.iter().any(|(f, _)| *f == file) {
                missing.push(file);
            }
        }
        missing.sort();
        assert_eq!(
            missing,
            Vec::<String>::new(),
            "area files missing from AREAS"
        );
    }
}
