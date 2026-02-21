#![allow(unused)]

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use tokio::sync::mpsc;

use sudoku_core::elo::{calculate_elo, elo_change};
use sudoku_core::protocol::{ClientMessage, GameMode, ServerMessage};
use sudoku_core::validation::is_board_complete;
use sudoku_core::{Board, Cell, Difficulty};

use crate::db;
use crate::state::*;

/// Top-level WebSocket handler -- spawned per connection.
pub async fn handle_socket(
    state: Arc<AppState>,
    mut socket: WebSocket,
    user_id: i64,
    username: String,
    rating: i32,
) {
    state.connection_count.fetch_add(1, Ordering::Relaxed);

    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Register connection handle.
    state.connections.insert(
        user_id,
        ConnectionHandle {
            user_id,
            username: username.clone(),
            rating,
            tx: tx.clone(),
            room_code: None,
            message_count: 0,
            rate_limit_window: Instant::now(),
        },
    );

    loop {
        tokio::select! {
            // Outbound: forward queued ServerMessage to the WebSocket.
            Some(msg) = rx.recv() => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            // Inbound: read from the WebSocket.
            maybe_msg = socket.recv() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        // Rate limiting: max 20 messages per second.
                        {
                            let mut conn = match state.connections.get_mut(&user_id) {
                                Some(c) => c,
                                None => break,
                            };
                            let now = Instant::now();
                            if now.duration_since(conn.rate_limit_window) > Duration::from_secs(1) {
                                conn.rate_limit_window = now;
                                conn.message_count = 0;
                            }
                            conn.message_count += 1;
                            if conn.message_count > 20 {
                                let _ = conn.tx.send(ServerMessage::Error {
                                    message: "Rate limited".into(),
                                });
                                continue;
                            }
                        }

                        let client_msg: ClientMessage = match serde_json::from_str(&text) {
                            Ok(m) => m,
                            Err(e) => {
                                let _ = tx.send(ServerMessage::Error {
                                    message: format!("Invalid message: {}", e),
                                });
                                continue;
                            }
                        };

                        handle_message(&state, user_id, &username, rating, &tx, client_msg).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    _ => continue,
                }
            }
        }
    }

    // Disconnected -- start grace period.
    let room_code = state
        .connections
        .get(&user_id)
        .and_then(|c| c.room_code.clone());

    if let Some(code) = room_code {
        // Notify opponent of disconnect.
        if let Some(opponent_id) = get_opponent(&state, &code, user_id) {
            send_to(&state, opponent_id, ServerMessage::OpponentDisconnected);
        }

        // 30-second grace period.
        let grace_state = state.clone();
        let grace_code = code.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            // If still disconnected (connection handle gone), forfeit.
            if !grace_state.connections.contains_key(&user_id) {
                forfeit_player(&grace_state, &grace_code, user_id).await;
            }
        });
    }

    // Remove matchmaking entries.
    for mut queue in state.matchmaking.iter_mut() {
        queue.value_mut().retain(|e| e.user_id != user_id);
    }

    state.connections.remove(&user_id);
    state.connection_count.fetch_sub(1, Ordering::Relaxed);
}

/// Public wrapper so the cleanup task in main.rs can call forfeit.
pub async fn forfeit_player_public(state: &AppState, room_code: &str, player_id: i64) {
    forfeit_player(state, room_code, player_id).await;
}

