//! Tab colours (M11): a colour you can give a session from its sidebar row's menu. It shows as a
//! dot on the row and on its row in the palette's workspaces, and is saved with the session.
//!
//! Each colour is a fixed hue; `Theme::ui` fits it to the theme (`UiColors::tab`) so the dot
//! stands out from every surface it's drawn on, as the status marks do.

use alacritty_terminal::vte::ansi::Rgb;
use serde::{Deserialize, Deserializer, Serialize};

/// A session's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TabColor {
    Blue,
    Purple,
    Magenta,
    Red,
    Pink,
    Orange,
    Yellow,
    Green,
    Teal,
    Cyan,
    Grey,
}

impl TabColor {
    /// Every colour, in the menu's order.
    pub const ALL: [TabColor; 11] = [
        Self::Blue,
        Self::Purple,
        Self::Magenta,
        Self::Red,
        Self::Pink,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Cyan,
        Self::Grey,
    ];

    /// Its name, as saved and as scripts see it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Magenta => "magenta",
            Self::Red => "red",
            Self::Pink => "pink",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Cyan => "cyan",
            Self::Grey => "grey",
        }
    }

    /// The colour named `name` (exact, lower case); none for anything else.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// The colour before it's fitted to a theme.
    pub fn base(self) -> Rgb {
        let hex = match self {
            Self::Blue => 0x3b82f6,
            Self::Purple => 0x8b5cf6,
            Self::Magenta => 0xd946ef,
            Self::Red => 0xef4444,
            Self::Pink => 0xf472b6,
            Self::Orange => 0xf97316,
            Self::Yellow => 0xeab308,
            Self::Green => 0x22c55e,
            Self::Teal => 0x14b8a6,
            Self::Cyan => 0x06b6d4,
            Self::Grey => 0x8b9099,
        };
        Rgb {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
        }
    }
}

/// A saved colour, read leniently: a name this fuxx doesn't know (a newer one's colour, a hand
/// edit) is no colour, not a sessions file that won't load.
pub fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Option<TabColor>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(TabColor::parse))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_back_and_unknown_ones_do_not() {
        for c in TabColor::ALL {
            assert_eq!(TabColor::parse(c.name()), Some(c));
        }
        assert_eq!(TabColor::parse("brown"), None);
        assert_eq!(TabColor::parse("Blue"), None);
        // Eleven distinct colours.
        let bases: std::collections::HashSet<_> = TabColor::ALL
            .map(|c| (c.base().r, c.base().g, c.base().b))
            .into();
        assert_eq!(bases.len(), 11);
    }

    #[test]
    fn all_is_in_declaration_order() {
        // `UiColors::tab` indexes its table by `c as usize`.
        assert!(
            TabColor::ALL
                .iter()
                .enumerate()
                .all(|(i, &c)| c as usize == i)
        );
    }
}
