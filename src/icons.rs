//! The app's icon set (chore 2026-11-20: "find icon with outline no backgrounds… we need to have
//! icon sets"): Lucide's outline icons (ISC, `assets/icons/lucide/LICENSE`, lucide-static 0.460.0),
//! embedded in the binary and served to GPUI as its asset source, so `svg()` can draw them, tinted
//! with the element's text colour. Each icon is a 24-unit square drawn with a 2-unit round stroke.
//!
//! `Icon` names the ones in use; `Assets` is the asset source (`application().with_assets`, and
//! the screenshot harness's).

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, Pixels, SharedString, Svg, prelude::*, svg};

/// An icon of the set (`tab_bar::Glyph` names the same set, for the bars' older call sites).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Terminal,
    Globe,
    File,
    Plus,
    Ellipsis,
    PanelRight,
    PanelLeft,
    Folder,
    FolderOpen,
    GitCompare,
    Activity,
    Back,
    Forward,
    Reload,
    Stop,
    Search,
    Bell,
    Gear,
    Package,
    CollapseAll,
    Branch,
    Pull,
    Push,
    Publish,
    Revert,
    ChevronRight,
    ChevronDown,
    Wrap,
    OpenExternal,
    ChangeUp,
    ChangeDown,
    SideBySide,
    Copy,
    Check,
    Outline,
    Info,
    Bulb,
    Report,
    Alert,
    Octagon,
    Eye,
    EyeOff,
    /// The agent composer (the designer's details): the send arrow, the modes' icons and the
    /// model menu's Usage row.
    ArrowUp,
    Shield,
    FilePen,
    ListChecks,
    Zap,
    Gauge,
    /// The agent chat (spec B-B): a todo's pending and active marks, an attachment chip.
    Circle,
    CircleDot,
    Paperclip,
}

impl Icon {
    #[cfg(test)]
    pub const ALL: [Icon; 51] = [
        Icon::Terminal,
        Icon::Globe,
        Icon::File,
        Icon::Plus,
        Icon::Ellipsis,
        Icon::PanelRight,
        Icon::PanelLeft,
        Icon::Folder,
        Icon::FolderOpen,
        Icon::GitCompare,
        Icon::Activity,
        Icon::Back,
        Icon::Forward,
        Icon::Reload,
        Icon::Stop,
        Icon::Search,
        Icon::Bell,
        Icon::Gear,
        Icon::Package,
        Icon::CollapseAll,
        Icon::Branch,
        Icon::Pull,
        Icon::Push,
        Icon::Publish,
        Icon::Revert,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::Wrap,
        Icon::OpenExternal,
        Icon::ChangeUp,
        Icon::ChangeDown,
        Icon::SideBySide,
        Icon::Copy,
        Icon::Check,
        Icon::Outline,
        Icon::Info,
        Icon::Bulb,
        Icon::Report,
        Icon::Alert,
        Icon::Octagon,
        Icon::Eye,
        Icon::EyeOff,
        Icon::ArrowUp,
        Icon::Shield,
        Icon::FilePen,
        Icon::ListChecks,
        Icon::Zap,
        Icon::Gauge,
        Icon::Circle,
        Icon::CircleDot,
        Icon::Paperclip,
    ];

