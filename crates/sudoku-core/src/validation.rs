use crate::board::Board;

/// Check if a value conflicts with any other cell in the same row/col/box
pub fn has_conflict(board: &Board, row: usize, col: usize) -> bool {
    let val = match board[row][col].value() {
        Some(v) => v,
        None => return false,
    };

    for c in 0..9 {
        if c != col {
            if board[row][c].value() == Some(val) {
                return true;
            }
        }
    }
    for r in 0..9 {
        if r != row {
            if board[r][col].value() == Some(val) {
                return true;
            }
        }
    }
    let box_r = (row / 3) * 3;
    let box_c = (col / 3) * 3;
    for r in box_r..box_r + 3 {
        for c in box_c..box_c + 3 {
            if r != row || c != col {
                if board[r][c].value() == Some(val) {
                    return true;
                }
            }
        }
    }
    false
}

/// Get all conflicting cell positions
pub fn get_all_conflicts(board: &Board) -> Vec<(usize, usize)> {
    let mut conflicts = Vec::new();
    for r in 0..9 {
        for c in 0..9 {
            if board[r][c].value().is_some() && has_conflict(board, r, c) {
                conflicts.push((r, c));
            }
        }
    }
    conflicts
}

/// Check if the board is completely and correctly filled
pub fn is_board_complete(board: &Board) -> bool {
    for r in 0..9 {
        for c in 0..9 {
            if board[r][c].value().is_none() {
                return false;
            }
            if has_conflict(board, r, c) {
                return false;
            }
        }
    }
    true
}

/// Get candidates (possible values) for an empty cell
pub fn get_candidates(board: &Board, row: usize, col: usize) -> Vec<u8> {
    if board[row][col].value().is_some() {
        return vec![];
    }

    let mut possible = vec![true; 10];
    possible[0] = false;

    for c in 0..9 {
        if let Some(v) = board[row][c].value() {
            possible[v as usize] = false;
        }
    }
    for r in 0..9 {
        if let Some(v) = board[r][col].value() {
            possible[v as usize] = false;
        }
    }
    let box_r = (row / 3) * 3;
    let box_c = (col / 3) * 3;
    for r in box_r..box_r + 3 {
        for c in box_c..box_c + 3 {
            if let Some(v) = board[r][c].value() {
                possible[v as usize] = false;
            }
        }
    }

    (1..=9).filter(|&v| possible[v as usize]).collect()
}
