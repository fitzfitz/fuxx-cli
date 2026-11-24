//! Undo and redo for a text field (the 2026-11-24 chores: "undo/redo what I typed … any text
//! input"), shared by `NameInput` and the agent box's `Composer`. Generic over what a field
//! keeps (its text and selection, and the box's chips). Typing groups into word-sized steps; a
//! paste, a cut, a deletion, a chip taken off or a prompt recalled is one step each. A new edit
//! drops what redo held; a send or a commit clears it all.

/// What an edit was, for grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// A typed character: letters and digits run on into one step; anything else ends it.
    Typed(char),
    Paste,
    Cut,
    Delete,
    /// An attachment chip added or taken off.
    Chip,
    /// An older prompt recalled with ↑ (or ↓ back).
    Recall,
}

/// The most steps kept.
pub const LIMIT: usize = 200;

#[derive(Clone, Debug)]
pub struct UndoHistory<S> {
    undo: Vec<S>,
    redo: Vec<S>,
    /// The last edit, while typing may still run on into it.
    last: Option<EditKind>,
}

impl<S> Default for UndoHistory<S> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
        }
    }
}

impl<S: Clone + PartialEq> UndoHistory<S> {
    /// An edit of `kind` is about to change the field from `before`.
    pub fn record(&mut self, before: S, kind: EditKind) {
        let word =
            |k: Option<EditKind>| matches!(k, Some(EditKind::Typed(c)) if c.is_alphanumeric());
        // A letter after a letter runs on into the same step.
        let runs_on = word(Some(kind)) && word(self.last);
        self.last = Some(kind);
        self.redo.clear();
        if runs_on || self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
    }

    /// Back one step from `now`: the state to show, if there is one.
    pub fn undo(&mut self, now: S) -> Option<S> {
        let back = self.undo.pop()?;
        self.redo.push(now);
        self.last = None;
        Some(back)
    }

    /// Forward one step from `now`, after an undo.
    pub fn redo(&mut self, now: S) -> Option<S> {
        let forward = self.redo.pop()?;
        self.undo.push(now);
        self.last = None;
        Some(forward)
    }

    /// A caret move or a click: the next typing starts a new step.
    pub fn break_run(&mut self) {
        self.last = None;
    }

    /// Sent or committed: nothing to go back to.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `word` into `text` one character at a time, recording each.
    fn type_in(h: &mut UndoHistory<String>, text: &mut String, word: &str) {
        for c in word.chars() {
            h.record(text.clone(), EditKind::Typed(c));
            text.push(c);
        }
    }

    #[test]
    fn typing_a_word_undoes_as_one_step() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "hello world");
        assert_eq!(h.undo(text.clone()).as_deref(), Some("hello "));
        assert_eq!(h.undo("hello ".into()).as_deref(), Some("hello"));
        assert_eq!(h.undo("hello".into()).as_deref(), Some(""));
        assert_eq!(h.undo(String::new()), None);
    }

    #[test]
    fn paste_is_one_step() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "ab");
        h.record(text.clone(), EditKind::Paste);
        text.push_str("cd");
        h.record(text.clone(), EditKind::Paste);
        text.push_str("ef");
        assert_eq!(h.undo(text.clone()).as_deref(), Some("abcd"));
        assert_eq!(h.undo("abcd".into()).as_deref(), Some("ab"));
    }

    #[test]
    fn redo_after_undo() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "hi");
        let back = h.undo(text.clone()).unwrap();
        assert_eq!(back, "");
        assert_eq!(h.redo(back).as_deref(), Some("hi"));
        assert_eq!(h.redo("hi".into()), None);
    }

    #[test]
    fn a_new_edit_drops_redo() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "hi");
        let back = h.undo(text.clone()).unwrap();
        h.record(back.clone(), EditKind::Paste);
        assert_eq!(h.redo("x".into()), None);
    }

    #[test]
    fn removing_a_chip_undoes() {
        let mut h: UndoHistory<(String, Vec<&str>)> = UndoHistory::default();
        let before = ("fix it".to_string(), vec!["shot.png"]);
        h.record(before.clone(), EditKind::Chip);
        let now = ("fix it".to_string(), vec![]);
        assert_eq!(h.undo(now), Some(before));
    }

    #[test]
    fn recall_undoes_to_the_draft() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "draft");
        h.record(text.clone(), EditKind::Recall);
        let recalled = "an older prompt".to_string();
        assert_eq!(h.undo(recalled).as_deref(), Some("draft"));
    }

    #[test]
    fn a_caret_move_breaks_the_word() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "ab");
        h.break_run();
        type_in(&mut h, &mut text, "cd");
        assert_eq!(h.undo(text.clone()).as_deref(), Some("ab"));
    }

    #[test]
    fn send_clears_history() {
        let mut h = UndoHistory::default();
        let mut text = String::new();
        type_in(&mut h, &mut text, "sent");
        h.clear();
        assert_eq!(h.undo(String::new()), None);
    }
}
