use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph, Row, Table};

use crate::client::{RowsKind, ViewState};
use crate::i18n::t;
use crate::protocol::ExploreCategory;
use crate::theme::Theme;
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
        ExploreCategory::Discovery => draw_discovery(frame, view, chunks[1]),
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

fn draw_discovery(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let block = Block::new()
        .title(s.discovery_title)
        .title_style(Theme::title());

    if view.new_releases.is_empty() {
        let message = if view.new_releases_loading {
            s.discovery_loading
        } else if view.new_releases_error {
            s.discovery_error
        } else {
            s.discovery_empty
        };
        let text = Paragraph::new(Span::styled(message, Theme::dim()))
            .centered()
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let rows = view.new_releases.iter().map(|album| {
        Row::new([
            Span::styled(&album.title, Theme::text()),
            Span::styled(&album.artist, Theme::text()),
            Span::styled(&album.release_date, Theme::dim()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Fill(3),
            Constraint::Fill(2),
            Constraint::Length(s.discovery_date.chars().count().max(10) as u16),
        ],
    )
    .header(Row::new([s.header_album, s.header_artist, s.discovery_date]).style(Theme::dim()))
    .block(block)
    .row_highlight_style(Theme::highlight())
    .highlight_symbol("> ");

    let mut table_state = view.table_state(RowsKind::Tab, view.new_releases_selected);
    frame.render_stateful_widget(table, area, &mut table_state);
    view.record_rows(
        area,
        2, // title + header
        table_state.offset(),
        view.new_releases.len(),
        RowsKind::Tab,
    );
}
