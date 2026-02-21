use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Wrap},
    Frame,
};

use crate::game::{Game, GameState};
use crate::hint::HintStage;
use crate::puzzle::{Cell, Difficulty};

// ── Constants ────────────────────────────────────────────────────────────────

/// Each cell occupies 7 characters of width.
/// Total width = 9*7 + 4 thick borders + 6 thin borders = 73
const GRID_WIDTH: u16 = 73;

/// 9 rows × 3 sub-rows each = 27, plus 4 thick horizontal lines + 6 thin = 37
const GRID_HEIGHT: u16 = 37;

// ── Public entry point ───────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, game: &Game) {
    match game.state {
        GameState::Menu => draw_menu(f, game),
        GameState::Playing => draw_playing(f, game),
        GameState::Paused => draw_paused(f, game),
        GameState::Won => draw_won(f, game),
    }

    if game.show_quit_confirm {
        draw_quit_confirm(f);
    }
}

// ── Menu screen ──────────────────────────────────────────────────────────────

fn draw_menu(f: &mut Frame, game: &Game) {
    let area = f.area();

    let chunks = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(8),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Min(0),
    ])
    .split(center_rect(60, 30, area));

    let title_lines = vec![
        Line::from(Span::styled(
            r"  ███████╗██╗   ██╗██████╗  ██████╗ ██╗  ██╗██╗   ██╗",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            r"  ██╔════╝██║   ██║██╔══██╗██╔═══██╗██║ ██╔╝██║   ██║",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            r"  ███████╗██║   ██║██║  ██║██║   ██║█████╔╝ ██║   ██║",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            r"  ╚════██║██║   ██║██║  ██║██║   ██║██╔═██╗ ██║   ██║",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            r"  ███████║╚██████╔╝██████╔╝╚██████╔╝██║  ██╗╚██████╔╝",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            r"  ╚══════╝ ╚═════╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
    ];

    let title = Paragraph::new(title_lines).alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    let diff_label = game.difficulty.label();
    let diff_color = match game.difficulty {
        Difficulty::Easy => Color::Green,
        Difficulty::Medium => Color::Yellow,
        Difficulty::Hard => Color::Magenta,
        Difficulty::Expert => Color::Red,
    };
    let selector_line = Line::from(vec![
        Span::styled("◄  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("  {}  ", diff_label),
            Style::default().fg(diff_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ►", Style::default().fg(Color::DarkGray)),
    ]);
    let selector = Paragraph::new(vec![
        Line::from(Span::styled("Select Difficulty", Style::default().fg(Color::White))),
        Line::from(""),
        selector_line,
    ])
    .alignment(Alignment::Center);
    f.render_widget(selector, chunks[3]);

    let controls = Paragraph::new(vec![
        Line::from(Span::styled(
            "Controls",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("←/→", Style::default().fg(Color::Yellow)),
            Span::styled("  Change difficulty", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::styled("  Start game", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::styled("  Quit", Style::default().fg(Color::Gray)),
        ]),
    ])
    .alignment(Alignment::Center);
    f.render_widget(controls, chunks[5]);
}

// ── Playing screen ───────────────────────────────────────────────────────────

fn draw_playing(f: &mut Frame, game: &Game) {
    let area = f.area();

    let has_hint = game.active_hint.is_some();
    let outer = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(if has_hint { 3 } else { 1 }),
    ])
    .split(area);

    let main_area = outer[0];
    let bottom_area = outer[1];

    let h_chunks = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(GRID_WIDTH + 2),
        Constraint::Length(2),
        Constraint::Length(28),
        Constraint::Min(0),
    ])
    .split(main_area);

    let grid_v = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(GRID_HEIGHT + 2),
        Constraint::Min(0),
    ])
    .split(h_chunks[1]);

    draw_grid(f, game, grid_v[1]);

    let panel_v = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(18),
        Constraint::Min(0),
    ])
    .split(h_chunks[3]);

    draw_info_panel(f, game, panel_v[1]);

    if has_hint {
        draw_hint_bar(f, game, bottom_area);
    } else {
        draw_key_hints(f, bottom_area);
    }
}

