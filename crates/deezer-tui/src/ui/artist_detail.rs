use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState, Tabs, Wrap,
};
use ratatui_image::StatefulImage;

use deezer_core::api::models::{ArtistAlbumEntry, ArtistDetail, ArtistSubTab, SimilarArtistEntry};

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

/// Draw the artist detail overlay (replaces the content area).
pub fn draw(frame: &mut Frame, view: &mut ViewState, area: Rect) {
    let s = t();
    if view.artist_detail_loading {
        let loading = Paragraph::new(Span::styled(s.loading_artist, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let Some(ref detail) = view.artist_detail else {
        let msg = Paragraph::new(Span::styled(s.no_artist_loaded, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    };

    // Two columns: 40% info (max 90) | rest content
    let left_w = (area.width * 40 / 100).min(52);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_w), Constraint::Min(0)])
        .split(area);

    let detail = detail.clone();
    draw_artist_info(frame, &detail, view, columns[0]);
    // Auto-switch focus to right if left column lost its scrollbar (e.g. on resize)
    if !view.artist_detail_left_scrollable && view.artist_detail_left_focused {
        view.artist_detail_left_focused = false;
    }
    draw_right_panel(frame, &detail, view, columns[1]);
}

/// Draw the left column: artist art + metadata.
fn draw_artist_info(frame: &mut Frame, detail: &ArtistDetail, view: &mut ViewState, area: Rect) {
    let s = t();
    let focused = view.artist_detail_left_focused;
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

    // Image grows with available height but is capped to avoid pushing metadata too far down
    let image_h = inner.height.saturating_sub(5).clamp(8, 24);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(image_h), // Artist art (adaptive)
            Constraint::Length(1),       // Spacer
            Constraint::Min(4),          // Metadata
        ])
        .split(inner);

    // Artist art: real image or placeholder
    if let Some(ref mut proto) = view.cover_image {
        let image_widget = StatefulImage::<ratatui_image::protocol::StatefulProtocol>::default();
        frame.render_stateful_widget(image_widget, chunks[0], proto);
    } else {
        draw_artist_art(frame, chunks[0]);
    }

    // Artist metadata with scrollbar
    let meta_area = chunks[2];
    let scroll = view.artist_detail_left_scroll;
    let content_lines = artist_metadata_line_count();
    let visible_lines = meta_area.height;

    let label_style = Style::default().fg(Theme::text_dim_color());
    let value_style = Theme::text();
    let title_style = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::BOLD);

    let mut lines = vec![
        Line::from(Span::styled(&detail.name, title_style)),
        Line::from(""),
    ];
    lines.push(Line::from(vec![
        Span::styled(s.fans_label, label_style),
        Span::styled(format_fans(detail.nb_fan), value_style),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(s.esc_back, Theme::dim())));
    lines.push(Line::from(Span::styled(s.enter_play_track, Theme::dim())));
    lines.push(Line::from(Span::styled("←/→  Switch tab", Theme::dim())));

    view.artist_detail_left_scrollable = content_lines > visible_lines;

    if content_lines > visible_lines {
        let max_scroll = content_lines.saturating_sub(visible_lines);
        view.artist_detail_left_scroll = scroll.min(max_scroll);
        let scroll = view.artist_detail_left_scroll;

        let text_area = Rect {
            width: meta_area.width.saturating_sub(1),
            ..meta_area
        };
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0));
        frame.render_widget(paragraph, text_area);

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
        view.artist_detail_left_scroll = 0;
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, meta_area);
    }
}

/// Estimate number of lines rendered by artist metadata.
fn artist_metadata_line_count() -> u16 {
    7 // name + blank + fans + blank + esc_back + enter_play + switch_tab
}

