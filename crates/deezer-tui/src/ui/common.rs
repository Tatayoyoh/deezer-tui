use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::client::{ClickTarget, ViewState};
use crate::theme::Theme;

/// Blank columns drawn between two category labels.
const CATEGORY_GAP: u16 = 2;

/// Draw the centered row of category chips shared by the tab pages, and record
/// each label as clickable.
pub fn draw_category_menu(
    frame: &mut Frame,
    view: &ViewState,
    area: Rect,
    labels: &[&str],
    current: usize,
) {
    draw_chip_menu(frame, view, area, labels, current, ClickTarget::Category);
}

/// Draw a centered row of chips, recording each one as clickable. `target` maps
/// a chip's index to the click it stands for.
pub fn draw_chip_menu(
    frame: &mut Frame,
    view: &ViewState,
    area: Rect,
    labels: &[&str],
    current: usize,
    target: fn(usize) -> ClickTarget,
) {
    let active = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    let mut spans = Vec::with_capacity(labels.len() * 2);
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " ".repeat(CATEGORY_GAP as usize),
                Theme::dim(),
            ));
        }
        spans.push(Span::styled(
            *label,
            if i == current { active } else { Theme::dim() },
        ));
    }

    let line = Line::from(spans);
    let total = line.width() as u16;
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);

    // Mirror the centering ratatui applies, so the recorded rects line up with
    // what was drawn. A menu too wide for the area is truncated, not centered.
    if total > area.width {
        return;
    }
    let mut x = area.x + (area.width - total) / 2;
    for (i, label) in labels.iter().enumerate() {
        let width = Span::raw(*label).width() as u16;
        let rect = Rect {
            x,
            y: area.y,
            width,
            height: 1,
        };
        view.record_click(rect, target(i));
        x += width + CATEGORY_GAP;
    }
}

/// Clickable rects of a `Tabs` widget's titles, in render order: ratatui pads
/// every title with one space on each side and separates them by a one-cell
/// divider.
pub fn tab_rects(area: Rect, titles: &[&str]) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(titles.len());
    let mut x = area.x;
    for title in titles {
        if x >= area.right() {
            break;
        }
        // A tab clipped by the area's edge stays clickable over what shows of it.
        let width = (Span::raw(*title).width() as u16 + 2).min(area.right() - x);
        rects.push(Rect {
            x,
            y: area.y,
            width,
            height: 1,
        });
        x += width + 1; // divider
    }
    rects
}

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
