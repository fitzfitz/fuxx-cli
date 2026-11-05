//! What macOS has to say about fuxx's motion: the "Reduce motion" setting (System Settings ›
//! Accessibility › Display) and whether the main window can be seen at all.
//!
//! GPUI's `with_animation` draws still when `App::reduce_motion` is set, and the status marks
//! read the same flag, but nothing in GPUI reads the macOS setting. `watch` sets that flag from
//! the `animations` setting and the macOS one. GPUI also knows when its window is covered (it
//! stops drawing) but doesn't say; `watch_window` keeps `WindowVisible` for the marks' ticker.
//!
//! Both follow AppKit's change notifications, so nothing wakes fuxx while nothing changes.

use std::ptr::NonNull;

use block2::RcBlock;
use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{App, Global};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSApplication, NSWindow, NSWindowDidChangeOcclusionStateNotification, NSWindowOcclusionState,
    NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName};

use crate::appearance::Appearance;

/// Whether macOS asks apps to reduce motion.
pub fn system_reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// The macOS setting as last read; the Workspace redraws (and re-checks its ticker) when it
/// changes.
pub struct SystemReduceMotion {
    pub on: bool,
    _observer: Observer,
}

impl Global for SystemReduceMotion {}

/// Whether any part of the main window can be seen (not hidden with the app, minimised, on
/// another Space or covered by other windows); the Workspace redraws when it changes.
pub struct WindowVisible {
    pub visible: bool,
    _observer: Observer,
}

impl Global for WindowVisible {}

/// A registration with a notification center. The center keeps it for the rest of the run; this
/// is the handle it gave back.
type Observer = Retained<ProtocolObject<dyn NSObjectProtocol>>;

/// Read the macOS setting now and on each change, and keep `App::reduce_motion` in step with it
/// and with the `animations` setting (re-read when the settings are). Call once, after the
/// `Appearance` global is set.
pub fn watch(cx: &mut App) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: a static NSString from AppKit.
    let name = unsafe { NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification };
    let observer = observe(&center, name, None, cx, |cx| {
        let on = system_reduce_motion();
        if cx.global::<SystemReduceMotion>().on != on {
            // `global_mut` tells the global's observers (the Workspace).
            cx.global_mut::<SystemReduceMotion>().on = on;
            apply(cx);
        }
    });
    cx.set_global(SystemReduceMotion {
        on: system_reduce_motion(),
        _observer: observer,
    });
    apply(cx);
    cx.observe_global::<Appearance>(apply).detach();
}

/// Motion is off when the `animations` setting is off or macOS asks for reduced motion.
fn apply(cx: &mut App) {
    let off = !cx.global::<Appearance>().animations || cx.global::<SystemReduceMotion>().on;
    cx.set_reduce_motion(off);
}

/// The numbers of the app's windows now; `watch_window` takes them from before the main window
/// opened, to tell it from the others.
pub fn window_numbers() -> Vec<isize> {
    app_windows().iter().map(|w| w.windowNumber()).collect()
}

/// Keep `WindowVisible` for the window opened since `before` (`window_numbers`). None found
/// (not expected): the window counts as always visible.
pub fn watch_window(before: &[isize], cx: &mut App) {
    let Some(window) = app_windows()
        .into_iter()
        .find(|w| !before.contains(&w.windowNumber()))
    else {
        eprintln!("[fuxx] motion: the main window wasn't found; marks animate even when hidden");
        return;
    };
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: a static NSString from AppKit.
    let name = unsafe { NSWindowDidChangeOcclusionStateNotification };
    let seen = window.clone();
    let observer = observe(&center, name, Some(&window), cx, move |cx| {
        let visible = is_visible(&seen);
        if cx.global::<WindowVisible>().visible != visible {
            cx.global_mut::<WindowVisible>().visible = visible;
        }
    });
    cx.set_global(WindowVisible {
        visible: is_visible(&window),
        _observer: observer,
    });
}

fn is_visible(window: &NSWindow) -> bool {
    window
        .occlusionState()
        .contains(NSWindowOcclusionState::Visible)
}

fn app_windows() -> Vec<Retained<NSWindow>> {
    let mtm = MainThreadMarker::new().expect("GPUI's App lives on the main thread");
    NSApplication::sharedApplication(mtm).windows().to_vec()
}

/// Run `on_change` in an app update each time `center` posts `name` (about `object`, or any).
/// AppKit may post while GPUI is mid-update, so the notification only wakes a task, which then
/// does the work; the task sleeps until the next one.
fn observe(
    center: &NSNotificationCenter,
    name: &NSNotificationName,
    object: Option<&NSWindow>,
    cx: &mut App,
    on_change: impl Fn(&mut App) + 'static,
) -> Observer {
    let (tx, mut rx) = mpsc::unbounded::<()>();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        let _ = tx.unbounded_send(());
    });
    // SAFETY: `object` is the window the name is about (or none); no queue, so the block runs
    // where AppKit posts (the main thread); it only sends on a channel, which is thread-safe.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name),
            object.map(|w| w.as_ref()),
            None,
            &block,
        )
    };
    cx.spawn(async move |cx| {
        while rx.next().await.is_some() {
            cx.update(|cx| on_change(cx));
        }
    })
    .detach();
    observer
}
