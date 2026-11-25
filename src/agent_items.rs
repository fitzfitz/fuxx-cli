//! The conversation's items as one store (the agent tab's perf chore, 2026-11-24): read as a
//! slice, changed only through methods that note what changed. Each item carries a version
//! (bumped by every change to it), the store keeps the lowest index changed since the tab last
//! looked (`take_dirty`), and a few indexes kept as items change, so nothing scans the whole
//! conversation per event or per frame: the running tool calls, the open permissions, the tool
//! ids, and how many items hang under each parent.

use std::collections::{BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};

use crate::agent_conversation::Item;
use crate::agent_tool_modal::parent_of;

#[derive(Clone, Default)]
pub struct Items {
    list: Vec<Item>,
    /// Each item's version: unique over the store's life (a new item at an old index, or one
    /// restored, never takes an old one).
    versions: Vec<u64>,
    clock: u64,
    /// The lowest index whose item changed, came or went since `take_dirty` (None: none).
    dirty: Option<usize>,
    running: BTreeSet<usize>,
    permissions: BTreeSet<usize>,
    /// Each tool id's calls (a replay may repeat one).
    tools: HashMap<String, BTreeSet<usize>>,
    /// Items under each parent (`parent_of`: a lane's words, steps and resumes), and how many
    /// of them are tool calls (a lane's steps).
    under: HashMap<String, BTreeSet<usize>>,
    steps: HashMap<String, usize>,
    /// Your messages relayed to a lane (`Item::You { to: Some(_) }`).
    relays: BTreeSet<usize>,
    /// The permissions and questions each lane asked (`by`).
    asked_by: HashMap<String, BTreeSet<usize>>,
    /// Each message's block still streaming (`Item::Agent { done: false }`), by its message.
    open_texts: HashMap<String, usize>,
}

impl Deref for Items {
    type Target = [Item];
    fn deref(&self) -> &[Item] {
        &self.list
    }
}

/// One item open for change: its version was bumped as it opened, and the store's indexes take
/// it again when it closes (whatever it became).
pub struct ItemMut<'a> {
    items: &'a mut Items,
    ix: usize,
}

impl Deref for ItemMut<'_> {
    type Target = Item;
    fn deref(&self) -> &Item {
        &self.items.list[self.ix]
    }
}

impl DerefMut for ItemMut<'_> {
    fn deref_mut(&mut self) -> &mut Item {
        &mut self.items.list[self.ix]
    }
}

impl Drop for ItemMut<'_> {
    fn drop(&mut self) {
        self.items.index(self.ix, 1);
    }
}

fn count(map: &mut HashMap<String, usize>, key: &str, by: isize) {
    if by > 0 {
        *map.entry(key.to_string()).or_insert(0) += 1;
    } else if let Some(n) = map.get_mut(key) {
        *n -= 1;
        if *n == 0 {
            map.remove(key);
        }
    }
}

impl Items {
    fn mark(&mut self, ix: usize) {
        self.dirty = Some(self.dirty.map_or(ix, |d| d.min(ix)));
    }

    fn stamp(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Item `ix` into the indexes (`by` 1) or out of them (-1).
    fn index(&mut self, ix: usize, by: isize) {
        let item = &self.list[ix];
        let set = |s: &mut BTreeSet<usize>, on: bool| {
            if on && by > 0 {
                s.insert(ix);
            } else {
                s.remove(&ix);
            }
        };
        let running = matches!(item, Item::Tool { running: true, .. });
        let open = matches!(item, Item::Permission { answer: None, .. });
        let tool = match item {
            Item::Tool { id, parent, .. } => Some((id.clone(), parent.clone())),
            _ => None,
        };
        let parent = parent_of(item).map(str::to_string);
        let relay = matches!(item, Item::You { to: Some(_), .. });
        let asker = match item {
            Item::Permission { by: Some(b), .. } | Item::Question { by: Some(b), .. } => {
                Some(b.clone())
            }
            _ => None,
        };
        if let Some(b) = asker {
            let asked = self.asked_by.entry(b.clone()).or_default();
            if by > 0 {
                asked.insert(ix);
            } else {
                asked.remove(&ix);
                if asked.is_empty() {
                    self.asked_by.remove(&b);
                }
            }
        }
        let text = match item {
            Item::Agent {
                message,
                done: false,
                ..
            } => Some(message.clone()),
            _ => None,
        };
        set(&mut self.running, running);
        set(&mut self.permissions, open);
        set(&mut self.relays, relay);
        if let Some(m) = text {
            if by > 0 {
                self.open_texts.insert(m, ix);
            } else if self.open_texts.get(&m) == Some(&ix) {
                self.open_texts.remove(&m);
            }
        }
        if let Some((id, parent)) = tool {
            let calls = self.tools.entry(id.clone()).or_default();
            if by > 0 {
                calls.insert(ix);
            } else {
                calls.remove(&ix);
                if calls.is_empty() {
                    self.tools.remove(&id);
                }
            }
            if let Some(p) = parent {
                count(&mut self.steps, &p, by);
            }
        }
        if let Some(p) = parent {
            let under = self.under.entry(p.clone()).or_default();
            if by > 0 {
                under.insert(ix);
            } else {
                under.remove(&ix);
                if under.is_empty() {
                    self.under.remove(&p);
                }
            }
        }
    }

    /// Every index built again (after items moved: rare).
    fn reindex(&mut self) {
        self.running.clear();
        self.permissions.clear();
        self.tools.clear();
        self.under.clear();
        self.steps.clear();
        self.relays.clear();
        self.asked_by.clear();
        self.open_texts.clear();
        for ix in 0..self.list.len() {
            self.index(ix, 1);
        }
    }

    pub fn push(&mut self, item: Item) {
        let ix = self.list.len();
        let v = self.stamp();
        self.list.push(item);
        self.versions.push(v);
        self.mark(ix);
        self.index(ix, 1);
    }

    /// Item `ix`, to change. A tool call keeps its id and parent (nothing changes them).
    pub fn get_mut(&mut self, ix: usize) -> ItemMut<'_> {
        let v = self.stamp();
        self.versions[ix] = v;
        self.mark(ix);
        self.index(ix, -1);
        ItemMut { items: self, ix }
    }

