use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use deezer_core::player::state::PlaybackStatus;

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Theme::border())
        .style(Style::default().bg(Theme::surface()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Track info
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Controls
        ])
        .split(inner);

    // Track info line
    let status_icon = match view.status {
        PlaybackStatus::Playing => Span::styled("  >> ", Style::default().fg(Color::Green)),
        PlaybackStatus::Paused => Span::styled("  || ", Style::default().fg(Color::Yellow)),
        PlaybackStatus::Loading => Span::styled("  .. ", Style::default().fg(Color::Cyan)),
        PlaybackStatus::Stopped => Span::styled("  [] ", Theme::dim()),
    };

    let (track_left, track_right) = if let Some(ref track) = view.current_track {
        (
            Line::from(vec![
                status_icon,
                Span::styled(
                    &track.title,
                    Style::default()
                        .fg(Theme::text_color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" - ", Theme::dim()),
                Span::styled(&track.artist, Style::default().fg(Theme::primary())),
                Span::styled(format!("  ({})", &track.album), Theme::dim()),
            ]),
            Line::from(Span::styled(
                format!("[{}] ", view.quality.as_api_format()),
                Theme::dim(),
            )),
        )
    } else {
        (
            Line::from(vec![
                status_icon,
                Span::styled(t().no_track_loaded, Theme::dim()),
            ]),
            Line::default(),
        )
    };
    let quality_width = track_right.width() as u16;
    let track_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(quality_width)])
        .split(chunks[0]);
    frame.render_widget(Paragraph::new(track_left), track_chunks[0]);
    frame.render_widget(
        Paragraph::new(track_right).alignment(Alignment::Right),
        track_chunks[1],
    );

    // Progress bar
    let ratio = view.progress_percent().min(1.0);
    let time_label = view.format_position();

    let progress = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Theme::progress_fill())
                .bg(Theme::progress_bg()),
        )
        .ratio(ratio)
        .label(Span::styled(time_label, Theme::text()));

    frame.render_widget(progress, chunks[1]);

    // Controls line
    let vol_pct = (view.volume * 100.0) as u8;
    let shuffle_style = if view.shuffle {
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::dim()
    };
    let s = t();
    let controls_left = Line::from(vec![
        Span::styled("  ", Theme::dim()),
        Span::styled("[Space]", Theme::text()),
        Span::styled(format!(" {}  ", s.play_pause), Theme::dim()),
        Span::styled("[n]", Theme::text()),
        Span::styled(format!(" {}  ", s.next), Theme::dim()),
        Span::styled("[b]", Theme::text()),
        Span::styled(format!(" {}  ", s.prev), Theme::dim()),
        Span::styled(format!("[s] {}", s.shuffle), shuffle_style),
        Span::styled("  ", Theme::dim()),
        Span::styled("[?]", Theme::text()),
        Span::styled(format!(" {}", s.help), Theme::dim()),
    ]);
    let vol_label = format!("[+/-] {}: {vol_pct}% ", s.vol);
    let vol_width = vol_label.len() as u16;
    let controls_right = Line::from(Span::styled(vol_label, Theme::dim()));
    let ctrl_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(vol_width)])
        .split(chunks[2]);
    frame.render_widget(Paragraph::new(controls_left), ctrl_chunks[0]);
    frame.render_widget(
        Paragraph::new(controls_right).alignment(Alignment::Right),
        ctrl_chunks[1],
    );
}
