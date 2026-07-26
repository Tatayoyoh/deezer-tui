use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::Theme;

/// Render a `[key] label` (or bare `key label`) hint string with the key on a
/// "chip" (brackets stripped) and the label dimmed. Keeps inline shortcut hints
/// consistent with the rest of the app.
pub fn shortcut_hint(text: &'static str) -> Line<'static> {
    // "[key] label": chip the bracket contents, dim the rest.
    if let Some(close) = text.find(']') {
        let open = text.find('[').map_or(0, |i| i + 1);
        return Line::from(vec![
            Span::styled(&text[open..close], Theme::shortcut_key()),
            Span::styled(&text[close + 1..], Theme::dim()),
        ]);
    }
    // Bare "key label": chip up to the first space.
    if let Some(sp) = text.find(' ') {
        return Line::from(vec![
            Span::styled(&text[..sp], Theme::shortcut_key()),
            Span::styled(&text[sp..], Theme::dim()),
        ]);
    }
    Line::from(Span::styled(text, Theme::shortcut_key()))
}

/// Render a string with one or more `[key]` segments as a Line: each bracketed
/// key becomes a chip (brackets stripped), surrounding text is dimmed. For
/// border titles and hints that embed several keys.
pub fn shortcut_line(text: &'static str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        if open > 0 {
            spans.push(Span::styled(&rest[..open], Theme::dim()));
        }
        spans.push(Span::styled(&rest[open + 1..close], Theme::shortcut_key()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest, Theme::dim()));
    }
    Line::from(spans)
}

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
