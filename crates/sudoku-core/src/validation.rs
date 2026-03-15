use crate::board::{peers, Board};

/// Check if a value conflicts with any other cell in the same row/col/box
pub fn has_conflict(board: &Board, row: usize, col: usize) -> bool {
    let val = match board[row][col].value() {
        Some(v) => v,
        None => return false,
    };

    for (r, c) in peers(row, col) {
        if board[r][c].value() == Some(val) {
            return true;
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

    let mut possible = [true; 10];
    possible[0] = false;

    for (r, c) in peers(row, col) {
        if let Some(v) = board[r][c].value() {
            possible[v as usize] = false;
        }
    }

    (1..=9).filter(|&v| possible[v as usize]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Cell;

    fn empty_board() -> [[Cell; 9]; 9] {
        [[Cell::Empty; 9]; 9]
    }

    #[test]
    fn test_has_conflict_row() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(5);
        board[0][5] = Cell::UserInput(5);
        assert!(has_conflict(&board, 0, 0));
        assert!(has_conflict(&board, 0, 5));
    }

    #[test]
    fn test_has_conflict_col() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(3);
        board[7][0] = Cell::UserInput(3);
        assert!(has_conflict(&board, 0, 0));
        assert!(has_conflict(&board, 7, 0));
    }

    #[test]
    fn test_has_conflict_box() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(9);
        board[2][2] = Cell::UserInput(9);
        assert!(has_conflict(&board, 0, 0));
        assert!(has_conflict(&board, 2, 2));
    }

    #[test]
    fn test_no_conflict() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(1);
        board[0][1] = Cell::Given(2);
        board[1][0] = Cell::Given(3);
        assert!(!has_conflict(&board, 0, 0));
        assert!(!has_conflict(&board, 0, 1));
        assert!(!has_conflict(&board, 1, 0));
    }

    #[test]
    fn test_get_candidates() {
        let mut board = empty_board();
        // Fill row 0 with 1-8 in columns 0-7
        for c in 0..8 {
            board[0][c] = Cell::Given((c + 1) as u8);
        }
        // Column 8 should only allow 9
        let candidates = get_candidates(&board, 0, 8);
        assert_eq!(candidates, vec![9]);
    }

    #[test]
    fn test_get_candidates_non_empty_cell() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(5);
        assert_eq!(get_candidates(&board, 0, 0), Vec::<u8>::new());
    }

    #[test]
    fn test_is_board_complete_empty() {
        let board = empty_board();
        assert!(!is_board_complete(&board));
    }

    #[test]
    fn test_is_board_complete_valid() {
        // A known valid solved board
        let vals: [[u8; 9]; 9] = [
            [5, 3, 4, 6, 7, 8, 9, 1, 2],
            [6, 7, 2, 1, 9, 5, 3, 4, 8],
            [1, 9, 8, 3, 4, 2, 5, 6, 7],
            [8, 5, 9, 7, 6, 1, 4, 2, 3],
            [4, 2, 6, 8, 5, 3, 7, 9, 1],
            [7, 1, 3, 9, 2, 4, 8, 5, 6],
            [9, 6, 1, 5, 3, 7, 2, 8, 4],
            [2, 8, 7, 4, 1, 9, 6, 3, 5],
            [3, 4, 5, 2, 8, 6, 1, 7, 9],
        ];
        let mut board = empty_board();
        for r in 0..9 {
            for c in 0..9 {
                board[r][c] = Cell::Given(vals[r][c]);
            }
        }
        assert!(is_board_complete(&board));
    }

    #[test]
    fn test_get_all_conflicts() {
        let mut board = empty_board();
        board[0][0] = Cell::Given(5);
        board[0][8] = Cell::UserInput(5);
        let conflicts = get_all_conflicts(&board);
        assert!(conflicts.contains(&(0, 0)));
        assert!(conflicts.contains(&(0, 8)));
        assert_eq!(conflicts.len(), 2);
    }
}