/// Dispatch a single client message.
async fn handle_message(
    state: &Arc<AppState>,
    user_id: i64,
    username: &str,
    rating: i32,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    msg: ClientMessage,
) {
    match msg {
        ClientMessage::Auth { token } => {
            // Already authenticated during WS upgrade; send confirmation.
            let _ = tx.send(ServerMessage::AuthOk {
                username: username.to_string(),
                rating,
            });
        }

        ClientMessage::CreateRoom { mode, difficulty } => {
            let (board, solution) = sudoku_core::puzzle::generate_puzzle(difficulty);
            let code = generate_room_code();

            let room = Room {
                code: code.clone(),
                mode,
                difficulty,
                state: RoomState::Waiting,
                player1_id: user_id,
                player2_id: None,
                board,
                solution,
                player_boards: {
                    let mut m = HashMap::new();
                    m.insert(user_id, board);
                    m
                },
                cell_ownership: HashMap::new(),
                shared_board: board,
                created_at: Instant::now(),
                last_activity: Instant::now(),
                started_at: None,
            };

            state.rooms.insert(code.clone(), room);

            // Associate connection with room.
            if let Some(mut conn) = state.connections.get_mut(&user_id) {
                conn.room_code = Some(code.clone());
            }

            let _ = tx.send(ServerMessage::RoomCreated { code });
            let _ = tx.send(ServerMessage::WaitingForOpponent);
        }

        ClientMessage::JoinRoom { code } => {
            let code = code.to_uppercase();
            let start_info = {
                let mut room = match state.rooms.get_mut(&code) {
                    Some(r) => r,
                    None => {
                        let _ = tx.send(ServerMessage::Error {
                            message: "Room not found".into(),
                        });
                        return;
                    }
                };

                if room.state != RoomState::Waiting {
                    let _ = tx.send(ServerMessage::Error {
                        message: "Room is not accepting players".into(),
                    });
                    return;
                }

                if room.player1_id == user_id {
                    let _ = tx.send(ServerMessage::Error {
                        message: "Cannot join your own room".into(),
                    });
                    return;
                }

                room.player2_id = Some(user_id);
                room.state = RoomState::Playing;
                room.started_at = Some(Instant::now());
                room.last_activity = Instant::now();
                let board_copy = room.board;
                room.player_boards.insert(user_id, board_copy);

                // Associate connection with room.
                if let Some(mut conn) = state.connections.get_mut(&user_id) {
                    conn.room_code = Some(code.clone());
                }

                Some((
                    room.mode,
                    room.difficulty,
                    board_to_wire(&room.board),
                    room.player1_id,
                ))
            };

            if let Some((mode, difficulty, wire_board, p1_id)) = start_info {
                let p1_name = state
                    .connections
                    .get(&p1_id)
                    .map(|c| c.username.clone())
                    .unwrap_or_default();
                let p1_rating = state
                    .connections
                    .get(&p1_id)
                    .map(|c| c.rating)
                    .unwrap_or(1200);

                // Send MatchStarted to player2 (joiner).
                let _ = tx.send(ServerMessage::MatchStarted {
                    mode,
                    difficulty,
                    board: wire_board.clone(),
                    opponent_name: p1_name,
                    opponent_rating: p1_rating,
                });

                // Send MatchStarted to player1 (creator).
                send_to(
                    state,
                    p1_id,
                    ServerMessage::MatchStarted {
                        mode,
                        difficulty,
                        board: wire_board,
                        opponent_name: username.to_string(),
                        opponent_rating: rating,
                    },
                );

                // For race mode, spawn progress broadcaster.
                if mode == GameMode::Race {
                    spawn_progress_broadcaster(state.clone(), code.clone(), user_id, p1_id);
                }
            }
        }

        ClientMessage::QuickMatch { mode, difficulty } => {
            let key = queue_key(mode, difficulty);

            // Try to find a match first.
            let matched = {
                let mut queue = state.matchmaking.entry(key.clone()).or_default();
                let now = Instant::now();

                let mut match_idx = None;
                for (i, entry) in queue.iter().enumerate() {
                    if entry.user_id == user_id {
                        // Already queued.
                        return;
                    }
                    let wait_secs = now.duration_since(entry.joined_at).as_secs();
                    let elo_range = if wait_secs > 30 { 400 } else { 200 };
                    if (rating - entry.rating).abs() <= elo_range {
                        match_idx = Some(i);
                        break;
                    }
                }

                if let Some(i) = match_idx {
                    Some(queue.remove(i))
                } else {
                    queue.push(QueueEntry {
                        user_id,
                        username: username.to_string(),
                        rating,
                        joined_at: now,
                    });
                    None
                }
            };

            if let Some(opponent) = matched {
                // Create a room and start the game.
                let (board, solution) = sudoku_core::puzzle::generate_puzzle(difficulty);
                let code = generate_room_code();

                let new_room = Room {
                    code: code.clone(),
                    mode,
                    difficulty,
                    state: RoomState::Playing,
                    player1_id: opponent.user_id,
                    player2_id: Some(user_id),
                    board,
                    solution,
                    player_boards: {
                        let mut m = HashMap::new();
                        m.insert(opponent.user_id, board);
                        m.insert(user_id, board);
                        m
                    },
                    cell_ownership: HashMap::new(),
                    shared_board: board,
                    created_at: Instant::now(),
                    last_activity: Instant::now(),
                    started_at: Some(Instant::now()),
                };

                state.rooms.insert(code.clone(), new_room);

                // Associate connections.
                if let Some(mut c) = state.connections.get_mut(&user_id) {
                    c.room_code = Some(code.clone());
                }
                if let Some(mut c) = state.connections.get_mut(&opponent.user_id) {
                    c.room_code = Some(code.clone());
                }

                let wire_board = board_to_wire(&board);

                // Send to opponent (player1).
                send_to(
                    state,
                    opponent.user_id,
                    ServerMessage::MatchStarted {
                        mode,
                        difficulty,
                        board: wire_board.clone(),
                        opponent_name: username.to_string(),
                        opponent_rating: rating,
                    },
                );

                // Send to us (player2).
                let _ = tx.send(ServerMessage::MatchStarted {
                    mode,
                    difficulty,
                    board: wire_board,
                    opponent_name: opponent.username,
                    opponent_rating: opponent.rating,
                });

                if mode == GameMode::Race {
                    spawn_progress_broadcaster(state.clone(), code, user_id, opponent.user_id);
                }
            } else {
                let _ = tx.send(ServerMessage::WaitingForOpponent);
            }
        }

        ClientMessage::PlaceNumber { row, col, value } => {
            let room_code =
                match state.connections.get(&user_id).and_then(|c| c.room_code.clone()) {
                    Some(c) => c,
                    None => {
                        let _ = tx.send(ServerMessage::Error {
                            message: "Not in a room".into(),
                        });
                        return;
                    }
                };

            if row >= 9 || col >= 9 || value < 1 || value > 9 {
                let _ = tx.send(ServerMessage::MoveRejected {
                    row,
                    col,
                    reason: "Invalid position or value".into(),
                });
                return;
            }

            let result = {
                let mut room = match state.rooms.get_mut(&room_code) {
                    Some(r) => r,
                    None => return,
                };

                if room.state != RoomState::Playing {
                    let _ = tx.send(ServerMessage::Error {
                        message: "Game is not in progress".into(),
                    });
                    return;
                }

                room.last_activity = Instant::now();

                // Check if the cell is a given.
                if room.board[row][col].is_given() {
                    let _ = tx.send(ServerMessage::MoveRejected {
                        row,
                        col,
                        reason: "Cannot modify a given cell".into(),
                    });
                    return;
                }

                // Accept any value 1-9 — correctness checked at game end.

                match room.mode {
                    GameMode::Race => {
                        // Pre-read fields to avoid borrow conflicts.
                        let p1_id = room.player1_id;
                        let p2_id = room.player2_id;
                        let solution = room.solution;
                        let duration = room
                            .started_at
                            .map(|s| s.elapsed().as_secs() as i64)
                            .unwrap_or(0);
                        let opponent_id = if p1_id == user_id { p2_id } else { Some(p1_id) };
                        let initial_board = room.board;

                        // Ensure player board exists.
                        if !room.player_boards.contains_key(&user_id) {
                            room.player_boards.insert(user_id, initial_board);
                        }
                        let player_board = room.player_boards.get_mut(&user_id).unwrap();
                        player_board[row][col] = Cell::UserInput(value);

                        // Board is "complete" when all cells are filled (not necessarily correct).
                        let all_filled = player_board.iter().all(|row| {
                            row.iter().all(|cell| cell.value().is_some())
                        });
                        let my_filled = filled_count(player_board);

                        // Score = correct placements only.
                        let my_correct = correct_count(player_board, &solution);

                        let opp_filled = opponent_id
                            .and_then(|oid| room.player_boards.get(&oid))
                            .map(|b| filled_count(b))
                            .unwrap_or(0);
                        let opp_correct = opponent_id
                            .and_then(|oid| room.player_boards.get(&oid))
                            .map(|b| correct_count(b, &solution))
                            .unwrap_or(0);

                        if all_filled {
                            room.state = RoomState::Ended;
                        }

                        PlaceResult::Race {
                            complete: all_filled,
                            opponent_id,
                            duration,
                            p1_id,
                            p2_id,
                            my_filled,
                            opp_filled,
                            my_correct,
                            opp_correct,
                        }
                    }
                    GameMode::Shared => {
                        // First-write-wins: if already placed by someone, reject.
                        if room.cell_ownership.contains_key(&(row, col)) {
                            let _ = tx.send(ServerMessage::MoveRejected {
                                row,
                                col,
                                reason: "Cell already claimed".into(),
                            });
                            return;
                        }

                        room.shared_board[row][col] = Cell::UserInput(value);
                        room.cell_ownership.insert((row, col), user_id);

                        let solution = room.solution;
                        // Board complete when all cells filled.
                        let all_filled = room.shared_board.iter().all(|row| {
                            row.iter().all(|cell| cell.value().is_some())
                        });
                        let opponent_id = if room.player1_id == user_id {
                            room.player2_id
                        } else {
                            Some(room.player1_id)
                        };

                        // Score = correct cells placed by each player.
                        let my_score = count_correct_for_player(
                            &room.cell_ownership,
                            &room.shared_board,
                            &solution,
                            user_id,
                        );
                        let opp_score = opponent_id
                            .map(|oid| count_correct_for_player(
                                &room.cell_ownership,
                                &room.shared_board,
                                &solution,
                                oid,
                            ))
                            .unwrap_or(0);

                        if all_filled {
                            room.state = RoomState::Ended;
                        }

                        PlaceResult::Shared {
                            complete: all_filled,
                            opponent_id,
                            my_score,
                            opp_score,
                            duration: room
                                .started_at
                                .map(|s| s.elapsed().as_secs() as i64)
                                .unwrap_or(0),
                            p1_id: room.player1_id,
                            p2_id: room.player2_id,
                        }
                    }
                }
            };

            // Send move accepted.
            let _ = tx.send(ServerMessage::MoveAccepted { row, col, value });

            match result {
                PlaceResult::Race {
                    complete,
                    opponent_id,
                    duration,
                    p1_id,
                    p2_id,
                    my_filled: _,
                    opp_filled: _,
                    my_correct,
                    opp_correct,
                } => {
                    if complete {
                        // Winner = most correct cells. Tie goes to the finisher.
                        let opp_id = opponent_id.unwrap_or(user_id);
                        let (winner_id, loser_id, w_score, l_score) =
                            if my_correct >= opp_correct {
                                (user_id, opp_id, my_correct, opp_correct)
                            } else {
                                (opp_id, user_id, opp_correct, my_correct)
                            };
                        end_game(
                            state, &room_code, winner_id, loser_id, w_score, l_score,
                            duration, p1_id, p2_id,
                        )
                        .await;
                    }
                }
                PlaceResult::Shared {
                    complete,
                    opponent_id,
                    my_score,
                    opp_score,
                    duration,
                    p1_id,
                    p2_id,
                } => {
                    // Broadcast to opponent.
                    if let Some(oid) = opponent_id {
                        send_to(state, oid, ServerMessage::OpponentPlaced { row, col, value });
                    }

                    if complete {
                        // Winner is the player with more cells.
                        let (winner_id, loser_id, w_score, l_score) = if my_score >= opp_score {
                            (user_id, opponent_id.unwrap_or(user_id), my_score, opp_score)
                        } else {
                            (
                                opponent_id.unwrap_or(user_id),
                                user_id,
                                opp_score,
                                my_score,
                            )
                        };
                        end_game(
                            state, &room_code, winner_id, loser_id, w_score, l_score, duration,
                            p1_id, p2_id,
                        )
                        .await;
                    }
                }
            }
        }

        ClientMessage::EraseNumber { row, col } => {
            let room_code =
                match state.connections.get(&user_id).and_then(|c| c.room_code.clone()) {
                    Some(c) => c,
                    None => return,
                };

            if row >= 9 || col >= 9 {
                return;
            }

            let opponent_id = {
                let mut room = match state.rooms.get_mut(&room_code) {
                    Some(r) => r,
                    None => return,
                };

                if room.state != RoomState::Playing {
                    return;
                }

                room.last_activity = Instant::now();

                if room.board[row][col].is_given() {
                    return;
                }

                match room.mode {
                    GameMode::Race => {
                        if let Some(player_board) = room.player_boards.get_mut(&user_id) {
                            player_board[row][col] = Cell::Empty;
                        }
                        None // No broadcast in race mode.
                    }
                    GameMode::Shared => {
                        // Only the owner can erase.
                        if room.cell_ownership.get(&(row, col)) != Some(&user_id) {
                            return;
                        }
                        room.shared_board[row][col] = Cell::Empty;
                        room.cell_ownership.remove(&(row, col));

                        if room.player1_id == user_id {
                            room.player2_id
                        } else {
                            Some(room.player1_id)
                        }
                    }
                }
            };

            if let Some(oid) = opponent_id {
                send_to(state, oid, ServerMessage::OpponentErased { row, col });
            }
        }

        ClientMessage::UpdateCursor { row, col } => {
            let room_code =
                match state.connections.get(&user_id).and_then(|c| c.room_code.clone()) {
                    Some(c) => c,
                    None => return,
                };

            if let Some(oid) = get_opponent(state, &room_code, user_id) {
                send_to(state, oid, ServerMessage::OpponentCursor { row, col });
            }
        }

        ClientMessage::Forfeit => {
            let room_code =
                match state.connections.get(&user_id).and_then(|c| c.room_code.clone()) {
                    Some(c) => c,
                    None => return,
                };
            forfeit_player(state, &room_code, user_id).await;
        }

        ClientMessage::Rematch => {
            let room_code =
                match state.connections.get(&user_id).and_then(|c| c.room_code.clone()) {
                    Some(c) => c,
                    None => return,
                };

            let new_room_info = {
                let room = match state.rooms.get(&room_code) {
                    Some(r) => r,
                    None => return,
                };
                if room.state != RoomState::Ended {
                    return;
                }
                let opponent_id = if room.player1_id == user_id {
                    room.player2_id
                } else {
                    Some(room.player1_id)
                };
                (room.mode, room.difficulty, opponent_id)
            };

            let (mode, difficulty, opponent_id) = new_room_info;
            let opponent_id = match opponent_id {
                Some(id) => id,
                None => return,
            };

            // Generate new puzzle and room.
            let (board, solution) = sudoku_core::puzzle::generate_puzzle(difficulty);
            let new_code = generate_room_code();

            let new_room = Room {
                code: new_code.clone(),
                mode,
                difficulty,
                state: RoomState::Playing,
                player1_id: user_id,
                player2_id: Some(opponent_id),
                board,
                solution,
                player_boards: {
                    let mut m = HashMap::new();
                    m.insert(user_id, board);
                    m.insert(opponent_id, board);
                    m
                },
                cell_ownership: HashMap::new(),
                shared_board: board,
                created_at: Instant::now(),
                last_activity: Instant::now(),
                started_at: Some(Instant::now()),
            };

            state.rooms.insert(new_code.clone(), new_room);

            // Update connections.
            if let Some(mut c) = state.connections.get_mut(&user_id) {
                c.room_code = Some(new_code.clone());
            }
            if let Some(mut c) = state.connections.get_mut(&opponent_id) {
                c.room_code = Some(new_code.clone());
            }

            let wire_board = board_to_wire(&board);
            let opp_name = state
                .connections
                .get(&opponent_id)
                .map(|c| c.username.clone())
                .unwrap_or_default();
            let opp_rating = state
                .connections
                .get(&opponent_id)
                .map(|c| c.rating)
                .unwrap_or(1200);

            let _ = tx.send(ServerMessage::MatchStarted {
                mode,
                difficulty,
                board: wire_board.clone(),
                opponent_name: opp_name,
                opponent_rating: opp_rating,
            });

            send_to(
                state,
                opponent_id,
                ServerMessage::MatchStarted {
                    mode,
                    difficulty,
                    board: wire_board,
                    opponent_name: username.to_string(),
                    opponent_rating: rating,
                },
            );

            if mode == GameMode::Race {
                spawn_progress_broadcaster(state.clone(), new_code, user_id, opponent_id);
            }

            // Clean up old room.
            state.rooms.remove(&room_code);
        }

        ClientMessage::Ping => {
            let _ = tx.send(ServerMessage::Pong);
        }
    }
}

