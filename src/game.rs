use crate::hint::{find_hint, Hint, HintStage};
use crate::puzzle::{
    generate_puzzle, get_all_conflicts, get_candidates, is_board_complete, Board, Cell, Difficulty,
    SolutionBoard,
};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Playing,
    Paused,
    Won,
}

#[derive(Clone, Debug)]
pub enum Move {
    PlaceNumber {
        row: usize,
        col: usize,
        old: Cell,
        new: Cell,
    },
    Erase {
        row: usize,
        col: usize,
        old: Cell,
    },
    TogglePencilMark {
        row: usize,
        col: usize,
        value: u8,
    },
}

pub struct Game {
    pub board: Board,
    pub solution: SolutionBoard,
    pub pencil_marks: [[Vec<u8>; 9]; 9],
    pub difficulty: Difficulty,
    pub selected_row: usize,
    pub selected_col: usize,
    pub state: GameState,
    pub pencil_mode: bool,
    pub mistakes: u32,
    pub move_history: Vec<Move>,
    pub timer_start: Option<Instant>,
    pub elapsed_secs: u64,
    pub paused_elapsed: u64,
    pub conflicts: Vec<(usize, usize)>,
    pub show_conflicts: bool,
    pub active_hint: Option<Hint>,
    pub hint_stage: HintStage,
    pub hints_used: u32,
    pub show_quit_confirm: bool,
}

impl Game {
    pub fn new() -> Self {
        Self {
            board: [[Cell::Empty; 9]; 9],
            solution: [[0u8; 9]; 9],
            pencil_marks: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
            difficulty: Difficulty::Easy,
            selected_row: 4,
            selected_col: 4,
            state: GameState::Menu,
            pencil_mode: false,
            mistakes: 0,
            move_history: Vec::new(),
            timer_start: None,
            elapsed_secs: 0,
            paused_elapsed: 0,
            conflicts: Vec::new(),
            show_conflicts: false,
            active_hint: None,
            hint_stage: HintStage::ShowTechnique,
            hints_used: 0,
            show_quit_confirm: false,
        }
    }

    pub fn start_new_game(&mut self) {
        let (board, solution) = generate_puzzle(self.difficulty);
        self.board = board;
        self.solution = solution;
        self.pencil_marks = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
        self.selected_row = 4;
        self.selected_col = 4;
        self.state = GameState::Playing;
        self.pencil_mode = false;
        self.mistakes = 0;
        self.move_history.clear();
        self.timer_start = Some(Instant::now());
        self.elapsed_secs = 0;
        self.paused_elapsed = 0;
        self.conflicts.clear();
        self.show_conflicts = false;
        self.active_hint = None;
        self.hints_used = 0;
        self.show_quit_confirm = false;
    }

    pub fn move_cursor(&mut self, dr: i32, dc: i32) {
        let new_row = (self.selected_row as i32 + dr).rem_euclid(9) as usize;
        let new_col = (self.selected_col as i32 + dc).rem_euclid(9) as usize;
        self.selected_row = new_row;
        self.selected_col = new_col;
    }

    pub fn place_number(&mut self, num: u8) {
        if self.state != GameState::Playing {
            return;
        }
        let r = self.selected_row;
        let c = self.selected_col;

        if self.board[r][c].is_given() {
            return;
        }

        if self.pencil_mode {
            self.toggle_pencil_mark(num);
            return;
        }

        let old = self.board[r][c];
        let new = Cell::UserInput(num);
        self.board[r][c] = new;
        self.pencil_marks[r][c].clear();

        // Remove this number from pencil marks in same row/col/box
        self.clear_related_pencil_marks(r, c, num);

        self.move_history.push(Move::PlaceNumber { row: r, col: c, old, new });

        // Check if it's wrong
        if self.solution[r][c] != num {
            self.mistakes += 1;
        }

        // Update conflicts
        self.conflicts = get_all_conflicts(&self.board);

        // Check win
        if is_board_complete(&self.board) {
            self.state = GameState::Won;
            if let Some(start) = self.timer_start {
                self.elapsed_secs = self.paused_elapsed + start.elapsed().as_secs();
            }
        }
    }

    fn clear_related_pencil_marks(&mut self, row: usize, col: usize, val: u8) {
        // Row
        for c in 0..9 {
            self.pencil_marks[row][c].retain(|&v| v != val);
        }
        // Col
        for r in 0..9 {
            self.pencil_marks[r][col].retain(|&v| v != val);
        }
        // Box
        let box_r = (row / 3) * 3;
        let box_c = (col / 3) * 3;
        for r in box_r..box_r + 3 {
            for c in box_c..box_c + 3 {
                self.pencil_marks[r][c].retain(|&v| v != val);
            }
        }
    }

