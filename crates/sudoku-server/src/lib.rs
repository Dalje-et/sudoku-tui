#![allow(unused)]

pub mod db;
pub mod routes;
pub mod state;
pub mod ws;

use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::routing::{get, post};
use axum::Router;
use dashmap::DashMap;
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::CorsLayer;

use crate::state::{AppState, RoomState};

/// Build a fully configured Router + shared state.
pub async fn build_app(db_url: &str) -> (Router, Arc<AppState>) {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect to SQLite");

    db::init_db(&pool)
        .await
        .expect("Failed to initialize database");

    let state = Arc::new(AppState {
        db: pool,
        rooms: DashMap::new(),
        sessions: DashMap::new(),
        connections: DashMap::new(),
        matchmaking: DashMap::new(),
        connection_count: AtomicU32::new(0),
        max_connections: 100,
    });

    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                cleanup(&state).await;
            }
        });
    }

    let app = Router::new()
        .route("/health", get(routes::health))
        .route("/auth/device", post(routes::device_auth))
        .route("/auth/poll", post(routes::auth_poll))
        .route("/leaderboard", get(routes::leaderboard))
        .route("/profile/{username}", get(routes::profile))
        .route("/ws", get(routes::ws_upgrade))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    (app, state)
}

async fn cleanup(state: &AppState) {
    let now = Instant::now();
    let mut to_remove = Vec::new();
    let mut to_forfeit = Vec::new();

    for entry in state.rooms.iter() {
        let room = entry.value();
        match room.state {
            RoomState::Waiting => {
                if now.duration_since(room.created_at) > Duration::from_secs(600) {
                    to_remove.push(room.code.clone());
                }
            }
            RoomState::Playing => {
                if now.duration_since(room.last_activity) > Duration::from_secs(300) {
                    to_forfeit.push((room.code.clone(), room.player1_id));
                }
            }
            RoomState::Ended => {
                if now.duration_since(room.last_activity) > Duration::from_secs(120) {
                    to_remove.push(room.code.clone());
                }
            }
        }
    }

    for code in to_remove {
        state.rooms.remove(&code);
    }

    for (code, player_id) in to_forfeit {
        ws::forfeit_player_public(state, &code, player_id).await;
    }
}
