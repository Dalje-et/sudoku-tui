#[tokio::main]
async fn main() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:sudoku.db?mode=rwc".to_string());
    let (app, _state) = sudoku_server::build_app(&db_url).await;

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    if std::env::var("GITHUB_CLIENT_ID").is_err() {
        println!("╔══════════════════════════════════════════════════╗");
        println!("║  SUDOKU SERVER — DEV MODE                       ║");
        println!("║  GitHub OAuth disabled. Auto-creating dev users. ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();
        println!("Run the client with:");
        println!(
            "  SUDOKU_SERVER_URL=ws://localhost:{} cargo run -p sudoku-tui",
            port
        );
        println!();
    }

    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}
