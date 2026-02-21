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
