use std::io;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::task::JoinHandle;

use crate::game::{Game, GameState};
use crate::net::NetworkClient;
use crate::ui;
use sudoku_core::protocol::{
    AuthPollResponse, ClientMessage, DeviceAuthResponse, GameMode, LeaderboardEntry, ServerMessage,
};
use sudoku_core::Cell;

/// Result types for background async operations
enum AsyncResult {
    AuthStarted(Result<DeviceAuthResponse, String>),
    Connected(Result<NetworkClient, String>),
    DevConnected(Result<(NetworkClient, String), String>),
    LeaderboardLoaded(Result<Vec<LeaderboardEntry>, String>),
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Install rustls crypto provider before any TLS usage
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_run())
}

async fn async_run() -> Result<(), Box<dyn std::error::Error>> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new();
    let mut net_client: Option<NetworkClient> = None;
    let mut username: Option<String> = None;
    let mut saved_token: Option<String> = None;

    if let Some((token, name)) = NetworkClient::load_token() {
        username = Some(name);
        saved_token = Some(token);
    }

    let result = run_loop(
        &mut terminal,
        &mut game,
        &mut net_client,
        &mut username,
        &mut saved_token,
    )
    .await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    game: &mut Game,
    net_client: &mut Option<NetworkClient>,
    username: &mut Option<String>,
    saved_token: &mut Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_stream = EventStream::new();
    let tick_rate = Duration::from_millis(250);
    let mut auth_poll_deadline = tokio::time::Instant::now() + Duration::from_secs(60);

    // In-flight background task (only one at a time)
    let mut inflight: Option<JoinHandle<AsyncResult>> = None;

    loop {
        terminal.draw(|f| ui::draw(f, game))?;

        // Spawn background tasks for pending async operations.
        // These run concurrently so the UI stays responsive.
        if game.pending_auth_start && inflight.is_none() {
            game.pending_auth_start = false;
            game.state = GameState::AuthScreen;
            game.auth_status = Some("Connecting to server...".to_string());

            inflight = Some(tokio::spawn(async {
                AsyncResult::AuthStarted(
                    NetworkClient::start_device_auth()
                        .await
                        .map_err(|e| e.to_string()),
                )
            }));
        }

        if game.pending_connect && inflight.is_none() {
            game.pending_connect = false;
            game.auth_status = Some("Connecting...".to_string());

            if crate::net::client::is_local() && saved_token.is_none() {
                inflight = Some(tokio::spawn(async {
                    AsyncResult::DevConnected(
                        NetworkClient::dev_auth_and_connect()
                            .await
                            .map_err(|e| e.to_string()),
                    )
                }));
            } else if let Some(token) = saved_token.clone() {
                inflight = Some(tokio::spawn(async move {
                    AsyncResult::Connected(
                        NetworkClient::connect(&token)
                            .await
                            .map_err(|e| e.to_string()),
                    )
                }));
            }
        }

        if game.pending_leaderboard && inflight.is_none() {
            game.pending_leaderboard = false;
            game.auth_status = Some("Loading leaderboard...".to_string());

            inflight = Some(tokio::spawn(async {
                AsyncResult::LeaderboardLoaded(
                    NetworkClient::fetch_leaderboard()
                        .await
                        .map_err(|e| e.to_string()),
                )
            }));
        }

        // Build a future that resolves when the inflight task completes,
        // or pends forever if there is no inflight task.
        let inflight_fut = async {
            match &mut inflight {
                Some(handle) => handle.await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            maybe_event = event_stream.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_event {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    // Allow Esc to cancel in-flight operations
                    if key.code == KeyCode::Esc && inflight.is_some() {
                        if let Some(handle) = inflight.take() {
                            handle.abort();
                        }
                        game.auth_status = None;
                        game.pending_menu_action = None;
                        game.state = GameState::MultiplayerMenu;
                        continue;
                    }
                    if handle_key(game, key, net_client, username, saved_token) {
                        return Ok(());
                    }
                }
            }
            result = inflight_fut => {
                inflight = None;
                match result {
                    Ok(AsyncResult::AuthStarted(Ok(resp))) => {
                        game.auth_code = Some(resp.user_code);
                        game.auth_uri = Some(resp.verification_uri);
                        game.auth_poll_interval = resp.interval.max(5);
                        game.auth_polling = true;
                        auth_poll_deadline = tokio::time::Instant::now()
                            + Duration::from_secs(game.auth_poll_interval);
                        game.auth_status =
                            Some("Please enter the code shown below at the URL".to_string());
                    }
                    Ok(AsyncResult::AuthStarted(Err(e))) => {
                        game.auth_status = Some(format!("Auth failed: {}", e));
                        game.auth_polling = false;
                    }
                    Ok(AsyncResult::Connected(Ok(client))) => {
                        *net_client = Some(client);
                        game.auth_status = None;
                        if let Some(action) = game.pending_menu_action.take() {
                            execute_menu_action(game, action, net_client);
                        } else {
                            // Post-auth connect: return to multiplayer menu
                            game.state = GameState::MultiplayerMenu;
                        }
                    }
                    Ok(AsyncResult::Connected(Err(e))) => {
                        // Clear stale token so next attempt triggers re-auth
                        // (e.g. server DB was wiped on redeploy)
                        *saved_token = None;
                        *username = None;
                        NetworkClient::clear_token();
                        game.error_message = Some(format!("Connection failed: {} — please try again to re-authenticate", e));
                        game.pending_menu_action = None;
                        game.auth_status = None;
                        game.state = GameState::MultiplayerMenu;
                    }
                    Ok(AsyncResult::DevConnected(Ok((client, name)))) => {
                        *username = Some(name);
                        *net_client = Some(client);
                        game.auth_status = None;
                        if let Some(action) = game.pending_menu_action.take() {
                            execute_menu_action(game, action, net_client);
                        }
                    }
                    Ok(AsyncResult::DevConnected(Err(e))) => {
                        game.error_message = Some(format!("Connection failed: {}", e));
                        game.pending_menu_action = None;
                        game.auth_status = None;
                    }
                    Ok(AsyncResult::LeaderboardLoaded(Ok(entries))) => {
                        game.leaderboard_entries = entries;
                        game.leaderboard_scroll = 0;
                        game.state = GameState::Leaderboard;
                        game.auth_status = None;
                    }
                    Ok(AsyncResult::LeaderboardLoaded(Err(e))) => {
                        game.error_message = Some(format!("Failed to load: {}", e));
                        game.auth_status = None;
                    }
                    Err(_) => {
                        // JoinHandle error (task panicked or was cancelled)
                        game.error_message = Some("Operation failed".to_string());
                        game.auth_status = None;
                        game.pending_menu_action = None;
                    }
                }
            }
            server_msg = recv_server_msg(net_client) => {
                if let Some(msg) = server_msg {
                    handle_server_message(game, msg);
                }
            }
            _ = tokio::time::sleep_until(auth_poll_deadline), if game.auth_polling => {
                if let Some(code) = game.auth_code.clone() {
                    match NetworkClient::poll_auth(&code).await {
                        Ok(AuthPollResponse::Complete { token, username: name }) => {
                            let _ = NetworkClient::save_token(&token, &name);
                            *username = Some(name.clone());
                            *saved_token = Some(token.clone());
                            game.auth_polling = false;
                            game.auth_code = None;
                            game.auth_uri = None;

                            // Spawn connect as a background task too
                            let t = token.clone();
                            inflight = Some(tokio::spawn(async move {
                                AsyncResult::Connected(
                                    NetworkClient::connect(&t)
                                        .await
                                        .map_err(|e| e.to_string()),
                                )
                            }));
                            game.auth_status = Some(format!("Logged in as {} — connecting...", name));
                        }
                        Ok(AuthPollResponse::Pending) => {
                            auth_poll_deadline = tokio::time::Instant::now()
                                + Duration::from_secs(game.auth_poll_interval);
                        }
                        Ok(AuthPollResponse::Expired) => {
                            game.auth_polling = false;
                            game.auth_status = Some("Auth code expired. Try again.".to_string());
                        }
                        Err(e) => {
                            game.auth_status = Some(format!("Poll error: {}", e));
                            auth_poll_deadline = tokio::time::Instant::now()
                                + Duration::from_secs(game.auth_poll_interval);
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tick_rate) => {}
        }
    }
}

async fn recv_server_msg(net_client: &mut Option<NetworkClient>) -> Option<ServerMessage> {
    match net_client {
        Some(client) => client.receiver.recv().await,
        None => std::future::pending::<Option<ServerMessage>>().await,
    }
}

fn handle_server_message(game: &mut Game, msg: ServerMessage) {
    match msg {
        ServerMessage::AuthOk { username, rating } => {
            game.auth_status = Some(format!("Logged in as {} ({})", username, rating));
        }
        ServerMessage::RoomCreated { code } => {
            game.room_code = Some(code);
            game.state = GameState::Lobby;
        }
        ServerMessage::WaitingForOpponent => {
            game.state = GameState::Lobby;
        }
        ServerMessage::MatchStarted {
            mode,
            difficulty,
            board: board_data,
            opponent_name,
            opponent_rating,
        } => {
            let mut board = [[Cell::Empty; 9]; 9];
            let mut solution = [[0u8; 9]; 9];
            for r in 0..9 {
                for c in 0..9 {
                    let v = board_data[r][c];
                    if v != 0 {
                        board[r][c] = Cell::Given(v);
                    }
                    solution[r][c] = v;
                }
            }
            game.difficulty = difficulty;
            game.start_multiplayer_game(board, solution, mode, opponent_name, opponent_rating);
        }
        ServerMessage::MoveAccepted { .. } => {}
        ServerMessage::MoveRejected { row, col, reason } => {
            game.board[row][col] = Cell::Empty;
            game.error_message = Some(reason);
        }
        ServerMessage::OpponentProgress {
            filled_count,
            momentum,
        } => {
            if let Some(mp) = &mut game.multiplayer {
                mp.opponent_filled = filled_count;
                mp.opponent_momentum = momentum;
            }
        }
        ServerMessage::OpponentPlaced { row, col, value } => {
            if let Some(mp) = &mut game.multiplayer {
                mp.cell_owner[row][col] = crate::game::CellOwner::Opponent;
            }
            game.board[row][col] = Cell::UserInput(value);
        }
        ServerMessage::OpponentErased { row, col } => {
            if let Some(mp) = &mut game.multiplayer {
                mp.cell_owner[row][col] = crate::game::CellOwner::None;
            }
            game.board[row][col] = Cell::Empty;
        }
        ServerMessage::OpponentCursor { row, col } => {
            if let Some(mp) = &mut game.multiplayer {
                mp.opponent_cursor = Some((row, col));
            }
        }
        ServerMessage::GameEnd {
            won,
            your_score,
            opponent_score,
            elo_change,
            new_rating,
        } => {
            if let Some(start) = game.timer_start {
                game.elapsed_secs = game.paused_elapsed + start.elapsed().as_secs();
            }
            if let Some(mp) = &mut game.multiplayer {
                mp.result = Some(crate::game::GameResult {
                    won,
                    your_score,
                    opponent_score,
                    elo_change,
                    new_rating,
                });
            }
            game.state = GameState::MultiplayerEnd;
        }
        ServerMessage::BoardIncomplete { wrong_cells } => {
            game.error_message = Some(format!("{} cells are incorrect — fix them!", wrong_cells));
        }
        ServerMessage::OpponentDisconnected => {}
        ServerMessage::OpponentReconnected => {}
        ServerMessage::Error { message } => {
            game.error_message = Some(message);
        }
        ServerMessage::Pong => {}
    }
}

// handle_key is now sync — all async work is deferred via pending_* flags
fn handle_key(
    game: &mut Game,
    key: KeyEvent,
    net_client: &mut Option<NetworkClient>,
    username: &mut Option<String>,
    saved_token: &mut Option<String>,
) -> bool {
    match game.state {
        GameState::Menu => handle_menu_key(game, key),
        GameState::Playing => handle_playing_key(game, key),
        GameState::Paused => handle_paused_key(game, key),
        GameState::Won => handle_won_key(game, key),
        GameState::MultiplayerMenu => {
            handle_multiplayer_menu_key(game, key, net_client, username, saved_token)
        }
        GameState::AuthScreen => handle_auth_key(game, key),
        GameState::Lobby => handle_lobby_key(game, key),
        GameState::MultiplayerPlaying => handle_multiplayer_playing_key(game, key, net_client),
        GameState::MultiplayerEnd => handle_multiplayer_end_key(game, key, net_client),
        GameState::Leaderboard => handle_leaderboard_key(game, key),
    }
}

fn handle_menu_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Up | KeyCode::Left => game.difficulty = game.difficulty.prev(),
        KeyCode::Down | KeyCode::Right => game.difficulty = game.difficulty.next(),
        KeyCode::Enter => game.start_new_game(),
        KeyCode::Char('m') | KeyCode::Char('M') => {
            game.state = GameState::MultiplayerMenu;
            game.menu_selection = 0;
        }
        KeyCode::Char('q') | KeyCode::Esc => return true,
        _ => {}
    }
    false
}