// ── Grid rendering ───────────────────────────────────────────────────────────

fn draw_grid(f: &mut Frame, game: &Game, area: Rect) {
    let selected_val = game.selected_value();

    let hint_highlighted: Vec<(usize, usize)> = game
        .active_hint
        .as_ref()
        .map(|h| h.highlighted_cells.clone())
        .unwrap_or_default();
    let hint_target: Option<(usize, usize)> = game
        .active_hint
        .as_ref()
        .map(|h| (h.target_row, h.target_col));
    // In RevealValue stage, show the hint value in the target cell
    let hint_reveal_value: Option<u8> = if game.hint_stage == HintStage::RevealValue {
        game.active_hint.as_ref().map(|h| h.value)
    } else {
        None
    };

    let mut lines: Vec<Line> = Vec::with_capacity(GRID_HEIGHT as usize);

    for visual_row in 0..GRID_HEIGHT {
        let mut spans: Vec<Span> = Vec::new();
        let row_kind = classify_row(visual_row);

        match row_kind {
            RowKind::ThickBorder(border_idx) => {
                spans.push(thick_horizontal_line(border_idx));
            }
            RowKind::ThinBorder => {
                spans.push(thin_horizontal_line());
            }
            RowKind::CellRow(grid_row, sub_row) => {
                for seg in 0..19 {
                    let col_kind = classify_col(seg);
                    match col_kind {
                        ColKind::ThickBorder => {
                            spans.push(Span::styled("║", Style::default().fg(Color::White)));
                        }
                        ColKind::ThinBorder => {
                            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
                        }
                        ColKind::Cell(grid_col) => {
                            let cell = game.board[grid_row][grid_col];
                            let is_selected =
                                grid_row == game.selected_row && grid_col == game.selected_col;
                            let is_conflict = game.show_conflicts
                                && game.conflicts.contains(&(grid_row, grid_col));
                            let is_hint_highlight =
                                hint_highlighted.contains(&(grid_row, grid_col));
                            let is_hint_target = hint_target == Some((grid_row, grid_col));
                            let is_same_number = if let Some(sv) = selected_val {
                                cell.value() == Some(sv) && !is_selected
                            } else {
                                false
                            };

                            let bg = if is_selected {
                                Color::Yellow
                            } else if is_hint_target {
                                Color::Green
                            } else if is_conflict {
                                Color::Red
                            } else if is_hint_highlight {
                                Color::Magenta
                            } else if is_same_number {
                                Color::DarkGray
                            } else {
                                Color::Reset
                            };

                            // If this is the hint target in RevealValue stage, show the value
                            let reveal = if is_hint_target { hint_reveal_value } else { None };

                            let cell_span = render_cell(
                                cell,
                                &game.pencil_marks[grid_row][grid_col],
                                bg,
                                is_selected,
                                sub_row,
                                reveal,
                            );
                            spans.push(cell_span);
                        }
                    }
                }
            }
        }

        lines.push(Line::from(spans));
    }

    let block = Block::bordered()
        .title(" Sudoku ")
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::White));

    let grid_paragraph = Paragraph::new(lines).block(block);
    f.render_widget(grid_paragraph, area);
}

