//! Provider logos (chores after M11ab, the lead designer's spec B-A): the Claude and Antigravity
//! marks, drawn in their own colours with `img()` (the Lucide icons are tinted with `svg()`). See
//! `assets/logos/NOTICE.md` for where they come from.
//!
//! GPUI rasterises an SVG image at twice the file's own width and height, so each size drawn gets
//! its own copy of the file sized to it (`sized_svg`), made once (`image`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, Image, ImageFormat, RenderImage};

pub const CLAUDE_SVG: &str = include_str!("../assets/logos/claude.svg");
pub const ANTIGRAVITY_SVG: &str = include_str!("../assets/logos/antigravity.svg");

/// A provider's logo (`ProviderInfo::logo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Logo {
    Claude,
    Antigravity,
}

impl Logo {
    fn svg(self) -> &'static str {
        match self {
            Logo::Claude => CLAUDE_SVG,
            Logo::Antigravity => ANTIGRAVITY_SVG,
        }
    }
}

/// `svg` with its root `width` and `height` set to `size` (a viewBox made from its own size if
/// it has none, so the drawing scales rather than crops).
pub fn sized_svg(svg: &str, size: f32) -> String {
    let Some(start) = svg.find("<svg") else {
        return svg.to_string();
    };
    let Some(end) = svg[start..].find('>').map(|e| start + e) else {
        return svg.to_string();
    };
    let attrs = &svg[start + 4..end];
    let value = |name: &str| {
        let key = format!(" {name}=\"");
        let at = attrs.find(&key)? + key.len();
        let len = attrs[at..].find('"')?;
        Some(attrs[at..at + len].to_string())
    };
    let mut kept = attrs.to_string();
    for name in ["width", "height"] {
        if let Some(v) = value(name) {
            kept = kept.replacen(&format!(" {name}=\"{v}\""), "", 1);
        }
    }
    let view_box = match (attrs.contains("viewBox="), value("width"), value("height")) {
        (false, Some(w), Some(h)) => format!(" viewBox=\"0 0 {w} {h}\""),
        _ => String::new(),
    };
    format!(
        "{}<svg width=\"{size}\" height=\"{size}\"{view_box}{kept}{}",
        &svg[..start],
        &svg[end..]
    )
}

/// `logo` sized `size` points, ready to draw (`img`): rasterised now, once per logo and size,
/// so the first frame that shows it has it (an `Image` would be decoded on a task, and a cached
/// part like the tab bar wouldn't be drawn again when it was ready). `None` if it can't be read.
pub fn image(logo: Logo, size: f32, cx: &App) -> Option<Arc<RenderImage>> {
    cached((logo, (size * 100.0).round() as u32), || {
        Image::from_bytes(ImageFormat::Svg, sized_svg(logo.svg(), size).into_bytes())
            .to_image_data(cx.svg_renderer())
            .ok()
    })
}

/// A logo at a size (hundredths of a point), and what was made of it.
type Made = HashMap<(Logo, u32), Option<Arc<RenderImage>>>;

thread_local! {
    /// The logos made so far (the UI thread's).
    static MADE: RefCell<Made> = RefCell::new(HashMap::new());
}

/// The image made for `key`, made by `make` the first time.
fn cached(
    key: (Logo, u32),
    make: impl FnOnce() -> Option<Arc<RenderImage>>,
) -> Option<Arc<RenderImage>> {
    if let Some(made) = MADE.with(|m| m.borrow().get(&key).cloned()) {
        return made;
    }
    let made = make();
    MADE.with(|m| m.borrow_mut().insert(key, made.clone()));
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sized_logo_keeps_its_drawing_and_takes_the_size() {
        let claude = sized_svg(CLAUDE_SVG, 14.0);
        assert!(
            claude.starts_with("<svg width=\"14\" height=\"14\" "),
            "{}",
            &claude[..80]
        );
        assert!(claude.contains("viewBox=\"0 0 24 24\""));
        // Antigravity's file has a size and no viewBox: its 96 × 96 becomes the viewBox.
        let agy = sized_svg(ANTIGRAVITY_SVG, 18.0);
        assert!(
            agy.starts_with("<svg width=\"18\" height=\"18\" "),
            "{}",
            &agy[..80]
        );
        assert!(agy.contains("viewBox=\"0 0 96 96\""));
        assert_eq!(
            agy.matches("width=\"96\"").count(),
            0,
            "its own size is replaced"
        );
    }

    #[test]
    fn each_logo_and_size_is_made_once() {
        let mut made = 0;
        for _ in 0..3 {
            cached((Logo::Antigravity, 1234), || {
                made += 1;
                None
            });
        }
        assert_eq!(made, 1);
    }
}
