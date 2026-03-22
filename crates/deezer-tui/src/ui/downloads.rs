use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::ViewState;
use crate::i18n::t;
use crate::protocol::OfflineCategory;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Category menu
            Constraint::Length(1), // Spacer
            Constraint::Min(3),    // Content table
        ])
        .split(area);

    draw_category_menu(frame, view.offline_category, chunks[0]);

    match view.offline_category {
        OfflineCategory::Tracks => draw_tracks_table(frame, view, chunks[2]),
        OfflineCategory::Albums => draw_albums_table(frame, view, chunks[2]),
    }
}

fn draw_category_menu(frame: &mut Frame, current: OfflineCategory, area: Rect) {
    let s = t();
    let spans: Vec<Span> = OfflineCategory::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, cat)| {
            let mut parts = Vec::new();
            if i > 0 {
                parts.push(Span::styled("  ", Theme::dim()));
            }
            if *cat == current {
                parts.push(Span::styled(
                    s.offline_category_label(*cat),
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            } else {
                parts.push(Span::styled(s.offline_category_label(*cat), Theme::dim()));
            }
            parts
        })
        .collect();

    let line = Line::from(spans);
    let menu = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(menu, area);
}

fn draw_tracks_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();

    if view.offline_loading {
        let loading =
            Paragraph::new(Span::styled(s.loading, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    if view.offline_tracks.is_empty() {
        let empty = Paragraph::new(Span::styled(s.offline_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_album, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = view
        .offline_tracks
        .iter()
        .enumerate()
        .map(|(i, ot)| {
            let track = &ot.track;
            let dur = track.duration_secs();
            let is_current = view
                .current_track
                .as_ref()
                .is_some_and(|ct| ct.track_id == track.track_id);

            let num_style = if is_current {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            };
            let prefix = if is_current { "▶" } else { "" };

            Row::new(vec![
                Cell::from(Span::styled(format!("{}{:>3}", prefix, i + 1), num_style)),
                Cell::from(Span::styled(&track.title, Theme::text())),
                Cell::from(Span::styled(
                    &track.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(&track.album, Theme::dim())),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let title = format!(
        " {} ({}) ",
        s.offline_category_label(OfflineCategory::Tracks),
        view.offline_tracks.len()
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
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

    let mut table_state = TableState::default().with_selected(Some(view.offline_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn draw_albums_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    use crate::client::OfflineTreeItem;

    let s = t();

    if view.offline_albums.is_empty() {
        let empty = Paragraph::new(Span::styled(s.offline_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let tree_items = view.offline_tree_items();

    let rows: Vec<Row> = tree_items
        .iter()
        .enumerate()
        .map(|(_, item)| match item {
            OfflineTreeItem::Album(album_idx) => {
                let album = &view.offline_albums[*album_idx];
                let expanded = view.offline_expanded.contains(&album.album_id);
                let arrow = if expanded { "▾" } else { "▸" };
                Row::new(vec![
                    Cell::from(Span::styled(
                        format!(" {arrow}"),
                        Style::default()
                            .fg(Theme::primary())
                            .add_modifier(Modifier::BOLD),
                    )),
                    Cell::from(Span::styled(
                        &album.title,
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Theme::text_color()),
                    )),
                    Cell::from(Span::styled(
                        &album.artist,
                        Style::default().fg(Theme::primary()),
                    )),
                    Cell::from(Span::styled(
                        format!("{} tracks", album.nb_tracks),
                        Theme::dim(),
                    )),
                    Cell::from(Span::raw("")),
                ])
            }
            OfflineTreeItem::Track(album_idx, track_idx) => {
                let track = &view.offline_albums[*album_idx].tracks[*track_idx];
                let dur = track.duration_secs();
                let is_current = view
                    .current_track
                    .as_ref()
                    .is_some_and(|ct| ct.track_id == track.track_id);
                let prefix = if is_current { "  ▶" } else { "   " };
                let num_style = if is_current {
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Theme::dim()
                };

                Row::new(vec![
                    Cell::from(Span::styled(
                        format!("{}{:>3}", prefix, track_idx + 1),
                        num_style,
                    )),
                    Cell::from(Span::styled(&track.title, Theme::text())),
                    Cell::from(Span::raw("")),
                    Cell::from(Span::raw("")),
                    Cell::from(Span::styled(
                        format!("{}:{:02}", dur / 60, dur % 60),
                        Theme::dim(),
                    )),
                ])
            }
        })
        .collect();

    let title = format!(
        " {} ({}) ",
        s.offline_category_label(OfflineCategory::Albums),
        view.offline_albums.len()
    );
    let hint = Line::from(vec![
        Span::styled(s.hint_expand_collapse, Theme::dim()),
        Span::raw(" "),
    ]);
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Length(6),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::NONE)
            .title(title)
            .title_style(Theme::title())
            .title(ratatui::widgets::block::Title::from(hint).alignment(Alignment::Right)),
    )
    .row_highlight_style(Theme::highlight())
    .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(view.offline_tree_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
