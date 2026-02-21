use crate::puzzle::{Board, Cell, get_candidates};

#[derive(Clone, Debug)]
pub struct Hint {
    pub technique: HintTechnique,
    pub target_row: usize,
    pub target_col: usize,
    pub value: u8,
    pub highlighted_cells: Vec<(usize, usize)>,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintTechnique {
    NakedSingle,
    HiddenSingle,
    DirectReveal, // Fallback: just reveal from solution
}

impl HintTechnique {
    pub fn label(&self) -> &str {
        match self {
            HintTechnique::NakedSingle => "Naked Single",
            HintTechnique::HiddenSingle => "Hidden Single",
            HintTechnique::DirectReveal => "Direct Reveal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintStage {
    /// Show technique name and highlight region
    ShowTechnique,
    /// Reveal the value
    RevealValue,
}

/// Find the best available hint for the current board state
pub fn find_hint(board: &Board, solution: &[[u8; 9]; 9]) -> Option<Hint> {
    // Try Naked Single first (easiest to understand)
    if let Some(hint) = find_naked_single(board) {
        return Some(hint);
    }

    // Try Hidden Single
    if let Some(hint) = find_hidden_single(board) {
        return Some(hint);
    }

    // Fallback: reveal from solution
    find_direct_reveal(board, solution)
}

/// Naked Single: a cell where only one candidate is possible
fn find_naked_single(board: &Board) -> Option<Hint> {
    for r in 0..9 {
        for c in 0..9 {
            if board[r][c] != Cell::Empty {
                continue;
            }
            let candidates = get_candidates(board, r, c);
            if candidates.len() == 1 {
                let val = candidates[0];
                // Highlight the row, column, and box that constrain this cell
                let mut highlighted = Vec::new();

                // Add row cells that have values
                for cc in 0..9 {
                    if cc != c && board[r][cc].value().is_some() {
                        highlighted.push((r, cc));
                    }
                }
                // Add col cells that have values
                for rr in 0..9 {
                    if rr != r && board[rr][c].value().is_some() {
                        highlighted.push((rr, c));
                    }
                }
                // Add box cells that have values
                let box_r = (r / 3) * 3;
                let box_c = (c / 3) * 3;
                for rr in box_r..box_r + 3 {
                    for cc in box_c..box_c + 3 {
                        if (rr != r || cc != c) && board[rr][cc].value().is_some() {
                            if !highlighted.contains(&(rr, cc)) {
                                highlighted.push((rr, cc));
                            }
                        }
                    }
                }

                return Some(Hint {
                    technique: HintTechnique::NakedSingle,
                    target_row: r,
                    target_col: c,
                    value: val,
                    highlighted_cells: highlighted,
                    explanation: format!(
                        "Naked Single: R{}C{} can only be {} — all other values are taken by its row, column, and box",
                        r + 1, c + 1, val
                    ),
                });
            }
        }
    }
    None
}

/// Hidden Single: a value that can only go in one cell within a row/col/box
fn find_hidden_single(board: &Board) -> Option<Hint> {
    // Check each row
    for r in 0..9 {
        for val in 1..=9u8 {
            // Skip if value already in row
            if (0..9).any(|c| board[r][c].value() == Some(val)) {
                continue;
            }
            let possible_cols: Vec<usize> = (0..9)
                .filter(|&c| board[r][c] == Cell::Empty && get_candidates(board, r, c).contains(&val))
                .collect();

            if possible_cols.len() == 1 {
                let c = possible_cols[0];
                let highlighted: Vec<(usize, usize)> = (0..9)
                    .filter(|&cc| cc != c)
                    .map(|cc| (r, cc))
                    .collect();

                return Some(Hint {
                    technique: HintTechnique::HiddenSingle,
                    target_row: r,
                    target_col: c,
                    value: val,
                    highlighted_cells: highlighted,
                    explanation: format!(
                        "Hidden Single: {} can only go in R{}C{} within row {}",
                        val, r + 1, c + 1, r + 1
                    ),
                });
            }
        }
    }

    // Check each column
    for c in 0..9 {
        for val in 1..=9u8 {
            if (0..9).any(|r| board[r][c].value() == Some(val)) {
                continue;
            }
            let possible_rows: Vec<usize> = (0..9)
                .filter(|&r| board[r][c] == Cell::Empty && get_candidates(board, r, c).contains(&val))
                .collect();

            if possible_rows.len() == 1 {
                let r = possible_rows[0];
                let highlighted: Vec<(usize, usize)> = (0..9)
                    .filter(|&rr| rr != r)
                    .map(|rr| (rr, c))
                    .collect();

                return Some(Hint {
                    technique: HintTechnique::HiddenSingle,
                    target_row: r,
                    target_col: c,
                    value: val,
                    highlighted_cells: highlighted,
                    explanation: format!(
                        "Hidden Single: {} can only go in R{}C{} within column {}",
                        val, r + 1, c + 1, c + 1
                    ),
                });
            }
        }
    }

    // Check each box
    for box_r in (0..9).step_by(3) {
        for box_c in (0..9).step_by(3) {
            for val in 1..=9u8 {
                let mut found = false;
                for r in box_r..box_r + 3 {
                    for c in box_c..box_c + 3 {
                        if board[r][c].value() == Some(val) {
                            found = true;
                        }
                    }
                }
                if found {
                    continue;
                }

                let possible: Vec<(usize, usize)> = (box_r..box_r + 3)
                    .flat_map(|r| (box_c..box_c + 3).map(move |c| (r, c)))
                    .filter(|&(r, c)| board[r][c] == Cell::Empty && get_candidates(board, r, c).contains(&val))
                    .collect();

                if possible.len() == 1 {
                    let (r, c) = possible[0];
                    let highlighted: Vec<(usize, usize)> = (box_r..box_r + 3)
                        .flat_map(|rr| (box_c..box_c + 3).map(move |cc| (rr, cc)))
                        .filter(|&(rr, cc)| rr != r || cc != c)
                        .collect();

                    return Some(Hint {
                        technique: HintTechnique::HiddenSingle,
                        target_row: r,
                        target_col: c,
                        value: val,
                        highlighted_cells: highlighted,
                        explanation: format!(
                            "Hidden Single: {} can only go in R{}C{} within its 3×3 box",
                            val, r + 1, c + 1
                        ),
                    });
                }
            }
        }
    }

    None
}

/// Fallback: find any empty cell and reveal from solution
fn find_direct_reveal(board: &Board, solution: &[[u8; 9]; 9]) -> Option<Hint> {
    for r in 0..9 {
        for c in 0..9 {
            if board[r][c] == Cell::Empty {
                return Some(Hint {
                    technique: HintTechnique::DirectReveal,
                    target_row: r,
                    target_col: c,
                    value: solution[r][c],
                    highlighted_cells: vec![(r, c)],
                    explanation: format!(
                        "Direct Reveal: R{}C{} = {} (no simple technique found)",
                        r + 1, c + 1, solution[r][c]
                    ),
                });
            }
        }
    }
    None
}
