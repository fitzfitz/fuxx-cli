//! M11ag: Ask the agent from a file tab's selection: the bar under a finished selection
//! ("Ask claude ⌘↵", "Copy with path"), where it sits, and what its buttons do.

use super::*;
use crate::code_ask::{self, CodeRef};
use gpui::Anchor;

/// The gap between the selection's row and the bar (above or below it, or off the text's bottom
/// edge when it has to sit inside).
pub(super) const BAR_GAP: f32 = 6.0;
/// The words of the bar's buttons: Ask names its agent (`ask_words`).
const TOO_LONG: &str = "Too long to ask";
const COPY_WITH_PATH: &str = "Copy with path";
/// The space between the two buttons.
const BUTTONS_GAP: f32 = 6.0;

/// Where the bar's top edge goes, in points down the text area (`view_h` tall): `first_top` and
/// `last_bottom` are the selection's first row's top and last row's bottom, in the same terms
/// (a row scrolled off the top has a negative top). Just below the last row when it is on
/// screen and the bar fits under it; else above the first row when it fits there; else inside,
/// `gap` off the bottom. None when the last row is off screen (nothing to hang the bar on).
pub fn sel_bar_y(
    first_top: f32,
    last_bottom: f32,
    view_h: f32,
    bar_h: f32,
    gap: f32,
) -> Option<f32> {
    if last_bottom <= 0.0 || last_bottom > view_h {
        return None;
    }
    let below = last_bottom + gap;
    if below + bar_h <= view_h {
        return Some(below);
    }
    let above = first_top - gap - bar_h;
    if above >= 0.0 {
        return Some(above);
    }
    Some(view_h - gap - bar_h)
}

/// The bar's Ask button for `to`: the agent it names and the key; none: Ask isn't offered.
pub fn ask_words(to: AskTo) -> Option<String> {
    match to {
        AskTo::Agent(name) => Some(format!("Ask {name} \u{2318}\u{21b5}")),
        AskTo::CopyOnly => None,
    }
}

/// The bar's parts, whose drawn places the scenes read.
#[derive(Clone, Copy)]
enum Part {
    Bar,
    Ask,
    Copy,
}

/// Where the bar and its two buttons were drawn this frame (the scenes read them).
#[cfg(feature = "ui-shots")]
#[derive(Clone, Copy, Default)]
pub(super) struct BarAt {
    bar: Option<Bounds<Pixels>>,
    ask: Option<Bounds<Pixels>>,
    copy: Option<Bounds<Pixels>>,
}

impl FileTab {
    /// The selection as code to ask about or copy (File mode; Diff mode's comes with its own
    /// task); None with no selection.
    pub(super) fn code_ref(&self) -> Option<CodeRef> {
        if self.mode != FileMode::Plain || self.preview_shown() {
            return None;
        }
        let (a, b) = self.selection?;
        let real = std::fs::canonicalize(&self.path)
            .ok()
            .filter(|real| *real != self.path);
        code_ask::file_ref(&self.path, real.as_deref(), self.shown_doc()?, a, b)
    }

    /// Whether the bar is up: a finished (not dragging), non-empty selection in the text, not the
    /// Preview.
    pub(super) fn bar_shown(&self) -> bool {
        self.selection.is_some_and(|(a, b)| a != b)
            && !self.selecting
            && !self.preview_shown()
            && self.shown_doc().is_some()
    }

    /// Past the Ask cap, worked out from the line lengths (no copy of the text), stopping once
    /// past it.
    fn too_long_to_ask(&self) -> bool {
        let (Some(doc), Some((a, b))) = (self.shown_doc(), self.selection) else {
            return false;
        };
        if self.mode != FileMode::Plain {
            return false;
        }
        let (first, last) = code_ask::whole_lines(a, b, doc.lines.len().saturating_sub(1));
        let lines = last - first + 1;
        let mut bytes = lines - 1;
        for i in first..=last {
            if code_ask::too_long_for(lines, bytes) {
                break;
            }
            bytes += doc.line(i).len();
        }
        code_ask::too_long_for(lines, bytes)
    }

