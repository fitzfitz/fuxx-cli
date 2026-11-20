//! The app's icon set (chore 2026-11-20: "find icon with outline no backgrounds… we need to have
//! icon sets"): Lucide's outline icons (ISC, `assets/icons/lucide/LICENSE`, lucide-static 0.460.0),
//! embedded in the binary and served to GPUI as its asset source, so `svg()` can draw them, tinted
//! with the element's text colour. Each icon is a 24-unit square drawn with a 2-unit round stroke.
//!
//! `Icon` names the ones in use; `Assets` is the asset source (`application().with_assets`, and
//! the screenshot harness's).

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, Pixels, SharedString, Svg, prelude::*, svg};

/// An icon of the set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Terminal,
    Globe,
    File,
    Plus,
    Ellipsis,
    PanelRight,
    Folder,
    GitCompare,
    Activity,
}

impl Icon {
    #[cfg(test)]
    pub const ALL: [Icon; 9] = [
        Icon::Terminal,
        Icon::Globe,
        Icon::File,
        Icon::Plus,
        Icon::Ellipsis,
        Icon::PanelRight,
        Icon::Folder,
        Icon::GitCompare,
        Icon::Activity,
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
            Icon::Folder => "icons/lucide/folder.svg",
            Icon::GitCompare => "icons/lucide/git-compare.svg",
            Icon::Activity => "icons/lucide/activity.svg",
        }
    }
}

/// The embedded files, by asset path.
const FILES: [(&str, &[u8]); 9] = [
    (
        "icons/lucide/terminal.svg",
        include_bytes!("../assets/icons/lucide/terminal.svg"),
    ),
    (
        "icons/lucide/globe.svg",
        include_bytes!("../assets/icons/lucide/globe.svg"),
    ),
    (
        "icons/lucide/file.svg",
        include_bytes!("../assets/icons/lucide/file.svg"),
    ),
    (
        "icons/lucide/plus.svg",
        include_bytes!("../assets/icons/lucide/plus.svg"),
    ),
    (
        "icons/lucide/ellipsis.svg",
        include_bytes!("../assets/icons/lucide/ellipsis.svg"),
    ),
    (
        "icons/lucide/panel-right.svg",
        include_bytes!("../assets/icons/lucide/panel-right.svg"),
    ),
    (
        "icons/lucide/folder.svg",
        include_bytes!("../assets/icons/lucide/folder.svg"),
    ),
    (
        "icons/lucide/git-compare.svg",
        include_bytes!("../assets/icons/lucide/git-compare.svg"),
    ),
    (
        "icons/lucide/activity.svg",
        include_bytes!("../assets/icons/lucide/activity.svg"),
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
        assert_eq!(Assets.list("icons/").unwrap().len(), Icon::ALL.len());
    }
}
