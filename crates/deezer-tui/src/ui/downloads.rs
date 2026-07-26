use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::{ClickTarget, RowsKind, ViewState};
use crate::i18n::t;
use crate::protocol::OfflineCategory;
use crate::theme::Theme;
use crate::ui::common;
use crate::ui::common::{shortcut_hint, shortcut_line, track_number};

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Category menu
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // Filter input
            Constraint::Min(3),    // Content table
        ])
        .split(area);

    draw_category_menu(frame, view, chunks[0]);
    draw_filter_input(frame, view, chunks[2]);

    match view.offline_category {
        OfflineCategory::Tracks => draw_tracks_table(frame, view, chunks[3]),
        OfflineCategory::Albums => draw_albums_table(frame, view, chunks[3]),
        OfflineCategory::Playlists => draw_playlists_table(frame, view, chunks[3]),
    }
}

fn draw_category_menu(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let labels: Vec<&str> = OfflineCategory::ALL
        .iter()
        .map(|cat| s.offline_category_label(*cat))
        .collect();
    let current = OfflineCategory::ALL
        .iter()
        .position(|cat| *cat == view.offline_category)
        .unwrap_or(0);
    common::draw_category_menu(frame, view, area, &labels, current);
}

fn draw_filter_input(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let is_typing = view.offline_filter_typing;
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

    let input_text = if view.offline_filter_input.is_empty() && !is_typing {
        Span::styled(s.offline_filter_placeholder, Theme::dim())
    } else {
        Span::styled(&view.offline_filter_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);
    view.record_click(area, ClickTarget::FilterInput);

    if is_typing {
        let cursor_x = area.x + 1 + view.offline_filter_input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_tracks_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();

    if view.offline_loading {
        let loading =
            Paragraph::new(Span::styled(s.loading, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let filtered = view.offline_tracks_filtered();
    if filtered.is_empty() {
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

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, (_, ot))| {
            let track = &ot.track;
            let dur = track.duration_secs();
            let is_current = view
                .current_track
                .as_ref()
                .is_some_and(|ct| ct.track_id == track.track_id);

            Row::new(vec![
                Cell::from(track_number(i, is_current)),
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
        filtered.len()
    );
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

    let selected = if view.offline_filter_active() {
        view.offline_filter_selected
    } else {
        view.offline_selected
    };
    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
    view.record_rows(
        area,
        2, // title + header
        table_state.offset(),
        filtered.len(),
        RowsKind::Tab,
    );
}

fn draw_albums_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();

    let albums = view.offline_albums_filtered();
    if albums.is_empty() {
        let empty = Paragraph::new(Span::styled(s.offline_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = albums
        .iter()
        .map(|(_, album)| {
            Row::new(vec![
                Cell::from(Span::styled(&album.title, Theme::text())),
                Cell::from(Span::styled(
                    &album.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(
                    format!("{} {}", album.nb_tracks, s.header_tracks),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let header = Row::new(vec![
        Cell::from(Span::styled(s.header_album, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_tracks, Theme::dim())),
    ])
    .height(1);

    let title = format!(
        " {} ({}) ",
        s.offline_category_label(OfflineCategory::Albums),
        albums.len()
    );
    render_list(frame, view, area, rows, header, title);
}

fn draw_playlists_table(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();

    let playlists = view.offline_playlists_filtered();
    if playlists.is_empty() {
        let empty = Paragraph::new(Span::styled(s.offline_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = playlists
        .iter()
        .map(|(_, playlist)| {
            Row::new(vec![
                Cell::from(Span::styled(&playlist.title, Theme::text())),
                Cell::from(Span::styled(
                    &playlist.creator,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(
                    format!("{} {}", playlist.nb_tracks, s.header_tracks),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let header = Row::new(vec![
        Cell::from(Span::styled(s.header_playlist, Theme::dim())),
        Cell::from(Span::styled(s.header_creator, Theme::dim())),
        Cell::from(Span::styled(s.header_tracks, Theme::dim())),
    ])
    .height(1);

    let title = format!(
        " {} ({}) ",
        s.offline_category_label(OfflineCategory::Playlists),
        playlists.len()
    );
    render_list(frame, view, area, rows, header, title);
}

/// Shared rendering for the flat album and playlist lists. `Enter` opens the
/// item's track list in a modal.
fn render_list(
    frame: &mut Frame,
    view: &ViewState,
    area: Rect,
    rows: Vec<Row>,
    header: Row,
    title: String,
) {
    let row_count = rows.len();
    let mut hint_spans = shortcut_hint(t().hint_open_tracks).spans;
    hint_spans.push(Span::raw(" "));
    let hint = Line::from(hint_spans);
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(45),
            Constraint::Percentage(35),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::NONE)
            .title(title)
            .title_style(Theme::title())
            .title_top(hint.alignment(Alignment::Right)),
    )
    .row_highlight_style(Theme::highlight())
    .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(view.offline_list_selected()));
    frame.render_stateful_widget(table, area, &mut table_state);
    view.record_rows(
        area,
        2, // title + header
        table_state.offset(),
        row_count,
        RowsKind::Tab,
    );
}
