use ratatui::prelude::*;

use crate::client::ViewState;
use crate::i18n::t;
use crate::protocol::ExploreCategory;
use crate::ui::{categories, common, moods, radio};

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Sub-category menu
            Constraint::Min(3),    // Sub-category content
        ])
        .split(area);

    draw_category_menu(frame, view, chunks[0]);

    match view.explore_category {
        ExploreCategory::Moods => moods::draw(frame, view, chunks[1]),
        ExploreCategory::Categories => categories::draw(frame, view, chunks[1]),
        ExploreCategory::Radios => radio::draw(frame, view, chunks[1]),
    }
}

fn draw_category_menu(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let labels: Vec<&str> = ExploreCategory::ALL
        .iter()
        .map(|cat| s.explore_category_label(*cat))
        .collect();
    let current = ExploreCategory::ALL
        .iter()
        .position(|cat| *cat == view.explore_category)
        .unwrap_or(0);
    common::draw_category_menu(frame, view, area, &labels, current);
}
