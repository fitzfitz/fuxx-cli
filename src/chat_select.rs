//! Selecting text in an agent tab's chat (chore after M11ar): drag across your messages and the
//! agent's replies (paragraphs, headings, lists, tables, quotes, code), double-click a word,
//! triple-click a paragraph; ⌘C (or Edit → Copy) copies, ⌘A with an empty box selects it all.
//!
//! The chat is drawn with GPUI's `StyledText`, which has no selection of its own. Each piece of
//! chat text is wrapped in a [`SelectableText`]: it paints the selection's wash behind the text
//! and, once painted, registers its layout in the tab's [`ChatSelect`], which the tab's mouse
//! handlers hit-test. A piece is named by a [`Key`] (its chat row, then its order in the row), so
//! a selection holds across frames and across rows scrolled out of sight.
//!
//! A piece's links (wave 3) register with it, so a right-click finds the link under the
//! pointer (`link_at`): `InteractiveText`'s clicks are the left button's only.
//!
//! `Sel`, `selected_text`, `word_at`, `hit`, `nearest`, `wash` and `link_at` are the pure model.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString,
    TextLayout, Window, fill, point, size,
};

/// A piece of chat text: its chat row, then its order among the row's pieces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub row: usize,
    pub piece: usize,
}

/// A place in the chat's text: a piece and a byte index in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub key: Key,
    pub ix: usize,
}

/// A selection: where the drag began (`anchor`) and where it is now (`head`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub anchor: Pos,
    pub head: Pos,
}

impl Sel {
    pub fn new(anchor: Pos, head: Pos) -> Self {
        Self { anchor, head }
    }

