//! One live terminal (filled in by Task 4). For now: the size type.

use alacritty_terminal::grid::Dimensions;

/// Terminal size in cells. Implements alacritty's `Dimensions` so `Term` can use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows // Term::new/resize only read screen_lines + columns
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}
