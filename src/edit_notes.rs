//! The agent's own reason for an edit (M11ag Part 2): the turn's last text before the edit, kept
//! as plain characters to show under the changed lines.

use std::sync::Arc;

use crate::providers::EditText;

/// A reason is cut at this many characters.
pub const REASON_MAX: usize = 2000;

/// The text as a reason: whitespace runs become one space, the ends are trimmed, and it is cut
/// at `REASON_MAX` characters (on a character boundary). `None` if nothing is left. The
/// characters stay raw: no Markdown is read.
pub fn reason_text(raw: &str) -> Option<Arc<str>> {
    let mut out = String::new();
    for word in raw.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() > REASON_MAX {
            break;
        }
    }
    if let Some((at, _)) = out.char_indices().nth(REASON_MAX) {
        out.truncate(at);
    }
    if out.is_empty() {
        None
    } else {
        Some(out.into())
    }
}

/// What an edit that finished OK carries to the Workspace: the agent's reason, the id of the
/// message it came from (one reason, several edits), and the new text of each change.
#[derive(Clone, Debug, PartialEq)]
pub struct EditNote {
    pub reason: Arc<str>,
    pub reason_id: u64,
    pub texts: Vec<EditText>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_text_collapses_whitespace_and_caps_on_a_char_boundary() {
        let long = "é".repeat(3000);
        let cut = reason_text(&long).expect("text");
        assert_eq!(cut.chars().count(), 2000);
        assert!(cut.chars().all(|c| c == 'é'));
        assert_eq!(reason_text("a \n\n  b").as_deref(), Some("a b"));
        assert_eq!(reason_text("   "), None);
        assert_eq!(
            reason_text("**bold** `x`\n```rs\nfn f() {}\n```").as_deref(),
            Some("**bold** `x` ```rs fn f() {} ```")
        );
    }
}