fn handle_playing_key(game: &mut Game, key: KeyEvent) -> bool {
    if game.show_quit_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => return true,
            _ => game.show_quit_confirm = false,
        }
        return false;
    }

    if game.active_hint.is_some() {
        match key.code {
            KeyCode::Char('?') => game.request_hint(),
            KeyCode::Esc => game.dismiss_hint(),
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Up => game.move_cursor(-1, 0),
        KeyCode::Down => game.move_cursor(1, 0),
        KeyCode::Left => game.move_cursor(0, -1),
        KeyCode::Right => game.move_cursor(0, 1),
        KeyCode::Char(c) => return handle_playing_char(game, c, key.modifiers),
        KeyCode::Delete | KeyCode::Backspace => game.erase(),
        KeyCode::Esc => game.show_quit_confirm = true,
        _ => {}
    }
    false
}

fn handle_playing_char(game: &mut Game, c: char, modifiers: KeyModifiers) -> bool {
    match c {
        '1'..='9' => game.place_number(c as u8 - b'0'),
        '0' => game.erase(),
        'p' | 'P' => game.pencil_mode = !game.pencil_mode,
        '?' => game.request_hint(),
        'u' | 'U' => game.undo(),
        'z' if modifiers.contains(KeyModifiers::CONTROL) => game.undo(),
        'v' | 'V' => game.validate(),
        ' ' => game.toggle_pause(),
        'q' | 'Q' => game.show_quit_confirm = true,
        _ => {}
    }
    false
}

