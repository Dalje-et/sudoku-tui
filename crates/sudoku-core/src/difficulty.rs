use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

impl Difficulty {
    pub fn label(&self) -> &str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
            Difficulty::Expert => "Expert",
        }
    }

    pub fn givens_range(&self) -> (usize, usize) {
        match self {
            Difficulty::Easy => (40, 45),
            Difficulty::Medium => (32, 39),
            Difficulty::Hard => (27, 31),
            Difficulty::Expert => (22, 26),
        }
    }

    pub fn all() -> &'static [Difficulty] {
        &[
            Difficulty::Easy,
            Difficulty::Medium,
            Difficulty::Hard,
            Difficulty::Expert,
        ]
    }

    pub fn next(&self) -> Difficulty {
        match self {
            Difficulty::Easy => Difficulty::Medium,
            Difficulty::Medium => Difficulty::Hard,
            Difficulty::Hard => Difficulty::Expert,
            Difficulty::Expert => Difficulty::Easy,
        }
    }

    pub fn prev(&self) -> Difficulty {
        match self {
            Difficulty::Easy => Difficulty::Expert,
            Difficulty::Medium => Difficulty::Easy,
            Difficulty::Hard => Difficulty::Medium,
            Difficulty::Expert => Difficulty::Hard,
        }
    }
}
