// The Help menu (M11d), and which window a Help item acts on.

use gpui::{Menu, MenuItem};

use crate::{ShowCommands, ShowShortcuts};

/// The Help menu: the two panels (Keyboard Shortcuts shows ⌘/, its binding).
pub fn menu() -> Menu {
    Menu::new("Help").items([
        MenuItem::action("Keyboard Shortcuts", ShowShortcuts),
        MenuItem::action("fuxx Command Reference", ShowCommands),
    ])
}

/// Which window a Help item that needs one goes to, from each open window's facts: whether it
/// is a main fuxx window (its root is a `Workspace`) and whether it is the key window. The key
/// window if it is a main one; else the first main one (with a popped-out browser window or
/// Preferences in front); else none.
pub fn help_target(windows: &[(bool, bool)]) -> Option<usize> {
    windows
        .iter()
        .position(|&(main, key)| main && key)
        .or_else(|| windows.iter().position(|&(main, _)| main))
}

#[cfg(test)]
mod tests {
    use super::help_target;

    #[test]
    fn help_goes_to_the_key_window_if_it_is_a_main_one() {
        // Preferences (or a popped-out page) is key: the main window behind it.
        assert_eq!(help_target(&[(false, true), (true, false)]), Some(1));
        // Two main windows: the key one.
        assert_eq!(help_target(&[(true, false), (true, true)]), Some(1));
        // No main window: nowhere.
        assert_eq!(help_target(&[(false, true)]), None);
        assert_eq!(help_target(&[]), None);
    }
}
