//! The ⌘K palette's keys, scopes and Recent list (M11j). Pure.
#![allow(dead_code)] // used by the palette (M11j tasks 2+)

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Workspace,
    Action,
    File,
    Setting,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Workspace(u64),
    Action(&'static str),
    Preset(usize),
    File(PathBuf),
    Setting(&'static str),
    More(Kind),
}

impl Key {
    pub fn kind(&self) -> Kind {
        match self {
            Key::Workspace(_) => Kind::Workspace,
            Key::Action(_) | Key::Preset(_) => Kind::Action,
            Key::File(_) => Kind::File,
            Key::Setting(_) => Kind::Setting,
            Key::More(k) => *k,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Actions,
    Workspaces,
    Files,
    Settings,
}

impl Scope {
    pub const ALL: [Scope; 5] = [
        Scope::All,
        Scope::Actions,
        Scope::Workspaces,
        Scope::Files,
        Scope::Settings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Actions => "Actions",
            Scope::Workspaces => "Workspaces",
            Scope::Files => "Files",
            Scope::Settings => "Settings",
        }
    }
    pub fn placeholder(self) -> &'static str {
        match self {
            Scope::All => "Search actions, workspaces, files and settings",
            Scope::Actions => "Run an action",
            Scope::Workspaces => "Go to a workspace \u{2014} number, name, group or question",
            Scope::Files => "Open a file",
            Scope::Settings => "Find a setting",
        }
    }
    fn index(self) -> usize {
        Scope::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
    pub fn next(self) -> Scope {
        Scope::ALL[(self.index() + 1) % Scope::ALL.len()]
    }
    pub fn prev(self) -> Scope {
        Scope::ALL[(self.index() + Scope::ALL.len() - 1) % Scope::ALL.len()]
    }
    pub fn kind(self) -> Option<Kind> {
        match self {
            Scope::All => None,
            Scope::Actions => Some(Kind::Action),
            Scope::Workspaces => Some(Kind::Workspace),
            Scope::Files => Some(Kind::File),
            Scope::Settings => Some(Kind::Setting),
        }
    }
    pub fn of(kind: Kind) -> Scope {
        match kind {
            Kind::Action => Scope::Actions,
            Kind::Workspace => Scope::Workspaces,
            Kind::File => Scope::Files,
            Kind::Setting => Scope::Settings,
        }
    }
}

/// On All only, a query starting with `>` searches Actions (the rest trimmed at its start).
pub fn parse_query(scope: Scope, text: &str) -> (Scope, &str) {
    match (scope, text.strip_prefix('>')) {
        (Scope::All, Some(rest)) => (Scope::Actions, rest.trim_start()),
        _ => (scope, text),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Open,
    Close,
    Rescope,
}

/// Closed: open. Open on the asked scope: close. Open on another: rescope (text kept).
pub fn palette_step(open: Option<Scope>, asked: Scope) -> Step {
    match open {
        None => Step::Open,
        Some(s) if s == asked => Step::Close,
        Some(_) => Step::Rescope,
    }
}

pub const RECENT_MAX: usize = 20;
pub const RECENT_SHOWN: usize = 5;

#[derive(Default, Debug)]
pub struct Recent {
    keys: Vec<Key>,
}

impl Recent {
    pub fn push(&mut self, key: Key) {
        if matches!(key, Key::More(_)) {
            return;
        }
        self.keys.retain(|k| *k != key);
        self.keys.insert(0, key);
        self.keys.truncate(RECENT_MAX);
    }
    pub fn iter(&self) -> impl Iterator<Item = &Key> {
        self.keys.iter()
    }
    pub fn rank(&self, key: &Key) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_scopes_cycle_round() {
        assert_eq!(Scope::All.next(), Scope::Actions);
        assert_eq!(Scope::Settings.next(), Scope::All);
        assert_eq!(Scope::All.prev(), Scope::Settings);
        assert_eq!(Scope::Actions.prev(), Scope::All);
        for s in Scope::ALL {
            assert_eq!(s.next().prev(), s);
            assert!(!s.label().is_empty());
            assert!(!s.placeholder().is_empty());
            if let Some(k) = s.kind() {
                assert_eq!(Scope::of(k), s);
            }
        }
        assert_eq!(Scope::All.kind(), None);
        assert_eq!(Scope::Files.label(), "Files");
        assert_eq!(Scope::Files.placeholder(), "Open a file");
        assert_eq!(
            Scope::Workspaces.placeholder(),
            "Go to a workspace \u{2014} number, name, group or question"
        );
        assert_eq!(
            parse_query(Scope::All, "> split"),
            (Scope::Actions, "split")
        );
        assert_eq!(
            parse_query(Scope::Workspaces, ">x"),
            (Scope::Workspaces, ">x")
        );
        assert_eq!(parse_query(Scope::All, "split"), (Scope::All, "split"));
    }

    #[test]
    fn palette_step_toggles_and_rescopes() {
        assert_eq!(palette_step(None, Scope::All), Step::Open);
        assert_eq!(palette_step(Some(Scope::All), Scope::All), Step::Close);
        assert_eq!(
            palette_step(Some(Scope::All), Scope::Workspaces),
            Step::Rescope
        );
        assert_eq!(palette_step(Some(Scope::Files), Scope::All), Step::Rescope);
    }

    #[test]
    fn palette_key_kinds() {
        assert_eq!(Key::Workspace(1).kind(), Kind::Workspace);
        assert_eq!(Key::Preset(0).kind(), Kind::Action);
        assert_eq!(Key::File("a".into()).kind(), Kind::File);
        assert_eq!(Key::Setting("x").kind(), Kind::Setting);
        assert_eq!(Key::More(Kind::File).kind(), Kind::File);
    }

    #[test]
    fn palette_recent_keeps_the_last_twenty_newest_first() {
        let mut r = Recent::default();
        for i in 0..25 {
            r.push(Key::Workspace(i));
        }
        let all: Vec<_> = r.iter().cloned().collect();
        assert_eq!(all.len(), RECENT_MAX);
        assert_eq!(all[0], Key::Workspace(24));
        assert_eq!(all[19], Key::Workspace(5));
        assert_eq!(r.rank(&Key::Workspace(24)), Some(0));
        assert_eq!(r.rank(&Key::Workspace(0)), None);
        r.push(Key::Workspace(10));
        assert_eq!(r.iter().next(), Some(&Key::Workspace(10)));
        assert_eq!(r.iter().count(), RECENT_MAX);
        assert_eq!(r.rank(&Key::Workspace(24)), Some(1));
        r.push(Key::More(Kind::File));
        assert_eq!(r.iter().count(), RECENT_MAX);
        assert_eq!(r.rank(&Key::More(Kind::File)), None);
        assert_eq!(r.iter().next(), Some(&Key::Workspace(10)));
    }
}
