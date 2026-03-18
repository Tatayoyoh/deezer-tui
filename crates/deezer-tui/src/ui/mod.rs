pub mod album_detail;
pub mod common;
pub mod downloads;
pub mod favorites;
pub mod login;
pub mod player;
pub mod popup;
pub mod radio;
pub mod search;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Tabs};

use crate::client::{Overlay, ViewState};
use crate::protocol::{ActiveTab, Screen};
use crate::theme::Theme;

/// Root draw function — dispatches to the correct screen.
pub fn draw(frame: &mut Frame, view: &ViewState) {
    match view.screen {
        Screen::Login => login::draw(frame, view),
        Screen::Main => draw_main(frame, view),
    }
}

/// Draw the main screen with tabs + content + player bar.
fn draw_main(frame: &mut Frame, view: &ViewState) {
    let area = frame.area();

    // Full-screen themed background
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::bg())),
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

    // Content area: album detail overlay replaces tab content
    if view.overlay == Some(Overlay::AlbumDetail) {
        album_detail::draw(frame, view, chunks[1]);
    } else {
        match view.active_tab {
            ActiveTab::Search => search::draw(frame, view, chunks[1]),
            ActiveTab::Favorites => favorites::draw(frame, view, chunks[1]),
            ActiveTab::Radio => radio::draw(frame, view, chunks[1]),
            ActiveTab::Downloads => downloads::draw(frame, chunks[1]),
        }
    }

    // Player bar
    player::draw(frame, view, chunks[2]);

    // Popup overlay (drawn on top of everything)
    popup::draw(frame, view);
}

fn draw_tabs(frame: &mut Frame, view: &ViewState, area: Rect) {
    let tab_titles = vec![
        Line::from(" Search "),
        Line::from(" Favorites "),
        Line::from(" Radios / Podcasts "),
        Line::from(" Downloads "),
    ];

    let selected = match view.active_tab {
        ActiveTab::Search => 0,
        ActiveTab::Favorites => 1,
        ActiveTab::Radio => 2,
        ActiveTab::Downloads => 3,
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Theme::border())
                .title(" deezer-tui ")
                .title_style(Theme::title()),
        )
        .select(selected)
        .style(Theme::tab_inactive())
        .highlight_style(Theme::tab_active())
        .divider("|");

    frame.render_widget(tabs, area);
}
