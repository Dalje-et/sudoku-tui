#![allow(unused)]

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use sudoku_core::protocol::{
    AuthPollResponse, DeviceAuthResponse, LeaderboardEntry, PlayerProfile,
};

use crate::db;
use crate::state::AppState;
use crate::ws;

fn is_dev_mode() -> bool {
    std::env::var("GITHUB_CLIENT_ID").is_err()
}

// ── Health ──────────────────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

// ── Device Auth (GitHub or Dev Mode) ────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhDeviceCode {
    user_code: String,
    device_code: String,
    verification_uri: String,
    interval: u64,
}

/// Counter for generating unique dev user codes
static DEV_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub async fn device_auth(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DeviceAuthResponse>, StatusCode> {
    if is_dev_mode() {
        // Dev mode: generate a fake code that poll will recognize
        let n = DEV_COUNTER.fetch_add(1, Ordering::Relaxed);
        let user_code = format!("DEV-{:04}", n);

        // Stash it so poll can find it
        state.sessions.insert(
            format!("device:{}", user_code),
            crate::state::Session {
                user_id: 0,
                username: format!("dev_player_{}", n),
                expires_at: String::new(),
            },
        );

        return Ok(Json(DeviceAuthResponse {
            user_code,
            verification_uri: "http://localhost (dev mode - no action needed)".to_string(),
            interval: 1,
        }));
    }

    let client_id =
        std::env::var("GITHUB_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", &client_id), ("scope", &"read:user".to_string())])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let body: GhDeviceCode = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    state.sessions.insert(
        format!("device:{}", body.user_code),
        crate::state::Session {
            user_id: 0,
            username: body.device_code.clone(),
            expires_at: String::new(),
        },
    );

    Ok(Json(DeviceAuthResponse {
        user_code: body.user_code,
        verification_uri: body.verification_uri,
        interval: body.interval,
    }))
}

// ── Auth Poll ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AuthPollRequest {
    pub user_code: String,
}

#[derive(Deserialize)]
struct GhTokenResp {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct GhUser {
    id: u64,
    login: String,
    avatar_url: String,
}

pub async fn auth_poll(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthPollRequest>,
) -> Result<Json<AuthPollResponse>, StatusCode> {
    let device_key = format!("device:{}", req.user_code);

    if is_dev_mode() {
        // Dev mode: immediately create a user and return success
        let session = state
            .sessions
            .get(&device_key)
            .map(|s| s.username.clone());
        let dev_username = match session {
            Some(name) => name,
            None => return Ok(Json(AuthPollResponse::Expired)),
        };

        // Upsert dev user in DB (use username as github_id)
        let user_id = db::upsert_user(&state.db, &dev_username, &dev_username, "")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Create session
        let token = db::create_session(&state.db, user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Clean up device entry
        state.sessions.remove(&device_key);

        println!("[dev] Authenticated user: {} (id={})", dev_username, user_id);

        return Ok(Json(AuthPollResponse::Complete {
            token,
            username: dev_username,
        }));
    }

    // Production: GitHub OAuth flow
    let client_id =
        std::env::var("GITHUB_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let device_code = state
        .sessions
        .get(&device_key)
        .map(|s| s.username.clone());
    let device_code = match device_code {
        Some(code) => code,
        None => return Ok(Json(AuthPollResponse::Expired)),
    };

    let client = reqwest::Client::new();

    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("device_code", &device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let token_resp: GhTokenResp = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if let Some(access_token) = token_resp.access_token {
        let user: GhUser = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "sudoku-server")
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?
            .json()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;

        let user_id = db::upsert_user(
            &state.db,
            &user.id.to_string(),
            &user.login,
            &user.avatar_url,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let token = db::create_session(&state.db, user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        state.sessions.remove(&device_key);

        Ok(Json(AuthPollResponse::Complete {
            token,
            username: user.login,
        }))
    } else {
        match token_resp.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => {
                Ok(Json(AuthPollResponse::Pending))
            }
            _ => {
                state.sessions.remove(&device_key);
                Ok(Json(AuthPollResponse::Expired))
            }
        }
    }
}

// ── Leaderboard ─────────────────────────────────────────────────────────

pub async fn leaderboard(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LeaderboardEntry>>, StatusCode> {
    let rows = db::get_leaderboard(&state.db, 100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries: Vec<LeaderboardEntry> = rows
        .into_iter()
        .map(|r| LeaderboardEntry {
            rank: r.rank,
            username: r.username,
            rating: r.rating,
            wins: r.wins,
            losses: r.losses,
        })
        .collect();

    Ok(Json(entries))
}

// ── Profile ─────────────────────────────────────────────────────────────

pub async fn profile(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> Result<Json<PlayerProfile>, StatusCode> {
    let user = db::get_user_by_username(&state.db, &username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(PlayerProfile {
        username: user.username,
        avatar_url: user.avatar_url,
        rating: user.rating,
        wins: user.wins as u32,
        losses: user.losses as u32,
    }))
}

// ── WebSocket upgrade ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn ws_upgrade(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let (user_id, username) = db::get_session(&state.db, &query.token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let current = state
        .connection_count
        .load(std::sync::atomic::Ordering::Relaxed);
    if current >= state.max_connections {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let user = db::get_user(&state.db, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let rating = user.rating;

    Ok(ws.on_upgrade(move |socket| {
        ws::handle_socket(state, socket, user_id, username, rating)
    }))
}
