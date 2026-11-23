//! The agent's question card (batch 3, chore 8): what it shows and what each key does, one
//! question at a time. Pure and GPUI-free; `agent_ask_view` draws it and `agent_tab` wires it.

use std::time::{Duration, Instant};

use crate::agent_conversation::Reply;
use crate::providers::{QAnswer, Question};

/// Keys this soon after the card appears are ignored: what you were typing doesn't answer it.
pub const GUARD: Duration = Duration::from_millis(250);
/// How long Esc keeps Skip armed (the designer: a second Esc within it skips).
pub const SKIP_ARM: Duration = Duration::from_millis(2000);
/// The preview well's most lines.
pub const PREVIEW_MAX_LINES: usize = 12;
/// The summary line's most characters (its row truncates again to its width).
pub const SUMMARY_CHARS: usize = 140;

/// A key the card takes (key context `AgentAsk`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AskKey {
    /// 1–9: pick that option; one past the options is Other.
    Digit(usize),
    Up,
    Down,
    /// ← / →: the question before or after.
    Prev,
    Next,
    Space,
    Enter,
    Tab,
    /// A letter: the box's (it isn't bound in `AgentAsk`, so the box types it itself); kept so
    /// the rule is said once, here.
    #[allow(dead_code)]
    Letter(char),
    Esc,
}

/// What the tab does after a key.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Nothing changed (or the key came too soon).
    None,
    /// The card changed: draw it again.
    Moved,
    /// The keyboard goes to the box (its text is "Other"), with this character typed there.
    ToBox(Option<char>),
    /// Send these answers (one per question, in order).
    Send(Vec<QAnswer>),
    Skip,
}

/// The open card's state.
#[derive(Clone, Debug)]
pub struct AskUi {
    pub id: String,
    pub questions: Vec<Question>,
    /// The question shown.
    pub step: usize,
    /// The row under the cursor: an option, or (one past them) Other.
    pub cursor: usize,
    /// Each question's picked options, in the options' order (single-select: at most one).
    pub picks: Vec<Vec<usize>>,
    /// Each question's words of your own (from the box).
    pub other: Vec<Option<String>>,
    shown_at: Instant,
    /// Esc armed Skip then (a second Esc within `SKIP_ARM` skips).
    armed: Option<Instant>,
    /// The review's Enter lit a missing answer: the next sends anyway.
    anyway: bool,
    /// The question shown was opened from the review: Enter (or a pick) goes back to it.
    from_review: bool,
}

impl AskUi {
    pub fn new(id: String, questions: Vec<Question>, now: Instant) -> Self {
        let n = questions.len();
        let mut ui = AskUi {
            id,
            questions,
            step: 0,
            cursor: 0,
            picks: vec![Vec::new(); n],
            other: vec![None; n],
            shown_at: now,
            armed: None,
            anyway: false,
            from_review: false,
        };
        ui.cursor = ui.start_cursor(0);
        ui
    }

    /// When the card appeared (the palette answers the oldest first).
    pub fn shown_at(&self) -> Instant {
        self.shown_at
    }

    /// How many steps: the questions, and (two or more) the review.
    pub fn steps(&self) -> usize {
        let n = self.questions.len();
        n + usize::from(n >= 2)
    }

    /// The review step is shown (the user's UX bar, 2026-11-23).
    pub fn on_review(&self) -> bool {
        self.step >= self.questions.len()
    }

