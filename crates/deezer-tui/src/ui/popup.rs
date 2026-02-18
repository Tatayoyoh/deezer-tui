use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::client::{PopupMenu, SubMenu, ViewState};
use crate::theme::Theme;

/// Draw the popup overlay if one is active.
pub fn draw(frame: &mut Frame, view: &ViewState) {
    // Draw toast notification (takes priority display but doesn't block popup)
    if let Some(ref toast) = view.toast {
        draw_toast(frame, &toast.message);
    }

    let Some(ref popup) = view.popup else {
        return;
    };

    match &popup.sub_menu {
        Some(SubMenu::PlaylistPicker { playlists, selected, loading }) => {
            draw_playlist_picker(frame, playlists, *selected, *loading, &popup.track.title);
        }
        Some(SubMenu::TrackInfo) => {
            draw_track_info(frame, popup);
        }
        None => {
            draw_main_menu(frame, popup);
        }
    }
}

fn draw_main_menu(frame: &mut Frame, popup: &PopupMenu) {
    let area = frame.area();
    let popup_area = centered_rect(40, popup.items.len() as u16 + 4, area);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Build title
    let title = if let Some(ref t) = popup.title {
        format!(" {} ", t)
    } else {
        format!(" {} — {} ", popup.track.title, popup.track.artist)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Build list items
    let items: Vec<ListItem> = popup.items.iter().enumerate().map(|(i, item)| {
        if item.is_header {
            ListItem::new(Line::from(Span::styled(
                &item.label,
                Style::default().fg(Theme::PRIMARY).add_modifier(Modifier::BOLD),
            )))
        } else {
            let prefix = if i == popup.selected { " > " } else { "   " };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}", prefix, item.label),
                if i == popup.selected {
                    Theme::highlight()
                } else {
                    Theme::text()
                },
            )))
        }
    }).collect();

    let list = List::new(items);

    let mut list_state = ListState::default();
    list_state.select(Some(popup.selected));

    frame.render_widget(list, inner);
}

fn draw_playlist_picker(
    frame: &mut Frame,
    playlists: &[deezer_core::api::models::PlaylistData],
    selected: usize,
    loading: bool,
    track_title: &str,
) {
    let area = frame.area();
    let height = if loading { 5 } else { (playlists.len() as u16).min(20) + 4 };
    let popup_area = centered_rect(45, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .title(format!(" Add \"{}\" to playlist ", track_title))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if loading {
        let loading_text = Paragraph::new("Loading playlists...")
            .style(Theme::dim())
            .alignment(Alignment::Center);
        frame.render_widget(loading_text, inner);
        return;
    }

    if playlists.is_empty() {
        let empty_text = Paragraph::new("No playlists found")
            .style(Theme::dim())
            .alignment(Alignment::Center);
        frame.render_widget(empty_text, inner);
        return;
    }

    let items: Vec<ListItem> = playlists.iter().enumerate().map(|(i, pl)| {
        let prefix = if i == selected { " > " } else { "   " };
        let text = format!("{}{} ({} tracks)", prefix, pl.title, pl.nb_songs);
        ListItem::new(Line::from(Span::styled(
            text,
            if i == selected { Theme::highlight() } else { Theme::text() },
        )))
    }).collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn draw_track_info(frame: &mut Frame, popup: &PopupMenu) {
    let area = frame.area();
    let popup_area = centered_rect(50, 12, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .title(" Track Info ")
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let track = &popup.track;
    let dur = track.duration_secs();

    let info_lines = vec![
        Line::from(vec![
            Span::styled("Title:    ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(&track.title, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Artist:   ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(&track.artist, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Album:    ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(&track.album, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Duration: ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(format!("{}:{:02}", dur / 60, dur % 60), Theme::text()),
        ]),
        Line::from(vec![
            Span::styled("Track ID: ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(&track.track_id, Theme::text()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc to close",
            Theme::dim(),
        )),
    ];

    let paragraph = Paragraph::new(info_lines);
    frame.render_widget(paragraph, inner);
}

/// Draw a temporary toast notification at the bottom center of the screen.
fn draw_toast(frame: &mut Frame, message: &str) {
    let area = frame.area();
    let width = (message.len() as u16 + 6).min(area.width.saturating_sub(4));
    let height = 3;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + area.height.saturating_sub(height + 5); // Above the player bar

    let toast_area = Rect::new(x, y, width, height);
    frame.render_widget(Clear, toast_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Theme::SUCCESS))
        .style(Style::default().bg(Theme::SURFACE));

    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    let text = Paragraph::new(message)
        .style(Theme::text())
        .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

/// Create a centered rectangle with a given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_height = height.min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    Rect::new(x, y, popup_width, popup_height)
}
