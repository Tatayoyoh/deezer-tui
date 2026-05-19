use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::client::ViewState;
use crate::i18n::t;
use crate::protocol::ExploreCategory;
use crate::theme::Theme;
use crate::ui::{categories, moods, radio};

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Sub-category menu
            Constraint::Min(3),    // Sub-category content
        ])
        .split(area);

    draw_category_menu(frame, view.explore_category, chunks[0]);

    match view.explore_category {
        ExploreCategory::Moods => moods::draw(frame, view, chunks[1]),
        ExploreCategory::Categories => categories::draw(frame, view, chunks[1]),
        ExploreCategory::Radios => radio::draw(frame, view, chunks[1]),
    }
}

fn draw_category_menu(frame: &mut Frame, current: ExploreCategory, area: Rect) {
    let s = t();
    let spans: Vec<Span> = ExploreCategory::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, cat)| {
            let mut parts = Vec::new();
            if i > 0 {
                parts.push(Span::styled("  ", Theme::dim()));
            }
            if *cat == current {
                parts.push(Span::styled(
                    s.explore_category_label(*cat),
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            } else {
                parts.push(Span::styled(s.explore_category_label(*cat), Theme::dim()));
            }
            parts
        })
        .collect();

    let line = Line::from(spans);
    let menu = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(menu, area);
}
