#![allow(unused)]

mod db;
mod routes;
mod state;
mod ws;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::routing::{get, post};
use axum::Router;
use dashmap::DashMap;
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::CorsLayer;

use crate::state::{AppState, RoomState};

#[tokio::main]
async fn main() {
    // Database setup.
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:sudoku.db?mode=rwc")
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

    // Spawn background cleanup task.
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
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    if std::env::var("GITHUB_CLIENT_ID").is_err() {
        println!("╔══════════════════════════════════════════════════╗");
        println!("║  SUDOKU SERVER — DEV MODE                       ║");
        println!("║  GitHub OAuth disabled. Auto-creating dev users. ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();
        println!("Run the client with:");
        println!("  SUDOKU_SERVER_URL=ws://localhost:{} cargo run -p sudoku-tui", port);
        println!();
    }

    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}

/// Background task: remove stale rooms and forfeit idle games.
async fn cleanup(state: &AppState) {
    let now = Instant::now();
    let mut to_remove = Vec::new();
    let mut to_forfeit = Vec::new();

    for entry in state.rooms.iter() {
        let room = entry.value();
        match room.state {
            RoomState::Waiting => {
                // Remove rooms waiting longer than 10 minutes.
                if now.duration_since(room.created_at) > Duration::from_secs(600) {
                    to_remove.push(room.code.clone());
                }
            }
            RoomState::Playing => {
                // Forfeit games idle longer than 5 minutes.
                if now.duration_since(room.last_activity) > Duration::from_secs(300) {
                    to_forfeit.push((room.code.clone(), room.player1_id));
                }
            }
            RoomState::Ended => {
                // Clean up ended rooms after 2 minutes.
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
