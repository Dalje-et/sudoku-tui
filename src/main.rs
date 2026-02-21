mod app;
mod game;
mod hint;
mod puzzle;
mod ui;

fn main() {
    if let Err(e) = app::run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
