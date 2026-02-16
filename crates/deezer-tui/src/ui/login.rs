use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Clear};

use crate::client::ViewState;
use crate::theme::Theme;
use crate::ui::common;

pub fn draw(frame: &mut Frame, view: &ViewState) {
    let area = frame.area();

    // Full-screen dark background
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::BG)),
        area,
    );

    // Center the login form vertically
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(12),
            Constraint::Percentage(25),
        ])
        .split(area);

    // Center horizontally
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1]);

    let form_area = horizontal[1];

    // Split form area into logo + input + error
    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Logo
            Constraint::Length(1), // Spacing
            Constraint::Length(3), // Input
            Constraint::Length(2), // Error / hint
        ])
        .split(form_area);

    // Logo
    let logo = common::deezer_logo();
    frame.render_widget(logo, form_chunks[0]);

    // ARL Input field
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .title(" ARL Token ")
        .title_style(Theme::title());

    let input_text = if view.login_input.is_empty() {
        Span::styled(
            "Paste your ARL token from browser cookies...",
            Theme::dim(),
        )
    } else {
        // Mask the ARL token for privacy
        let masked: String = "*".repeat(view.login_input.len().min(40));
        let suffix = if view.login_input.len() > 40 {
            format!("... ({})", view.login_input.len())
        } else {
            String::new()
        };
        Span::styled(format!("{masked}{suffix}"), Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, form_chunks[2]);

    // Set cursor position
    let cursor_x = form_chunks[2].x + 1 + view.login_cursor.min(form_chunks[2].width as usize - 2) as u16;
    let cursor_y = form_chunks[2].y + 1;
    frame.set_cursor_position(Position::new(cursor_x, cursor_y));

    // Error or hint
    let hint_text = if view.login_loading {
        Span::styled("Connecting...", Style::default().fg(Color::Cyan))
    } else if let Some(ref error) = view.login_error {
        Span::styled(error.as_str(), Style::default().fg(Color::Red))
    } else {
        Span::styled(
            "Press Enter to connect | Esc to quit",
            Theme::dim(),
        )
    };
    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    frame.render_widget(hint, form_chunks[3]);
}
