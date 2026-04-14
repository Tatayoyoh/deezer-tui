pub mod album_detail;
pub mod artist_detail;
pub mod common;
pub mod downloads;
pub mod favorites;
pub mod login;
pub mod player;
pub mod popup;
pub mod radio;
pub mod search;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

use crate::client::{Overlay, ViewState};
use crate::i18n::t;
use crate::protocol::{ActiveTab, Screen};
use crate::theme::Theme;

/// Root draw function — dispatches to the correct screen.
pub fn draw(frame: &mut Frame, view: &mut ViewState) {
    match view.screen {
        Screen::Login => login::draw(frame, view),
        Screen::Main => draw_main(frame, view),
    }
}

/// Draw the main screen with tabs + content + player bar.
fn draw_main(frame: &mut Frame, view: &mut ViewState) {
    let area = frame.area();

    // Full-screen themed background
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::bg_with_opacity())),
        area,
    );

    // Layout: tabs (3 lines) | content (fill) | player bar (4 lines)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(5),    // Content area
            Constraint::Length(4), // Player bar
        ])
        .split(area);

    // Tab bar
    draw_tabs(frame, view, chunks[0]);

    // Content area: detail overlays replace tab content.
    // Show album/artist detail as background whenever it appears anywhere in the overlay chain
    // (current overlay or any stacked overlay). This handles cases where Help/Settings/Info
    // are pushed on top of a detail view.
    let all_overlays =
        std::iter::once(view.overlay.as_ref()).chain(view.overlay_stack.iter().rev().map(Some));
    let mut show_album = false;
    let mut show_artist = false;
    for o in all_overlays {
        match o {
            Some(Overlay::AlbumDetail { .. }) => {
                show_album = true;
                break;
            }
            Some(Overlay::ArtistDetail) => {
                show_artist = true;
                break;
            }
            _ => {}
        }
    }

    if show_album {
        album_detail::draw(frame, view, chunks[1]);
    } else if show_artist {
        artist_detail::draw(frame, view, chunks[1]);
    } else {
        match view.active_tab {
            ActiveTab::Search => search::draw(frame, view, chunks[1]),
            ActiveTab::Favorites => favorites::draw(frame, view, chunks[1]),
            ActiveTab::Radio => radio::draw(frame, view, chunks[1]),
            ActiveTab::Downloads => downloads::draw(frame, view, chunks[1]),
        }
    }

    // Player bar
    player::draw(frame, view, chunks[2]);

    // Popup overlay (drawn on top of everything)
    popup::draw(frame, view);
}

fn draw_tabs(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();
    let (tab_titles, selected) = if view.is_offline {
        (vec![Line::from(s.tab_offline_downloads)], 0)
    } else {
        (
            vec![
                Line::from(s.tab_search),
                Line::from(s.tab_favorites),
                Line::from(s.tab_radios),
                Line::from(s.tab_offline),
            ],
            match view.active_tab {
                ActiveTab::Search => 0,
                ActiveTab::Favorites => 1,
                ActiveTab::Radio => 2,
                ActiveTab::Downloads => 3,
            },
        )
    };

    let header_title = " Deezer-TUI ";

    let mut block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Theme::border())
        .title(header_title)
        .title_style(Theme::dim());

    if let Some(ref msg) = view.status_msg {
        let status_line = Line::from(vec![Span::styled(
            format!(" {msg} "),
            Style::default().fg(Color::Cyan),
        )]);
        block = block.title_top(status_line.alignment(Alignment::Right));
    }

    let tabs = Tabs::new(tab_titles)
        .block(block)
        .select(selected)
        .style(Theme::tab_inactive())
        .highlight_style(Theme::tab_active())
        .divider("|");

    frame.render_widget(tabs, area);

    // Draw hint on the same line as the tabs, aligned right.
    // In offline mode: show offline indicator instead of tab-switch hint.
    if view.is_offline {
        let text = s.offline_indicator;
        let text_len = text.chars().count() as u16;
        if area.width > text_len {
            let indicator_area = Rect {
                x: area.x + area.width - text_len,
                y: area.y + 1,
                width: text_len,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    text,
                    Style::default()
                        .fg(Theme::secondary())
                        .add_modifier(Modifier::BOLD),
                )),
                indicator_area,
            );
        }
    } else {
        let hint_text = format!("[Tab] {} ", s.help_switch_tabs);
        let hint_len = hint_text.len() as u16;
        if area.width > hint_len {
            let hint_area = Rect {
                x: area.x + area.width - hint_len,
                y: area.y + 1,
                width: hint_len,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(hint_text, Theme::dim())),
                hint_area,
            );
        }
    }
}
