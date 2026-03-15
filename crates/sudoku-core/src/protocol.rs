use serde::{Deserialize, Serialize};

use crate::difficulty::Difficulty;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Race,
    Shared,
}

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Auth {
        token: String,
    },
    CreateRoom {
        mode: GameMode,
        difficulty: Difficulty,
    },
    JoinRoom {
        code: String,
    },
    QuickMatch {
        mode: GameMode,
        difficulty: Difficulty,
    },
    PlaceNumber {
        row: usize,
        col: usize,
        value: u8,
    },
    EraseNumber {
        row: usize,
        col: usize,
    },
    UpdateCursor {
        row: usize,
        col: usize,
    },
    Forfeit,
    Rematch,
    Ping,
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AuthOk {
        username: String,
        rating: i32,
    },
    RoomCreated {
        code: String,
    },
    WaitingForOpponent,
    MatchStarted {
        mode: GameMode,
        difficulty: Difficulty,
        /// Serialized board (givens only)
        board: Vec<Vec<u8>>,
        opponent_name: String,
        opponent_rating: i32,
    },
    MoveAccepted {
        row: usize,
        col: usize,
        value: u8,
    },
    MoveRejected {
        row: usize,
        col: usize,
        reason: String,
    },
    /// Board is full but has incorrect cells — keep playing
    BoardIncomplete {
        wrong_cells: u32,
    },
    /// Race mode: opponent's progress (bitmap of filled cells + momentum)
    OpponentProgress {
        filled_count: u32,
        momentum: f32,
    },
    /// Shared mode: opponent placed a number
    OpponentPlaced {
        row: usize,
        col: usize,
        value: u8,
    },
    /// Shared mode: opponent erased their own number
    OpponentErased {
        row: usize,
        col: usize,
    },
    OpponentCursor {
        row: usize,
        col: usize,
    },
    GameEnd {
        won: bool,
        your_score: u32,
        opponent_score: u32,
        elo_change: i32,
        new_rating: i32,
    },
    OpponentDisconnected,
    OpponentReconnected,
    Error {
        message: String,
    },
    Pong,
}

/// Leaderboard entry returned by REST API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub username: String,
    pub rating: i32,
    pub wins: u32,
    pub losses: u32,
}

/// Player profile returned by REST API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub username: String,
    pub avatar_url: String,
    pub rating: i32,
    pub wins: u32,
    pub losses: u32,
}

/// Device auth flow response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthResponse {
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
}

/// Auth poll response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum AuthPollResponse {
    Pending,
    Complete { token: String, username: String },
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_roundtrip_place_number() {
        let msg = ClientMessage::PlaceNumber { row: 3, col: 7, value: 5 };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ClientMessage::PlaceNumber { row: 3, col: 7, value: 5 }));
    }

    #[test]
    fn client_message_roundtrip_create_room() {
        let msg = ClientMessage::CreateRoom { mode: GameMode::Shared, difficulty: Difficulty::Hard };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ClientMessage::CreateRoom { mode: GameMode::Shared, difficulty: Difficulty::Hard }));
    }

    #[test]
    fn client_message_roundtrip_unit_variants() {
        for msg in [ClientMessage::Forfeit, ClientMessage::Rematch, ClientMessage::Ping] {
            let json = serde_json::to_string(&msg).unwrap();
            let _: ClientMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn server_message_roundtrip_game_end() {
        let msg = ServerMessage::GameEnd {
            won: true,
            your_score: 42,
            opponent_score: 30,
            elo_change: 16,
            new_rating: 1216,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ServerMessage::GameEnd { won: true, elo_change: 16, .. }));
    }

    #[test]
    fn server_message_roundtrip_unit_variants() {
        for msg in [ServerMessage::WaitingForOpponent, ServerMessage::OpponentDisconnected, ServerMessage::Pong] {
            let json = serde_json::to_string(&msg).unwrap();
            let _: ServerMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn auth_poll_response_tagged_serde() {
        let pending = AuthPollResponse::Pending;
        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("\"status\":\"Pending\""));

        let complete = AuthPollResponse::Complete {
            token: "abc".into(),
            username: "player1".into(),
        };
        let json = serde_json::to_string(&complete).unwrap();
        let parsed: AuthPollResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, AuthPollResponse::Complete { .. }));
    }
}
