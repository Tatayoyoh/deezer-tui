use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use deezer_core::player::state::{PlaybackStatus, RepeatMode};

use crate::client::ViewState;
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

    let track_info = if let Some(ref track) = view.current_track {
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
        ])
    } else {
        Line::from(vec![
            status_icon,
            Span::styled("No track loaded", Theme::dim()),
        ])
    };
    frame.render_widget(Paragraph::new(track_info), chunks[0]);

    // Progress bar
    let ratio = view.progress_percent().min(1.0);
    let time_label = view.format_position();
    let quality_label = if view.current_track.is_some() {
        format!("  {}", view.quality.as_api_format())
    } else {
        String::new()
    };

    let progress = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Theme::progress_fill())
                .bg(Theme::progress_bg()),
        )
        .ratio(ratio)
        .label(Span::styled(
            format!("{time_label}{quality_label}"),
            Theme::text(),
        ));

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
    let repeat_label = match view.repeat {
        RepeatMode::Off => "[r] Repeat",
        RepeatMode::Queue => "[r] Repeat All",
        RepeatMode::Track => "[r] Repeat One",
    };
    let repeat_style = if view.repeat != RepeatMode::Off {
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::dim()
    };

    let status_text = view.status_msg.as_deref().unwrap_or("");

    let controls = Line::from(vec![
        Span::styled("  ", Theme::dim()),
        Span::styled("[p]", Theme::text()),
        Span::styled(" Play/Pause  ", Theme::dim()),
        Span::styled("[n]", Theme::text()),
        Span::styled(" Next  ", Theme::dim()),
        Span::styled("[b]", Theme::text()),
        Span::styled(" Prev  ", Theme::dim()),
        Span::styled("[s] Shuffle", shuffle_style),
        Span::styled("  ", Theme::dim()),
        Span::styled(repeat_label, repeat_style),
        Span::styled(format!("  [+/-] Vol: {vol_pct}%"), Theme::dim()),
        Span::styled(
            format!("  {status_text}  "),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled("[?]", Theme::text()),
        Span::styled(" Help", Theme::dim()),
    ]);

    frame.render_widget(Paragraph::new(controls), chunks[2]);
}