    /// Its two ends in reading order.
    pub fn ordered(&self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The bytes of piece `key` (`len` long) it covers, if any.
    pub fn range_in(&self, key: Key, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if self.is_empty() || key < start.key || key > end.key {
            return None;
        }
        let from = if key == start.key {
            start.ix.min(len)
        } else {
            0
        };
        let to = if key == end.key { end.ix.min(len) } else { len };
        (from < to).then_some(from..to)
    }
}

/// The selected text, from the pieces in reading order: a row's pieces one per line, rows apart
/// by a blank line (as the chat shows them).
pub fn selected_text(sel: &Sel, pieces: &[(Key, &str)]) -> String {
    let mut out = String::new();
    let mut last_row = None;
    for (key, text) in pieces {
        let Some(r) = sel.range_in(*key, text.len()) else {
            continue;
        };
        let Some(part) = text.get(r) else {
            continue;
        };
        match last_row {
            Some(row) if row == key.row => out.push('\n'),
            Some(_) => out.push_str("\n\n"),
            None => {}
        }
        out.push_str(part);
        last_row = Some(key.row);
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word around byte `ix` of `text` (letters, digits, `_`); on anything else, that character.
pub fn word_at(text: &str, ix: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }
    let mut ix = ix.min(text.len());
    while !text.is_char_boundary(ix) {
        ix -= 1;
    }
    if ix == text.len() {
        // Past the end: the last character.
        ix = text.char_indices().last().map_or(0, |(i, _)| i);
    }
    let here = text[ix..].chars().next().unwrap_or(' ');
    if !is_word(here) {
        return ix..ix + here.len_utf8();
    }
    let start = text[..ix]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(ix, |(i, _)| i);
    let end = text[ix..]
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map_or(text.len(), |(i, _)| ix + i);
    start..end
}

/// The piece whose box holds `p`.
pub fn hit(boxes: &[(Key, Bounds<Pixels>)], p: Point<Pixels>) -> Option<usize> {
    boxes.iter().position(|(_, b)| b.contains(&p))
}

/// The piece a drag at `p` reaches: the one holding it, else the one on its line nearest
/// across, else the nearest above or below.
pub fn nearest(boxes: &[(Key, Bounds<Pixels>)], p: Point<Pixels>) -> Option<usize> {
    if let Some(i) = hit(boxes, p) {
        return Some(i);
    }
    let across = |b: &Bounds<Pixels>| {
        if p.x < b.left() {
            b.left() - p.x
        } else if p.x > b.right() {
            p.x - b.right()
        } else {
            Pixels::ZERO
        }
    };
    let on_line = boxes
        .iter()
        .enumerate()
        .filter(|(_, (_, b))| p.y >= b.top() && p.y <= b.bottom())
        .min_by(|(_, (_, a)), (_, (_, b))| across(a).partial_cmp(&across(b)).unwrap())
        .map(|(i, _)| i);
    if on_line.is_some() {
        return on_line;
    }
    let up_down = |b: &Bounds<Pixels>| {
        if p.y < b.top() {
            b.top() - p.y
        } else {
            p.y - b.bottom()
        }
    };
    boxes
        .iter()
        .enumerate()
        .min_by(|(_, (_, a)), (_, (_, b))| up_down(a).partial_cmp(&up_down(b)).unwrap())
        .map(|(i, _)| i)
}

/// The wash's boxes for a selection from `start` to `end` (each a line's top-left, from
/// `TextLayout::position_for_index`) in a piece at `bounds` whose lines are `line` tall: one box
/// on a single line; else the first line to the right edge, the full lines between, the last
/// line from the left edge.
pub fn wash(
    start: Point<Pixels>,
    end: Point<Pixels>,
    line: Pixels,
    bounds: Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let row = |x0: Pixels, x1: Pixels, y: Pixels| Bounds::new(point(x0, y), size(x1 - x0, line));
    if (end.y - start.y).abs() < line / 2.0 {
        return vec![row(start.x, end.x, start.y)];
    }
    let mut out = vec![row(start.x, bounds.right(), start.y)];
    let mut y = start.y + line;
    while y + line / 2.0 < end.y {
        out.push(row(bounds.left(), bounds.right(), y));
        y += line;
    }
    out.push(row(bounds.left(), end.x, end.y));
    out
}

/// A piece's links: each one's bytes and its address.
pub type Links = Rc<Vec<(Range<usize>, String)>>;

/// The address of the link at `pos`, among the pieces' `links` (wave 3: the chat's and the
/// Preview's right-click menu).
pub fn link_at(links: &BTreeMap<Key, Links>, pos: Pos) -> Option<&str> {
    crate::md_preview::link_under(links.get(&pos.key)?, pos.ix)
}

/// A painted piece: its key, layout, where it was drawn and its links.
struct Painted {
    key: Key,
    layout: TextLayout,
    bounds: Bounds<Pixels>,
    links: Option<Links>,
}

/// An agent tab's selection and the chat text painted this frame (shared with what draws it).
#[derive(Default)]
pub struct ChatSelect {
    sel: Cell<Option<Sel>>,
    painted: RefCell<Vec<Painted>>,
    /// Every piece's text as last drawn, for a selection reaching rows out of sight.
    texts: RefCell<BTreeMap<Key, SharedString>>,
    /// While building a frame: the row being drawn and its next piece.
    row: Cell<usize>,
    next: Cell<usize>,
}

impl ChatSelect {
    pub fn sel(&self) -> Option<Sel> {
        self.sel.get()
    }

    pub fn set(&self, sel: Option<Sel>) {
        self.sel.set(sel);
    }

    /// A new frame: what was painted is forgotten (drawn again, it registers again).
    pub fn begin_frame(&self) {
        self.painted.borrow_mut().clear();
    }

    /// Row `row` is being drawn: its pieces count from 0, and what it showed before is
    /// forgotten (drawn, it registers again; a reply that shrank leaves nothing stale).
    pub fn begin_row(&self, row: usize) {
        self.row.set(row);
        self.next.set(0);
        self.texts.borrow_mut().retain(|k, _| k.row != row);
    }

    /// The key of the next piece drawn in the current row.
    pub fn next_key(&self) -> Key {
        let piece = self.next.get();
        self.next.set(piece + 1);
        Key {
            row: self.row.get(),
            piece,
        }
    }

    /// A row's text changed or went (a new turn, a `/clear`): forget what it showed.
    pub fn forget_from(&self, row: usize) {
        self.texts.borrow_mut().retain(|k, _| k.row < row);
    }

    fn register(&self, key: Key, layout: TextLayout, text: SharedString, links: Option<Links>) {
        let bounds = layout.bounds();
        let mut painted = self.painted.borrow_mut();
        painted.retain(|p| p.key != key);
        painted.push(Painted {
            key,
            layout,
            bounds,
            links,
        });
        self.texts.borrow_mut().insert(key, text);
    }

    /// The place under `p`: on text (`exact`, a press), or the nearest text (a drag).
    pub fn pos_at(&self, p: Point<Pixels>, exact: bool) -> Option<Pos> {
        let painted = self.painted.borrow();
        let boxes: Vec<(Key, Bounds<Pixels>)> = painted.iter().map(|q| (q.key, q.bounds)).collect();
        let i = if exact {
            hit(&boxes, p)
        } else {
            nearest(&boxes, p)
        }?;
        let piece = &painted[i];
        let ix = match piece.layout.index_for_position(p) {
            Ok(ix) | Err(ix) => ix,
        };
        Some(Pos { key: piece.key, ix })
    }

    /// The address of the link right under `p` (on text painted last frame), if any.
    pub fn link_at(&self, p: Point<Pixels>) -> Option<String> {
        let pos = self.pos_at(p, true)?;
        let painted = self.painted.borrow();
        let links: BTreeMap<Key, Links> = painted
            .iter()
            .filter(|q| q.key == pos.key)
            .filter_map(|q| Some((q.key, q.links.clone()?)))
            .collect();
        link_at(&links, pos).map(str::to_string)
    }

    /// The pieces painted last frame and where (for the ui-shots scenes).
    #[cfg(feature = "ui-shots")]
    pub fn painted_boxes(&self) -> Vec<(Key, Bounds<Pixels>)> {
        self.painted
            .borrow()
            .iter()
            .map(|p| (p.key, p.bounds))
            .collect()
    }

    /// The text of the piece at `key`, as last drawn.
    pub fn text_of(&self, key: Key) -> Option<SharedString> {
        self.texts.borrow().get(&key).cloned()
    }

    /// The selected text: every piece drawn so far in reading order (rows between that never
    /// came into sight are left out).
    pub fn copied(&self) -> Option<String> {
        let sel = self.sel.get().filter(|s| !s.is_empty())?;
        let texts = self.texts.borrow();
        let pieces: Vec<(Key, &str)> = texts.iter().map(|(k, t)| (*k, t.as_ref())).collect();
        Some(selected_text(&sel, &pieces)).filter(|t| !t.is_empty())
    }

    /// The whole chat as far as it has been drawn.
    pub fn select_all(&self) {
        let texts = self.texts.borrow();
        if let (Some((first, _)), Some((last, t))) =
            (texts.first_key_value(), texts.last_key_value())
        {
            let sel = Sel::new(
                Pos { key: *first, ix: 0 },
                Pos {
                    key: *last,
                    ix: t.len(),
                },
            );
            drop(texts);
            self.sel.set(Some(sel));
        }
    }
}

/// A piece of chat text that can be selected: `child` draws `text` with `layout` (a
/// `StyledText`'s, maybe inside an `InteractiveText`); this paints the selection's wash under it
/// and registers it once painted.
pub struct SelectableText {
    key: Key,
    text: SharedString,
    layout: TextLayout,
    child: AnyElement,
    state: Rc<ChatSelect>,
    wash: Hsla,
    links: Option<Links>,
}

impl SelectableText {
    pub fn new(
        state: Rc<ChatSelect>,
        text: SharedString,
        layout: TextLayout,
        child: AnyElement,
        wash: Hsla,
    ) -> Self {
        let key = state.next_key();
        Self {
            key,
            text,
            layout,
            child,
            state,
            wash,
            links: None,
        }
    }

    /// Its links (each one's bytes and address), for the right-click menu.
    pub fn links(mut self, links: Links) -> Self {
        self.links = Some(links);
        self
    }
}

impl IntoElement for SelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableText {
    type RequestLayoutState = ();
    /// Its box, for the text cursor (it blocks nothing).
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // The text cursor over it; a link's pointer (set as the child paints) still wins.
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        let range = self
            .state
            .sel()
            .and_then(|s| s.range_in(self.key, self.layout.len()));
        if let Some(r) = range
            && let (Some(a), Some(b)) = (
                self.layout.position_for_index(r.start),
                self.layout.position_for_index(r.end),
            )
        {
            for q in wash(a, b, self.layout.line_height(), self.layout.bounds()) {
                window.paint_quad(fill(q, self.wash));
            }
        }
        self.child.paint(window, cx);
        self.state.register(
            self.key,
            self.layout.clone(),
            self.text.clone(),
            self.links.clone(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    fn k(row: usize, piece: usize) -> Key {
        Key { row, piece }
    }
    fn at(row: usize, piece: usize, ix: usize) -> Pos {
        Pos {
            key: k(row, piece),
            ix,
        }
    }
    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn a_selection_reads_in_order_whichever_way_it_was_dragged() {
        let down = Sel::new(at(1, 0, 4), at(3, 2, 1));
        let up = Sel::new(at(3, 2, 1), at(1, 0, 4));
        assert_eq!(down.ordered(), (at(1, 0, 4), at(3, 2, 1)));
        assert_eq!(up.ordered(), down.ordered());
        assert!(Sel::new(at(2, 0, 3), at(2, 0, 3)).is_empty());
        assert!(!down.is_empty());
    }

    #[test]
    fn each_piece_gets_its_part_of_the_selection() {
        let sel = Sel::new(at(2, 1, 6), at(4, 0, 3));
        assert_eq!(sel.range_in(k(2, 0), 20), None, "before it");
        assert_eq!(
            sel.range_in(k(2, 1), 20),
            Some(6..20),
            "its first piece, from the anchor"
        );
        assert_eq!(
            sel.range_in(k(3, 5), 9),
            Some(0..9),
            "a piece between, whole"
        );
        assert_eq!(
            sel.range_in(k(4, 0), 20),
            Some(0..3),
            "its last piece, to the head"
        );
        assert_eq!(sel.range_in(k(4, 1), 20), None, "after it");
        // Within one piece, dragged backwards; an index past a shorter text is held to it.
        let back = Sel::new(at(5, 0, 30), at(5, 0, 2));
        assert_eq!(back.range_in(k(5, 0), 10), Some(2..10));
        assert_eq!(
            Sel::new(at(5, 0, 2), at(5, 0, 2)).range_in(k(5, 0), 10),
            None
        );
    }

    #[test]
    fn the_copied_text_keeps_lines_and_paragraphs() {
        let pieces = [
            (k(0, 0), "Explain the fix"),
            (k(1, 0), "What changed"),
            (k(1, 1), "The test expects past dates."),
            (k(1, 2), "Parse dates first"),
            (k(2, 0), "Thanks"),
        ];
        let sel = Sel::new(at(0, 0, 8), at(1, 2, 5));
        assert_eq!(
            selected_text(&sel, &pieces),
            "the fix\n\nWhat changed\nThe test expects past dates.\nParse"
        );
        let one = Sel::new(at(1, 1, 9), at(1, 1, 4));
        assert_eq!(selected_text(&one, &pieces), "test ");
    }

    #[test]
    fn a_double_click_takes_the_word() {
        let t = "call is_past_date(now), then 42.";
        assert_eq!(&t[word_at(t, 7)], "is_past_date");
        assert_eq!(&t[word_at(t, 0)], "call");
        assert_eq!(&t[word_at(t, 30)], "42");
        assert_eq!(&t[word_at(t, 17)], "(", "punctuation alone");
        assert_eq!(word_at("", 0), 0..0);
        // Past the end, or inside a multi-byte letter, still a word.
        assert_eq!(&t[word_at(t, 99)], ".");
        let u = "café au lait";
        assert_eq!(&u[word_at(u, 4)], "café");
    }

    #[test]
    fn a_press_hits_only_text_a_drag_reaches_the_nearest() {
        let boxes = [
            (k(0, 0), b(100.0, 10.0, 300.0, 20.0)),
            (k(1, 0), b(20.0, 60.0, 500.0, 40.0)),
            (k(1, 1), b(20.0, 110.0, 200.0, 20.0)),
        ];
        assert_eq!(hit(&boxes, point(px(150.0), px(20.0))), Some(0));
        assert_eq!(hit(&boxes, point(px(30.0), px(120.0))), Some(2));
        assert_eq!(
            hit(&boxes, point(px(50.0), px(20.0))),
            None,
            "left of a bubble"
        );
        assert_eq!(
            hit(&boxes, point(px(300.0), px(45.0))),
            None,
            "between rows"
        );
        // A drag: across on its line, else the nearest row above or below.
        assert_eq!(nearest(&boxes, point(px(50.0), px(20.0))), Some(0));
        assert_eq!(nearest(&boxes, point(px(600.0), px(120.0))), Some(2));
        assert_eq!(
            nearest(&boxes, point(px(300.0), px(52.0))),
            Some(1),
            "just above row 1"
        );
        assert_eq!(
            nearest(&boxes, point(px(300.0), px(-40.0))),
            Some(0),
            "above it all"
        );
        assert_eq!(
            nearest(&boxes, point(px(300.0), px(900.0))),
            Some(2),
            "below it all"
        );
        assert_eq!(nearest(&[], point(px(0.0), px(0.0))), None);
    }

    #[test]
    fn link_at_finds_the_link_under_a_place() {
        let mut links: BTreeMap<Key, Links> = BTreeMap::new();
        links.insert(
            k(2, 1),
            Rc::new(vec![
                (4..12, "https://a.dev/docs".to_string()),
                (20..25, "src/main.rs".to_string()),
            ]),
        );
        assert_eq!(link_at(&links, at(2, 1, 4)), Some("https://a.dev/docs"));
        assert_eq!(link_at(&links, at(2, 1, 11)), Some("https://a.dev/docs"));
        assert_eq!(link_at(&links, at(2, 1, 12)), None, "just past it");
        assert_eq!(link_at(&links, at(2, 1, 22)), Some("src/main.rs"));
        assert_eq!(link_at(&links, at(2, 0, 5)), None, "another piece");
        assert_eq!(link_at(&links, at(3, 1, 5)), None, "a piece with no links");
    }

    #[test]
    fn the_wash_covers_the_selected_lines() {
        let piece = b(10.0, 100.0, 400.0, 60.0);
        let line = px(20.0);
        // One line.
        assert_eq!(
            wash(
                point(px(50.0), px(100.0)),
                point(px(90.0), px(100.0)),
                line,
                piece
            ),
            vec![b(50.0, 100.0, 40.0, 20.0)]
        );
        // Three lines: the first to the right edge, the middle whole, the last from the left.
        assert_eq!(
            wash(
                point(px(50.0), px(100.0)),
                point(px(70.0), px(140.0)),
                line,
                piece
            ),
            vec![
                b(50.0, 100.0, 360.0, 20.0),
                b(10.0, 120.0, 400.0, 20.0),
                b(10.0, 140.0, 60.0, 20.0),
            ]
        );
    }
}