    /// Each item `pick` picks, changed by `change` (only those are marked).
    pub fn change_where(
        &mut self,
        pick: impl Fn(&Item) -> bool,
        mut change: impl FnMut(&mut Item),
    ) {
        for ix in 0..self.list.len() {
            if pick(&self.list[ix]) {
                change(&mut self.get_mut(ix));
            }
        }
    }

    pub fn truncate(&mut self, len: usize) {
        if len < self.list.len() {
            self.list.truncate(len);
            self.versions.truncate(len);
            self.mark(len);
            self.reindex();
        }
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }

    pub fn remove(&mut self, ix: usize) -> Item {
        let item = self.list.remove(ix);
        self.versions.remove(ix);
        self.mark(ix);
        self.reindex();
        item
    }

    pub fn insert(&mut self, ix: usize, item: Item) {
        let v = self.stamp();
        self.list.insert(ix, item);
        self.versions.insert(ix, v);
        self.mark(ix);
        self.reindex();
    }

    pub fn split_off(&mut self, at: usize) -> Vec<Item> {
        let cut = self.list.split_off(at);
        self.versions.truncate(at);
        self.mark(at);
        self.reindex();
        cut
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Item>) {
        for item in items {
            self.push(item);
        }
    }

    /// Item `ix`'s version.
    pub fn version(&self, ix: usize) -> u64 {
        self.versions[ix]
    }

    /// The lowest index changed since the last call (None: nothing), and nothing after it.
    pub fn take_dirty(&mut self) -> Option<usize> {
        self.dirty.take()
    }

    /// The running tool calls' indexes, in order.
    pub fn running(&self) -> &BTreeSet<usize> {
        &self.running
    }

    /// The open permissions' indexes, in order.
    pub fn open_permissions(&self) -> &BTreeSet<usize> {
        &self.permissions
    }

    /// There is a tool call `id`.
    pub fn has_tool(&self, id: &str) -> bool {
        self.tools.contains_key(id)
    }

    /// Tool call `id`'s index (its first, if a replay repeated it).
    pub fn tool_index(&self, id: &str) -> Option<usize> {
        self.tools.get(id)?.first().copied()
    }

    /// How many items hang under `parent` (`parent_of`).
    pub fn kids(&self, parent: &str) -> usize {
        self.under.get(parent).map_or(0, BTreeSet::len)
    }

    /// The items under `parent` (`parent_of`), in order.
    pub fn under(&self, parent: &str) -> Option<&BTreeSet<usize>> {
        self.under.get(parent)
    }

    /// The permissions and questions lane `lane` asked, in order.
    pub fn asked_by(&self, lane: &str) -> Option<&BTreeSet<usize>> {
        self.asked_by.get(lane)
    }

    /// Your messages relayed to a lane, in order.
    pub fn relays(&self) -> &BTreeSet<usize> {
        &self.relays
    }

    /// Message `message`'s block still streaming, if any.
    pub fn open_text(&self, message: &str) -> Option<usize> {
        self.open_texts.get(message).copied()
    }

