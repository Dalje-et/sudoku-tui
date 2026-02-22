use crate::hint::{find_hint, Hint, HintStage};
use sudoku_core::protocol::LeaderboardEntry;
use sudoku_core::puzzle::generate_puzzle;
use sudoku_core::validation::{get_all_conflicts, get_candidates, is_board_complete};
use sudoku_core::{Board, Cell, Difficulty, SolutionBoard};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Playing,
    Paused,
    Won,
    MultiplayerMenu,
    AuthScreen,
    Lobby,
    MultiplayerPlaying,
    MultiplayerEnd,
    Leaderboard,
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

/// Multiplayer-specific state
pub struct MultiplayerState {
    pub opponent_name: String,
    pub opponent_rating: i32,
    pub mode: sudoku_core::protocol::GameMode,
    /// Race mode: opponent's filled cell count
    pub opponent_filled: u32,
    /// Race mode: opponent's momentum (placements/min)
    pub opponent_momentum: f32,
    /// Shared mode: opponent's cursor position
    pub opponent_cursor: Option<(usize, usize)>,
    /// Shared mode: cell ownership (who placed what)
    pub cell_owner: [[CellOwner; 9]; 9],
    /// Game result
    pub result: Option<GameResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellOwner {
    None,
    Mine,
    Opponent,
    Given,
}

pub struct GameResult {
    pub won: bool,
    pub your_score: u32,
    pub opponent_score: u32,
    pub elo_change: i32,
    pub new_rating: i32,
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
    // Multiplayer
    pub multiplayer: Option<MultiplayerState>,
    // Menu selection index for multiplayer menu
    pub menu_selection: usize,
    // Auth
    pub auth_code: Option<String>,
    pub auth_uri: Option<String>,
    pub auth_status: Option<String>,
    // Lobby
    pub room_code: Option<String>,
    // Room code input buffer for joining
    pub room_input: String,
    // Joining mode active
    pub joining_room: bool,
    // Error message to display (cleared on next action)
    pub error_message: Option<String>,
    // Auth polling state
    pub auth_polling: bool,
    pub auth_poll_interval: u64,
    // Pending async operations (triggered by key press, executed in event loop)
    pub pending_auth_start: bool,
    pub pending_connect: bool,
    pub pending_leaderboard: bool,
    // What menu action to resume after connecting
    pub pending_menu_action: Option<usize>,
    // Leaderboard
    pub leaderboard_entries: Vec<LeaderboardEntry>,
    pub leaderboard_scroll: usize,
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
            multiplayer: None,
            menu_selection: 0,
            auth_code: None,
            auth_uri: None,
            auth_status: None,
            room_code: None,
            room_input: String::new(),
            joining_room: false,
            error_message: None,
            auth_polling: false,
            auth_poll_interval: 5,
            pending_auth_start: false,
            pending_connect: false,
            pending_leaderboard: false,
            pending_menu_action: None,
            leaderboard_entries: Vec::new(),
            leaderboard_scroll: 0,
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
        self.multiplayer = None;
    }

    pub fn start_multiplayer_game(
        &mut self,
        board: Board,
        solution: SolutionBoard,
        mode: sudoku_core::protocol::GameMode,
        opponent_name: String,
        opponent_rating: i32,
    ) {
        self.board = board;
        self.solution = solution;
        self.pencil_marks = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
        self.selected_row = 4;
        self.selected_col = 4;
        self.state = GameState::MultiplayerPlaying;
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

        let mut cell_owner = [[CellOwner::None; 9]; 9];
        for r in 0..9 {
            for c in 0..9 {
                if board[r][c].is_given() {
                    cell_owner[r][c] = CellOwner::Given;
                }
            }
        }

        self.multiplayer = Some(MultiplayerState {
            opponent_name,
            opponent_rating,
            mode,
            opponent_filled: 0,
            opponent_momentum: 0.0,
            opponent_cursor: None,
            cell_owner,
            result: None,
        });
    }

