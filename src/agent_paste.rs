//! ⌘V in the agent tab's box (chore after M11ar, 2026-11-24): what the clipboard holds and what
//! the tab makes of it. Files copied in Finder come in as a drop does (image pills, path pills);
//! a TIFF or BMP image is made PNG first; anything fuxx can't use leaves a notice row, never
//! silence. Pure parts first; the tab's side after them.

use std::path::PathBuf;

use gpui::{ClipboardEntry, ClipboardItem, Context, ImageFormat};

/// The notice for a ⌘V that found nothing the box can take.
pub(crate) const NOTHING_TO_PASTE: &str =
    "Nothing to paste: the clipboard has no text, image or file fuxx can use.";

/// The notice for a clipboard image that couldn't be made PNG.
pub(crate) const UNREADABLE_IMAGE: &str = "Couldn't read that image from the clipboard.";

/// What ⌘V in the box found on the clipboard, in the order it is looked for: files (Finder's
/// copy also carries their names as text, which they win over), text, an image, nothing.
#[derive(Debug, PartialEq)]
pub(crate) enum Pasted {
    Paths(Vec<PathBuf>),
    Text(String),
    Image(gpui::Image),
    Nothing,
}

/// What the clipboard's `item` pastes as (`Pasted`).
pub(crate) fn pasted(item: Option<ClipboardItem>) -> Pasted {
    let (mut paths, mut text, mut image) = (Vec::new(), String::new(), None);
    for entry in item.into_iter().flat_map(ClipboardItem::into_entries) {
        match entry {
            ClipboardEntry::ExternalPaths(p) => paths.extend_from_slice(p.paths()),
            ClipboardEntry::String(s) => text.push_str(s.text()),
            ClipboardEntry::Image(i) => {
                image.get_or_insert(i);
            }
        }
    }
    if !paths.is_empty() {
        Pasted::Paths(paths)
    } else if !text.is_empty() {
        Pasted::Text(text)
    } else {
        image.map_or(Pasted::Nothing, Pasted::Image)
    }
}

/// What the box does with a pasted image of a format: take it as it is (its media type), make
/// it PNG first, or refuse it (the format's name, for the notice).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ImageTake {
    As(&'static str),
    ToPng,
    Refuse(&'static str),
}

/// What the box does with a pasted image of `format` (`ImageTake`): the agents take PNG, JPEG,
/// GIF and WebP; TIFF and BMP are made PNG; SVG, ICO and PNM are refused.
pub(crate) fn image_take(format: ImageFormat) -> ImageTake {
    match format {
        ImageFormat::Png => ImageTake::As("image/png"),
        ImageFormat::Jpeg => ImageTake::As("image/jpeg"),
        ImageFormat::Gif => ImageTake::As("image/gif"),
        ImageFormat::Webp => ImageTake::As("image/webp"),
        ImageFormat::Tiff | ImageFormat::Bmp => ImageTake::ToPng,
        ImageFormat::Svg => ImageTake::Refuse("SVG"),
        ImageFormat::Ico => ImageTake::Refuse("ICO"),
        ImageFormat::Pnm => ImageTake::Refuse("PNM"),
    }
}

/// The notice for a pasted image in a format fuxx can't take (`format`: its name).
pub(crate) fn cant_paste(format: &str) -> String {
    format!("fuxx can't paste {format} images. Try PNG or JPEG.")
}

/// A TIFF or BMP image's `bytes` as PNG, through macOS's own image reader (`NSBitmapImageRep`,
/// as the agent browser's screenshots are encoded), or the notice saying it couldn't be read.
/// Decodes and encodes an image: call it off the UI thread.
pub(crate) fn to_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let data = objc2_foundation::NSData::with_bytes(bytes);
    crate::browser::png_from_data(&data).map_err(|_| UNREADABLE_IMAGE.to_string())
}

/// A 2 × 1 BMP: bottom-up rows of BGR pixels, each row padded to 4 bytes.
#[cfg(test)]
fn bmp_2x1() -> Vec<u8> {
    let pixels: [u8; 8] = [0, 0, 255, 0, 255, 0, 0, 0];
    let mut b = Vec::new();
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes()); // the info header's size
    b.extend_from_slice(&2i32.to_le_bytes()); // width
    b.extend_from_slice(&1i32.to_le_bytes()); // height
    b.extend_from_slice(&1u16.to_le_bytes()); // planes
    b.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    b.extend_from_slice(&[0; 4]); // no compression
    b.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0; 16]); // resolution, palette
    b.extend_from_slice(&pixels);
    b
}

/// A 3 × 2 TIFF: little-endian, one strip of 8-bit grey, uncompressed.
#[cfg(any(test, feature = "ui-shots"))]
pub(crate) fn tiff_3x2() -> Vec<u8> {
    let pixels: [u8; 6] = [0, 128, 255, 255, 128, 0];
    let entries: [(u16, u16, u32); 9] = [
        (256, 3, 3), // width
        (257, 3, 2), // height
        (258, 3, 8), // bits per sample
        (259, 3, 1), // no compression
        (262, 3, 1), // black is zero
        (273, 4, 0), // the strip's offset (set below)
        (277, 3, 1), // samples per pixel
        (278, 3, 2), // rows per strip
        (279, 4, 6), // the strip's length
    ];
    let ifd_len = 2 + entries.len() * 12 + 4;
    let strip = (8 + ifd_len) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"II*\0");
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, value) in entries {
        let value = if tag == 273 { strip } else { value };
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        if kind == 3 {
            b.extend_from_slice(&(value as u16).to_le_bytes());
            b.extend_from_slice(&[0; 2]);
        } else {
            b.extend_from_slice(&value.to_le_bytes());
        }
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&pixels);
    b
}

