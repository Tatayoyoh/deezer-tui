use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::client::ViewState;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    if view.favorites_loading {
        let loading = Paragraph::new(Span::styled("Loading favorites...", Theme::dim()))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border())
                    .title(" Favorites ")
                    .title_style(Theme::title()),
            );
        frame.render_widget(loading, area);
    } else if view.favorites.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No favorites yet — add some on Deezer!",
            Theme::dim(),
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(" Favorites ")
                .title_style(Theme::title()),
        );
        frame.render_widget(empty, area);
    } else {
        let items: Vec<ListItem> = view
            .favorites
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let dur = track.duration_secs();
                let line = Line::from(vec![
                    Span::styled(format!(" {:>3}. ", i + 1), Theme::dim()),
                    Span::styled(&track.title, Theme::text()),
                    Span::styled(" - ", Theme::dim()),
                    Span::styled(&track.artist, Style::default().fg(Theme::PRIMARY)),
                    Span::styled(
                        format!("  {}  [{}:{:02}]", &track.album, dur / 60, dur % 60),
                        Theme::dim(),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let title = format!(" Favorites ({}) ", view.favorites.len());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border())
                    .title(title)
                    .title_style(Theme::title()),
            )
            .highlight_style(Theme::highlight())
            .highlight_symbol("> ");

        let mut list_state = ListState::default().with_selected(Some(view.favorites_selected));
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}