    /// The review's rows: each question's header and its answer (None: not answered), the
    /// picks' shown labels and your own words in quotes (the designer).
    pub fn review(&self) -> Vec<(String, Option<String>)> {
        self.questions
            .iter()
            .enumerate()
            .map(|(q, question)| {
                let a = self.answer_of(q);
                let mut parts: Vec<String> = a
                    .labels
                    .iter()
                    .map(|l| shown_label(l).to_string())
                    .collect();
                if let Some(o) = a.other.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
                    parts.push(format!("\u{201c}{o}\u{201d}"));
                }
                (
                    header_of(question, q),
                    (!parts.is_empty()).then(|| parts.join(", ")),
                )
            })
            .collect()
    }

    /// Show step `step` (a step chip clicked, ← / →).
    pub fn go_to(&mut self, step: usize) {
        self.step = step.min(self.steps().saturating_sub(1));
        self.cursor = self.start_cursor(self.step);
        self.from_review = false;
    }

    /// A review row opened (clicked, or Space on it): its question, Enter coming back here.
    pub fn open_from_review(&mut self, q: usize) {
        if q < self.questions.len() {
            self.go_to(q);
            self.from_review = true;
        }
    }

    /// Esc armed Skip, and it is still armed at `now` (the button says "Esc again to skip").
    pub fn skip_armed(&self, now: Instant) -> bool {
        self.armed
            .is_some_and(|at| now.saturating_duration_since(at) < SKIP_ARM)
    }

    /// The review's first Enter lit a missing answer: the next sends anyway.
    pub fn send_anyway(&self) -> bool {
        self.anyway
    }

    /// Option `option` clicked: as its number key, at once (no guard for a click).
    pub fn click(&mut self, option: usize, now: Instant) -> Effect {
        let Some(q) = self.current() else {
            return Effect::None;
        };
        if option >= q.options.len() {
            return Effect::None;
        }
        self.pick(option, now)
    }

    /// Key `key` at `now`.
    pub fn key(&mut self, key: AskKey, now: Instant) -> Effect {
        if self.too_soon(now) {
            return Effect::None;
        }
        if key == AskKey::Esc {
            if self.skip_armed(now) {
                return Effect::Skip;
            }
            self.armed = Some(now);
            return Effect::Moved;
        }
        // Any other key disarms Skip.
        let was_armed = self.armed.take().is_some();
        let effect = self.take_key(key, now);
        if was_armed && effect == Effect::None {
            Effect::Moved
        } else {
            effect
        }
    }

    fn take_key(&mut self, key: AskKey, now: Instant) -> Effect {
        match key {
            AskKey::Esc => Effect::Skip,
            AskKey::Tab => Effect::ToBox(None),
            AskKey::Letter(c) => Effect::ToBox(Some(c)),
            AskKey::Prev => self.step_to(self.step.checked_sub(1)),
            AskKey::Next => self.step_to(Some(self.step + 1)),
            _ if self.on_review() => match key {
                AskKey::Enter => {
                    let missing = (0..self.questions.len()).find(|&q| !self.answered(q));
                    match missing {
                        Some(q) if !self.anyway => {
                            self.anyway = true;
                            self.cursor = q;
                            Effect::Moved
                        }
                        _ => Effect::Send(self.answers()),
                    }
                }
                AskKey::Up | AskKey::Down => {
                    let last = self.questions.len().saturating_sub(1);
                    let to = if key == AskKey::Up {
                        self.cursor.saturating_sub(1)
                    } else {
                        (self.cursor + 1).min(last)
                    };
                    if to == self.cursor {
                        return Effect::None;
                    }
                    self.cursor = to;
                    Effect::Moved
                }
                AskKey::Space => {
                    self.open_from_review(self.cursor);
                    Effect::Moved
                }
                _ => Effect::None,
            },
            AskKey::Digit(d) => {
                let n = self.current().map_or(0, |q| q.options.len());
                match d {
                    _ if (1..=n).contains(&d) => self.pick(d - 1, now),
                    _ if d == n + 1 => Effect::ToBox(None),
                    _ => Effect::ToBox(char::from_digit(d as u32, 10)),
                }
            }
            AskKey::Up => self.move_cursor(-1),
            AskKey::Down => self.move_cursor(1),
            AskKey::Space => {
                let Some(q) = self.current() else {
                    return Effect::None;
                };
                if self.cursor >= q.options.len() {
                    return Effect::ToBox(None);
                }
                if q.multi {
                    self.toggle(self.cursor);
                } else {
                    self.picks[self.step] = vec![self.cursor];
                    self.other[self.step] = None;
                }
                Effect::Moved
            }
            AskKey::Enter => {
                let Some(q) = self.current() else {
                    return Effect::None;
                };
                if self.cursor >= q.options.len() {
                    return Effect::ToBox(None);
                }
                if q.multi {
                    if !self.answered(self.step) {
                        return Effect::None;
                    }
                } else if self.other_of(self.step).is_none() {
                    // Your own words stay the answer; otherwise the row under the cursor is it.
                    self.picks[self.step] = vec![self.cursor];
                }
                self.advance()
            }
        }
    }

    /// Enter in the box with `text`: it is the current question's Other, and the card moves on
    /// (empty: the card's own Enter). On the review, text is left in the box.
    pub fn box_enter(&mut self, text: &str, now: Instant) -> Effect {
        if text.trim().is_empty() {
            return self.key(AskKey::Enter, now);
        }
        if self.too_soon(now) || self.on_review() {
            return Effect::None;
        }
        self.other[self.step] = Some(text.trim().to_string());
        if !self.questions[self.step].multi {
            self.picks[self.step].clear();
        }
        self.advance()
    }

    /// Whether question `q` has an answer (a pick, or words of your own).
    pub fn answered(&self, q: usize) -> bool {
        !self.answer_of(q).is_empty()
    }

    /// The answers, one per question.
    pub fn answers(&self) -> Vec<QAnswer> {
        (0..self.questions.len())
            .map(|q| self.answer_of(q))
            .collect()
    }

    /// The question shown (None on the review).
    pub fn current(&self) -> Option<&Question> {
        self.questions.get(self.step)
    }

    fn too_soon(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.shown_at) < GUARD
    }

    fn other_of(&self, q: usize) -> Option<&str> {
        self.other
            .get(q)?
            .as_deref()
            .map(str::trim)
            .filter(|o| !o.is_empty())
    }

    fn answer_of(&self, q: usize) -> QAnswer {
        let Some(question) = self.questions.get(q) else {
            return QAnswer::default();
        };
        QAnswer {
            labels: self.picks[q]
                .iter()
                .filter_map(|&i| question.options.get(i).map(|o| o.label.clone()))
                .collect(),
            other: self.other_of(q).map(String::from),
        }
    }

    /// Where the cursor starts on step `step`: its first pick, else a recommended option, else
    /// the first.
    fn start_cursor(&self, step: usize) -> usize {
        let Some(q) = self.questions.get(step) else {
            return 0;
        };
        self.picks[step]
            .first()
            .copied()
            .or_else(|| {
                q.options
                    .iter()
                    .position(|o| recommended(&o.label).is_some())
            })
            .unwrap_or(0)
    }

    fn step_to(&mut self, step: Option<usize>) -> Effect {
        match step.filter(|&s| s < self.steps()) {
            Some(s) if s != self.step => {
                self.go_to(s);
                Effect::Moved
            }
            _ => Effect::None,
        }
    }

    fn move_cursor(&mut self, by: isize) -> Effect {
        let rows = self.current().map_or(0, |q| q.options.len() + 1);
        let to = self
            .cursor
            .saturating_add_signed(by)
            .min(rows.saturating_sub(1));
        if to == self.cursor {
            return Effect::None;
        }
        self.cursor = to;
        Effect::Moved
    }

    fn toggle(&mut self, option: usize) {
        let picks = &mut self.picks[self.step];
        match picks.iter().position(|&p| p == option) {
            Some(at) => {
                picks.remove(at);
            }
            None => {
                picks.push(option);
                picks.sort_unstable();
            }
        }
    }

    /// Option `option` picked (its number, or a click): a multi-select toggles it; a
    /// single-select takes it and moves on, except on its last question.
    fn pick(&mut self, option: usize, _now: Instant) -> Effect {
        let multi = self.current().is_some_and(|q| q.multi);
        self.cursor = option;
        if multi {
            self.toggle(option);
            return Effect::Moved;
        }
        self.picks[self.step] = vec![option];
        self.other[self.step] = None;
        if self.from_review {
            self.go_to(self.questions.len());
        } else if self.step + 1 < self.questions.len() {
            self.go_to(self.step + 1);
        }
        Effect::Moved
    }

    /// On to the next step (a question, then the review); from the last, send.
    fn advance(&mut self) -> Effect {
        if self.from_review {
            self.go_to(self.questions.len());
            return Effect::Moved;
        }
        if self.step + 1 < self.steps() {
            self.go_to(self.step + 1);
            Effect::Moved
        } else {
            Effect::Send(self.answers())
        }
    }
}