// -- Helpers ------------------------------------------------------------------

enum PlaceResult {
    Race {
        complete: bool,
        opponent_id: Option<i64>,
        duration: i64,
        p1_id: i64,
        p2_id: Option<i64>,
        my_filled: u32,
        opp_filled: u32,
        my_correct: u32,
        opp_correct: u32,
    },
    Shared {
        complete: bool,
        opponent_id: Option<i64>,
        my_score: u32,
        opp_score: u32,
        duration: i64,
        p1_id: i64,
        p2_id: Option<i64>,
    },
}

/// Count correct cells placed by a specific player on the shared board.
fn count_correct_for_player(
    ownership: &HashMap<(usize, usize), i64>,
    board: &Board,
    solution: &[[u8; 9]; 9],
    player_id: i64,
) -> u32 {
    let mut count = 0u32;
    for ((r, c), owner) in ownership.iter() {
        if *owner == player_id {
            if let Some(v) = board[*r][*c].value() {
                if v == solution[*r][*c] {
                    count += 1;
                }
            }
        }
    }
    count
}

fn send_to(state: &AppState, user_id: i64, msg: ServerMessage) {
    if let Some(conn) = state.connections.get(&user_id) {
        let _ = conn.tx.send(msg);
    }
}

fn get_opponent(state: &AppState, room_code: &str, user_id: i64) -> Option<i64> {
    state.rooms.get(room_code).and_then(|room| {
        if room.player1_id == user_id {
            room.player2_id
        } else {
            Some(room.player1_id)
        }
    })
}