    /// The window point of the bar's top-right corner, from the last drawing (geometry and row
    /// map) and the scroll now; none when the bar isn't up or the selection's last row is off
    /// screen. Reads state only.
    pub(super) fn sel_bar_at(&self) -> Option<Point<Pixels>> {
        if !self.bar_shown() {
            return None;
        }
        let (a, b) = self.selection?;
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let g = self.geom.get()?;
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        // A folded selection's hidden ends: the bar hangs on the rows that are shown.
        let first = (lo.line..=hi.line).find_map(|l| map.first_row(l))?;
        let last_line = (lo.line..=hi.line)
            .rev()
            .find(|l| map.first_row(*l).is_some())?;
        let mut last = map.first_row(last_line)?;
        // A wrapped line's other rows.
        while map
            .row(last + 1)
            .is_some_and(|r| r.line() == Some(last_line))
        {
            last += 1;
        }
        let scroll_y = self.mode_scroll().y.clamp(px(0.0), g.max_scroll.y);
        let scroll_y = f32::from(scroll_y);
        let y = sel_bar_y(
            map.top(first) - scroll_y,
            map.top(last + 1) - scroll_y,
            f32::from(g.bounds.size.height),
            BUTTON,
            BAR_GAP,
        )?;
        Some(point(g.bounds.right(), g.bounds.top() + px(y)))
    }

    /// Ask (the button, or ⌘↵): the selection goes to the workspace as a chip for an agent tab's
    /// box. Nothing without a selection, in a workspace with no agent tabs to ask (an SSH one),
    /// or past the cap.
    pub(super) fn ask(&mut self, cx: &mut Context<Self>) {
        if self.ask_to == AskTo::CopyOnly {
            return;
        }
        if let Some(code) = self.code_ref().filter(|code| !code.too_long()) {
            cx.emit(FileTabEvent::Ask(code));
        }
    }

