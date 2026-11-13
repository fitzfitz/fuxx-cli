//! M11p Task 6: the moving status marks (Working, Waiting) animate in their own small views,
//! so the marks' 100 ms ticker turns them without notifying the `Workspace` (which would redraw
//! every cached chrome part, see `chrome_cache`).
//!
//! - `MarkClock` is the marks' shared clock (`status_mark`'s `phase`). The ticker
//!   (`Workspace::tick_marks`) advances it and notifies only it.
//! - A `MarkView` draws one session's mark (`status_mark::mark_on`) with the clock's phase. It
//!   observes the clock and, while its mark moves, redraws on each tick. The `Workspace` keeps
//!   one per (place, key) in `Workspace::marks` and gives it its status, size and fill each time
//!   the part that shows it is drawn (`Workspace::mark_view`).
//!
//! The limit (GPUI, pinned rev): a notify of a view marks that view **and every view above it**
//! dirty (`Window::mark_view_dirty`, window.rs:1814), and a dirty cached view is drawn again
//! (view.rs:390). So a tick still redraws the part holding a moving mark (a tab bar with a
//! Working tab, the status bar following one). What it no longer redraws: every other part (the
//! title area, the other tab bars, the sidebar unless a closed group's header moves), and the
//! sidebar's rows aren't built again (the `Workspace` isn't notified; its thin `render` still
//! runs, as every view's ancestor). `Workspace::sync_chrome` reads every mark view and the clock
//! each frame, so the window keeps them among the entities it shows (see `chrome_cache`).

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{Context, Entity, IntoElement, Pixels, Render, Subscription, Window};

use crate::appearance::Appearance;
use crate::chrome_cache::{self, Counted};
use crate::status::Status;
use crate::status_mark;

/// The status marks' shared clock: 0 to 1 over a beat (see `status_mark`).
pub struct MarkClock {
    pub phase: f32,
}

/// One status mark, drawn with the clock's phase.
pub struct MarkView {
    clock: Entity<MarkClock>,
    status: Status,
    size: Pixels,
    fill: Rgb,
    _observe: Subscription,
    /// How many times it was drawn (the UI screenshots read it).
    #[cfg(feature = "ui-shots")]
    pub renders: u64,
}

impl MarkView {
    pub fn new(
        clock: &Entity<MarkClock>,
        status: Status,
        size: Pixels,
        fill: Rgb,
        cx: &mut Context<Self>,
    ) -> Self {
        // Only a moving mark looks different on the next tick; a still one would only redraw
        // its part for nothing.
        let observe = cx.observe(clock, |this: &mut Self, _, cx| {
            if this.moves() {
                cx.notify();
            }
        });
        Self {
            clock: clock.clone(),
            status,
            size,
            fill,
            _observe: observe,
            #[cfg(feature = "ui-shots")]
            renders: 0,
        }
    }

    /// Whether its mark moves with the clock (Working turns, Waiting breathes).
    pub fn moves(&self) -> bool {
        matches!(self.status, Status::Working | Status::Waiting)
    }

    /// What it shows, as the part drawing it has it now. No notify: the part is being drawn,
    /// and a view inside it (not cached) is drawn with it, after this.
    pub fn set(&mut self, status: Status, size: Pixels, fill: Rgb) {
        self.status = status;
        self.size = size;
        self.fill = fill;
    }
}

impl Render for MarkView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        chrome_cache::bump(Counted::Mark);
        #[cfg(feature = "ui-shots")]
        {
            self.renders += 1;
        }
        let ui = cx.global::<Appearance>().theme.ui();
        let phase = self.clock.read(cx).phase;
        status_mark::mark_on(self.status, &ui, phase, None, self.size, self.fill).into_any_element()
    }
}
