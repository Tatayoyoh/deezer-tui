use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

/// Deezer logo in pixel art using Unicode block characters.
/// Rendered in Deezer purple.
pub fn deezer_logo() -> Paragraph<'static> {
    let logo = vec![
        Line::from(Span::styled(
            r"  ____                                   ",
            Style::default().fg(Theme::PRIMARY),
        )),
        Line::from(Span::styled(
            r" |  _ \  ___  ___ _______ _ __           ",
            Style::default().fg(Theme::PRIMARY),
        )),
        Line::from(Span::styled(
            r" | | | |/ _ \/ _ \_  / _ \ '__|          ",
            Style::default().fg(Theme::PRIMARY),
        )),
        Line::from(Span::styled(
            r" | |_| |  __/  __// /  __/ |             ",
            Style::default().fg(Theme::SECONDARY),
        )),
        Line::from(Span::styled(
            r" |____/ \___|\___/___\___|_|  TUI        ",
            Style::default().fg(Theme::SECONDARY),
        )),
    ];

    Paragraph::new(logo).alignment(Alignment::Center)
}

/// Centered text helper.
pub fn centered_text<'a>(text: &'a str, style: Style) -> Paragraph<'a> {
    Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center)
}
