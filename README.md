# Sudoku TUI

A beautiful terminal-based Sudoku game built with [Ratatui](https://ratatui.rs/) in Rust.

![Menu Screen](assets/menu.png)

## Features

- **Puzzle Generation** — Every puzzle has a unique solution, generated with a backtracking algorithm. Four difficulty levels from Easy to Expert.
- **Pencil Marks** — Toggle pencil mode and mark candidates in a tic-tac-toe mini-grid layout inside each cell.
- **Visual Hints** — Step-by-step hints that highlight relevant cells, explain the solving technique (Naked Single, Hidden Single), and reveal the answer progressively.
- **Validation** — Check your board for conflicts at any time. Errors are highlighted in red.
- **Undo** — Full move history. Undo any placement, erasure, or pencil mark.
- **Timer & Stats** — Track your time, mistakes, and hints used. Pause anytime.

## Screenshots

### Gameplay

![Gameplay](assets/gameplay.png)

### Pencil Marks

Pencil marks display as a 3x3 mini-grid inside each cell — each digit sits in its natural position.

![Pencil Marks](assets/pencil.png)

### Hint System

Hints highlight the relevant row/column/box in magenta, the target cell in green, and explain the technique at the bottom. Press `?` to step through: technique → reveal → place.

![Hint System](assets/hint.png)

## Install

### Homebrew (macOS)

```bash
brew tap Dalje-et/sudoku-tui
brew install sudoku-tui
```

### From Source

```bash
git clone https://github.com/Dalje-et/sudoku-tui.git
cd sudoku-tui
cargo build --release
./target/release/sudoku-tui
```

### Binary Downloads

Pre-built binaries for macOS (Apple Silicon & Intel) and Linux (x86_64 & aarch64) are available on the [Releases](https://github.com/Dalje-et/sudoku-tui/releases) page.

## Controls

| Key | Action |
|-----|--------|
| `Arrow keys` | Move cursor |
| `1-9` | Place number (or toggle pencil mark in pencil mode) |
| `Delete` / `Backspace` / `0` | Erase |
| `p` | Toggle pencil mode |
| `?` | Request hint (press again to reveal, again to place) |
| `Esc` | Dismiss hint / quit |
| `u` / `Ctrl+Z` | Undo |
| `v` | Validate board (highlight conflicts) |
| `Space` | Pause / resume |
| `q` | Quit |

## Difficulty Levels

| Level | Givens | Description |
|-------|--------|-------------|
| Easy | 40–45 | Great for beginners |
| Medium | 32–39 | Requires some deduction |
| Hard | 27–31 | Needs advanced techniques |
| Expert | 22–26 | Minimal clues, maximum challenge |

## Hint Techniques

The hint system analyzes the board and finds the simplest applicable technique:

1. **Naked Single** — A cell where only one value is possible (all others are eliminated by its row, column, and box).
2. **Hidden Single** — A value that can only go in one cell within a row, column, or box.
3. **Direct Reveal** — Fallback when no simple technique applies. Reveals the answer from the solution.

## Built With

- [Ratatui](https://ratatui.rs/) — Rust TUI framework
- [Crossterm](https://github.com/crossterm-rs/crossterm) — Terminal manipulation
- [rand](https://crates.io/crates/rand) — Random number generation

## License

MIT
