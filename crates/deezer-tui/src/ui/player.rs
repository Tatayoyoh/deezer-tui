use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use deezer_core::player::state::{PlaybackStatus, RepeatMode};

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
            Constraint::Length(1), // Progress bar + volume
            Constraint::Length(1), // Controls + Flow button
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

    // Progress bar + volume (right)
    let s = t();
    let vol_pct = (view.volume * 100.0) as u8;
    let vol_rest = format!(" {}: {vol_pct}% ", s.vol);
    let vol_width = (1 + 3 + vol_rest.len()) as u16; // " " + "+/-" chip + rest

    let progress_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(vol_width)])
        .split(chunks[1]);

    let ratio = view.progress_percent().min(1.0);
    let time_label = view.format_position();
    let progress = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Theme::progress_fill())
                .bg(Theme::progress_bg()),
        )
        .ratio(ratio)
        .label(time_label);
    frame.render_widget(progress, progress_chunks[0]);

    let vol_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("+/-", Theme::shortcut_key()),
        Span::styled(vol_rest, Theme::dim()),
    ]);
    frame.render_widget(
        Paragraph::new(vol_line).alignment(Alignment::Right),
        progress_chunks[1],
    );

    // Controls line + Flow button (right)
    let shuffle_style = if view.shuffle {
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::dim()
    };
    let (repeat_label, repeat_style) = match view.repeat {
        RepeatMode::Off => (s.repeat, Theme::dim()),
        RepeatMode::Queue => (
            s.repeat_all,
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        RepeatMode::Track => (
            s.repeat_one,
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
    };
    // Shortcut keys render as a white "chip" (no brackets, no padding); labels
    // have no bg.
    let key = Theme::shortcut_key();
    let controls_left = Line::from(vec![
        Span::styled("  ", Theme::dim()),
        Span::styled("?", key),
        Span::styled(format!(" {}  ", s.help), Theme::dim()),
        Span::styled("Space", key),
        Span::styled(format!(" {}  ", s.play_pause), Theme::dim()),
        Span::styled("n", key),
        Span::styled(format!(" {}  ", s.next), Theme::dim()),
        Span::styled("b", key),
        Span::styled(format!(" {}  ", s.prev), Theme::dim()),
        Span::styled("s", key),
        Span::styled(format!(" {}  ", s.shuffle), shuffle_style),
        Span::styled("r", key),
        Span::styled(format!(" {}", repeat_label), repeat_style),
    ]);

    // Flow is special: the whole "f Flow" chip is white on a secondary bg,
    // with only the "f" key underlined.
    let flow_style = Theme::shortcut_flow_chip();
    let flow_label = format!(" f {} ", s.flow);
    let flow_width = flow_label.chars().count() as u16;
    let controls_right = Line::from(vec![
        Span::styled(" ", flow_style),
        Span::styled("f", flow_style.add_modifier(Modifier::UNDERLINED)),
        Span::styled(format!(" {} ", s.flow), flow_style),
    ]);

    let ctrl_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(flow_width)])
        .split(chunks[2]);
    frame.render_widget(Paragraph::new(controls_left), ctrl_chunks[0]);
    frame.render_widget(
        Paragraph::new(controls_right).alignment(Alignment::Right),
        ctrl_chunks[1],
    );
}
