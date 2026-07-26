pub mod album_detail;
pub mod artist_detail;
pub mod categories;
pub mod common;
pub mod downloads;
pub mod explore;
pub mod favorites;
pub mod genre_detail;
pub mod login;
pub mod moods;
pub mod player;
pub mod popup;
pub mod radio;
pub mod search;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};

use crate::client::{ClickTarget, Overlay, ViewState};
use crate::i18n::t;
use crate::protocol::{ActiveTab, Screen};
use crate::theme::Theme;

/// Height in rows of the always-visible bottom player bar.
pub const PLAYER_BAR_HEIGHT: u16 = 4;

/// Root draw function — dispatches to the correct screen.
pub fn draw(frame: &mut Frame, view: &mut ViewState) {
    // Clickable regions are rebuilt by this pass.
    view.clear_click_areas();
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
            Constraint::Length(2),                 // Tab bar (tabs + bottom border)
            Constraint::Min(5),                    // Content area
            Constraint::Length(PLAYER_BAR_HEIGHT), // Player bar
        ])
        .split(area);

    // Content area: detail overlays replace tab content.
    // Show album/artist detail as background whenever it appears anywhere in the overlay chain
    // (current overlay or any stacked overlay). This handles cases where Help/Settings/Info
    // are pushed on top of a detail view.
    let all_overlays =
        std::iter::once(view.overlay.as_ref()).chain(view.overlay_stack.iter().rev().map(Some));
    let mut show_album = false;
    let mut show_artist = false;
    let mut show_genre = false;
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
            Some(Overlay::GenreDetail { .. }) => {
                show_genre = true;
                break;
            }
            _ => {}
        }
    }

    // Tab bar (computed after show_album/show_artist/show_genre)
    draw_tabs(
        frame,
        view,
        chunks[0],
        show_album || show_artist || show_genre,
    );

    if show_album {
        album_detail::draw(frame, view, chunks[1]);
    } else if show_artist {
        artist_detail::draw(frame, view, chunks[1]);
    } else if show_genre {
        genre_detail::draw(frame, view, chunks[1]);
    } else {
        match view.active_tab {
            ActiveTab::Search => search::draw(frame, view, chunks[1]),
            ActiveTab::Favorites => favorites::draw(frame, view, chunks[1]),
            ActiveTab::Explore => explore::draw(frame, view, chunks[1]),
            ActiveTab::Downloads => downloads::draw(frame, view, chunks[1]),
        }
    }

    // Player bar
    player::draw(frame, view, chunks[2]);

    // Popup overlay (drawn on top of everything)
    popup::draw(frame, view);

    // Status notification: right-aligned on the player bar's top separator.
    // Drawn last so popups' backdrop dimming doesn't wash it out.
    draw_notification(frame, view, chunks[2]);
}

/// Draw the transient status notification as a right-aligned chip on the
/// player bar's top separator line. Hidden once the message has expired.
fn draw_notification(frame: &mut Frame, view: &ViewState, player_area: Rect) {
    let Some(msg) = view.visible_status_msg() else {
        return;
    };
    let line = Line::from(Span::styled(format!(" {msg} "), Theme::notification()));
    let width = (line.width() as u16).min(player_area.width);
    if width == 0 || player_area.height == 0 {
        return;
    }
    let notif_area = Rect {
        x: player_area.x + player_area.width - width,
        y: player_area.y,
        width,
        height: 1,
    };
    frame.render_widget(Clear, notif_area);
    frame.render_widget(Paragraph::new(line), notif_area);
}

/// Where the header "Back" hint navigates to: the previous overlay's name (the
/// entry just below the current one on the stack), or the active tab when there
/// is nothing stacked underneath.
fn back_destination(view: &ViewState) -> String {
    let s = t();
    let name = match view.overlay_stack.last() {
        Some(Overlay::ArtistDetail) => view.artist_detail.as_ref().map(|a| a.name.as_str()),
        Some(Overlay::AlbumDetail { .. }) => view.album_detail.as_ref().map(|a| a.title.as_str()),
        Some(Overlay::PlaylistDetail { .. }) => {
            view.playlist_detail.as_ref().map(|p| p.title.as_str())
        }
        Some(Overlay::GenreDetail { .. }) => view.genre_detail.as_ref().map(|g| g.name.as_str()),
        _ => None,
    };
    let dest = match name {
        Some(n) if !n.is_empty() => n,
        _ => match view.active_tab {
            ActiveTab::Search => s.tab_search,
            ActiveTab::Favorites => s.tab_favorites,
            ActiveTab::Explore => s.tab_explore,
            ActiveTab::Downloads => s.tab_offline,
        }
        .trim(),
    };
    // Cap length so the header can't overflow the tab bar.
    const MAX: usize = 32;
    if dest.chars().count() > MAX {
        format!("{}…", dest.chars().take(MAX - 1).collect::<String>())
    } else {
        dest.to_string()
    }
}

fn draw_tabs(frame: &mut Frame, view: &ViewState, area: Rect, show_back: bool) {
    let s = t();

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Theme::border());

    if show_back {
        // Replace tabs with an "Esc Back to <destination>" navigation hint.
        let dest = back_destination(view);
        let back_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc", Theme::shortcut_key()),
            Span::styled(format!(" {} {} ", s.back_to, dest), Theme::dim()),
        ]);
        let width = (back_line.width() as u16).min(area.width);
        let paragraph = Paragraph::new(back_line).block(block);
        frame.render_widget(paragraph, area);
        view.record_click(
            Rect {
                width,
                height: 1,
                ..area
            },
            ClickTarget::Back,
        );
        return;
    }

    let (titles, selected) = if view.is_offline {
        (vec![s.tab_offline_downloads], 0)
    } else {
        (
            vec![s.tab_search, s.tab_favorites, s.tab_explore, s.tab_offline],
            match view.active_tab {
                ActiveTab::Search => 0,
                ActiveTab::Favorites => 1,
                ActiveTab::Explore => 2,
                ActiveTab::Downloads => 3,
            },
        )
    };

    if !view.is_offline {
        const TABS: [ActiveTab; 4] = [
            ActiveTab::Search,
            ActiveTab::Favorites,
            ActiveTab::Explore,
            ActiveTab::Downloads,
        ];
        for (rect, tab) in common::tab_rects(area, &titles).into_iter().zip(TABS) {
            view.record_click(rect, ClickTarget::Tab(tab));
        }
    }

    let tab_titles: Vec<Line> = titles.iter().map(|t| Line::from(*t)).collect();
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
                y: area.y,
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
        let hint_text = format!("Tab {} ", s.help_switch_tabs);
        let hint_len = hint_text.len() as u16;
        if area.width > hint_len {
            let hint_area = Rect {
                x: area.x + area.width - hint_len,
                y: area.y,
                width: hint_len,
                height: 1,
            };
            let hint_line = Line::from(vec![
                Span::styled("Tab", Theme::shortcut_key()),
                Span::styled(format!(" {} ", s.help_switch_tabs), Theme::dim()),
            ]);
            frame.render_widget(Paragraph::new(hint_line), hint_area);
        }
    }
}