/// How the docked copy is drawn (the designer): the whole card, a compact one (the question
/// clamped, labels only, no well), or (a short chat) its head row only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockKind {
    Full,
    Compact,
    Tiny,
}

/// The tiny dock's height (its head row in the card's padding).
pub const TINY_DOCK_H: f32 = 44.0;

/// The dock's kind and height for a card `card_h` tall (None: not measured) in a chat
/// `view_h` tall: whole while it fits `DOCK_MAX` (and 45 % of the chat), else compact at that
/// height; under `TINY_DOCK_BELOW`, tiny.
pub fn dock_kind(card_h: Option<f32>, view_h: f32) -> (DockKind, f32) {
    use crate::ui_tokens::ask::{DOCK_MAX, DOCK_SHARE, TINY_DOCK_BELOW};
    if view_h < TINY_DOCK_BELOW {
        return (DockKind::Tiny, TINY_DOCK_H);
    }
    let max = DOCK_MAX.min(DOCK_SHARE * view_h);
    match card_h {
        Some(h) if h <= max => (DockKind::Full, h),
        _ => (DockKind::Compact, max),
    }
}

/// Whether the docked copy shows: the in-chat card's top (`top`; None when it isn't laid out,
/// `below` then saying whether it is under the view) sits lower than the chat's `bottom` less
/// the last row's padding and the dock's height `dock_h`. Nothing before the chat is laid out.
pub fn dock_shows(top: Option<f32>, below: bool, bottom: f32, dock_h: f32, laid_out: bool) -> bool {
    if !laid_out {
        return false;
    }
    match top {
        Some(t) => t > bottom - crate::ui_tokens::chat::LAST_BOTTOM - dock_h,
        None => below,
    }
}

