use rand::seq::SliceRandom;
use rand::rng;
use rand::RngExt;

use crate::board::{Board, Cell, SolutionBoard};
use crate::difficulty::Difficulty;

/// Check if placing `val` at (row, col) is valid on a raw u8 grid
fn is_valid_placement(grid: &[[u8; 9]; 9], row: usize, col: usize, val: u8) -> bool {
    for c in 0..9 {
        if grid[row][c] == val {
            return false;
        }
    }
    for r in 0..9 {
        if grid[r][col] == val {
            return false;
        }
    }
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
    1
}

/// Generate a puzzle with the given difficulty
pub fn generate_puzzle(difficulty: Difficulty) -> (Board, SolutionBoard) {
    let solution = generate_complete_board();
    let mut rng = rng();

    let (min_givens, max_givens) = difficulty.givens_range();
    let target_givens = rng.random_range(min_givens..=max_givens);
    let cells_to_remove = 81 - target_givens;

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