/// Render a single cell sub-row (5 chars wide).
///
/// `sub_row`: 0 = top, 1 = middle, 2 = bottom of the cell.
/// `reveal`: if Some(v), render as if the cell contains this value (for hint reveal stage).
fn render_cell(
    cell: Cell,
    pencil_marks: &[u8],
    bg: Color,
    is_selected: bool,
    sub_row: usize,
    reveal: Option<u8>,
) -> Span<'static> {
    let fg_for_bg = if bg == Color::Yellow || bg == Color::Green {
        Color::Black
    } else if bg == Color::Red {
        Color::White
    } else {
        Color::Reset
    };

    // 7 chars wide, 3 rows tall
    let blank = "       "; // 7 spaces

    // If we're revealing a hint value, show it
    if let Some(v) = reveal {
        if cell == Cell::Empty {
            return if sub_row == 1 {
                Span::styled(
                    format!("   {}   ", v),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(blank, Style::default().bg(bg))
            };
        }
    }

    match cell {
        Cell::Given(v) => {
            if sub_row == 1 {
                let fg = if fg_for_bg != Color::Reset { fg_for_bg } else { Color::White };
                Span::styled(
                    format!("   {}   ", v),
                    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(blank, Style::default().bg(bg))
            }
        }
        Cell::UserInput(v) => {
            if sub_row == 1 {
                let fg = if fg_for_bg != Color::Reset { fg_for_bg } else { Color::Cyan };
                Span::styled(format!("   {}   ", v), Style::default().fg(fg).bg(bg))
            } else {
                Span::styled(blank, Style::default().bg(bg))
            }
        }
        Cell::Empty => {
            if pencil_marks.is_empty() {
                if is_selected && sub_row == 1 {
                    Span::styled("   ·   ", Style::default().fg(Color::DarkGray).bg(bg))
                } else {
                    Span::styled(blank, Style::default().bg(bg))
                }
            } else {
                // Pencil marks as spaced tic-tac-toe grid (7 chars wide):
                //   sub_row 0: " 1 2 3 "  (positions 1, 2, 3)
                //   sub_row 1: " 4 5 6 "  (positions 4, 5, 6)
                //   sub_row 2: " 7 8 9 "  (positions 7, 8, 9)
                let base = (sub_row * 3 + 1) as u8; // 1, 4, 7
                let c0 = if pencil_marks.contains(&base) {
                    (b'0' + base) as char
                } else {
                    ' '
                };
                let c1 = if pencil_marks.contains(&(base + 1)) {
                    (b'0' + base + 1) as char
                } else {
                    ' '
                };
                let c2 = if pencil_marks.contains(&(base + 2)) {
                    (b'0' + base + 2) as char
                } else {
                    ' '
                };
                let text = format!(" {} {} {} ", c0, c1, c2);
                let fg = if fg_for_bg != Color::Reset { fg_for_bg } else { Color::DarkGray };
                Span::styled(text, Style::default().fg(fg).bg(bg))
            }
        }
    }
}

// ── Row/column classification helpers ────────────────────────────────────────

#[derive(Debug)]
enum RowKind {
    ThickBorder(u8),
    ThinBorder,
    /// (grid_row 0-8, sub_row 0-2)
    CellRow(usize, usize),
}

/// Map visual row (0..37) to its kind.
///
/// Layout per box-section (12 rows):
///   thick_border
///   cell sub-row 0,1,2
///   thin_border
///   cell sub-row 0,1,2
///   thin_border
///   cell sub-row 0,1,2
/// Final thick_border at row 36.
fn classify_row(visual: u16) -> RowKind {
    match visual {
        0 => RowKind::ThickBorder(0),
        12 => RowKind::ThickBorder(1),
        24 => RowKind::ThickBorder(2),
        36 => RowKind::ThickBorder(3),
        4 | 8 | 16 | 20 | 28 | 32 => RowKind::ThinBorder,
        _ => {
            // Which box-section? (0..12 -> 0, 12..24 -> 1, 24..36 -> 2)
            let section = (visual / 12) as usize;
            let offset = visual % 12;
            // offset: 1-3 = cell 0 of section, 5-7 = cell 1, 9-11 = cell 2
            let (cell_in_section, sub_row) = if offset <= 3 {
                (0, (offset - 1) as usize)
            } else if offset <= 7 {
                (1, (offset - 5) as usize)
            } else {
                (2, (offset - 9) as usize)
            };
            let grid_row = section * 3 + cell_in_section;
            RowKind::CellRow(grid_row, sub_row)
        }
    }
}

enum ColKind {
    ThickBorder,
    ThinBorder,
    Cell(usize),
}

/// Map visual column segment (0..19) to its kind.
/// Same structure as before — thick borders at box boundaries, thin between cells.
fn classify_col(seg: usize) -> ColKind {
    match seg {
        0 | 6 | 12 | 18 => ColKind::ThickBorder,
        2 | 4 | 8 | 10 | 14 | 16 => ColKind::ThinBorder,
        1 => ColKind::Cell(0),
        3 => ColKind::Cell(1),
        5 => ColKind::Cell(2),
        7 => ColKind::Cell(3),
        9 => ColKind::Cell(4),
        11 => ColKind::Cell(5),
        13 => ColKind::Cell(6),
        15 => ColKind::Cell(7),
        17 => ColKind::Cell(8),
        _ => ColKind::ThinBorder,
    }
}

/// Build a thick horizontal border line (═ with junctions). 7 chars per cell.
fn thick_horizontal_line(border_idx: u8) -> Span<'static> {
    let (left, thick_cross, thin_cross, right) = match border_idx {
        0 => ('╔', '╦', '╤', '╗'),
        3 => ('╚', '╩', '╧', '╝'),
        _ => ('╠', '╬', '╪', '╣'),
    };

    let mut s = String::with_capacity(80);
    s.push(left);
    for box_idx in 0..3 {
        for cell_idx in 0..3 {
            s.push_str("═══════");
            if cell_idx < 2 {
                s.push(thin_cross);
            }
        }
        if box_idx < 2 {
            s.push(thick_cross);
        }
    }
    s.push(right);

    Span::styled(s, Style::default().fg(Color::White))
}