/// Of the questions waiting (`asks`: whose, and since when), the oldest's.
pub fn oldest<T: Copy>(asks: &[(T, Instant)]) -> Option<T> {
    asks.iter().min_by_key(|(_, at)| *at).map(|(t, _)| *t)
}

/// `label` without Claude's " (Recommended)" suffix, when it has one.
pub fn recommended(label: &str) -> Option<&str> {
    label
        .strip_suffix("(Recommended)")
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
}

/// An option's label as the card shows it: without " (Recommended)" (a tag says it instead).
pub fn shown_label(label: &str) -> &str {
    recommended(label).unwrap_or(label)
}

/// The words question `i` goes by: its chip, else "Question N" (the designer).
pub fn header_of(q: &Question, i: usize) -> String {
    if q.header.trim().is_empty() {
        format!("Question {}", i + 1)
    } else {
        q.header.trim().to_string()
    }
}

/// "⟨Label⟩ asks: ⟨the first question⟩", and " (+N more)" with more (`question()`, the banner,
/// the sidebar's line two).
pub fn asks_line(label: &str, questions: &[Question]) -> String {
    let first = questions.first().map_or("", |q| q.text.trim());
    match questions.len() {
        0 | 1 => format!("{label} asks: {first}"),
        n => format!("{label} asks: {first} (+{} more)", n - 1),
    }
}

/// `text` cut to `max` characters, the last an ellipsis.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut t: String = text.chars().take(max - 1).collect();
    t.push('…');
    t
}

/// `answer` as words: the picks' shown labels and your own, joined by ", ".
fn answer_words(answer: &QAnswer) -> String {
    let mut parts: Vec<&str> = answer.labels.iter().map(|l| shown_label(l)).collect();
    if let Some(o) = answer
        .other
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
    {
        parts.push(o);
    }
    parts.join(", ")
}

/// The line an answered, skipped or expired card leaves: how it ended, then (answered here)
/// each header and its answer, else the headers; at most `SUMMARY_CHARS`.
#[cfg(test)]
pub fn summary_line(questions: &[Question], reply: &Reply) -> String {
    let (head, rest) = summary_parts(questions, reply);
    cut(&format!("{head} \u{b7} {rest}"), SUMMARY_CHARS)
}

