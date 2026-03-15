use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cell {
    Given(u8),
    UserInput(u8),
    Empty,
}

impl Cell {
    pub fn value(&self) -> Option<u8> {
        match self {
            Cell::Given(v) | Cell::UserInput(v) => Some(*v),
            Cell::Empty => None,
        }
    }

    pub fn is_given(&self) -> bool {
        matches!(self, Cell::Given(_))
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Cell::Empty)
    }
}

pub type Board = [[Cell; 9]; 9];
pub type SolutionBoard = [[u8; 9]; 9];

/// Iterate all peer coordinates of (row, col) — same row, column, or box, excluding (row, col) itself.
pub fn peers(row: usize, col: usize) -> impl Iterator<Item = (usize, usize)> {
    let box_r = (row / 3) * 3;
    let box_c = (col / 3) * 3;
    let row_peers = (0..9).map(move |c| (row, c));
    let col_peers = (0..9).map(move |r| (r, col));
    let box_peers = (box_r..box_r + 3).flat_map(move |r| (box_c..box_c + 3).map(move |c| (r, c)));
    row_peers
        .chain(col_peers)
        .chain(box_peers)
        .filter(move |&(r, c)| r != row || c != col)
}

/// Check if placing `val` at (row, col) is valid on a raw u8 grid.
pub fn is_valid_placement(grid: &[[u8; 9]; 9], row: usize, col: usize, val: u8) -> bool {
    for (r, c) in peers(row, col) {
        if grid[r][c] == val {
            return false;
        }
    }
    true
}
