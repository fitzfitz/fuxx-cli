//! fuxx's icon in the Dock when it isn't running from the app package (a `cargo run` build): macOS
//! takes an app's icon from its package, so a bare program shows a generic one unless it sets its
//! own. The app copy keeps the icon in its package (`packaging/macos/fuxx.icns`).

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

use crate::native_notify;

/// The icon as one picture (made by `scripts/make_app_icon.py`, on Apple's icon grid like the
/// package's icon, so it's the size of its Dock neighbours).
const PNG: &[u8] = include_bytes!("../assets/logo/3d-cyber/fuxx-app-icon-512.png");

/// Show fuxx's icon in the Dock, unless the app package already does. Call once the app has
/// started, on the main thread; anywhere else it does nothing.
pub fn show_in_dock() {
    if native_notify::is_bundled() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(icon) = NSImage::initWithData(mtm.alloc(), &NSData::with_bytes(PNG)) {
        // SAFETY: on the main thread (`mtm`), with a valid image that AppKit retains.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&icon)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dock_picture_is_the_512_px_icon() {
        // A PNG: its signature, then the IHDR chunk's width and height (big-endian) at bytes 16..24.
        assert_eq!(&PNG[..8], b"\x89PNG\r\n\x1a\n");
        let side = |at: usize| u32::from_be_bytes(PNG[at..at + 4].try_into().unwrap());
        assert_eq!((side(16), side(20)), (512, 512));
    }
}
