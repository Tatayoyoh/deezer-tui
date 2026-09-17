use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::client::{ClickTarget, InputMode, RowsKind, ViewState};
use crate::i18n::t;
use crate::protocol::SearchCategory;
use crate::theme::Theme;
use crate::ui::common;
use crate::ui::common::{track_status, STATUS_WIDTH};

/// Column width constraints for a search category's table. Lives next to the
/// table it lays out, like the headers it lines up with, rather than in the IPC
/// types.
fn column_widths(category: SearchCategory) -> [Constraint; 5] {
    match category {
        SearchCategory::Album => [
            Constraint::Length(STATUS_WIDTH),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Length(0),
            Constraint::Length(10),
        ],
        SearchCategory::Artist => [
            Constraint::Length(STATUS_WIDTH),
            Constraint::Percentage(45),
            Constraint::Percentage(25),
            Constraint::Length(0),
            Constraint::Length(0),
        ],
        _ => [
            Constraint::Length(STATUS_WIDTH),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Length(6),
        ],
    }
}

pub fn draw(frame: &mut Frame, view: &mut ViewState, area: Rect) {
    let has_results = !view.search_display.is_empty() || view.search_loading;

    if has_results {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Length(1), // Category menu
                Constraint::Min(3),    // Results table
            ])
            .split(area);

        draw_search_input(frame, view, chunks[0]);
        draw_category_menu(frame, view, chunks[1]);
        draw_results_table(frame, view, chunks[2]);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(3),    // Results table (logo or empty msg)
            ])
            .split(area);

        draw_search_input(frame, view, chunks[0]);
        draw_results_table(frame, view, chunks[1]);
    }
}

fn draw_search_input(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let is_typing = view.input_mode == InputMode::Typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(common::shortcut_line(if is_typing {
            s.search_title_typing
        } else {
            s.search_title_normal
        }))
        .title_style(Theme::title());

    let input_text = if view.search_input.is_empty() && !is_typing {
        Span::styled(s.search_placeholder, Theme::dim())
    } else {
        Span::styled(&view.search_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);
    view.record_click(area, ClickTarget::FilterInput);

    if is_typing {
        let cursor_x = area.x + 1 + view.search_input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_category_menu(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let labels: Vec<&str> = SearchCategory::ALL
        .iter()
        .map(|cat| s.search_category_label(*cat))
        .collect();
    let current = SearchCategory::ALL
        .iter()
        .position(|cat| *cat == view.search_category)
        .unwrap_or(0);
    common::draw_category_menu(frame, view, area, &labels, current);
}

fn draw_results_table(frame: &mut Frame, view: &mut ViewState, area: Rect) {
    let s = t();
    if view.search_loading {
        let loading =
            Paragraph::new(Span::styled(s.searching, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    if view.search_display.is_empty() {
        if view.search_input.is_empty() {
            // No search performed yet — show text logo
            common::render_logo(frame, area);
        } else {
            let empty_msg = Paragraph::new(Span::styled(s.no_results, Theme::dim()))
                .alignment(Alignment::Center);
            frame.render_widget(empty_msg, area);
        }
        return;
    }

    let headers = s.search_category_headers(view.search_category);
    let header = Row::new(vec![
        Cell::from(Span::raw("")),
        Cell::from(Span::styled(headers[0], Theme::dim())),
        Cell::from(Span::styled(headers[1], Theme::dim())),
        Cell::from(Span::styled(headers[2], Theme::dim())),
        Cell::from(Span::styled(headers[3], Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = view
        .search_display
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_playing = item
                .track
                .as_ref()
                .is_some_and(|t| view.is_playing_track(&t.track_id));
            let is_fav = item
                .track
                .as_ref()
                .is_some_and(|t| view.is_track_favorite(&t.track_id));
            Row::new(vec![
                Cell::from(track_status(
                    i == view.search_selected,
                    is_playing,
                    is_fav,
                    view.status,
                )),
                Cell::from(Span::styled(&item.col1, Theme::text())),
                Cell::from(Span::styled(
                    &item.col2,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(&item.col3, Theme::dim())),
                Cell::from(Span::styled(&item.col4, Theme::dim())),
            ])
        })
        .collect();

    let title = s.results_title(view.search_display.len());
    let widths = column_widths(view.search_category);
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(title)
                .title_style(Theme::title()),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("");

    let mut table_state = view.table_state(RowsKind::Tab, view.search_selected);
    frame.render_stateful_widget(table, area, &mut table_state);
    view.record_rows(
        area,
        2, // title + header
        table_state.offset(),
        view.search_display.len(),
        RowsKind::Tab,
    );
}