/// Build a thin horizontal border line (─ with junctions). 7 chars per cell.
fn thin_horizontal_line() -> Span<'static> {
    let mut s = String::with_capacity(80);
    s.push('║');
    for box_idx in 0..3 {
        for cell_idx in 0..3 {
            s.push_str("───────");
            if cell_idx < 2 {
                s.push('┼');
            }
        }
        if box_idx < 2 {
            s.push('║');
        }
    }
    s.push('║');

    Span::styled(s, Style::default().fg(Color::DarkGray))
}

// ── Info panel ───────────────────────────────────────────────────────────────

fn draw_info_panel(f: &mut Frame, game: &Game, area: Rect) {
    let block = Block::bordered()
        .title(" Info ")
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::White));

    let difficulty_color = match game.difficulty {
        Difficulty::Easy => Color::Green,
        Difficulty::Medium => Color::Yellow,
        Difficulty::Hard => Color::Magenta,
        Difficulty::Expert => Color::Red,
    };

    let pencil_indicator = if game.pencil_mode {
        Span::styled(
            " ON ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("OFF", Style::default().fg(Color::DarkGray))
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(" Difficulty: ", Style::default().fg(Color::Gray)),
            Span::styled(
                game.difficulty.label(),
                Style::default().fg(difficulty_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Time:       ", Style::default().fg(Color::Gray)),
            Span::styled(
                game.format_time(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Mistakes:   ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", game.mistakes),
                Style::default().fg(if game.mistakes > 0 { Color::Red } else { Color::White }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Hints used: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", game.hints_used), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Pencil:     ", Style::default().fg(Color::Gray)),
            pencil_indicator,
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

// ── Hint bar ─────────────────────────────────────────────────────────────────

fn draw_hint_bar(f: &mut Frame, game: &Game, area: Rect) {
    if let Some(ref hint) = game.active_hint {
        let (stage_text, stage_color) = match game.hint_stage {
            HintStage::ShowTechnique => (
                format!(
                    " 💡 {}  │  Press ? again to reveal value, Esc to dismiss",
                    hint.explanation
                ),
                Color::Cyan,
            ),
            HintStage::RevealValue => (
                format!(
                    " ✓ R{}C{} = {}  │  Press ? to place it, Esc to dismiss",
                    hint.target_row + 1,
                    hint.target_col + 1,
                    hint.value
                ),
                Color::Green,
            ),
        };

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                stage_text,
                Style::default().fg(stage_color).add_modifier(Modifier::BOLD),
            )),
        ];

        let block = Block::new().style(Style::default().bg(Color::Rgb(30, 30, 50)));
        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}

// ── Key hints (bottom status bar) ────────────────────────────────────────────

fn draw_key_hints(f: &mut Frame, area: Rect) {
    let hints = Line::from(vec![
        Span::styled(" ←↑↓→", Style::default().fg(Color::Yellow)),
        Span::styled(" Move  ", Style::default().fg(Color::Gray)),
        Span::styled("1-9", Style::default().fg(Color::Yellow)),
        Span::styled(" Place  ", Style::default().fg(Color::Gray)),
        Span::styled("Del", Style::default().fg(Color::Yellow)),
        Span::styled(" Erase  ", Style::default().fg(Color::Gray)),
        Span::styled("p", Style::default().fg(Color::Yellow)),
        Span::styled(" Pencil  ", Style::default().fg(Color::Gray)),
        Span::styled("u", Style::default().fg(Color::Yellow)),
        Span::styled(" Undo  ", Style::default().fg(Color::Gray)),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::styled(" Hint  ", Style::default().fg(Color::Gray)),
        Span::styled("v", Style::default().fg(Color::Yellow)),
        Span::styled(" Check  ", Style::default().fg(Color::Gray)),
        Span::styled("Spc", Style::default().fg(Color::Yellow)),
        Span::styled(" Pause  ", Style::default().fg(Color::Gray)),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::styled(" Quit", Style::default().fg(Color::Gray)),
    ]);

    let bar = Paragraph::new(hints).style(Style::default().bg(Color::DarkGray));
    f.render_widget(bar, area);
}

// ── Paused screen ────────────────────────────────────────────────────────────

fn draw_paused(f: &mut Frame, game: &Game) {
    let area = f.area();

    let bg = Paragraph::new("").style(Style::default().bg(Color::Black));
    f.render_widget(bg, area);

    let popup = center_rect(34, 9, area);
    f.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Paused ")
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::Yellow));

    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "⏸  PAUSED",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Time: {}", game.format_time()),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::Gray)),
            Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" to resume", Style::default().fg(Color::Gray)),
        ]),
    ])
    .block(block)
    .alignment(Alignment::Center);

    f.render_widget(text, popup);
}

