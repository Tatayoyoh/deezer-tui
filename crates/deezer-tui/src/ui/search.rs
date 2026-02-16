use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::client::{InputMode, ViewState};
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(3),   // Results
        ])
        .split(area);

    // Search input
    let is_typing = view.input_mode == InputMode::Typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(if is_typing {
            " Search (Enter to submit, Esc to cancel) "
        } else {
            " Search (press / to type) "
        })
        .title_style(Theme::title());

    let input_text = if view.search_input.is_empty() && !is_typing {
        Span::styled("Press / to search tracks, artists, albums...", Theme::dim())
    } else {
        Span::styled(&view.search_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, chunks[0]);

    if is_typing {
        let cursor_x = chunks[0].x + 1 + view.search_input.len() as u16;
        let cursor_y = chunks[0].y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }

    // Results
    if view.search_loading {
        let loading = Paragraph::new(Span::styled("Searching...", Theme::dim()))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border())
                    .title(" Results ")
                    .title_style(Theme::dim()),
            );
        frame.render_widget(loading, chunks[1]);
    } else if view.search_results.is_empty() {
        let empty_msg = Paragraph::new(Span::styled("No results yet", Theme::dim()))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border())
                    .title(" Results ")
                    .title_style(Theme::dim()),
            );
        frame.render_widget(empty_msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = view
            .search_results
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let dur = track.duration_secs();
                let line = Line::from(vec![
                    Span::styled(format!(" {:>3}. ", i + 1), Theme::dim()),
                    Span::styled(&track.title, Theme::text()),
                    Span::styled(" - ", Theme::dim()),
                    Span::styled(&track.artist, Style::default().fg(Theme::PRIMARY)),
                    Span::styled(format!("  [{}:{:02}]", dur / 60, dur % 60), Theme::dim()),
                ]);
                ListItem::new(line)
            })
            .collect();

        let title = format!(" Results ({}) ", view.search_results.len());
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

        let mut list_state = ListState::default().with_selected(Some(view.search_selected));
        frame.render_stateful_widget(list, chunks[1], &mut list_state);
    }
}
