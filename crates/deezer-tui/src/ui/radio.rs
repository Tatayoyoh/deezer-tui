use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Filter input
            Constraint::Min(3),    // Radios table
        ])
        .split(area);

    // Filter input
    draw_filter_input(frame, view, chunks[0]);

    // Radios table
    draw_radios_table(frame, view, chunks[1]);
}

fn draw_filter_input(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let is_typing = view.radio_filter_typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(if is_typing {
            s.radios_filter_typing
        } else {
            s.radios_filter_normal
        })
        .title_style(Theme::title());

    let input_text = if view.radio_filter_input.is_empty() && !is_typing {
        Span::styled(s.radios_filter_placeholder, Theme::dim())
    } else {
        Span::styled(&view.radio_filter_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);

    if is_typing {
        let cursor_x = area.x + 1 + view.radio_filter_input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_radios_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    if view.radios_loading {
        let loading = Paragraph::new(Span::styled(s.radios_loading, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let filtered = &view.radios_filtered;

    if filtered.is_empty() {
        let empty = Paragraph::new(Span::styled(s.radios_no_results, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_radio, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, radio)| {
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&radio.title, Theme::text())),
            ])
        })
        .collect();

    let title = s.radios_count_title(filtered.len());
    let widths = [Constraint::Length(4), Constraint::Fill(1)];
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

    let mut table_state = TableState::default().with_selected(Some(view.radios_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