    /// Its asset path.
    pub fn path(self) -> &'static str {
        match self {
            Icon::Terminal => "icons/lucide/terminal.svg",
            Icon::Globe => "icons/lucide/globe.svg",
            Icon::File => "icons/lucide/file.svg",
            Icon::Plus => "icons/lucide/plus.svg",
            Icon::Ellipsis => "icons/lucide/ellipsis.svg",
            Icon::PanelRight => "icons/lucide/panel-right.svg",
            Icon::PanelLeft => "icons/lucide/panel-left.svg",
            Icon::Folder => "icons/lucide/folder.svg",
            Icon::FolderOpen => "icons/lucide/folder-open.svg",
            Icon::GitCompare => "icons/lucide/git-compare.svg",
            Icon::Activity => "icons/lucide/activity.svg",
            Icon::Back => "icons/lucide/chevron-left.svg",
            Icon::Forward => "icons/lucide/chevron-right.svg",
            Icon::Reload => "icons/lucide/rotate-cw.svg",
            Icon::Stop => "icons/lucide/x.svg",
            Icon::Search => "icons/lucide/search.svg",
            Icon::Bell => "icons/lucide/bell.svg",
            Icon::Gear => "icons/lucide/settings.svg",
            Icon::Package => "icons/lucide/package.svg",
            Icon::CollapseAll => "icons/lucide/chevrons-down-up.svg",
            Icon::Branch => "icons/lucide/git-branch.svg",
            Icon::Pull => "icons/lucide/arrow-down-to-line.svg",
            Icon::Push => "icons/lucide/arrow-up-from-line.svg",
            Icon::Publish => "icons/lucide/cloud-upload.svg",
            Icon::Revert => "icons/lucide/undo-2.svg",
            Icon::ChevronRight => "icons/lucide/chevron-right.svg",
            Icon::ChevronDown => "icons/lucide/chevron-down.svg",
            Icon::Wrap => "icons/lucide/wrap-text.svg",
            Icon::OpenExternal => "icons/lucide/external-link.svg",
            Icon::ChangeUp => "icons/lucide/arrow-up.svg",
            Icon::ChangeDown => "icons/lucide/arrow-down.svg",
            Icon::SideBySide => "icons/lucide/columns-2.svg",
            Icon::Copy => "icons/lucide/copy.svg",
            Icon::Check => "icons/lucide/check.svg",
            Icon::Outline => "icons/lucide/table-of-contents.svg",
            Icon::Info => "icons/lucide/info.svg",
            Icon::Bulb => "icons/lucide/lightbulb.svg",
            Icon::Report => "icons/lucide/message-square-warning.svg",
            Icon::Alert => "icons/lucide/triangle-alert.svg",
            Icon::Octagon => "icons/lucide/octagon-alert.svg",
            Icon::Eye => "icons/lucide/eye.svg",
            Icon::EyeOff => "icons/lucide/eye-off.svg",
            Icon::ArrowUp => "icons/lucide/arrow-up.svg",
            Icon::Shield => "icons/lucide/shield.svg",
            Icon::FilePen => "icons/lucide/file-pen.svg",
            Icon::ListChecks => "icons/lucide/list-checks.svg",
            Icon::Zap => "icons/lucide/zap.svg",
            Icon::Gauge => "icons/lucide/gauge.svg",
            Icon::Circle => "icons/lucide/circle.svg",
            Icon::CircleDot => "icons/lucide/circle-dot.svg",
            Icon::Paperclip => "icons/lucide/paperclip.svg",
        }
    }
}

