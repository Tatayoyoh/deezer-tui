use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::{InputMode, ViewState};
use crate::protocol::SearchCategory;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Length(1), // Category menu
            Constraint::Min(3),    // Results table
        ])
        .split(area);

    // Search input
    draw_search_input(frame, view, chunks[0]);

    // Category menu
    draw_category_menu(frame, view.search_category, chunks[1]);

    // Results table
    draw_results_table(frame, view, chunks[2]);
}

fn draw_search_input(frame: &mut Frame, view: &ViewState, area: Rect) {
    let is_typing = view.input_mode == InputMode::Typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(if is_typing {
            " Search (Enter to submit, Esc to cancel) "
        } else {
            " Search (press / to type) "
        })
        .title_style(Theme::title());

    let input_text = if view.search_input.is_empty() && !is_typing {
        Span::styled("Press / to search tracks, artists, albums...", Theme::dim())
    } else {
        Span::styled(&view.search_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);

    if is_typing {
        let cursor_x = area.x + 1 + view.search_input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_category_menu(frame: &mut Frame, current: SearchCategory, area: Rect) {
    let spans: Vec<Span> = SearchCategory::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, cat)| {
            let mut parts = Vec::new();
            if i > 0 {
                parts.push(Span::styled("  ", Theme::dim()));
            }
            if *cat == current {
                parts.push(Span::styled(
                    cat.label(),
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            } else {
                parts.push(Span::styled(cat.label(), Theme::dim()));
            }
            parts
        })
        .collect();

    let line = Line::from(spans);
    let menu = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(menu, area);
}

fn draw_results_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    if view.search_loading {
        let loading =
            Paragraph::new(Span::styled("Searching...", Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    if view.search_display.is_empty() {
        let empty_msg = Paragraph::new(Span::styled("No results yet", Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty_msg, area);
        return;
    }

    let headers = view.search_category.headers();
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
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
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
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

    let title = format!(" Results ({}) ", view.search_display.len());
    let widths = view.search_category.column_widths();
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(title)
                .title_style(Theme::title()),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(view.search_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
