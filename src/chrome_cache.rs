//! M11p: render counters for the window chrome, so the harness can prove what redraws.
//!
//! `bump` is an empty function without the `ui-shots` feature: call sites carry no `cfg` and a
//! release build pays nothing. With the feature each `Counted` part has an atomic counter the
//! `ui-shots` scenes read (`get`) and clear (`reset`).

/// A chrome part whose render passes are counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Counted {
    /// `Workspace::render`.
    Workspace,
    /// `render_sidebar`.
    Sidebar,
    /// `render_status_bar`.
    StatusBar,
    /// `render_tab_bar`, once per pane's bar.
    TabBar,
    /// The title area, once per `Workspace::render` (its name and band buttons, or the
    /// sidebar's band while the sidebar is shown).
    Title,
    /// `sidebar_rows`, the sidebar's row model (also called outside `render_sidebar`).
    SidebarRows,
}

#[cfg(feature = "ui-shots")]
mod counters {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::Counted;

    static COUNTS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

    pub fn bump(c: Counted) {
        COUNTS[c as usize].fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(c: Counted) -> u64 {
        COUNTS[c as usize].load(Ordering::Relaxed)
    }

    pub fn reset() {
        for count in &COUNTS {
            count.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(feature = "ui-shots")]
pub use counters::{bump, get, reset};

/// Count one render pass of `c` (nothing without the `ui-shots` feature).
#[cfg(not(feature = "ui-shots"))]
#[inline(always)]
pub fn bump(_: Counted) {}