fn handle_paused_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Enter => game.toggle_pause(),
        _ => {}
    }
    false
}

fn handle_won_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter | KeyCode::Char('n') => game.state = GameState::Menu,
        KeyCode::Char('q') | KeyCode::Esc => return true,
        _ => {}
    }
    false
}

// ── Multiplayer key handlers ────────────────────────────────────────────

const MP_MENU_ITEMS: &[&str] = &[
    "Create Room",
    "Join Room",
    "Quick Match",
    "Leaderboard",
    "Back",
];

fn handle_multiplayer_menu_key(
    game: &mut Game,
    key: KeyEvent,
    net_client: &mut Option<NetworkClient>,
    username: &mut Option<String>,
    saved_token: &mut Option<String>,
) -> bool {
    game.error_message = None;

    if game.joining_room {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_alphanumeric() && game.room_input.len() < 6 => {
                game.room_input.push(c.to_ascii_uppercase());
            }
            KeyCode::Backspace => {
                game.room_input.pop();
            }
            KeyCode::Enter if game.room_input.len() == 6 => {
                if let Some(client) = net_client.as_ref() {
                    client.send(ClientMessage::JoinRoom {
                        code: game.room_input.clone(),
                    });
                }
                game.joining_room = false;
            }
            KeyCode::Esc => {
                game.joining_room = false;
                game.room_input.clear();
            }
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Up => {
            if game.menu_selection > 0 {
                game.menu_selection -= 1;
            } else {
                game.menu_selection = MP_MENU_ITEMS.len() - 1;
            }
        }
        KeyCode::Down => {
            game.menu_selection = (game.menu_selection + 1) % MP_MENU_ITEMS.len();
        }
        KeyCode::Enter => {
            // Items 0-3 require auth + connection
            if game.menu_selection < 4 && net_client.is_none() {
                if crate::net::client::is_local() {
                    // Dev mode: silent auto-auth+connect
                    game.pending_connect = true;
                    game.pending_menu_action = Some(game.menu_selection);
                } else if username.is_none() {
                    // Production: GitHub device flow
                    game.pending_auth_start = true;
                } else if saved_token.is_some() {
                    // Already authed, just need to connect
                    game.pending_connect = true;
                    game.pending_menu_action = Some(game.menu_selection);
                }
                return false;
            }

            execute_menu_action(game, game.menu_selection, net_client);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            game.state = GameState::Menu;
        }
        _ => {}
    }
    false
}

