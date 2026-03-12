use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::ViewState;
use crate::protocol::FavoritesCategory;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Category menu
            Constraint::Length(2), // Shuffle button
            Constraint::Min(3),    // Favorites table
        ])
        .split(area);

    // Category menu
    draw_category_menu(frame, view.favorites_category, chunks[0]);

    // Shuffle button
    draw_shuffle_button(frame, chunks[1]);

    // Favorites table
    draw_favorites_table(frame, view, chunks[2]);
}

fn draw_category_menu(frame: &mut Frame, current: FavoritesCategory, area: Rect) {
    let spans: Vec<Span> = FavoritesCategory::ALL
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

fn draw_shuffle_button(frame: &mut Frame, area: Rect) {
    let button = Paragraph::new(Line::from(vec![
        Span::styled("  [g] ", Theme::dim()),
        Span::styled(
            "Jouer al\u{00e9}atoirement mes favoris",
            Style::default()
                .fg(Theme::secondary())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(button, area);
}

fn draw_favorites_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    if view.favorites_loading {
        let loading =
            Paragraph::new(Span::styled("Loading...", Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    if view.favorites_display.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No favorites yet \u{2014} add some on Deezer!",
            Theme::dim(),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let headers = view.favorites_category.headers();
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(headers[0], Theme::dim())),
        Cell::from(Span::styled(headers[1], Theme::dim())),
        Cell::from(Span::styled(headers[2], Theme::dim())),
        Cell::from(Span::styled(headers[3], Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = view
        .favorites_display
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

    let title = format!(" Favorites ({}) ", view.favorites_display.len());
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

    let mut table_state = TableState::default().with_selected(Some(view.favorites_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
