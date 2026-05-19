use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    if view.moods_loading {
        let loading = Paragraph::new(Span::styled(s.moods_loading, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let items = &view.moods;
    if items.is_empty() {
        let empty = Paragraph::new(Span::styled(s.moods_no_results, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.explore_cat_moods, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = items
        .iter()
        .enumerate()
        .map(|(i, m)| {
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&m.title, Theme::text())),
            ])
        })
        .collect();

    let widths = [Constraint::Length(4), Constraint::Fill(1)];
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(s.explore_cat_moods)
                .title_style(Theme::title()),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(view.moods_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
