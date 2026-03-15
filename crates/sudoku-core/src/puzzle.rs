use rand::seq::SliceRandom;
use rand::rng;
use rand::RngExt;

use crate::board::{is_valid_placement, Board, Cell, SolutionBoard};
use crate::difficulty::Difficulty;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_known_puzzle() {
        // A puzzle with a known solution
        #[rustfmt::skip]
        let mut grid: [[u8; 9]; 9] = [
            [5, 3, 0, 0, 7, 0, 0, 0, 0],
            [6, 0, 0, 1, 9, 5, 0, 0, 0],
            [0, 9, 8, 0, 0, 0, 0, 6, 0],
            [8, 0, 0, 0, 6, 0, 0, 0, 3],
            [4, 0, 0, 8, 0, 3, 0, 0, 1],
            [7, 0, 0, 0, 2, 0, 0, 0, 6],
            [0, 6, 0, 0, 0, 0, 2, 8, 0],
            [0, 0, 0, 4, 1, 9, 0, 0, 5],
            [0, 0, 0, 0, 8, 0, 0, 7, 9],
        ];
        assert!(solve(&mut grid));
        // Every cell should be filled
        for r in 0..9 {
            for c in 0..9 {
                assert_ne!(grid[r][c], 0);
            }
        }
        // Solution should be valid
        for r in 0..9 {
            for c in 0..9 {
                let val = grid[r][c];
                grid[r][c] = 0;
                assert!(is_valid_placement(&grid, r, c, val));
                grid[r][c] = val;
            }
        }
    }

    #[test]
    fn generated_puzzle_has_unique_solution() {
        let (board, solution) = generate_puzzle(Difficulty::Medium);

        // Convert board to u8 grid for count_solutions
        let mut grid = [[0u8; 9]; 9];
        for r in 0..9 {
            for c in 0..9 {
                if let Some(v) = board[r][c].value() {
                    grid[r][c] = v;
                }
            }
        }

        assert_eq!(count_solutions(&mut grid, 2), 1, "Puzzle should have exactly one solution");

        // The solution should be valid
        for r in 0..9 {
            for c in 0..9 {
                assert_ne!(solution[r][c], 0, "Solution has empty cell at ({}, {})", r, c);
            }
        }
    }

    #[test]
    fn generated_puzzle_givens_in_range() {
        for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert] {
            let (board, _) = generate_puzzle(difficulty);
            let givens: usize = board.iter().flatten().filter(|c| c.is_given()).count();
            let (min, max) = difficulty.givens_range();
            assert!(
                givens >= min && givens <= max,
                "{:?}: got {} givens, expected {}..={}",
                difficulty,
                givens,
                min,
                max,
            );
        }
    }

    #[test]
    fn generated_puzzle_solution_matches_givens() {
        let (board, solution) = generate_puzzle(Difficulty::Easy);
        for r in 0..9 {
            for c in 0..9 {
                if let Cell::Given(v) = board[r][c] {
                    assert_eq!(v, solution[r][c], "Given at ({},{}) doesn't match solution", r, c);
                }
            }
        }
    }
}
