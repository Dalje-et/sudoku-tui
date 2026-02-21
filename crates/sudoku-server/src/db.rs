#![allow(unused)]

use sqlx::{Row, SqlitePool};

/// Create all tables if they don't exist.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            github_id TEXT UNIQUE NOT NULL,
            username TEXT UNIQUE NOT NULL,
            avatar_url TEXT NOT NULL DEFAULT '',
            rating INTEGER NOT NULL DEFAULT 1200,
            wins INTEGER NOT NULL DEFAULT 0,
            losses INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            expires_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS matches (
            id INTEGER PRIMARY KEY,
            player1_id INTEGER NOT NULL,
            player2_id INTEGER NOT NULL,
            mode TEXT NOT NULL,
            difficulty TEXT NOT NULL,
            winner_id INTEGER,
            player1_elo_change INTEGER NOT NULL DEFAULT 0,
            player2_elo_change INTEGER NOT NULL DEFAULT 0,
            duration_secs INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (player1_id) REFERENCES users(id),
            FOREIGN KEY (player2_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert or update a user from GitHub OAuth. Returns the local user id.
pub async fn upsert_user(
    pool: &SqlitePool,
    github_id: &str,
    username: &str,
    avatar_url: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (github_id, username, avatar_url)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(github_id) DO UPDATE SET username = ?2, avatar_url = ?3",
    )
    .bind(github_id)
    .bind(username)
    .bind(avatar_url)
    .execute(pool)
    .await?;

    let row = sqlx::query("SELECT id FROM users WHERE github_id = ?1")
        .bind(github_id)
        .fetch_one(pool)
        .await?;

    Ok(row.get::<i64, _>("id"))
}

/// Create a new session token for the given user. Returns the token string.
pub async fn create_session(pool: &SqlitePool, user_id: i64) -> Result<String, sqlx::Error> {
    let token: String = {
        use rand::RngExt;
        let mut rng = rand::rng();
        (0..64)
            .map(|_| {
                let idx = rng.random_range(0..36u8);
                if idx < 10 {
                    (b'0' + idx) as char
                } else {
                    (b'a' + idx - 10) as char
                }
            })
            .collect()
    };

    // Expire in 30 days
    sqlx::query(
        "INSERT INTO sessions (token, user_id, expires_at)
         VALUES (?1, ?2, datetime('now', '+30 days'))",
    )
    .bind(&token)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(token)
}

/// Validate a session token. Returns (user_id, username) if valid.
pub async fn get_session(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<(i64, String)>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT s.user_id, u.username FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token = ?1 AND s.expires_at > datetime('now')",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.get::<i64, _>("user_id"), r.get::<String, _>("username"))))
}

/// Get a user by id.
pub async fn get_user(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, github_id, username, avatar_url, rating, wins, losses FROM users WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UserRow {
        id: r.get("id"),
        github_id: r.get("github_id"),
        username: r.get("username"),
        avatar_url: r.get("avatar_url"),
        rating: r.get("rating"),
        wins: r.get("wins"),
        losses: r.get("losses"),
    }))
}

/// Get a user by username.
pub async fn get_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, github_id, username, avatar_url, rating, wins, losses FROM users WHERE username = ?1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UserRow {
        id: r.get("id"),
        github_id: r.get("github_id"),
        username: r.get("username"),
        avatar_url: r.get("avatar_url"),
        rating: r.get("rating"),
        wins: r.get("wins"),
        losses: r.get("losses"),
    }))
}

/// Update ratings and win/loss counts after a match.
pub async fn update_ratings(
    pool: &SqlitePool,
    winner_id: i64,
    loser_id: i64,
    winner_new_rating: i32,
    loser_new_rating: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET rating = ?1, wins = wins + 1 WHERE id = ?2")
        .bind(winner_new_rating)
        .bind(winner_id)
        .execute(pool)
        .await?;

    sqlx::query("UPDATE users SET rating = ?1, losses = losses + 1 WHERE id = ?2")
        .bind(loser_new_rating)
        .bind(loser_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Record a completed match.
pub async fn record_match(
    pool: &SqlitePool,
    player1_id: i64,
    player2_id: i64,
    mode: &str,
    difficulty: &str,
    winner_id: Option<i64>,
    player1_elo_change: i32,
    player2_elo_change: i32,
    duration_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO matches (player1_id, player2_id, mode, difficulty, winner_id, player1_elo_change, player2_elo_change, duration_secs)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(player1_id)
    .bind(player2_id)
    .bind(mode)
    .bind(difficulty)
    .bind(winner_id)
    .bind(player1_elo_change)
    .bind(player2_elo_change)
    .bind(duration_secs)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get top users by rating.
pub async fn get_leaderboard(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<LeaderboardRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT username, rating, wins, losses FROM users ORDER BY rating DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| LeaderboardRow {
            rank: (i + 1) as u32,
            username: r.get("username"),
            rating: r.get("rating"),
            wins: r.get::<i32, _>("wins") as u32,
            losses: r.get::<i32, _>("losses") as u32,
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: i64,
    pub github_id: String,
    pub username: String,
    pub avatar_url: String,
    pub rating: i32,
    pub wins: i32,
    pub losses: i32,
}

#[derive(Debug, Clone)]
pub struct LeaderboardRow {
    pub rank: u32,
    pub username: String,
    pub rating: i32,
    pub wins: u32,
    pub losses: u32,
}
