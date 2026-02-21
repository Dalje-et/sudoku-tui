#![allow(unused)]

use std::sync::atomic::AtomicU32;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use sudoku_core::protocol::{GameMode, ServerMessage};
use sudoku_core::{Board, Cell, Difficulty, SolutionBoard};

/// Handle to push messages to a connected WebSocket client.
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub user_id: i64,
    pub username: String,
    pub rating: i32,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    pub room_code: Option<String>,
    /// Messages received in the current second window.
    pub message_count: u32,
    pub rate_limit_window: Instant,
}

/// An entry in the matchmaking queue.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub user_id: i64,
    pub username: String,
    pub rating: i32,
    pub joined_at: Instant,
}

/// Room state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomState {
    Waiting,
    Playing,
    Ended,
}

/// A game room.
#[derive(Debug, Clone)]
pub struct Room {
    pub code: String,
    pub mode: GameMode,
    pub difficulty: Difficulty,
    pub state: RoomState,
    pub player1_id: i64,
    pub player2_id: Option<i64>,
    /// The puzzle board (givens only).
    pub board: Board,
    /// The full solution.
    pub solution: SolutionBoard,
    /// Per-player boards for race mode: user_id -> board.
    pub player_boards: std::collections::HashMap<i64, Board>,
    /// Cell ownership for shared mode: (row, col) -> user_id who placed it.
    pub cell_ownership: std::collections::HashMap<(usize, usize), i64>,
    /// The shared board state (for shared mode).
    pub shared_board: Board,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub started_at: Option<Instant>,
}

/// A user session backed by the database.
#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: i64,
    pub username: String,
    pub expires_at: String,
}

/// Shared application state.
pub struct AppState {
    pub db: SqlitePool,
    pub rooms: DashMap<String, Room>,
    pub sessions: DashMap<String, Session>,
    pub connections: DashMap<i64, ConnectionHandle>,
    /// Matchmaking queues keyed by "mode:difficulty".
    pub matchmaking: DashMap<String, Vec<QueueEntry>>,
    pub connection_count: AtomicU32,
    pub max_connections: u32,
}

/// Generate a random 6-character uppercase alphanumeric room code.
pub fn generate_room_code() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect()
}

/// Convert a Board to the Vec<Vec<u8>> wire format (givens only, 0 for empty).
pub fn board_to_wire(board: &Board) -> Vec<Vec<u8>> {
    board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Cell::Given(v) => *v,
                    _ => 0,
                })
                .collect()
        })
        .collect()
}

/// Count user-placed (non-given, non-empty) cells in a board.
pub fn filled_count(board: &Board) -> u32 {
    let mut count = 0u32;
    for row in board.iter() {
        for cell in row.iter() {
            if matches!(cell, Cell::UserInput(_)) {
                count += 1;
            }
        }
    }
    count
}

/// Count user-placed cells that match the solution.
pub fn correct_count(board: &Board, solution: &[[u8; 9]; 9]) -> u32 {
    let mut count = 0u32;
    for r in 0..9 {
        for c in 0..9 {
            if let Cell::UserInput(v) = board[r][c] {
                if v == solution[r][c] {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Build matchmaking queue key from mode + difficulty.
pub fn queue_key(mode: GameMode, difficulty: Difficulty) -> String {
    format!("{:?}:{:?}", mode, difficulty)
}
