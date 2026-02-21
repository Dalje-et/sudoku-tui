use rand::seq::SliceRandom;
use rand::rng;
use rand::RngExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        &[Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert]
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

pub type Board = [[Cell; 9]; 9];
pub type SolutionBoard = [[u8; 9]; 9];

/// Check if placing `val` at (row, col) is valid on a raw u8 grid
fn is_valid_placement(grid: &[[u8; 9]; 9], row: usize, col: usize, val: u8) -> bool {
    // Check row
    for c in 0..9 {
        if grid[row][c] == val {
            return false;
        }
    }
    // Check column
    for r in 0..9 {
        if grid[r][col] == val {
            return false;
        }
    }
    // Check 3x3 box
    let box_r = (row / 3) * 3;
    let box_c = (col / 3) * 3;
    for r in box_r..box_r + 3 {
        for c in box_c..box_c + 3 {
            if grid[r][c] == val {
                return false;
            }
        }
    }
    true
}

/// Solve the grid in place using backtracking. Returns true if solved.
pub fn solve(grid: &mut [[u8; 9]; 9]) -> bool {
    for row in 0..9 {
        for col in 0..9 {
            if grid[row][col] == 0 {
                for val in 1..=9 {
                    if is_valid_placement(grid, row, col, val) {
                        grid[row][col] = val;
                        if solve(grid) {
                            return true;
                        }
                        grid[row][col] = 0;
                    }
                }
                return false;
            }
        }
    }
    true
}

/// Generate a complete valid Sudoku board
fn generate_complete_board() -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    let mut rng = rng();

    // Fill diagonal boxes first (they don't affect each other)
    for box_idx in 0..3 {
        let mut nums: Vec<u8> = (1..=9).collect();
        nums.shuffle(&mut rng);
        let start = box_idx * 3;
        let mut idx = 0;
        for r in start..start + 3 {
            for c in start..start + 3 {
                grid[r][c] = nums[idx];
                idx += 1;
            }
        }
    }

    // Solve the rest
    solve_shuffled(&mut grid);
    grid
}

/// Solve with randomized value ordering for variety
fn solve_shuffled(grid: &mut [[u8; 9]; 9]) -> bool {
    let mut rng = rng();
    for row in 0..9 {
        for col in 0..9 {
            if grid[row][col] == 0 {
                let mut vals: Vec<u8> = (1..=9).collect();
                vals.shuffle(&mut rng);
                for val in vals {
                    if is_valid_placement(grid, row, col, val) {
                        grid[row][col] = val;
                        if solve_shuffled(grid) {
                            return true;
                        }
                        grid[row][col] = 0;
                    }
                }
                return false;
            }
        }
    }
    true
}

/// Count solutions (up to limit) for uniqueness checking
fn count_solutions(grid: &mut [[u8; 9]; 9], limit: usize) -> usize {
    if limit == 0 {
        return 0;
    }

    for row in 0..9 {
        for col in 0..9 {
            if grid[row][col] == 0 {
                let mut count = 0;
                for val in 1..=9 {
                    if is_valid_placement(grid, row, col, val) {
                        grid[row][col] = val;
                        count += count_solutions(grid, limit - count);
                        grid[row][col] = 0;
                        if count >= limit {
                            return count;
                        }
                    }
                }
                return count;
            }
        }
    }
    1 // Found a solution
}

/// Generate a puzzle with the given difficulty
pub fn generate_puzzle(difficulty: Difficulty) -> (Board, SolutionBoard) {
    let solution = generate_complete_board();
    let mut rng = rng();

    let (min_givens, max_givens) = difficulty.givens_range();
    let target_givens = rng.random_range(min_givens..=max_givens);
    let cells_to_remove = 81 - target_givens;

    // Create list of all positions and shuffle
    let mut positions: Vec<(usize, usize)> = Vec::with_capacity(81);
    for r in 0..9 {
        for c in 0..9 {
            positions.push((r, c));
        }
    }
    positions.shuffle(&mut rng);

    let mut puzzle_grid = solution;
    let mut removed = 0;

    for (r, c) in positions {
        if removed >= cells_to_remove {
            break;
        }
        let backup = puzzle_grid[r][c];
        puzzle_grid[r][c] = 0;

        let mut test_grid = puzzle_grid;
        if count_solutions(&mut test_grid, 2) == 1 {
            removed += 1;
        } else {
            puzzle_grid[r][c] = backup;
        }
    }

    // Convert to Board
    let mut board = [[Cell::Empty; 9]; 9];
    for r in 0..9 {
        for c in 0..9 {
            if puzzle_grid[r][c] != 0 {
                board[r][c] = Cell::Given(puzzle_grid[r][c]);
            }
        }
    }

    (board, solution)
}

/// Check if a value conflicts with any other cell in the same row/col/box
pub fn has_conflict(board: &Board, row: usize, col: usize) -> bool {
    let val = match board[row][col].value() {
        Some(v) => v,
        None => return false,
    };

    // Check row
    for c in 0..9 {
        if c != col {
            if board[row][c].value() == Some(val) {
                return true;
            }
        }
    }
    // Check column
    for r in 0..9 {
        if r != row {
            if board[r][col].value() == Some(val) {
                return true;
            }
        }
    }
    // Check box
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

    let mut possible = vec![true; 10]; // index 0 unused
    possible[0] = false;

    // Eliminate from row
    for c in 0..9 {
        if let Some(v) = board[row][c].value() {
            possible[v as usize] = false;
        }
    }
    // Eliminate from column
    for r in 0..9 {
        if let Some(v) = board[r][col].value() {
            possible[v as usize] = false;
        }
    }
    // Eliminate from box
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