impl super::AgentTab {
    /// Files on the clipboard (copied in Finder), pasted: as a drop on the box, at the caret.
    pub(super) fn paste_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.drop_external(paths, true, cx);
    }

    /// An image pasted from the clipboard, as an image pill (`add_image`): one the agents take as
    /// it is, one made PNG first (off the UI thread, with the rest of its work), or a notice
    /// naming a format fuxx can't take.
    pub fn attach_pasted_image(&mut self, image: gpui::Image, cx: &mut Context<Self>) {
        let from = match image_take(image.format) {
            ImageTake::As(media_type) => (media_type, super::ImageFrom::Clipboard(image.bytes)),
            ImageTake::ToPng => ("image/png", super::ImageFrom::ClipboardToPng(image.bytes)),
            ImageTake::Refuse(format) => return self.notice(cant_paste(format), cx),
        };
        self.pasted += 1;
        let name = super::pasted_name(self.pasted);
        self.add_image(name, from.0, from.1, true, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardString, ExternalPaths};

    fn item(entries: Vec<ClipboardEntry>) -> Option<ClipboardItem> {
        Some(ClipboardItem { entries })
    }

    fn text(s: &str) -> ClipboardEntry {
        ClipboardEntry::String(ClipboardString::new(s.to_string()))
    }

    fn image(format: ImageFormat) -> gpui::Image {
        gpui::Image::from_bytes(format, b"made-up".to_vec())
    }

    #[test]
    fn files_win_over_the_names_finder_puts_beside_them() {
        let paths = vec![
            PathBuf::from("/tmp/shot.png"),
            PathBuf::from("/tmp/notes.md"),
        ];
        let finder = item(vec![
            ClipboardEntry::ExternalPaths(ExternalPaths(paths.clone().into())),
            text("shot.png\rnotes.md"),
        ]);
        assert_eq!(pasted(finder), Pasted::Paths(paths));
    }

    #[test]
    fn text_wins_over_an_image_and_is_joined() {
        let both = item(vec![
            text("Fix "),
            ClipboardEntry::Image(image(ImageFormat::Png)),
            text("this"),
        ]);
        assert_eq!(pasted(both), Pasted::Text("Fix this".into()));
    }

    #[test]
    fn an_image_alone_is_the_first_image() {
        let two = item(vec![
            ClipboardEntry::Image(image(ImageFormat::Tiff)),
            ClipboardEntry::Image(image(ImageFormat::Png)),
        ]);
        assert_eq!(pasted(two), Pasted::Image(image(ImageFormat::Tiff)));
    }

    #[test]
    fn an_empty_clipboard_or_empty_parts_paste_nothing() {
        assert_eq!(pasted(None), Pasted::Nothing);
        assert_eq!(pasted(item(Vec::new())), Pasted::Nothing);
        let empty = item(vec![
            text(""),
            ClipboardEntry::ExternalPaths(ExternalPaths(Default::default())),
        ]);
        assert_eq!(pasted(empty), Pasted::Nothing);
    }

    #[test]
    fn the_agents_formats_are_taken_tiff_and_bmp_made_png_the_rest_refused() {
        assert_eq!(image_take(ImageFormat::Png), ImageTake::As("image/png"));
        assert_eq!(image_take(ImageFormat::Jpeg), ImageTake::As("image/jpeg"));
        assert_eq!(image_take(ImageFormat::Gif), ImageTake::As("image/gif"));
        assert_eq!(image_take(ImageFormat::Webp), ImageTake::As("image/webp"));
        assert_eq!(image_take(ImageFormat::Tiff), ImageTake::ToPng);
        assert_eq!(image_take(ImageFormat::Bmp), ImageTake::ToPng);
        assert_eq!(image_take(ImageFormat::Svg), ImageTake::Refuse("SVG"));
        assert_eq!(image_take(ImageFormat::Ico), ImageTake::Refuse("ICO"));
        assert_eq!(image_take(ImageFormat::Pnm), ImageTake::Refuse("PNM"));
    }

    #[test]
    fn the_notices_say_what_happened_in_plain_words() {
        assert_eq!(
            NOTHING_TO_PASTE,
            "Nothing to paste: the clipboard has no text, image or file fuxx can use."
        );
        assert_eq!(
            cant_paste("SVG"),
            "fuxx can't paste SVG images. Try PNG or JPEG."
        );
        assert_eq!(
            UNREADABLE_IMAGE,
            "Couldn't read that image from the clipboard."
        );
    }

    #[test]
    fn tiff_and_bmp_are_made_png_of_the_same_size() {
        use crate::browser::png_size;
        let png = to_png(&tiff_3x2()).expect("the TIFF read");
        assert_eq!(png_size(&png), Some((3, 2)), "a PNG of the TIFF's size");
        let png = to_png(&bmp_2x1()).expect("the BMP read");
        assert_eq!(png_size(&png), Some((2, 1)), "a PNG of the BMP's size");
    }

    #[test]
    fn bytes_that_are_no_image_leave_the_notice() {
        assert_eq!(
            to_png(b"not an image at all"),
            Err(UNREADABLE_IMAGE.to_string())
        );
        assert_eq!(to_png(&[]), Err(UNREADABLE_IMAGE.to_string()));
    }
}