    pub fn move_cursor(&mut self, dr: i32, dc: i32) {
        let new_row = (self.selected_row as i32 + dr).rem_euclid(9) as usize;
        let new_col = (self.selected_col as i32 + dc).rem_euclid(9) as usize;
        self.selected_row = new_row;
        self.selected_col = new_col;
    }

    pub fn place_number(&mut self, num: u8) {
        if self.state != GameState::Playing && self.state != GameState::MultiplayerPlaying {
            return;
        }
        let r = self.selected_row;
        let c = self.selected_col;

        if self.board[r][c].is_given() {
            return;
        }

        // In multiplayer shared mode, can't overwrite opponent's cells
        if let Some(ref mp) = self.multiplayer {
            if mp.mode == sudoku_core::protocol::GameMode::Shared
                && mp.cell_owner[r][c] == CellOwner::Opponent
            {
                return;
            }
        }

        if self.pencil_mode && (self.state == GameState::Playing || self.state == GameState::MultiplayerPlaying) {
            self.toggle_pencil_mark(num);
            return;
        }

        let old = self.board[r][c];
        let new = Cell::UserInput(num);
        self.board[r][c] = new;
        self.pencil_marks[r][c].clear();
        self.clear_related_pencil_marks(r, c, num);
        self.move_history.push(Move::PlaceNumber {
            row: r,
            col: c,
            old,
            new,
        });

        if self.solution[r][c] != num {
            self.mistakes += 1;
        }

        self.conflicts = get_all_conflicts(&self.board);

        // Mark cell ownership in multiplayer
        if let Some(ref mut mp) = self.multiplayer {
            if mp.cell_owner[r][c] == CellOwner::None {
                mp.cell_owner[r][c] = CellOwner::Mine;
            }
        }

        if self.state == GameState::Playing && is_board_complete(&self.board) {
            self.state = GameState::Won;
            if let Some(start) = self.timer_start {
                self.elapsed_secs = self.paused_elapsed + start.elapsed().as_secs();
            }
        }
    }

    fn clear_related_pencil_marks(&mut self, row: usize, col: usize, val: u8) {
        for c in 0..9 {
            self.pencil_marks[row][c].retain(|&v| v != val);
        }
        for r in 0..9 {
            self.pencil_marks[r][col].retain(|&v| v != val);
        }
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
        if self.state != GameState::Playing && self.state != GameState::MultiplayerPlaying {
            return;
        }
        let r = self.selected_row;
        let c = self.selected_col;

        if self.board[r][c].is_given() {
            return;
        }

        // In multiplayer shared mode, can only erase own cells
        if let Some(ref mp) = self.multiplayer {
            if mp.mode == sudoku_core::protocol::GameMode::Shared
                && mp.cell_owner[r][c] != CellOwner::Mine
            {
                return;
            }
        }

        if let Cell::UserInput(_) = self.board[r][c] {
            let old = self.board[r][c];
            self.board[r][c] = Cell::Empty;
            self.move_history
                .push(Move::Erase { row: r, col: c, old });
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
            match self.hint_stage {
                HintStage::ShowTechnique => {
                    self.hint_stage = HintStage::RevealValue;
                }
                HintStage::RevealValue => {
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
            GameState::Won | GameState::MultiplayerEnd => self.elapsed_secs,
            GameState::Paused => self.paused_elapsed,
            GameState::Playing | GameState::MultiplayerPlaying => {
                self.paused_elapsed
                    + self
                        .timer_start
                        .map(|s| s.elapsed().as_secs())
                        .unwrap_or(0)
            }
            GameState::Menu | GameState::MultiplayerMenu | GameState::AuthScreen | GameState::Lobby | GameState::Leaderboard => 0,
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

    pub fn is_multiplayer(&self) -> bool {
        self.multiplayer.is_some()
    }

    /// Count filled (non-given, non-empty) cells on the board
    pub fn filled_count(&self) -> u32 {
        let mut count = 0u32;
        for r in 0..9 {
            for c in 0..9 {
                if matches!(self.board[r][c], Cell::UserInput(_)) {
                    count += 1;
                }
            }
        }
        count
    }
}
