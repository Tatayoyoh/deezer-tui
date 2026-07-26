use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::{ClickTarget, RowsKind, ViewState};
use crate::i18n::t;
use crate::protocol::FavoritesCategory;
use crate::theme::Theme;
use crate::ui::common;
use crate::ui::common::{shortcut_line, track_number};

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Category menu
            Constraint::Length(2), // Shuffle button
            Constraint::Length(3), // Filter input
            Constraint::Min(3),    // Favorites table
        ])
        .split(area);

    // Category menu
    draw_category_menu(frame, view, chunks[0]);

    // Shuffle button
    draw_shuffle_button(frame, view, chunks[1]);

    // Filter input
    draw_filter_input(frame, view, chunks[2]);

    // Favorites table
    draw_favorites_table(frame, view, chunks[3]);
}

fn draw_category_menu(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let labels: Vec<&str> = FavoritesCategory::ALL
        .iter()
        .map(|cat| s.favorites_category_label(*cat))
        .collect();
    let current = FavoritesCategory::ALL
        .iter()
        .position(|cat| *cat == view.favorites_category)
        .unwrap_or(0);
    common::draw_category_menu(frame, view, area, &labels, current);
}

fn draw_shuffle_button(frame: &mut Frame, view: &ViewState, area: Rect) {
    let label = t().shuffle_favorites;
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("g", Theme::shortcut_key()),
        Span::raw(" "),
        Span::styled(
            label,
            Style::default()
                .fg(Theme::text_color())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let width = line.width() as u16;
    frame.render_widget(Paragraph::new(line), area);
    view.record_click(
        Rect {
            x: area.x + 2,
            y: area.y,
            width: width.saturating_sub(2).min(area.width),
            height: 1,
        },
        ClickTarget::ShuffleFavorites,
    );
}

fn draw_filter_input(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let is_typing = view.favorites_filter_typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(shortcut_line(if is_typing {
            s.favorites_filter_typing
        } else {
            s.favorites_filter_normal
        }))
        .title_style(Theme::title());

    let input_text = if view.favorites_filter_input.is_empty() && !is_typing {
        Span::styled(s.favorites_filter_placeholder, Theme::dim())
    } else {
        Span::styled(&view.favorites_filter_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);
    view.record_click(area, ClickTarget::FilterInput);

    if is_typing {
        let cursor_x = area.x + 1 + view.favorites_filter_input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_favorites_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    if view.favorites_loading {
        let loading =
            Paragraph::new(Span::styled(s.loading, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    // Use filtered list when filter is active, otherwise full list
    let (items, selected): (Vec<_>, usize) = if view.favorites_filter_active() {
        let items: Vec<_> = view
            .favorites_filtered
            .iter()
            .map(|(_, item)| item)
            .collect();
        (items, view.favorites_filter_selected)
    } else {
        let items: Vec<_> = view.favorites_display.iter().collect();
        (items, view.favorites_selected)
    };

    if items.is_empty() {
        let msg = if view.favorites_filter_active() && !view.favorites_display.is_empty() {
            Span::styled(s.radios_no_results, Theme::dim())
        } else {
            Span::styled(s.no_favorites, Theme::dim())
        };
        let empty = Paragraph::new(msg).alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let headers = s.favorites_category_headers(view.favorites_category);
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(headers[0], Theme::dim())),
        Cell::from(Span::styled(headers[1], Theme::dim())),
        Cell::from(Span::styled(headers[2], Theme::dim())),
        Cell::from(Span::styled(headers[3], Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_playing = item
                .track
                .as_ref()
                .is_some_and(|t| view.is_playing_track(&t.track_id));
            Row::new(vec![
                Cell::from(track_number(i, is_playing)),
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

    let title = s.favorites_category_title(view.favorites_category, items.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::NONE)
            .title(title)
            .title_style(Theme::title()),
    )
    .row_highlight_style(Theme::highlight())
    .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
    view.record_rows(
        area,
        2, // title + header
        table_state.offset(),
        items.len(),
        RowsKind::Tab,
    );
}