/// `summary_line`'s two parts: how it ended ("Answered", "Skipped", "Not answered") and the
/// rest ("Test runner: Vitest · CI: Lint", or "Test runner, CI").
pub fn summary_parts(questions: &[Question], reply: &Reply) -> (&'static str, String) {
    let headers = || {
        questions
            .iter()
            .enumerate()
            .map(|(i, q)| header_of(q, i))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (head, rest) = match reply {
        Reply::Answered(a) if a.iter().any(|a| !a.is_empty()) => {
            let parts: Vec<String> = questions
                .iter()
                .enumerate()
                .filter_map(|(i, q)| {
                    let a = a.get(i).filter(|a| !a.is_empty())?;
                    Some(format!("{}: {}", header_of(q, i), answer_words(a)))
                })
                .collect();
            ("Answered", parts.join(" \u{b7} "))
        }
        Reply::Answered(_) => ("Answered", headers()),
        Reply::Skipped => ("Skipped", headers()),
        Reply::Expired => ("Not answered", headers()),
    };
    (head, cut(&rest, SUMMARY_CHARS))
}

/// The preview well's height in lines for `q`: its tallest preview, at most
/// `PREVIEW_MAX_LINES`; none for a multi-select or a question with no previews.
pub fn preview_lines(q: &Question) -> usize {
    if q.multi {
        return 0;
    }
    q.options
        .iter()
        .filter_map(|o| o.preview.as_deref())
        .map(|p| p.lines().count().max(1))
        .max()
        .unwrap_or(0)
        .min(PREVIEW_MAX_LINES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::QOption;

    fn option(label: &str, preview: Option<&str>) -> QOption {
        QOption {
            label: label.into(),
            description: String::new(),
            preview: preview.map(String::from),
        }
    }

    fn question(header: &str, labels: &[&str], multi: bool) -> Question {
        Question {
            header: header.into(),
            text: format!("{header}?"),
            options: labels.iter().map(|l| option(l, None)).collect(),
            multi,
        }
    }

    /// Three questions: single, multi, single.
    fn card(now: Instant) -> AskUi {
        AskUi::new(
            "q".into(),
            vec![
                question("Test runner", &["Vitest", "Jest"], false),
                question("CI checks", &["Lint", "Types", "Tests"], true),
                question("Coverage", &["Yes", "No"], false),
            ],
            now,
        )
    }

    fn later(t0: Instant) -> Instant {
        t0 + GUARD + Duration::from_millis(1)
    }

    fn one(labels: &[&str]) -> QAnswer {
        QAnswer {
            labels: labels.iter().map(|l| l.to_string()).collect(),
            other: None,
        }
    }

    #[test]
    fn cursor_starts_on_one_and_enter_sends_it() {
        let t0 = Instant::now();
        let mut ui = AskUi::new(
            "q".into(),
            vec![question("Test runner", &["Vitest", "Jest"], false)],
            t0,
        );
        assert_eq!((ui.step, ui.cursor), (0, 0));
        assert_eq!(
            ui.key(AskKey::Enter, later(t0)),
            Effect::Send(vec![one(&["Vitest"])])
        );
        // ↓ then Enter: the second.
        let mut ui = AskUi::new(
            "q".into(),
            vec![question("Test runner", &["Vitest", "Jest"], false)],
            t0,
        );
        assert_eq!(ui.key(AskKey::Down, later(t0)), Effect::Moved);
        assert_eq!(
            ui.key(AskKey::Enter, later(t0)),
            Effect::Send(vec![one(&["Jest"])])
        );
    }

    #[test]
    fn digit_picks_and_advances_except_on_the_last() {
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = card(t0);
        assert_eq!(ui.key(AskKey::Digit(2), t), Effect::Moved);
        assert_eq!(ui.picks[0], [1], "Jest");
        assert_eq!(ui.step, 1, "a single-select advances");
        assert_eq!(ui.key(AskKey::Digit(3), t), Effect::Moved);
        assert_eq!(ui.picks[1], [2]);
        assert_eq!(ui.step, 1, "a multi-select stays");
        ui.key(AskKey::Next, t);
        assert_eq!(ui.key(AskKey::Digit(1), t), Effect::Moved);
        assert_eq!(
            (ui.step, ui.picks[2].as_slice()),
            (2, &[0][..]),
            "the last stays"
        );
        // Up/Down stop at the ends: Other is the last row.
        ui.key(AskKey::Up, t);
        assert_eq!(ui.cursor, 0);
        for _ in 0..5 {
            ui.key(AskKey::Down, t);
        }
        assert_eq!(ui.cursor, 2, "two options, then Other");
        // ← goes back, its pick kept.
        ui.key(AskKey::Prev, t);
        ui.key(AskKey::Prev, t);
        assert_eq!((ui.step, ui.cursor), (0, 1), "the cursor on its pick");
        ui.key(AskKey::Prev, t);
        assert_eq!(ui.step, 0);
    }

    #[test]
    fn space_toggles_in_multi_and_enter_needs_a_check() {
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = card(t0);
        ui.key(AskKey::Next, t);
        assert!(ui.picks[1].is_empty(), "nothing checked at first");
        assert_eq!(ui.key(AskKey::Enter, t), Effect::None, "needs a check");
        assert_eq!(ui.step, 1);
        ui.key(AskKey::Space, t);
        ui.key(AskKey::Down, t);
        ui.key(AskKey::Down, t);
        ui.key(AskKey::Space, t);
        assert_eq!(ui.picks[1], [0, 2]);
        ui.key(AskKey::Space, t);
        assert_eq!(ui.picks[1], [0], "toggled off");
        ui.key(AskKey::Digit(2), t);
        assert_eq!(ui.picks[1], [0, 1], "in the options' order");
        assert_eq!(ui.key(AskKey::Enter, t), Effect::Moved);
        assert_eq!(ui.step, 2, "Next");
        // Space in a single-select picks without moving on.
        ui.key(AskKey::Down, t);
        assert_eq!(ui.key(AskKey::Space, t), Effect::Moved);
        assert_eq!((ui.step, ui.picks[2].as_slice()), (2, &[1][..]));
    }

    #[test]
    fn tab_and_letters_go_to_the_box_and_a_letter_is_forwarded() {
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = card(t0);
        assert_eq!(ui.key(AskKey::Tab, t), Effect::ToBox(None));
        assert_eq!(ui.key(AskKey::Letter('p'), t), Effect::ToBox(Some('p')));
        assert_eq!(
            ui.key(AskKey::Digit(3), t),
            Effect::ToBox(None),
            "Other's number"
        );
        assert_eq!(
            ui.key(AskKey::Digit(7), t),
            Effect::ToBox(Some('7')),
            "past Other"
        );
        ui.key(AskKey::Down, t);
        ui.key(AskKey::Down, t);
        assert_eq!(
            ui.key(AskKey::Enter, t),
            Effect::ToBox(None),
            "Enter on Other"
        );
        assert_eq!(ui.key(AskKey::Space, t), Effect::ToBox(None));
        assert!(ui.picks[0].is_empty(), "nothing picked");
    }

    /// The user's UX bar (2026-11-23): with two or more questions the last step is a review of
    /// each header and its answer; Enter there sends (an unanswered one left out), a review row
    /// jumps back to its question.
    #[test]
    fn a_review_step_follows_the_questions_and_enter_sends() {
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = card(t0);
        assert_eq!(ui.steps(), 4, "three questions and the review");
        ui.key(AskKey::Digit(1), t); // Vitest, on to CI checks
        ui.key(AskKey::Next, t); // CI checks left unanswered
        assert_eq!(ui.key(AskKey::Enter, t), Effect::Moved, "Coverage: Yes, on");
        assert!(ui.on_review());
        assert!(ui.answered(0) && !ui.answered(1) && ui.answered(2));
        assert_eq!(
            ui.review(),
            vec![
                ("Test runner".to_string(), Some("Vitest".to_string())),
                ("CI checks".to_string(), None),
                ("Coverage".to_string(), Some("Yes".to_string())),
            ]
        );
        assert_eq!(ui.key(AskKey::Digit(1), t), Effect::None, "no options here");
        // The designer: an answer missing, the first Enter lights it ("Send anyway").
        assert!(!ui.send_anyway());
        assert_eq!(ui.key(AskKey::Enter, t), Effect::Moved);
        assert!(ui.on_review() && ui.send_anyway());
        assert_eq!(ui.cursor, 1, "the light on the missing one");
        // Space opens the lit row's question; Enter there comes back to the review.
        ui.key(AskKey::Space, t);
        assert_eq!((ui.step, ui.on_review()), (1, false));
        ui.key(AskKey::Space, t); // Lint
        assert_eq!(ui.key(AskKey::Enter, t), Effect::Moved);
        assert!(ui.on_review(), "back to the review, not on to Coverage");
        assert_eq!(
            ui.key(AskKey::Enter, t),
            Effect::Send(vec![one(&["Vitest"]), one(&["Lint"]), one(&["Yes"])]),
            "all answered: Enter sends"
        );
        // Review ↑↓ move the light; ← goes to the last question.
        let mut ui = card(t0);
        ui.go_to(3);
        ui.key(AskKey::Down, t);
        ui.key(AskKey::Down, t);
        ui.key(AskKey::Down, t);
        assert_eq!(ui.cursor, 2, "stops at the last row");
        ui.key(AskKey::Prev, t);
        assert_eq!(ui.step, 2);
        // Sent anyway with two unanswered: they are left out (empty).
        let mut ui = card(t0);
        ui.key(AskKey::Digit(1), t);
        ui.go_to(3);
        ui.key(AskKey::Enter, t);
        assert_eq!(
            ui.key(AskKey::Enter, t),
            Effect::Send(vec![
                one(&["Vitest"]),
                QAnswer::default(),
                QAnswer::default()
            ])
        );
        // A review row clicked: its question, and back to the review after.
        let mut ui = card(t0);
        ui.go_to(3);
        ui.open_from_review(2);
        assert_eq!(ui.step, 2);
        ui.key(AskKey::Digit(2), t);
        assert!(ui.on_review(), "a pick there goes back to the review");
        // One question: no review, Enter sends.
        let mut ui = AskUi::new("q".into(), vec![question("Only", &["A"], false)], t0);
        assert_eq!(ui.steps(), 1);
        assert_eq!(ui.key(AskKey::Next, t), Effect::None);
    }

    /// The user's UX bar: "(Recommended)" (Claude's convention) shows as a tag and gets the
    /// cursor; the label goes back as it was.
    #[test]
    fn a_recommended_option_gets_the_cursor_and_keeps_its_label() {
        assert_eq!(recommended("Vitest (Recommended)"), Some("Vitest"));
        assert_eq!(recommended("Vitest"), None);
        assert_eq!(shown_label("Jest (Recommended)"), "Jest");
        assert_eq!(shown_label("Jest"), "Jest");
        let t0 = Instant::now();
        let mut ui = AskUi::new(
            "q".into(),
            vec![question("Runner", &["Vitest", "Jest (Recommended)"], false)],
            t0,
        );
        assert_eq!(ui.cursor, 1);
        assert_eq!(
            ui.key(AskKey::Enter, later(t0)),
            Effect::Send(vec![one(&["Jest (Recommended)"])])
        );
    }

    #[test]
    fn a_click_picks_without_the_guard() {
        let t0 = Instant::now();
        let mut ui = card(t0);
        assert_eq!(ui.click(1, t0), Effect::Moved);
        assert_eq!((ui.picks[0].as_slice(), ui.step), (&[1][..], 1));
        assert_eq!(ui.click(5, t0), Effect::None, "no such option");
    }

    #[test]
    fn keys_in_the_first_250ms_are_ignored() {
        let t0 = Instant::now();
        let mut ui = card(t0);
        let soon = t0 + Duration::from_millis(100);
        for key in [
            AskKey::Digit(1),
            AskKey::Enter,
            AskKey::Esc,
            AskKey::Down,
            AskKey::Tab,
        ] {
            assert_eq!(ui.key(key, soon), Effect::None, "{key:?}");
        }
        assert_eq!(ui.box_enter("typed ahead", soon), Effect::None);
        assert_eq!((ui.step, ui.cursor), (0, 0));
        assert!(ui.picks.iter().all(Vec::is_empty));
        assert_eq!(ui.key(AskKey::Down, later(t0)), Effect::Moved);
    }

    #[test]
    fn esc_skips() {
        // The designer: Esc arms Skip, a second within SKIP_ARM skips; another key, or the
        // time passing, disarms.
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = card(t0);
        assert_eq!(ui.key(AskKey::Esc, t), Effect::Moved);
        assert!(ui.skip_armed(t));
        assert_eq!(ui.key(AskKey::Esc, t), Effect::Skip);
        let mut ui = card(t0);
        ui.key(AskKey::Esc, t);
        ui.key(AskKey::Down, t);
        assert!(!ui.skip_armed(t), "another key disarms");
        assert_eq!(
            ui.key(AskKey::Esc, t),
            Effect::Moved,
            "armed again, not skipped"
        );
        let late = t + SKIP_ARM + Duration::from_millis(1);
        assert!(!ui.skip_armed(late), "the time passed");
        assert_eq!(ui.key(AskKey::Esc, late), Effect::Moved);
    }

    #[test]
    fn box_text_is_other_for_the_current_question() {
        let t0 = Instant::now();
        let t = later(t0);
        let mut ui = AskUi::new(
            "q".into(),
            vec![question("Test runner", &["Vitest", "Jest"], false)],
            t0,
        );
        ui.key(AskKey::Digit(1), t);
        let other = QAnswer {
            labels: Vec::new(),
            other: Some("Node's own".into()),
        };
        assert_eq!(
            ui.box_enter("Node's own", t),
            Effect::Send(vec![other]),
            "your words win over a single-select's pick"
        );
        // In a multi-select they join the picks; on a card of several it moves on.
        let mut ui = card(t0);
        ui.key(AskKey::Digit(1), t);
        ui.key(AskKey::Digit(1), t);
        assert_eq!(ui.box_enter("a11y", t), Effect::Moved, "on to Coverage");
        assert_eq!(ui.step, 2);
        assert_eq!(
            ui.other[1].as_deref(),
            Some("a11y"),
            "the multi-select's own words"
        );
        assert_eq!(ui.picks[1], [0]);
        // Empty: the card's own Enter (Coverage: Yes, then the review sends).
        assert_eq!(ui.box_enter("  ", t), Effect::Moved);
        assert!(ui.on_review());
        assert_eq!(
            ui.box_enter("", t),
            Effect::Send(vec![
                one(&["Vitest"]),
                QAnswer {
                    labels: vec!["Lint".into()],
                    other: Some("a11y".into())
                },
                one(&["Yes"]),
            ])
        );
    }

    #[test]
    fn summary_line_truncates() {
        let qs = vec![
            question("Test runner", &["Vitest", "Jest"], false),
            question("CI checks", &["Lint", "Types"], true),
            Question {
                header: String::new(),
                text: "Should I also add a coverage report to the README?".into(),
                options: vec![option("Yes", None)],
                multi: false,
            },
        ];
        let answers = vec![
            one(&["Vitest (Recommended)"]),
            QAnswer {
                labels: vec!["Lint".into(), "Types".into()],
                other: Some("a11y".into()),
            },
            QAnswer::default(),
        ];
        assert_eq!(
            summary_line(&qs, &Reply::Answered(answers)),
            "Answered · Test runner: Vitest · CI checks: Lint, Types, a11y"
        );
        // The designer: skipped and expired list the headers with commas.
        assert_eq!(
            summary_line(&qs, &Reply::Skipped),
            "Skipped · Test runner, CI checks, Question 3"
        );
        assert_eq!(
            summary_line(&qs[..1], &Reply::Expired),
            "Not answered · Test runner"
        );
        assert_eq!(
            summary_line(&qs[2..], &Reply::Answered(Vec::new())),
            "Answered · Question 1",
            "answered elsewhere: the headers; no header: its number"
        );
        assert_eq!(
            summary_parts(&qs[..1], &Reply::Skipped),
            ("Skipped", "Test runner".to_string())
        );
        let long = vec![one(&[&"word ".repeat(60)])];
        let line = summary_line(&qs[..1], &Reply::Answered(long));
        assert_eq!(line.chars().count(), SUMMARY_CHARS);
        assert!(line.ends_with('…'), "{line}");
    }

    /// The designer: `question()` and the banner say the first question, and how many more.
    #[test]
    fn the_question_line_says_how_many_more() {
        let qs = vec![
            question("Test runner", &["Vitest"], false),
            question("CI", &["Lint"], true),
            question("Coverage", &["Yes"], false),
        ];
        assert_eq!(
            asks_line("Claude", &qs),
            "Claude asks: Test runner? (+2 more)"
        );
        assert_eq!(asks_line("Reader", &qs[..1]), "Reader asks: Test runner?");
    }

    /// The designer's dock rule: the copy shows while the in-chat card's top sits lower than
    /// the chat's bottom less the last row's padding and the dock's height (so they swap at the
    /// same pixel); a card not laid out shows it when it is under the view.
    #[test]
    fn the_dock_shows_while_the_card_is_below_its_place() {
        let (bottom, dock) = (600.0, 200.0);
        let line = bottom - crate::ui_tokens::chat::LAST_BOTTOM - dock;
        assert!(
            !dock_shows(Some(line), false, bottom, dock, true),
            "at the pixel: the card"
        );
        assert!(
            dock_shows(Some(line + 1.0), false, bottom, dock, true),
            "lower: the dock"
        );
        assert!(!dock_shows(Some(100.0), false, bottom, dock, true));
        assert!(
            dock_shows(None, true, bottom, dock, true),
            "under the view, not laid out"
        );
        assert!(
            !dock_shows(None, false, bottom, dock, true),
            "above the view: no dock"
        );
        assert!(
            !dock_shows(Some(900.0), true, bottom, dock, false),
            "no layout yet: none"
        );
    }

    #[test]
    fn the_dock_is_full_compact_or_tiny() {
        use crate::ui_tokens::ask::{DOCK_MAX, TINY_DOCK_BELOW};
        assert_eq!(dock_kind(Some(200.0), 600.0), (DockKind::Full, 200.0));
        assert_eq!(
            dock_kind(Some(300.0), 600.0),
            (DockKind::Compact, 270.0),
            "over 45 % of the chat"
        );
        assert_eq!(
            dock_kind(Some(400.0), 1000.0),
            (DockKind::Compact, DOCK_MAX)
        );
        assert_eq!(
            dock_kind(None, 1000.0),
            (DockKind::Compact, DOCK_MAX),
            "not measured"
        );
        assert_eq!(
            dock_kind(Some(100.0), TINY_DOCK_BELOW - 1.0).0,
            DockKind::Tiny
        );
    }

    #[test]
    fn the_oldest_waiting_question_is_answered_first() {
        let t0 = Instant::now();
        let asks = [
            ("b", t0 + Duration::from_secs(2)),
            ("a", t0),
            ("c", t0 + Duration::from_secs(1)),
        ];
        assert_eq!(oldest(&asks), Some("a"));
        assert_eq!(oldest::<&str>(&[]), None);
    }

    #[test]
    fn previews_well_height_is_the_questions_max_capped() {
        let mut q = question("Runner", &["A", "B"], false);
        assert_eq!(preview_lines(&q), 0, "no previews");
        q.options[0].preview = Some("one\ntwo\nthree".into());
        q.options[1].preview = Some("one".into());
        assert_eq!(preview_lines(&q), 3, "the tallest");
        q.options[1].preview = Some("x\n".repeat(40));
        assert_eq!(preview_lines(&q), PREVIEW_MAX_LINES, "capped");
        q.multi = true;
        assert_eq!(preview_lines(&q), 0, "single-select only");
    }
}
