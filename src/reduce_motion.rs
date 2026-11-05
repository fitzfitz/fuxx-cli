//! macOS "Reduce motion" (System Settings › Accessibility › Display) and the one switch for all
//! of fuxx's motion.
//!
//! GPUI's `with_animation` draws still when `App::reduce_motion` is set, and the status marks
//! read the same flag, but nothing in GPUI reads the macOS setting. `watch` sets that flag from
//! the `animations` setting and the macOS one, read at start and again every 5 s (a cheap read
//! with no redraw unless it changed).

use std::time::Duration;

use gpui::{App, Global};
use objc2_app_kit::NSWorkspace;

use crate::appearance::Appearance;

/// How often the macOS setting is read again.
const RECHECK: Duration = Duration::from_secs(5);

/// Whether macOS asks apps to reduce motion.
pub fn system_reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// The macOS setting as last read; the Workspace redraws (and re-checks its ticker) when it
/// changes.
pub struct SystemReduceMotion(pub bool);

impl Global for SystemReduceMotion {}

/// Read the macOS setting now and every `RECHECK`, and keep `App::reduce_motion` in step with it
/// and with the `animations` setting (re-read when the settings are). Call once, after the
/// `Appearance` global is set.
pub fn watch(cx: &mut App) {
    cx.set_global(SystemReduceMotion(system_reduce_motion()));
    apply(cx);
    cx.observe_global::<Appearance>(apply).detach();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(RECHECK).await;
            let now = system_reduce_motion();
            cx.update(|cx| {
                if cx.global::<SystemReduceMotion>().0 != now {
                    cx.set_global(SystemReduceMotion(now));
                    apply(cx);
                }
            });
        }
    })
    .detach();
}

/// Motion is off when the `animations` setting is off or macOS asks for reduced motion.
fn apply(cx: &mut App) {
    let off = !cx.global::<Appearance>().animations || cx.global::<SystemReduceMotion>().0;
    cx.set_reduce_motion(off);
}
