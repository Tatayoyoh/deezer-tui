use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::client::{LoginMode, ViewState};
use crate::theme::Theme;
use crate::ui::common;

pub fn draw(frame: &mut Frame, view: &ViewState) {
    let area = frame.area();

    // Full-screen dark background
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::bg())),
        area,
    );

    // Center the login form vertically
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(14),
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

    match view.login_mode {
        LoginMode::Button => draw_button_mode(frame, view, form_area),
        LoginMode::ArlInput => {
            view.login_button_area.set(None);
            draw_arl_mode(frame, view, form_area);
        }
    }
}

/// Default login screen: logo + big "Login" button.
fn draw_button_mode(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Logo
            Constraint::Length(2), // Spacing
            Constraint::Length(3), // Button
            Constraint::Length(1), // Spacing
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Logo
    frame.render_widget(common::deezer_logo(), chunks[0]);

    // Login button
    let button_text = if view.login_loading {
        "  Connecting...  "
    } else {
        "  Login with Deezer  "
    };

    let button_style = if view.login_loading {
        Style::default()
            .fg(Theme::bg())
            .bg(Color::Rgb(100, 100, 100))
    } else {
        Style::default()
            .fg(Color::White)
            .bg(Theme::primary())
            .add_modifier(Modifier::BOLD)
    };

    let button = Paragraph::new(button_text)
        .style(button_style)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if view.login_loading {
                    Style::default().fg(Color::Rgb(100, 100, 100))
                } else {
                    Style::default().fg(Theme::primary())
                }),
        );
    // Record button area for mouse hit-testing
    view.login_button_area.set(Some(chunks[2]));
    frame.render_widget(button, chunks[2]);

    // Hint / error
    let hint_text = if view.login_loading {
        Span::styled("Connecting...", Style::default().fg(Color::Cyan))
    } else if let Some(ref error) = view.login_error {
        Span::styled(error.as_str(), Style::default().fg(Color::Red))
    } else {
        Span::styled(
            "Enter: login | w: connect with ARL | Esc: quit",
            Theme::dim(),
        )
    };
    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}

/// ARL input mode: logo + ARL text field.
fn draw_arl_mode(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Logo
            Constraint::Length(1), // Spacing
            Constraint::Length(3), // Input
            Constraint::Length(1), // Spacing
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Logo
    frame.render_widget(common::deezer_logo(), chunks[0]);

    // ARL Input field
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .title(" ARL Token ")
        .title_style(Theme::title());

    let input_text = if view.login_input.is_empty() {
        Span::styled("Paste your ARL token from browser cookies...", Theme::dim())
    } else {
        let masked: String = "*".repeat(view.login_input.len().min(40));
        let suffix = if view.login_input.len() > 40 {
            format!("... ({})", view.login_input.len())
        } else {
            String::new()
        };
        Span::styled(format!("{masked}{suffix}"), Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, chunks[2]);

    // Set cursor position
    let cursor_x = chunks[2].x + 1 + view.login_cursor.min(chunks[2].width as usize - 2) as u16;
    let cursor_y = chunks[2].y + 1;
    frame.set_cursor_position(Position::new(cursor_x, cursor_y));

    // Hint / error
    let hint_text = if view.login_loading {
        Span::styled("Connecting...", Style::default().fg(Color::Cyan))
    } else if let Some(ref error) = view.login_error {
        Span::styled(error.as_str(), Style::default().fg(Color::Red))
    } else {
        Span::styled("Enter: connect | Esc: back", Theme::dim())
    };
    let hint = Paragraph::new(hint_text).alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}
