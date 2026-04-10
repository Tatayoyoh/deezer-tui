use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::Theme;

/// Deezer logo in pixel art using Unicode block characters.
/// Rendered in Deezer purple.
// pub fn deezer_logo() -> Paragraph<'static> {
//     let logo = vec![
//         Line::from(Span::styled(
//             "     \u{2593}\u{2593}       \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "   \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "   \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "     \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "       \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "         \u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//     ];

//     Paragraph::new(logo)
// }

pub fn deezer_logo() -> Paragraph<'static> {
    let logo = vec![
        Line::from(Span::styled(
            r"  ____                                   ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" |  _ \  ___  ___ _______ _ __           ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" | | | |/ _ \/ _ \_  / _ \ '__|          ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" | |_| |  __/  __// /  __/ |             ",
            Style::default().fg(Theme::secondary()),
        )),
        Line::from(Span::styled(
            r" |____/ \___|\___/___\___|_|  TUI        ",
            Style::default().fg(Theme::secondary()),
        )),
    ];

    Paragraph::new(logo).alignment(Alignment::Center)
}

/// Renders the Deezer logo centered within area.
pub fn render_logo(frame: &mut Frame, area: Rect) {
    // for pixel logo
    // const LOGO_W: u16 = 21;
    // const LOGO_H: u16 = 9;
    const LOGO_W: u16 = 44;
    const LOGO_H: u16 = 7;
    let logo_area = Rect {
        x: area.x + (area.width.saturating_sub(LOGO_W)) / 2 + 2,
        y: area.y + (area.height.saturating_sub(LOGO_H)) / 2 - 1,
        width: LOGO_W.min(area.width),
        height: LOGO_H.min(area.height),
    };
    frame.render_widget(deezer_logo(), logo_area);
}

/// Centered text helper.
pub fn centered_text<'a>(text: &'a str, style: Style) -> Paragraph<'a> {
    Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center)
}