/// The embedded files, by asset path.
const FILES: [(&str, &[u8]); 49] = [
    (
        "icons/lucide/circle.svg",
        include_bytes!("../assets/icons/lucide/circle.svg"),
    ),
    (
        "icons/lucide/circle-dot.svg",
        include_bytes!("../assets/icons/lucide/circle-dot.svg"),
    ),
    (
        "icons/lucide/paperclip.svg",
        include_bytes!("../assets/icons/lucide/paperclip.svg"),
    ),
    (
        "icons/lucide/shield.svg",
        include_bytes!("../assets/icons/lucide/shield.svg"),
    ),
    (
        "icons/lucide/file-pen.svg",
        include_bytes!("../assets/icons/lucide/file-pen.svg"),
    ),
    (
        "icons/lucide/list-checks.svg",
        include_bytes!("../assets/icons/lucide/list-checks.svg"),
    ),
    (
        "icons/lucide/zap.svg",
        include_bytes!("../assets/icons/lucide/zap.svg"),
    ),
    (
        "icons/lucide/gauge.svg",
        include_bytes!("../assets/icons/lucide/gauge.svg"),
    ),
    (
        "icons/lucide/activity.svg",
        include_bytes!("../assets/icons/lucide/activity.svg"),
    ),
    (
        "icons/lucide/arrow-down.svg",
        include_bytes!("../assets/icons/lucide/arrow-down.svg"),
    ),
    (
        "icons/lucide/arrow-down-to-line.svg",
        include_bytes!("../assets/icons/lucide/arrow-down-to-line.svg"),
    ),
    (
        "icons/lucide/arrow-up.svg",
        include_bytes!("../assets/icons/lucide/arrow-up.svg"),
    ),
    (
        "icons/lucide/arrow-up-from-line.svg",
        include_bytes!("../assets/icons/lucide/arrow-up-from-line.svg"),
    ),
    (
        "icons/lucide/bell.svg",
        include_bytes!("../assets/icons/lucide/bell.svg"),
    ),
    (
        "icons/lucide/check.svg",
        include_bytes!("../assets/icons/lucide/check.svg"),
    ),
    (
        "icons/lucide/chevron-down.svg",
        include_bytes!("../assets/icons/lucide/chevron-down.svg"),
    ),
    (
        "icons/lucide/chevron-left.svg",
        include_bytes!("../assets/icons/lucide/chevron-left.svg"),
    ),
    (
        "icons/lucide/chevron-right.svg",
        include_bytes!("../assets/icons/lucide/chevron-right.svg"),
    ),
    (
        "icons/lucide/chevrons-down-up.svg",
        include_bytes!("../assets/icons/lucide/chevrons-down-up.svg"),
    ),
    (
        "icons/lucide/cloud-upload.svg",
        include_bytes!("../assets/icons/lucide/cloud-upload.svg"),
    ),
    (
        "icons/lucide/columns-2.svg",
        include_bytes!("../assets/icons/lucide/columns-2.svg"),
    ),
    (
        "icons/lucide/copy.svg",
        include_bytes!("../assets/icons/lucide/copy.svg"),
    ),
    (
        "icons/lucide/ellipsis.svg",
        include_bytes!("../assets/icons/lucide/ellipsis.svg"),
    ),
    (
        "icons/lucide/external-link.svg",
        include_bytes!("../assets/icons/lucide/external-link.svg"),
    ),
    (
        "icons/lucide/eye.svg",
        include_bytes!("../assets/icons/lucide/eye.svg"),
    ),
    (
        "icons/lucide/eye-off.svg",
        include_bytes!("../assets/icons/lucide/eye-off.svg"),
    ),
    (
        "icons/lucide/file.svg",
        include_bytes!("../assets/icons/lucide/file.svg"),
    ),
    (
        "icons/lucide/folder.svg",
        include_bytes!("../assets/icons/lucide/folder.svg"),
    ),
    (
        "icons/lucide/folder-open.svg",
        include_bytes!("../assets/icons/lucide/folder-open.svg"),
    ),
    (
        "icons/lucide/git-branch.svg",
        include_bytes!("../assets/icons/lucide/git-branch.svg"),
    ),
    (
        "icons/lucide/git-compare.svg",
        include_bytes!("../assets/icons/lucide/git-compare.svg"),
    ),
    (
        "icons/lucide/globe.svg",
        include_bytes!("../assets/icons/lucide/globe.svg"),
    ),
    (
        "icons/lucide/info.svg",
        include_bytes!("../assets/icons/lucide/info.svg"),
    ),
    (
        "icons/lucide/lightbulb.svg",
        include_bytes!("../assets/icons/lucide/lightbulb.svg"),
    ),
    (
        "icons/lucide/message-square-warning.svg",
        include_bytes!("../assets/icons/lucide/message-square-warning.svg"),
    ),
    (
        "icons/lucide/octagon-alert.svg",
        include_bytes!("../assets/icons/lucide/octagon-alert.svg"),
    ),
    (
        "icons/lucide/package.svg",
        include_bytes!("../assets/icons/lucide/package.svg"),
    ),
    (
        "icons/lucide/panel-left.svg",
        include_bytes!("../assets/icons/lucide/panel-left.svg"),
    ),
    (
        "icons/lucide/panel-right.svg",
        include_bytes!("../assets/icons/lucide/panel-right.svg"),
    ),
    (
        "icons/lucide/plus.svg",
        include_bytes!("../assets/icons/lucide/plus.svg"),
    ),
    (
        "icons/lucide/rotate-cw.svg",
        include_bytes!("../assets/icons/lucide/rotate-cw.svg"),
    ),
    (
        "icons/lucide/search.svg",
        include_bytes!("../assets/icons/lucide/search.svg"),
    ),
    (
        "icons/lucide/settings.svg",
        include_bytes!("../assets/icons/lucide/settings.svg"),
    ),
    (
        "icons/lucide/table-of-contents.svg",
        include_bytes!("../assets/icons/lucide/table-of-contents.svg"),
    ),
    (
        "icons/lucide/terminal.svg",
        include_bytes!("../assets/icons/lucide/terminal.svg"),
    ),
    (
        "icons/lucide/triangle-alert.svg",
        include_bytes!("../assets/icons/lucide/triangle-alert.svg"),
    ),
    (
        "icons/lucide/undo-2.svg",
        include_bytes!("../assets/icons/lucide/undo-2.svg"),
    ),
    (
        "icons/lucide/wrap-text.svg",
        include_bytes!("../assets/icons/lucide/wrap-text.svg"),
    ),
    (
        "icons/lucide/x.svg",
        include_bytes!("../assets/icons/lucide/x.svg"),
    ),
];

/// The app's asset source: the embedded icons.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(FILES
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(FILES
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}

/// `icon` drawn `size` square in `color`.
pub fn icon(icon: Icon, size: Pixels, color: Hsla) -> Svg {
    svg()
        .path(icon.path())
        .flex_none()
        .size(size)
        .text_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_from_the_embedded_assets_as_an_outline_svg() {
        for i in Icon::ALL {
            let bytes = Assets.load(i.path()).expect("loads").expect("present");
            let text = std::str::from_utf8(&bytes).expect("text");
            assert!(text.contains("<svg"), "{i:?}: an svg");
            // Outline: no fill, stroked in the colour it's drawn in.
            assert!(text.contains(r#"fill="none""#), "{i:?}: no fill");
            assert!(text.contains(r#"stroke="currentColor""#), "{i:?}: stroked");
        }
    }

    #[test]
    fn an_unknown_path_is_none_and_the_list_has_every_icon() {
        assert!(Assets.load("icons/lucide/nope.svg").unwrap().is_none());
        assert_eq!(Assets.list("icons/").unwrap().len(), FILES.len());
        // Every file is some icon's.
        for (path, _) in FILES {
            assert!(Icon::ALL.iter().any(|i| i.path() == path), "{path}: unused");
        }
    }
}
