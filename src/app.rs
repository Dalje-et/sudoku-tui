use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::game::{Game, GameState};
use crate::ui;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Set up panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new();
    let result = run_loop(&mut terminal, &mut game);

    // Clean up terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    game: &mut Game,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| ui::draw(f, game))?;

        // Poll with 250ms timeout for timer updates
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Only handle Press events (crossterm sends Press+Release on Windows)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if handle_key(game, key) {
                    return Ok(());
                }
            }
        }
    }
}

/// Handle a key event. Returns true if the app should quit.
fn handle_key(game: &mut Game, key: KeyEvent) -> bool {
    match game.state {
        GameState::Menu => handle_menu_key(game, key),
        GameState::Playing => handle_playing_key(game, key),
        GameState::Paused => handle_paused_key(game, key),
        GameState::Won => handle_won_key(game, key),
    }
}

fn handle_menu_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Up | KeyCode::Left => {
            game.difficulty = game.difficulty.prev();
        }
        KeyCode::Down | KeyCode::Right => {
            game.difficulty = game.difficulty.next();
        }
        KeyCode::Enter => {
            game.start_new_game();
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            return true;
        }
        _ => {}
    }
    false
}

fn handle_playing_key(game: &mut Game, key: KeyEvent) -> bool {
    // Handle quit confirmation first
    if game.show_quit_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => return true,
            _ => {
                game.show_quit_confirm = false;
            }
        }
        return false;
    }

    // When a hint is active, only allow hint-related keys
    if game.active_hint.is_some() {
        match key.code {
            KeyCode::Char('?') => {
                game.request_hint(); // advance hint stage
            }
            KeyCode::Esc => {
                game.dismiss_hint();
            }
            _ => {} // ignore everything else during hint
        }
        return false;
    }

    match key.code {
        // Movement: arrow keys
        KeyCode::Up => game.move_cursor(-1, 0),
        KeyCode::Down => game.move_cursor(1, 0),
        KeyCode::Left => game.move_cursor(0, -1),
        KeyCode::Right => game.move_cursor(0, 1),

        KeyCode::Char(c) => {
            return handle_playing_char(game, c, key.modifiers);
        }

        // Erase
        KeyCode::Delete | KeyCode::Backspace => game.erase(),

        // Quit
        KeyCode::Esc => {
            game.show_quit_confirm = true;
        }

        _ => {}
    }
    false
}

fn handle_playing_char(game: &mut Game, c: char, modifiers: KeyModifiers) -> bool {
    match c {
        // Numbers 1-9: place number or toggle pencil mark
        '1'..='9' => {
            let num = c as u8 - b'0';
            game.place_number(num);
        }

        // Erase with 0
        '0' => game.erase(),

        // Toggle pencil mode
        'p' | 'P' => game.pencil_mode = !game.pencil_mode,

        // Hint (? key)
        '?' => game.request_hint(),

        // Undo
        'u' | 'U' => game.undo(),

        // Ctrl+Z undo
        'z' if modifiers.contains(KeyModifiers::CONTROL) => game.undo(),

        // Validate
        'v' | 'V' => game.validate(),

        // Pause
        ' ' => game.toggle_pause(),

        // Quit
        'q' | 'Q' => {
            game.show_quit_confirm = true;
        }

        _ => {}
    }
    false
}

fn handle_paused_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Enter => {
            game.toggle_pause();
        }
        _ => {}
    }
    false
}

fn handle_won_key(game: &mut Game, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter | KeyCode::Char('n') => {
            game.state = GameState::Menu;
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            return true;
        }
        _ => {}
    }
    false
}
