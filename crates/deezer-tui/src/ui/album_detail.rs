use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState, Wrap,
};
use ratatui_image::StatefulImage;

use deezer_core::api::models::AlbumDetail;

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

/// Draw the album detail overlay (replaces the content area).
pub fn draw(frame: &mut Frame, view: &mut ViewState, area: Rect) {
    let s = t();
    if view.album_detail_loading {
        let loading = Paragraph::new(Span::styled(s.loading_album, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let Some(ref detail) = view.album_detail else {
        let msg = Paragraph::new(Span::styled(s.no_album_loaded, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    };

    // Two columns: 40% info | 60% track list
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let detail = detail.clone();
    draw_album_info(frame, &detail, view, columns[0]);
    draw_track_list(
        frame,
        &detail,
        view.album_detail_selected,
        view.album_detail_left_focused,
        columns[1],
    );
}

/// Draw the left column: album cover + metadata.
fn draw_album_info(frame: &mut Frame, detail: &AlbumDetail, view: &mut ViewState, area: Rect) {
    let focused = view.album_detail_left_focused;
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(if focused {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Album art
            Constraint::Length(1),  // Spacer
            Constraint::Min(6),     // Metadata
        ])
        .split(inner);

    // Album art: real image or placeholder
    if let Some(ref mut proto) = view.cover_image {
        let image_widget = StatefulImage::<ratatui_image::protocol::StatefulProtocol>::default();
        frame.render_stateful_widget(image_widget, chunks[0], proto);
    } else {
        draw_album_art(frame, chunks[0]);
    }

    // Album metadata with scrollbar
    let meta_area = chunks[2];
    let scroll = view.album_detail_left_scroll;
    let content_lines = album_metadata_line_count(detail);
    let visible_lines = meta_area.height;

    if content_lines > visible_lines {
        // Clamp scroll
        let max_scroll = content_lines.saturating_sub(visible_lines);
        view.album_detail_left_scroll = scroll.min(max_scroll);
        let scroll = view.album_detail_left_scroll;

        // Reserve 1 col for scrollbar
        let text_area = Rect {
            width: meta_area.width.saturating_sub(1),
            ..meta_area
        };
        draw_album_metadata(frame, detail, text_area, scroll);

        // thumb = 1/4 of track: content_length=4, viewport=1 → ratio 1/4
        let thumb_pos = if max_scroll > 0 {
            (scroll as usize * 3) / max_scroll as usize
        } else {
            0
        };
        let mut sb_state = ScrollbarState::new(4)
            .viewport_content_length(1)
            .position(thumb_pos);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).thumb_style(Theme::dim()),
            meta_area,
            &mut sb_state,
        );
    } else {
        view.album_detail_left_scroll = 0;
        draw_album_metadata(frame, detail, meta_area, 0);
    }
}

/// Estimate number of lines rendered by album metadata (without wrapping).
fn album_metadata_line_count(detail: &AlbumDetail) -> u16 {
    let mut n: u16 = 3; // title + artist + blank
    if !detail.release_date.is_empty() {
        n += 1;
    }
    n += 1; // tracks
    if !detail.label.is_empty() {
        n += 1;
    }
    n += 4; // blank + esc_back + enter_play + download_hint
    n
}

/// Draw a placeholder album cover using Unicode block characters.
fn draw_album_art(frame: &mut Frame, area: Rect) {
    let width = area.width.min(24);
    let height = area.height.min(12);

    // Align art to the left
    let art_area = Rect::new(area.x, area.y, width, height);

    let primary = Theme::primary();
    let dim_color = Theme::text_dim_color();

    // Simple album cover using block characters
    let mut lines = Vec::new();

    // Top border
    let top = "┌".to_string() + &"─".repeat((width - 2) as usize) + "┐";
    lines.push(Line::from(Span::styled(
        top,
        Style::default().fg(dim_color),
    )));

    // Inner area with music note
    let inner_h = height.saturating_sub(2) as usize;
    let mid = inner_h / 2;

    for i in 0..inner_h {
        let content = if i == mid.saturating_sub(1) {
            format!("│{:^w$}│", "♫  ♪  ♫", w = (width - 2) as usize)
        } else if i == mid {
            format!("│{:^w$}│", "♪  ♫  ♪", w = (width - 2) as usize)
        } else if i == mid + 1 {
            format!("│{:^w$}│", "♫  ♪  ♫", w = (width - 2) as usize)
        } else {
            format!("│{}│", " ".repeat((width - 2) as usize))
        };

        let style = if i >= mid.saturating_sub(1) && i <= mid + 1 {
            Style::default().fg(primary)
        } else {
            Style::default().fg(dim_color)
        };

        lines.push(Line::from(Span::styled(content, style)));
    }

    // Bottom border
    let bottom = "└".to_string() + &"─".repeat((width - 2) as usize) + "┘";
    lines.push(Line::from(Span::styled(
        bottom,
        Style::default().fg(dim_color),
    )));

    let art = Paragraph::new(lines);
    frame.render_widget(art, art_area);
}

/// Draw album metadata (title, artist, date, tracks, label).
fn draw_album_metadata(frame: &mut Frame, detail: &AlbumDetail, area: Rect, scroll: u16) {
    let s = t();
    let label_style = Style::default().fg(Theme::text_dim_color());
    let value_style = Theme::text();
    let title_style = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::BOLD);

    let mut lines = vec![
        Line::from(Span::styled(&detail.title, title_style)),
        Line::from(Span::styled(&detail.artist, value_style)),
        Line::from(""),
    ];

    if !detail.release_date.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(s.date_label, label_style),
            Span::styled(&detail.release_date, value_style),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(s.tracks_label, label_style),
        Span::styled(format!("{}", detail.nb_tracks), value_style),
    ]));

    if !detail.label.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(s.label_label, label_style),
            Span::styled(&detail.label, value_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(s.esc_back, Theme::dim())));
    lines.push(Line::from(Span::styled(s.enter_play_track, Theme::dim())));
    lines.push(Line::from(vec![
        Span::styled("[d]", Theme::dim()),
        Span::styled(s.hint_download_album, Theme::dim()),
    ]));

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

/// Draw the right column: track list.
fn draw_track_list(
    frame: &mut Frame,
    detail: &AlbumDetail,
    selected: usize,
    left_focused: bool,
    area: Rect,
) {
    let s = t();
    if detail.tracks.is_empty() {
        let msg =
            Paragraph::new(Span::styled(s.no_tracks, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = detail
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let dur = track.duration_secs();
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&track.title, Theme::text())),
                Cell::from(Span::styled(
                    &track.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let title = s.album_tracks_title(&detail.title, detail.tracks.len());
    let widths = [
        Constraint::Length(4),
        Constraint::Percentage(50),
        Constraint::Percentage(30),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(title)
                .title_style(Theme::title())
                .padding(ratatui::widgets::Padding::new(1, 1, 0, 0)),
        )
        .row_highlight_style(if left_focused {
            Style::default()
        } else {
            Theme::highlight()
        })
        .highlight_symbol(if left_focused { "  " } else { "> " });

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
