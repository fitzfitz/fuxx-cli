//! The ordered list of sessions and which one is active. Generic and GPUI-free, so the
//! switching / closing rules are unit-tested.

pub struct SessionList<T> {
    items: Vec<T>,
    active: Option<usize>,
}

impl<T> SessionList<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active: None,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&T> {
        self.active.map(|i| &self.items[i])
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    pub fn position(&self, pred: impl Fn(&T) -> bool) -> Option<usize> {
        self.items.iter().position(pred)
    }

    /// Append and make it active (a new session is where you want to type).
    #[allow(dead_code)] // tests only; sessions are added with `insert`
    pub fn push(&mut self, item: T) -> usize {
        self.items.push(item);
        let index = self.items.len() - 1;
        self.active = Some(index);
        index
    }

    /// Insert at `index` (clamped to the end) and make it active.
    pub fn insert(&mut self, index: usize, item: T) -> usize {
        let index = index.min(self.items.len());
        self.items.insert(index, item);
        self.active = Some(index);
        index
    }

    /// Reorder by `key` (stable); the same item stays active.
    pub fn sort_by_key<K: Ord>(&mut self, key: impl Fn(&T) -> K) {
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        order.sort_by_key(|&i| key(&self.items[i]));
        self.active = self.active.and_then(|a| order.iter().position(|&i| i == a));
        let mut old: Vec<Option<T>> = std::mem::take(&mut self.items)
            .into_iter()
            .map(Some)
            .collect();
        self.items = order.into_iter().filter_map(|i| old[i].take()).collect();
    }

    /// Returns false (and changes nothing) when out of range.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.active = Some(index);
            true
        } else {
            false
        }
    }

    /// Remove; if it was active, the next one (or the new last one) becomes active.
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.items.len() {
            return None;
        }
        let item = self.items.remove(index);
        self.active = match self.active {
            _ if self.items.is_empty() => None,
            Some(a) if a > index => Some(a - 1),
            Some(a) if a == index => Some(index.min(self.items.len() - 1)),
            other => other,
        };
        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> SessionList<usize> {
        let mut l = SessionList::new();
        for i in 0..n {
            l.push(i);
        }
        l
    }

    #[test]
    fn push_makes_the_new_item_active() {
        let l = list(3);
        assert_eq!(l.active_index(), Some(2));
        assert_eq!(l.active(), Some(&2));
    }

    #[test]
    fn select_rejects_out_of_range() {
        let mut l = list(2);
        assert!(l.select(0));
        assert!(!l.select(5));
        assert_eq!(l.active_index(), Some(0));
    }

    #[test]
    fn removing_the_active_item_activates_its_neighbour() {
        let mut l = list(3);
        l.select(1);
        assert_eq!(l.remove(1), Some(1));
        assert_eq!(l.active(), Some(&2)); // the next one slid into its place
        l.remove(1);
        assert_eq!(l.active(), Some(&0)); // removed the last: previous one
    }

    #[test]
    fn removing_before_the_active_item_keeps_it_active() {
        let mut l = list(3);
        l.select(2);
        l.remove(0);
        assert_eq!(l.active(), Some(&2));
        assert_eq!(l.active_index(), Some(1));
    }

    #[test]
    fn removing_the_last_item_leaves_nothing_active() {
        let mut l = list(1);
        l.remove(0);
        assert!(l.is_empty());
        assert_eq!(l.active_index(), None);
        assert_eq!(l.remove(0), None);
    }

    #[test]
    fn insert_puts_the_item_at_the_index_and_activates_it() {
        let mut l = list(3);
        assert_eq!(l.insert(1, 9), 1);
        assert_eq!(l.items(), &[0, 9, 1, 2]);
        assert_eq!(l.active(), Some(&9));
        assert_eq!(l.insert(99, 7), 4); // clamped to the end
    }

    #[test]
    fn sorting_keeps_the_same_item_active() {
        let mut l = list(5);
        l.select(1);
        // Reverse order: item 1 is now at index 3, and still active.
        l.sort_by_key(|&i| std::cmp::Reverse(i));
        assert_eq!(l.items(), [4, 3, 2, 1, 0]);
        assert_eq!(l.active(), Some(&1));
        let mut empty: SessionList<usize> = SessionList::new();
        empty.sort_by_key(|&i| i);
        assert_eq!(empty.active_index(), None);
    }
}