    /// Copy with path: the text Ask would give the agent, with the path as M11ai's heading link
    /// has it (repo-relative, else under home, else whole). No cap.
    pub(super) fn copy_with_path(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code_ref() else {
            return;
        };
        let rel = self.git_place.as_ref().map(|p| p.rel.as_path());
        let home = crate::workspace::home_dir();
        let text = code_ask::copy_text(&code, rel, home.as_deref());
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Esc with no find bar and no outline panel to close takes a selection (so the bar goes);
    /// with none it goes on.
    pub(super) fn esc_takes_selection(&self) -> bool {
        self.selection.is_some_and(|(a, b)| a != b) && !self.preview_shown()
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        cx.notify();
    }

    /// The bar, an overlay at the window point `sel_bar_at` gives (anchored by its top-right
    /// corner, so the code doesn't move): "Ask claude ⌘↵" (or "Too long to ask", faint and
    /// doing nothing) when there is an agent to ask, and "Copy with path". A press on it never
    /// reaches the text (no new selection).
    pub(super) fn ask_bar(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        #[cfg(feature = "ui-shots")]
        self.bar_at.set(BarAt::default());
        let at = self.sel_bar_at()?;
        let long = self.too_long_to_ask();
        let ask = ask_words(self.ask_to).map(|words| {
            let button = if long {
                // The same button, faint: its click does nothing.
                div()
                    .id("file-ask")
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(BUTTON))
                    .px(px(10.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(hsla(ui.panel_border))
                    .bg(hsla(ui.panel))
                    .text_size(px(text::NAME))
                    .text_color(hsla(ui.text_dim))
                    .child(TOO_LONG)
            } else {
                Self::text_button("file-ask", words, ui, |this, _, cx| this.ask(cx), cx)
            };
            self.mark_bar_part(button, Part::Ask)
        });
        let copy = Self::text_button(
            "file-copy-with-path",
            COPY_WITH_PATH,
            ui,
            |this, _, cx| this.copy_with_path(cx),
            cx,
        );
        let copy = self.mark_bar_part(copy, Part::Copy);
        let bar = div()
            .flex()
            .flex_row()
            .gap(px(BUTTONS_GAP))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(ask)
            .child(copy);
        let bar = self.mark_bar_part(bar, Part::Bar);
        Some(
            deferred(anchored().position(at).anchor(Anchor::TopRight).child(bar))
                .into_any_element(),
        )
    }

    /// `part` with a canvas over it that notes where it was drawn (the scenes' probe).
    #[cfg(feature = "ui-shots")]
    fn mark_bar_part<E: ParentElement>(&self, part: E, which: Part) -> E {
        let cell = self.bar_at.clone();
        part.child(
            canvas(
                move |b, _, _| {
                    let mut at = cell.get();
                    match which {
                        Part::Bar => at.bar = Some(b),
                        Part::Ask => at.ask = Some(b),
                        Part::Copy => at.copy = Some(b),
                    }
                    cell.set(at);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    #[cfg(not(feature = "ui-shots"))]
    fn mark_bar_part<E>(&self, part: E, _which: Part) -> E {
        part
    }
}

/// What the UI screenshots read of the bar.
#[cfg(feature = "ui-shots")]
impl FileTab {
    /// The bar as last drawn (none: not drawn).
    pub fn sel_bar_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bar_at.get().bar
    }

    /// The Ask button's words and whether it is enabled (none: no Ask button).
    pub fn ask_button(&self) -> Option<(String, bool)> {
        self.bar_at.get().ask?;
        let long = self.too_long_to_ask();
        let words = if long {
            TOO_LONG.to_string()
        } else {
            ask_words(self.ask_to)?
        };
        Some((words, !long))
    }

    pub fn ask_button_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bar_at.get().ask
    }

    pub fn copy_button_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bar_at.get().copy
    }

    /// Select `from` to `to` (each `(line, col)`, 0-based) as a finished drag would.
    pub fn select_for_shots(
        &mut self,
        from: (usize, usize),
        to: (usize, usize),
        cx: &mut Context<Self>,
    ) {
        let pos = |(line, col)| Pos { line, col };
        self.selection = Some((pos(from), pos(to)));
        self.selecting = false;
        cx.notify();
    }

    /// Whether Esc would take the selection now (else it goes on).
    pub fn esc_takes_selection_for_shots(&self) -> bool {
        self.esc_takes_selection()
    }

    /// A made-up repo place for the file, `rel` inside a repo whose root is the file's path less
    /// `rel` (git is never run), so Copy with path reads as it does in a repo.
    pub fn pretend_rel_for_shots(&mut self, rel: &str, cx: &mut Context<Self>) {
        let mut root = self.path.clone();
        for _ in Path::new(rel).components() {
            root.pop();
        }
        self.git_place = Some(GitPlace {
            git: PathBuf::from("git"),
            root,
            rel: PathBuf::from(rel),
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sel_bar_y_below_above_inside_and_hidden() {
        let y = |first: f32, last: f32| sel_bar_y(first, last, 400.0, 28.0, 6.0);
        // Room below the last row: the gap under it.
        assert_eq!(y(80.0, 100.0), Some(106.0));
        // No room below: above the first row, a gap over it.
        assert_eq!(y(200.0, 390.0), Some(166.0));
        // No room above either: inside, a gap off the bottom.
        assert_eq!(y(10.0, 395.0), Some(366.0));
        // The last row off the bottom or the top: hidden.
        assert_eq!(y(10.0, 420.0), None);
        assert_eq!(y(-60.0, 0.0), None);
        assert_eq!(y(-60.0, -20.0), None);
    }

    #[test]
    fn ask_names_its_agent_or_is_not_offered() {
        assert_eq!(
            ask_words(AskTo::Agent("claude")).as_deref(),
            Some("Ask claude \u{2318}\u{21b5}")
        );
        assert_eq!(
            ask_words(AskTo::Agent("agy")).as_deref(),
            Some("Ask agy \u{2318}\u{21b5}")
        );
        assert_eq!(ask_words(AskTo::CopyOnly), None);
    }
}
