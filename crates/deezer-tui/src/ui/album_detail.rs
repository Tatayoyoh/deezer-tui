use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};

use deezer_core::api::models::AlbumDetail;

use crate::client::ViewState;
use crate::theme::Theme;

/// Draw the album detail overlay (replaces the content area).
pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    if view.album_detail_loading {
        let loading = Paragraph::new(Span::styled("Loading album...", Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let Some(ref detail) = view.album_detail else {
        let msg = Paragraph::new(Span::styled("No album loaded", Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    };

    // Two columns: 40% info | 60% track list
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    draw_album_info(frame, detail, columns[0]);
    draw_track_list(frame, detail, view.album_detail_selected, columns[1]);
}

/// Draw the left column: album cover placeholder + metadata.
fn draw_album_info(frame: &mut Frame, detail: &AlbumDetail, area: Rect) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Theme::border())
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Album art placeholder
            Constraint::Length(1),  // Spacer
            Constraint::Min(6),     // Metadata
        ])
        .split(inner);

    // Album art placeholder (pixel art)
    draw_album_art(frame, chunks[0]);

    // Album metadata
    draw_album_metadata(frame, detail, chunks[2]);
}

/// Draw a placeholder album cover using Unicode block characters.
fn draw_album_art(frame: &mut Frame, area: Rect) {
    let width = area.width.min(24);
    let height = area.height.min(12);

    // Center the art in the available area
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let art_area = Rect::new(x, area.y, width, height);

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
fn draw_album_metadata(frame: &mut Frame, detail: &AlbumDetail, area: Rect) {
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
            Span::styled("Date:    ", label_style),
            Span::styled(&detail.release_date, value_style),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Tracks:  ", label_style),
        Span::styled(format!("{}", detail.nb_tracks), value_style),
    ]));

    if !detail.label.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Label:   ", label_style),
            Span::styled(&detail.label, value_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Esc  Back", Theme::dim())));
    lines.push(Line::from(Span::styled("Enter  Play track", Theme::dim())));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw the right column: track list.
fn draw_track_list(frame: &mut Frame, detail: &AlbumDetail, selected: usize, area: Rect) {
    if detail.tracks.is_empty() {
        let msg =
            Paragraph::new(Span::styled("No tracks", Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled("Titre", Theme::dim())),
        Cell::from(Span::styled("Artiste", Theme::dim())),
        Cell::from(Span::styled("Durée", Theme::dim())),
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

    let title = format!(" {} — {} tracks ", detail.title, detail.tracks.len());
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
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}