// ── Won screen ───────────────────────────────────────────────────────────────

fn draw_won(f: &mut Frame, game: &Game) {
    let area = f.area();

    let bg = Paragraph::new("").style(Style::default().bg(Color::Black));
    f.render_widget(bg, area);

    let popup = center_rect(40, 13, area);
    f.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Victory! ")
        .border_type(BorderType::Double)
        .style(Style::default().fg(Color::Green));

    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "CONGRATULATIONS!",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "You completed the puzzle!",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Time:       ", Style::default().fg(Color::Gray)),
            Span::styled(
                game.format_time(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Mistakes:   ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", game.mistakes),
                Style::default().fg(if game.mistakes == 0 { Color::Green } else { Color::Red }),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Hints used: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", game.hints_used), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Difficulty: ", Style::default().fg(Color::Gray)),
            Span::styled(
                game.difficulty.label(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter for new game, Q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(block)
    .alignment(Alignment::Center);

    f.render_widget(text, popup);
}

// ── Quit confirmation dialog ─────────────────────────────────────────────────

fn draw_quit_confirm(f: &mut Frame) {
    let area = f.area();
    let popup = center_rect(36, 7, area);

    f.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Quit? ")
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::Red));

    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Are you sure you want to quit?",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Y", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("/", Style::default().fg(Color::Gray)),
            Span::styled("Enter", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Yes   ", Style::default().fg(Color::Gray)),
            Span::styled("Any key", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" No", Style::default().fg(Color::Gray)),
        ]),
    ])
    .block(block)
    .alignment(Alignment::Center);

    f.render_widget(text, popup);
}

// ── Layout helpers ───────────────────────────────────────────────────────────

fn center_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .split(area);

    let horiz = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .split(vert[1]);

    horiz[1]
}