    fn toggle_pencil_mark(&mut self, num: u8) {
        let r = self.selected_row;
        let c = self.selected_col;

        if self.board[r][c].value().is_some() {
            return;
        }

        self.move_history.push(Move::TogglePencilMark {
            row: r,
            col: c,
            value: num,
        });

        if self.pencil_marks[r][c].contains(&num) {
            self.pencil_marks[r][c].retain(|&v| v != num);
        } else {
            self.pencil_marks[r][c].push(num);
            self.pencil_marks[r][c].sort();
        }
    }

    pub fn erase(&mut self) {
        if self.state != GameState::Playing {
            return;
        }
        let r = self.selected_row;
        let c = self.selected_col;

        if self.board[r][c].is_given() {
            return;
        }

        if let Cell::UserInput(_) = self.board[r][c] {
            let old = self.board[r][c];
            self.board[r][c] = Cell::Empty;
            self.move_history.push(Move::Erase { row: r, col: c, old });
            self.conflicts = get_all_conflicts(&self.board);
        } else if !self.pencil_marks[r][c].is_empty() {
            self.pencil_marks[r][c].clear();
        }
    }

    pub fn undo(&mut self) {
        if self.state != GameState::Playing {
            return;
        }

        if let Some(mv) = self.move_history.pop() {
            match mv {
                Move::PlaceNumber { row, col, old, .. } => {
                    self.board[row][col] = old;
                }
                Move::Erase { row, col, old } => {
                    self.board[row][col] = old;
                }
                Move::TogglePencilMark { row, col, value } => {
                    if self.pencil_marks[row][col].contains(&value) {
                        self.pencil_marks[row][col].retain(|&v| v != value);
                    } else {
                        self.pencil_marks[row][col].push(value);
                        self.pencil_marks[row][col].sort();
                    }
                }
            }
            self.conflicts = get_all_conflicts(&self.board);
        }
    }

    pub fn validate(&mut self) {
        self.show_conflicts = true;
        self.conflicts = get_all_conflicts(&self.board);
    }

    pub fn request_hint(&mut self) {
        if self.state != GameState::Playing {
            return;
        }

        if self.active_hint.is_some() {
            // Advance hint stage
            match self.hint_stage {
                HintStage::ShowTechnique => {
                    self.hint_stage = HintStage::RevealValue;
                }
                HintStage::RevealValue => {
                    // Apply the hint
                    if let Some(ref hint) = self.active_hint {
                        let r = hint.target_row;
                        let c = hint.target_col;
                        let v = hint.value;
                        if self.board[r][c] == Cell::Empty {
                            self.board[r][c] = Cell::UserInput(v);
                            self.pencil_marks[r][c].clear();
                            self.clear_related_pencil_marks(r, c, v);
                            self.conflicts = get_all_conflicts(&self.board);

                            if is_board_complete(&self.board) {
                                self.state = GameState::Won;
                                if let Some(start) = self.timer_start {
                                    self.elapsed_secs =
                                        self.paused_elapsed + start.elapsed().as_secs();
                                }
                            }
                        }
                    }
                    self.active_hint = None;
                    self.hint_stage = HintStage::ShowTechnique;
                }
            }
        } else {
            if let Some(hint) = find_hint(&self.board, &self.solution) {
                self.active_hint = Some(hint);
                self.hint_stage = HintStage::ShowTechnique;
                self.hints_used += 1;
            }
        }
    }

    pub fn dismiss_hint(&mut self) {
        self.active_hint = None;
        self.hint_stage = HintStage::ShowTechnique;
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            GameState::Playing => {
                if let Some(start) = self.timer_start {
                    self.paused_elapsed += start.elapsed().as_secs();
                }
                self.state = GameState::Paused;
                self.timer_start = None;
            }
            GameState::Paused => {
                self.timer_start = Some(Instant::now());
                self.state = GameState::Playing;
            }
            _ => {}
        }
    }

    pub fn get_elapsed_secs(&self) -> u64 {
        match self.state {
            GameState::Won => self.elapsed_secs,
            GameState::Paused => self.paused_elapsed,
            GameState::Playing => {
                self.paused_elapsed
                    + self
                        .timer_start
                        .map(|s| s.elapsed().as_secs())
                        .unwrap_or(0)
            }
            GameState::Menu => 0,
        }
    }

    pub fn format_time(&self) -> String {
        let secs = self.get_elapsed_secs();
        let mins = secs / 60;
        let s = secs % 60;
        format!("{:02}:{:02}", mins, s)
    }

    pub fn selected_value(&self) -> Option<u8> {
        self.board[self.selected_row][self.selected_col].value()
    }

    pub fn auto_pencil_marks(&mut self) {
        for r in 0..9 {
            for c in 0..9 {
                if self.board[r][c] == Cell::Empty {
                    self.pencil_marks[r][c] = get_candidates(&self.board, r, c);
                }
            }
        }
    }
}
