pub mod common;
pub mod favorites;
pub mod login;
pub mod player;
pub mod radio;
pub mod search;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::{ActiveTab, App, Screen};
use crate::theme::Theme;

/// Root draw function — dispatches to the correct screen.
pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Login => login::draw(frame, app),
        Screen::Main => draw_main(frame, app),
    }
}

/// Draw the main screen with tabs + content + player bar.
fn draw_main(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Layout: tabs (3 lines) | content (fill) | player bar (4 lines)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tab bar
            Constraint::Min(5),    // Content area
            Constraint::Length(4), // Player bar
        ])
        .split(area);

    // Tab bar
    draw_tabs(frame, app, chunks[0]);

    // Content area (based on active tab)
    match app.active_tab {
        ActiveTab::Search => search::draw(frame, app, chunks[1]),
        ActiveTab::Favorites => favorites::draw(frame, app, chunks[1]),
        ActiveTab::Radio => radio::draw(frame, app, chunks[1]),
    }

    // Player bar
    player::draw(frame, app, chunks[2]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let tab_titles = vec![
        Line::from(" Search "),
        Line::from(" Favorites "),
        Line::from(" Radios / Podcasts "),
    ];

    let selected = match app.active_tab {
        ActiveTab::Search => 0,
        ActiveTab::Favorites => 1,
        ActiveTab::Radio => 2,
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