/// Execute a multiplayer menu action (called after auth + connect are ready)
fn execute_menu_action(
    game: &mut Game,
    action: usize,
    net_client: &mut Option<NetworkClient>,
) {
    match action {
        0 => {
            // Create Room
            if let Some(client) = net_client.as_ref() {
                client.send(ClientMessage::CreateRoom {
                    mode: GameMode::Race,
                    difficulty: game.difficulty,
                });
            }
        }
        1 => {
            // Join Room
            game.joining_room = true;
            game.room_input.clear();
        }
        2 => {
            // Quick Match
            if let Some(client) = net_client.as_ref() {
                client.send(ClientMessage::QuickMatch {
                    mode: GameMode::Race,
                    difficulty: game.difficulty,
                });
            }
            game.state = GameState::Lobby;
            game.room_code = None;
        }
        3 => {
            // Leaderboard — defer to async
            game.pending_leaderboard = true;
        }
        4 => {
            // Back
            game.state = GameState::Menu;
        }
        _ => {}
    }
}

fn handle_auth_key(game: &mut Game, key: KeyEvent) -> bool {
    if let KeyCode::Esc = key.code {
        game.state = GameState::MultiplayerMenu;
        game.auth_code = None;
        game.auth_uri = None;
        game.auth_polling = false;
    }
    false
}