/// Draw a placeholder artist image using Unicode block characters.
fn draw_artist_art(frame: &mut Frame, area: Rect) {
    let width = area.width;
    let height = area.height;

    // Align art to the left
    let art_area = Rect::new(area.x, area.y, width, height);

    let primary = Theme::primary();
    let dim_color = Theme::text_dim_color();

    let mut lines = Vec::new();

    // Top border
    let top = "┌".to_string() + &"─".repeat((width - 2) as usize) + "┐";
    lines.push(Line::from(Span::styled(
        top,
        Style::default().fg(dim_color),
    )));

    // Inner area with person icon
    let inner_h = height.saturating_sub(2) as usize;
    let mid = inner_h / 2;

    for i in 0..inner_h {
        let content = if i == mid.saturating_sub(1) {
            format!("│{:^w$}│", "♪", w = (width - 2) as usize)
        } else if i == mid {
            format!("│{:^w$}│", "◉", w = (width - 2) as usize)
        } else if i == mid + 1 {
            format!("│{:^w$}│", "♪", w = (width - 2) as usize)
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

/// Draw the right panel: sub-tab bar + content list.
fn draw_right_panel(frame: &mut Frame, detail: &ArtistDetail, view: &ViewState, area: Rect) {
    let s = t();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Sub-tab bar
            Constraint::Min(3),    // Content
        ])
        .split(area);

    // Sub-tab bar
    let tab_titles = vec![
        Line::from(s.artist_top_tracks),
        Line::from(s.artist_albums),
        Line::from(s.artist_lives),
        Line::from(s.artist_other),
        Line::from(s.artist_similar),
    ];

    let selected_tab = match view.artist_detail_sub_tab {
        ArtistSubTab::TopTracks => 0,
        ArtistSubTab::Albums => 1,
        ArtistSubTab::Lives => 2,
        ArtistSubTab::Other => 3,
        ArtistSubTab::Similar => 4,
    };

    let tab_active_style = if view.artist_detail_left_focused {
        Theme::tab_inactive()
    } else {
        Theme::tab_active()
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Theme::border()),
        )
        .select(selected_tab)
        .style(Theme::tab_inactive())
        .highlight_style(tab_active_style)
        .divider("|");

    frame.render_widget(tabs, chunks[0]);

    // Content based on sub-tab
    match view.artist_detail_sub_tab {
        ArtistSubTab::TopTracks => {
            draw_top_tracks(frame, detail, view.artist_detail_selected, chunks[1]);
        }
        ArtistSubTab::Similar => {
            draw_similar_artists(
                frame,
                &detail.similar_artists,
                view.artist_detail_selected,
                chunks[1],
            );
        }
        other => {
            let albums = detail.albums_for_tab(other);
            let show_type = other == ArtistSubTab::Other;
            draw_album_list(
                frame,
                &albums,
                view.artist_detail_selected,
                show_type,
                chunks[1],
            );
        }
    }
}

/// Draw the top tracks list.
fn draw_top_tracks(frame: &mut Frame, detail: &ArtistDetail, selected: usize, area: Rect) {
    let s = t();
    if detail.top_tracks.is_empty() {
        let msg =
            Paragraph::new(Span::styled(s.no_tracks, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_album, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = detail
        .top_tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let dur = track.duration_secs();
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&track.title, Theme::text())),
                Cell::from(Span::styled(
                    &track.album,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

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
                .padding(ratatui::widgets::Padding::new(1, 1, 0, 0)),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Draw the album list for Albums/Lives/Other sub-tabs.
fn draw_album_list(
    frame: &mut Frame,
    albums: &[&ArtistAlbumEntry],
    selected: usize,
    show_type: bool,
    area: Rect,
) {
    let s = t();
    if albums.is_empty() {
        let msg =
            Paragraph::new(Span::styled(s.no_albums, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let mut header_cells = vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
    ];
    if show_type {
        header_cells.push(Cell::from(Span::styled("Type", Theme::dim())));
    }
    header_cells.push(Cell::from(Span::styled(s.date_label.trim(), Theme::dim())));
    header_cells.push(Cell::from(Span::styled(s.header_fans, Theme::dim())));

    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = albums
        .iter()
        .enumerate()
        .map(|(i, album)| {
            let mut cells = vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&album.title, Theme::text())),
            ];
            if show_type {
                cells.push(Cell::from(Span::styled(
                    &album.record_type,
                    Style::default().fg(Theme::primary()),
                )));
            }
            cells.push(Cell::from(Span::styled(&album.release_date, Theme::dim())));
            cells.push(Cell::from(Span::styled(
                format_fans(album.fans),
                Theme::dim(),
            )));
            Row::new(cells)
        })
        .collect();

    let widths: Vec<Constraint> = if show_type {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(40),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(8),
        ]
    } else {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(50),
            Constraint::Length(12),
            Constraint::Length(8),
        ]
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(ratatui::widgets::Padding::new(1, 1, 0, 0)),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

/// Draw the similar artists list for the Similar sub-tab.
fn draw_similar_artists(
    frame: &mut Frame,
    artists: &[SimilarArtistEntry],
    selected: usize,
    area: Rect,
) {
    let s = t();
    if artists.is_empty() {
        let msg =
            Paragraph::new(Span::styled(s.no_tracks, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.fans_label.trim(), Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = artists
        .iter()
        .enumerate()
        .map(|(i, artist)| {
            Row::new(vec![
                Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                Cell::from(Span::styled(&artist.name, Theme::text())),
                Cell::from(Span::styled(format_fans(artist.nb_fan), Theme::dim())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Min(20),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(ratatui::widgets::Padding::new(1, 1, 0, 0)),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn format_fans(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
