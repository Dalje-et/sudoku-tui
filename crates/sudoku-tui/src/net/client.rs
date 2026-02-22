use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use sudoku_core::protocol::{
    AuthPollResponse, ClientMessage, DeviceAuthResponse, LeaderboardEntry, PlayerProfile,
    ServerMessage,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_SERVER_URL: &str = "wss://sudoku-tui-server.onrender.com";

fn server_url() -> String {
    std::env::var("SUDOKU_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string())
}

fn http_base_url() -> String {
    let ws_url = server_url();
    ws_url
        .replace("wss://", "https://")
        .replace("ws://", "http://")
}

fn is_local_server() -> bool {
    let url = server_url();
    url.contains("localhost") || url.contains("127.0.0.1")
}

fn auth_file_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sudoku-tui");
    config_dir.join("auth.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AuthData {
    token: String,
    username: String,
}

pub struct NetworkClient {
    pub sender: mpsc::UnboundedSender<ClientMessage>,
    pub receiver: mpsc::UnboundedReceiver<ServerMessage>,
}

impl NetworkClient {
    /// Connect to the server via WebSocket with the given auth token
    pub async fn connect(token: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/ws?token={}", server_url(), token);

        // Build a rustls config that only advertises HTTP/1.1 in ALPN.
        // Cloudflare/Render negotiate HTTP/2 by default, which breaks
        // WebSocket upgrade (requires HTTP/1.1).
        let connector = if url.starts_with("wss://") {
            let roots = rustls::RootCertStore::from_iter(
                webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
            );
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            // config.alpn_protocols is empty by default = no ALPN = HTTP/1.1
            Some(tokio_tungstenite::Connector::Rustls(Arc::new(config)))
        } else {
            None
        };

        let (ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(
            &url,
            None,
            false,
            connector,
        )
        .await?;
        let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

        let (client_tx, mut client_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let (server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();

        // Sender task: forward client messages to WebSocket
        tokio::spawn(async move {
            while let Some(msg) = client_rx.recv().await {
                let json = serde_json::to_string(&msg).unwrap();
                if ws_sink.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        });

        // Receiver task: forward WebSocket messages to channel
        tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_stream_rx.next().await {
                match msg {
                    Message::Text(text) => {
                        if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                            if server_tx.send(server_msg).is_err() {
                                break;
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });

        Ok(Self {
            sender: client_tx,
            receiver: server_rx,
        })
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.sender.send(msg);
    }

    /// Start the GitHub device auth flow
    pub async fn start_device_auth(
    ) -> Result<DeviceAuthResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/auth/device", http_base_url());
        let resp = reqwest::Client::new().post(&url).send().await?;
        let body = resp.json::<DeviceAuthResponse>().await?;
        Ok(body)
    }

    /// Poll for auth completion
    pub async fn poll_auth(
        user_code: &str,
    ) -> Result<AuthPollResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/auth/poll", http_base_url());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&serde_json::json!({ "user_code": user_code }))
            .send()
            .await?;
        let body = resp.json::<AuthPollResponse>().await?;
        Ok(body)
    }

    /// Fetch leaderboard
    pub async fn fetch_leaderboard(
    ) -> Result<Vec<LeaderboardEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/leaderboard", http_base_url());
        let resp = reqwest::get(&url).await?;
        let entries = resp.json::<Vec<LeaderboardEntry>>().await?;
        Ok(entries)
    }

    /// Fetch player profile
    pub async fn fetch_profile(
        username: &str,
    ) -> Result<PlayerProfile, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/profile/{}", http_base_url(), username);
        let resp = reqwest::get(&url).await?;
        let profile = resp.json::<PlayerProfile>().await?;
        Ok(profile)
    }

    /// Save auth token to disk (skipped for local dev servers)
    pub fn save_token(token: &str, username: &str) -> std::io::Result<()> {
        if is_local_server() {
            return Ok(());
        }
        let path = auth_file_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = AuthData {
            token: token.to_string(),
            username: username.to_string(),
        };
        let json = serde_json::to_string(&data).unwrap();
        std::fs::write(path, json)
    }

    /// Load saved auth token from disk (skipped for local dev servers)
    pub fn load_token() -> Option<(String, String)> {
        if is_local_server() {
            return None;
        }
        let path = auth_file_path();
        let data = std::fs::read_to_string(path).ok()?;
        let auth: AuthData = serde_json::from_str(&data).ok()?;
        Some((auth.token, auth.username))
    }
}