async fn forfeit_player(state: &AppState, room_code: &str, forfeiter_id: i64) {
    let info = {
        let mut room = match state.rooms.get_mut(room_code) {
            Some(r) => r,
            None => return,
        };

        if room.state != RoomState::Playing {
            return;
        }

        room.state = RoomState::Ended;

        let winner_id = if room.player1_id == forfeiter_id {
            room.player2_id
        } else {
            Some(room.player1_id)
        };

        let duration = room
            .started_at
            .map(|s| s.elapsed().as_secs() as i64)
            .unwrap_or(0);

        (
            winner_id,
            room.player1_id,
            room.player2_id,
            duration,
            room.mode,
            room.difficulty,
        )
    };

    let (winner_id, p1_id, p2_id, duration, mode, difficulty) = info;
    let winner_id = match winner_id {
        Some(id) => id,
        None => return,
    };
    let p2_id = match p2_id {
        Some(id) => id,
        None => return,
    };

    // Get ratings.
    let winner_rating = state
        .connections
        .get(&winner_id)
        .map(|c| c.rating)
        .unwrap_or(1200);
    let loser_rating = state
        .connections
        .get(&forfeiter_id)
        .map(|c| c.rating)
        .unwrap_or(1200);

    let new_winner_rating = calculate_elo(winner_rating, loser_rating, true);
    let new_loser_rating = calculate_elo(loser_rating, winner_rating, false);
    let winner_change = new_winner_rating - winner_rating;
    let loser_change = new_loser_rating - loser_rating;

    // Update DB.
    let _ = db::update_ratings(
        &state.db,
        winner_id,
        forfeiter_id,
        new_winner_rating,
        new_loser_rating,
    )
    .await;

    let (p1_elo_change, p2_elo_change) = if p1_id == winner_id {
        (winner_change, loser_change)
    } else {
        (loser_change, winner_change)
    };
    let _ = db::record_match(
        &state.db,
        p1_id,
        p2_id,
        &format!("{:?}", mode),
        &format!("{:?}", difficulty),
        Some(winner_id),
        p1_elo_change,
        p2_elo_change,
        duration,
    )
    .await;

    // Notify winner.
    send_to(
        state,
        winner_id,
        ServerMessage::GameEnd {
            won: true,
            your_score: 0,
            opponent_score: 0,
            elo_change: winner_change,
            new_rating: new_winner_rating,
        },
    );

    // Notify loser.
    send_to(
        state,
        forfeiter_id,
        ServerMessage::GameEnd {
            won: false,
            your_score: 0,
            opponent_score: 0,
            elo_change: loser_change,
            new_rating: new_loser_rating,
        },
    );

    // Update connection ratings.
    if let Some(mut c) = state.connections.get_mut(&winner_id) {
        c.rating = new_winner_rating;
    }
    if let Some(mut c) = state.connections.get_mut(&forfeiter_id) {
        c.rating = new_loser_rating;
    }
}