    /// How many tool calls hang under `parent`.
    pub fn steps(&self, parent: &str) -> usize {
        self.steps.get(parent).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::Answer;

    fn tool(id: &str, parent: Option<&str>, running: bool) -> Item {
        Item::Tool {
            id: id.into(),
            name: "Bash".into(),
            summary: String::new(),
            parent: parent.map(String::from),
            running,
            ok: None,
            result: String::new(),
            detail: None,
            output: None,
            dropped_lines: 0,
            activity: None,
        }
    }

    fn asking(id: &str) -> Item {
        Item::Permission {
            id: id.into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            answer: None,
            by: None,
            plan: None,
        }
    }

    /// Running calls, open permissions, items and calls under "a", the items under it, the
    /// relays, and message "m"'s open block.
    type Indexes = (
        Vec<usize>,
        Vec<usize>,
        usize,
        usize,
        Vec<usize>,
        Vec<usize>,
        Option<usize>,
    );

    /// The indexes as a full scan finds them.
    fn scan(items: &Items) -> Indexes {
        let running = items
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i, Item::Tool { running: true, .. }))
            .map(|(ix, _)| ix)
            .collect();
        let open = items
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i, Item::Permission { answer: None, .. }))
            .map(|(ix, _)| ix)
            .collect();
        let kids = items.iter().filter(|i| parent_of(i) == Some("a")).count();
        let steps = items
            .iter()
            .filter(|i| matches!(i, Item::Tool { parent: Some(p), .. } if p == "a"))
            .count();
        let at = |f: &dyn Fn(&Item) -> bool| -> Vec<usize> {
            (0..items.len()).filter(|&ix| f(&items[ix])).collect()
        };
        let under = at(&|i| parent_of(i) == Some("a"));
        let relays = at(&|i| matches!(i, Item::You { to: Some(_), .. }));
        let text = items
            .iter()
            .rposition(|i| matches!(i, Item::Agent { message, done: false, .. } if message == "m"));
        (running, open, kids, steps, under, relays, text)
    }

    fn indexed(items: &Items) -> Indexes {
        (
            items.running().iter().copied().collect(),
            items.open_permissions().iter().copied().collect(),
            items.kids("a"),
            items.steps("a"),
            items
                .under("a")
                .map(|u| u.iter().copied().collect())
                .unwrap_or_default(),
            items.relays().iter().copied().collect(),
            items.open_text("m"),
        )
    }

    #[test]
    fn a_change_bumps_its_version_and_marks_the_lowest_index() {
        let mut items = Items::default();
        items.push(tool("a", None, true));
        items.push(Item::Notice("n".into()));
        assert_eq!(items.take_dirty(), Some(0));
        assert_eq!(items.take_dirty(), None, "taken");
        let (v0, v1) = (items.version(0), items.version(1));
        if let Item::Notice(t) = &mut *items.get_mut(1) {
            t.push('!');
        }
        assert_ne!(items.version(1), v1);
        assert_eq!(items.version(0), v0, "the other one keeps its version");
        assert_eq!(items.take_dirty(), Some(1));
        items.truncate(1);
        items.push(Item::Notice("n".into()));
        assert_ne!(
            items.version(1),
            v1,
            "a new item at an old index: a new version"
        );
        assert_eq!(items.take_dirty(), Some(1));
    }

    #[test]
    fn the_indexes_follow_every_change() {
        let mut items = Items::default();
        items.push(tool("a", None, true));
        items.push(tool("s1", Some("a"), true));
        items.push(asking("p"));
        items.push(Item::Agent {
            message: "m".into(),
            text: "t".into(),
            done: false,
            parent: Some("a".into()),
        });
        assert_eq!(indexed(&items), scan(&items));
        assert!(items.has_tool("s1") && !items.has_tool("p"));
        if let Item::Tool { running, .. } = &mut *items.get_mut(1) {
            *running = false;
        }
        if let Item::Permission { answer, .. } = &mut *items.get_mut(2) {
            *answer = Some(Answer::Allowed);
        }
        assert_eq!(indexed(&items), scan(&items));
        items.push(Item::You {
            text: "pass it on".into(),
            attachments: Vec::new(),
            id: None,
            taken: false,
            to: Some(crate::agent_conversation::Relay {
                lane: "a".into(),
                about: false,
                state: crate::agent_conversation::RelayState::Sent,
                settled_at: None,
            }),
        });
        assert_eq!(indexed(&items), scan(&items));
        if let Item::Agent { done, .. } = &mut *items.get_mut(3) {
            *done = true;
        }
        assert_eq!(indexed(&items), scan(&items), "a finished block isn't open");
        items.insert(0, asking("q"));
        assert_eq!(indexed(&items), scan(&items));
        items.remove(1);
        assert_eq!(indexed(&items), scan(&items));
        assert!(!items.has_tool("a"));
        let cut = items.split_off(1);
        assert_eq!(indexed(&items), scan(&items));
        items.extend(cut);
        assert_eq!(indexed(&items), scan(&items));
        items.clear();
        assert_eq!(indexed(&items), scan(&items));
        assert_eq!(items.take_dirty(), Some(0));
    }
}