fn handle_lobby_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            game.state = GameState::MultiplayerMenu;
            game.room_code = None;
        }
        _ => {}
    }
    false
}

fn handle_leaderboard_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Up => {
            if game.leaderboard_scroll > 0 {
                game.leaderboard_scroll -= 1;
            }
        }
        KeyCode::Down => {
            let max_scroll = game.leaderboard_entries.len().saturating_sub(20);
            if game.leaderboard_scroll < max_scroll {
                game.leaderboard_scroll += 1;
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            game.state = GameState::MultiplayerMenu;
        }
        _ => {}
    }
    false
}

fn handle_multiplayer_playing_key(
    game: &mut Game,
    key: KeyEvent,
    net_client: &mut Option<NetworkClient>,
) -> bool {
    if game.show_quit_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(client) = net_client.as_ref() {
                    client.send(ClientMessage::Forfeit);
                }
                game.state = GameState::MultiplayerMenu;
                game.show_quit_confirm = false;
                return false;
            }
            _ => game.show_quit_confirm = false,
        }
        return false;
    }

    game.error_message = None;

    match key.code {
        KeyCode::Up => {
            game.move_cursor(-1, 0);
            send_cursor_update(game, net_client);
        }
        KeyCode::Down => {
            game.move_cursor(1, 0);
            send_cursor_update(game, net_client);
        }
        KeyCode::Left => {
            game.move_cursor(0, -1);
            send_cursor_update(game, net_client);
        }
        KeyCode::Right => {
            game.move_cursor(0, 1);
            send_cursor_update(game, net_client);
        }
        KeyCode::Char(ch @ '1'..='9') => {
            let num = ch as u8 - b'0';
            let r = game.selected_row;
            let c = game.selected_col;

            if game.pencil_mode {
                game.place_number(num);
            } else {
                game.place_number(num);
                if let Some(client) = net_client.as_ref() {
                    client.send(ClientMessage::PlaceNumber {
                        row: r,
                        col: c,
                        value: num,
                    });
                }
            }
        }
        KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('0') => {
            let r = game.selected_row;
            let c = game.selected_col;
            game.erase();
            if let Some(client) = net_client.as_ref() {
                client.send(ClientMessage::EraseNumber { row: r, col: c });
            }
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            game.pencil_mode = !game.pencil_mode;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            game.show_quit_confirm = true;
        }
        _ => {}
    }
    false
}

fn handle_multiplayer_end_key(
    game: &mut Game,
    key: KeyEvent,
    net_client: &mut Option<NetworkClient>,
) -> bool {
    match key.code {
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if let Some(client) = net_client.as_ref() {
                client.send(ClientMessage::Rematch);
            }
        }
        KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => {
            game.state = GameState::MultiplayerMenu;
            game.multiplayer = None;
        }
        _ => {}
    }
    false
}

fn send_cursor_update(game: &Game, net_client: &mut Option<NetworkClient>) {
    if let Some(client) = net_client.as_ref() {
        client.send(ClientMessage::UpdateCursor {
            row: game.selected_row,
            col: game.selected_col,
        });
    }
}