async fn end_game(
    state: &AppState,
    room_code: &str,
    winner_id: i64,
    loser_id: i64,
    winner_score: u32,
    loser_score: u32,
    duration: i64,
    p1_id: i64,
    p2_id: Option<i64>,
) {
    let p2_id = match p2_id {
        Some(id) => id,
        None => return,
    };

    let winner_rating = state
        .connections
        .get(&winner_id)
        .map(|c| c.rating)
        .unwrap_or(1200);
    let loser_rating = state
        .connections
        .get(&loser_id)
        .map(|c| c.rating)
        .unwrap_or(1200);

    let new_winner_rating = calculate_elo(winner_rating, loser_rating, true);
    let new_loser_rating = calculate_elo(loser_rating, winner_rating, false);
    let winner_change = new_winner_rating - winner_rating;
    let loser_change = new_loser_rating - loser_rating;

    let _ = db::update_ratings(
        &state.db,
        winner_id,
        loser_id,
        new_winner_rating,
        new_loser_rating,
    )
    .await;

    let room_mode = state
        .rooms
        .get(room_code)
        .map(|r| (r.mode, r.difficulty));
    if let Some((mode, difficulty)) = room_mode {
        let (p1_elo_change, p2_elo_change) = if p1_id == winner_id {
            (winner_change, loser_change)
        } else {
            (loser_change, winner_change)
        };
        let _ = db::record_match(
            &state.db,
            p1_id,
            p2_id,
            &format!("{:?}", mode),
            &format!("{:?}", difficulty),
            Some(winner_id),
            p1_elo_change,
            p2_elo_change,
            duration,
        )
        .await;
    }

    // Notify winner.
    send_to(
        state,
        winner_id,
        ServerMessage::GameEnd {
            won: true,
            your_score: winner_score,
            opponent_score: loser_score,
            elo_change: winner_change,
            new_rating: new_winner_rating,
        },
    );

    // Notify loser.
    send_to(
        state,
        loser_id,
        ServerMessage::GameEnd {
            won: false,
            your_score: loser_score,
            opponent_score: winner_score,
            elo_change: loser_change,
            new_rating: new_loser_rating,
        },
    );

    // Update connection ratings.
    if let Some(mut c) = state.connections.get_mut(&winner_id) {
        c.rating = new_winner_rating;
    }
    if let Some(mut c) = state.connections.get_mut(&loser_id) {
        c.rating = new_loser_rating;
    }
}

/// Spawn a task that broadcasts OpponentProgress every 2 seconds for race mode.
fn spawn_progress_broadcaster(state: Arc<AppState>, room_code: String, p1: i64, p2: i64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;

            let room = match state.rooms.get(&room_code) {
                Some(r) => r,
                None => break,
            };

            if room.state != RoomState::Playing {
                break;
            }

            let p1_filled = room
                .player_boards
                .get(&p1)
                .map(|b| filled_count(b))
                .unwrap_or(0);
            let p2_filled = room
                .player_boards
                .get(&p2)
                .map(|b| filled_count(b))
                .unwrap_or(0);

            drop(room);

            // Send p2's progress to p1.
            send_to(
                &state,
                p1,
                ServerMessage::OpponentProgress {
                    filled_count: p2_filled,
                    momentum: 0.0,
                },
            );

            // Send p1's progress to p2.
            send_to(
                &state,
                p2,
                ServerMessage::OpponentProgress {
                    filled_count: p1_filled,
                    momentum: 0.0,
                },
            );
        }
    });
}
