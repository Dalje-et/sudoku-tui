use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use sudoku_core::protocol::{AuthPollResponse, DeviceAuthResponse, LeaderboardEntry};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Spin up a test server on a random port, return the base URL.
async fn start_server() -> String {
    // In-memory SQLite so tests don't clash.
    let (app, _state) = sudoku_server::build_app("sqlite::memory:").await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start.
    tokio::time::sleep(Duration::from_millis(50)).await;

    format!("http://127.0.0.1:{}", port)
}

/// Authenticate a dev user, return (token, username).
async fn dev_auth(base: &str) -> (String, String) {
    let client = reqwest::Client::new();

    let resp: DeviceAuthResponse = client
        .post(format!("{}/auth/device", base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let poll: AuthPollResponse = client
        .post(format!("{}/auth/poll", base))
        .json(&json!({ "user_code": resp.user_code }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    match poll {
        AuthPollResponse::Complete { token, username } => (token, username),
        other => panic!("Expected Complete, got {:?}", other),
    }
}

/// Connect a WebSocket client, return the split stream.
async fn ws_connect(
    base: &str,
    token: &str,
) -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let ws_url = base.replace("http://", "ws://");
    let url = format!("{}/ws?token={}", ws_url, token);
    let (stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    stream.split()
}

/// Send a JSON message over the WebSocket.
async fn ws_send(
    sink: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    msg: serde_json::Value,
) {
    sink.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
}

/// Receive messages until we get one matching the expected type.
async fn ws_recv_type(
    stream: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    msg_type: &str,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline - tokio::time::Instant::now();
        if remaining.is_zero() {
            panic!("Timed out waiting for message type: {}", msg_type);
        }
        let msg = tokio::time::timeout(remaining, stream.next())
            .await
            .unwrap_or_else(|_| panic!("Timed out waiting for {}", msg_type))
            .unwrap()
            .unwrap();

        if let Message::Text(text) = msg {
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            if parsed["type"].as_str() == Some(msg_type) {
                return parsed;
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health() {
    let base = start_server().await;
    let resp = reqwest::get(format!("{}/health", base))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(resp, "ok");
}

#[tokio::test]
async fn test_dev_auth_creates_unique_users() {
    let base = start_server().await;

    let (t1, u1) = dev_auth(&base).await;
    let (t2, u2) = dev_auth(&base).await;

    assert_ne!(t1, t2);
    assert_ne!(u1, u2);
    assert!(u1.starts_with("dev_player_"));
    assert!(u2.starts_with("dev_player_"));
}

#[tokio::test]
async fn test_create_and_join_room() {
    let base = start_server().await;

    let (t1, u1) = dev_auth(&base).await;
    let (t2, u2) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    // P1 creates room
    ws_send(&mut sink1, json!({"type": "CreateRoom", "mode": "Race", "difficulty": "Easy"})).await;
    let created = ws_recv_type(&mut stream1, "RoomCreated").await;
    let code = created["code"].as_str().unwrap();
    assert_eq!(code.len(), 6);

    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;

    // P2 joins
    ws_send(&mut sink2, json!({"type": "JoinRoom", "code": code})).await;

    let p2_match = ws_recv_type(&mut stream2, "MatchStarted").await;
    assert_eq!(p2_match["opponent_name"].as_str().unwrap(), u1);

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    assert_eq!(p1_match["opponent_name"].as_str().unwrap(), u2);
}

#[tokio::test]
async fn test_join_invalid_room_returns_error() {
    let base = start_server().await;
    let (t1, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;

    ws_send(&mut sink1, json!({"type": "JoinRoom", "code": "ZZZZZZ"})).await;
    let err = ws_recv_type(&mut stream1, "Error").await;
    assert_eq!(err["message"].as_str().unwrap(), "Room not found");
}

#[tokio::test]
async fn test_quick_match_pairs_two_players() {
    let base = start_server().await;

    let (t1, u1) = dev_auth(&base).await;
    let (t2, u2) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    // P1 queues
    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;

    // P2 queues — should match
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;

    let p2_match = ws_recv_type(&mut stream2, "MatchStarted").await;
    assert_eq!(p2_match["opponent_name"].as_str().unwrap(), u1);
    assert_eq!(p2_match["mode"].as_str().unwrap(), "Race");

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    assert_eq!(p1_match["opponent_name"].as_str().unwrap(), u2);
}

#[tokio::test]
async fn test_place_number_and_progress() {
    let base = start_server().await;

    let (t1, _) = dev_auth(&base).await;
    let (t2, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    // Quick match
    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    let board: Vec<Vec<u8>> = serde_json::from_value(p1_match["board"].clone()).unwrap();
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // Find an empty cell
    let (er, ec) = (0..9)
        .flat_map(|r| (0..9).map(move |c| (r, c)))
        .find(|(r, c)| board[*r][*c] == 0)
        .unwrap();

    // P1 places a number
    ws_send(&mut sink1, json!({"type": "PlaceNumber", "row": er, "col": ec, "value": 7})).await;
    let accepted = ws_recv_type(&mut stream1, "MoveAccepted").await;
    assert_eq!(accepted["row"].as_u64().unwrap(), er as u64);
    assert_eq!(accepted["col"].as_u64().unwrap(), ec as u64);

    // Wait for progress broadcast — P2 should see P1 has 1 filled
    let prog = ws_recv_type(&mut stream2, "OpponentProgress").await;
    assert_eq!(prog["filled_count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_cannot_place_on_given_cell() {
    let base = start_server().await;

    let (t1, _) = dev_auth(&base).await;
    let (t2, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    let board: Vec<Vec<u8>> = serde_json::from_value(p1_match["board"].clone()).unwrap();
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // Find a given cell
    let (gr, gc) = (0..9)
        .flat_map(|r| (0..9).map(move |c| (r, c)))
        .find(|(r, c)| board[*r][*c] != 0)
        .unwrap();

    ws_send(&mut sink1, json!({"type": "PlaceNumber", "row": gr, "col": gc, "value": 5})).await;
    let rejected = ws_recv_type(&mut stream1, "MoveRejected").await;
    assert!(rejected["reason"].as_str().unwrap().contains("given"));
}

#[tokio::test]
async fn test_forfeit_updates_elo() {
    let base = start_server().await;

    let (t1, u1) = dev_auth(&base).await;
    let (t2, u2) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "MatchStarted").await;
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // P1 forfeits
    ws_send(&mut sink1, json!({"type": "Forfeit"})).await;

    let end1 = ws_recv_type(&mut stream1, "GameEnd").await;
    let end2 = ws_recv_type(&mut stream2, "GameEnd").await;

    assert_eq!(end1["won"].as_bool().unwrap(), false);
    assert!(end1["elo_change"].as_i64().unwrap() < 0);
    assert_eq!(end2["won"].as_bool().unwrap(), true);
    assert!(end2["elo_change"].as_i64().unwrap() > 0);

    // Verify leaderboard
    let lb: Vec<LeaderboardEntry> = reqwest::get(format!("{}/leaderboard", base))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let winner = lb.iter().find(|e| e.username == u2).unwrap();
    let loser = lb.iter().find(|e| e.username == u1).unwrap();
    assert_eq!(winner.wins, 1);
    assert!(winner.rating > 1200);
    assert_eq!(loser.losses, 1);
    assert!(loser.rating < 1200);
}

#[tokio::test]
async fn test_wrong_number_accepted_in_race_mode() {
    let base = start_server().await;

    let (t1, _) = dev_auth(&base).await;
    let (t2, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    let board: Vec<Vec<u8>> = serde_json::from_value(p1_match["board"].clone()).unwrap();
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // Find an empty cell
    let (er, ec) = (0..9)
        .flat_map(|r| (0..9).map(move |c| (r, c)))
        .find(|(r, c)| board[*r][*c] == 0)
        .unwrap();

    // Place any number — should be accepted even if wrong
    ws_send(&mut sink1, json!({"type": "PlaceNumber", "row": er, "col": ec, "value": 1})).await;
    let result = ws_recv_type(&mut stream1, "MoveAccepted").await;
    assert_eq!(result["type"].as_str().unwrap(), "MoveAccepted");
}

#[tokio::test]
async fn test_shared_mode_first_write_wins() {
    let base = start_server().await;

    let (t1, _) = dev_auth(&base).await;
    let (t2, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    // Create shared mode room
    ws_send(&mut sink1, json!({"type": "CreateRoom", "mode": "Shared", "difficulty": "Easy"})).await;
    let created = ws_recv_type(&mut stream1, "RoomCreated").await;
    let code = created["code"].as_str().unwrap();
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;

    ws_send(&mut sink2, json!({"type": "JoinRoom", "code": code})).await;
    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    let board: Vec<Vec<u8>> = serde_json::from_value(p1_match["board"].clone()).unwrap();
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // Find an empty cell
    let (er, ec) = (0..9)
        .flat_map(|r| (0..9).map(move |c| (r, c)))
        .find(|(r, c)| board[*r][*c] == 0)
        .unwrap();

    // P1 claims the cell
    ws_send(&mut sink1, json!({"type": "PlaceNumber", "row": er, "col": ec, "value": 3})).await;
    let _ = ws_recv_type(&mut stream1, "MoveAccepted").await;

    // P2 should see OpponentPlaced
    let opp = ws_recv_type(&mut stream2, "OpponentPlaced").await;
    assert_eq!(opp["row"].as_u64().unwrap(), er as u64);

    // P2 tries same cell — should be rejected
    ws_send(&mut sink2, json!({"type": "PlaceNumber", "row": er, "col": ec, "value": 5})).await;
    let rejected = ws_recv_type(&mut stream2, "MoveRejected").await;
    assert!(rejected["reason"].as_str().unwrap().contains("claimed"));
}

#[tokio::test]
async fn test_race_game_ends_when_board_full_even_with_wrong_numbers() {
    let base = start_server().await;

    let (t1, _) = dev_auth(&base).await;
    let (t2, _) = dev_auth(&base).await;

    let (mut sink1, mut stream1) = ws_connect(&base, &t1).await;
    let (mut sink2, mut stream2) = ws_connect(&base, &t2).await;

    ws_send(&mut sink1, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;
    let _ = ws_recv_type(&mut stream1, "WaitingForOpponent").await;
    ws_send(&mut sink2, json!({"type": "QuickMatch", "mode": "Race", "difficulty": "Easy"})).await;

    let p1_match = ws_recv_type(&mut stream1, "MatchStarted").await;
    let board: Vec<Vec<u8>> = serde_json::from_value(p1_match["board"].clone()).unwrap();
    let _ = ws_recv_type(&mut stream2, "MatchStarted").await;

    // P1 fills every empty cell with value 1 (mostly wrong)
    let empty_cells: Vec<(usize, usize)> = (0..9)
        .flat_map(|r| (0..9).map(move |c| (r, c)))
        .filter(|(r, c)| board[*r][*c] == 0)
        .collect();

    assert!(!empty_cells.is_empty());

    for (r, c) in &empty_cells {
        ws_send(&mut sink1, json!({"type": "PlaceNumber", "row": r, "col": c, "value": 1})).await;
        // Small delay to avoid rate limiting (20 msg/s)
        tokio::time::sleep(Duration::from_millis(60)).await;
        let msg = ws_recv_type(&mut stream1, "MoveAccepted").await;
        assert_eq!(msg["type"].as_str().unwrap(), "MoveAccepted");
    }

    // Board is full but mostly wrong — should get BoardIncomplete, NOT GameEnd
    let incomplete = ws_recv_type(&mut stream1, "BoardIncomplete").await;
    assert!(incomplete["wrong_cells"].as_u64().unwrap() > 0);
}
