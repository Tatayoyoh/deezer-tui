use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::client::{Overlay, ViewState};
use crate::i18n::t;
use crate::protocol::GenreDetailSubTab;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &ViewState, area: Rect) {
    let s = t();

    if view.genre_detail_loading {
        let loading = Paragraph::new(Span::styled(s.genre_detail_loading, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(loading, area);
        return;
    }

    let Some(detail) = view.genre_detail.as_ref() else {
        let msg = Paragraph::new(Span::styled(s.genre_detail_no_data, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    };

    let (sub_tab, selected) = match view.overlay {
        Some(Overlay::GenreDetail { sub_tab, selected }) => (sub_tab, selected),
        _ => (GenreDetailSubTab::default(), 0),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Length(1), // Sub-tab menu
            Constraint::Min(3),    // Content
        ])
        .split(area);

    draw_title(frame, &detail.name, chunks[0]);
    draw_sub_tab_menu(frame, sub_tab, chunks[1]);
    draw_content(frame, detail, sub_tab, selected, chunks[2]);
}

fn draw_title(frame: &mut Frame, name: &str, area: Rect) {
    let title_line = Line::from(vec![Span::styled(
        format!(" {name} "),
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(title_line), area);
}

fn draw_sub_tab_menu(frame: &mut Frame, current: GenreDetailSubTab, area: Rect) {
    let s = t();
    let spans: Vec<Span> = GenreDetailSubTab::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let mut parts = Vec::new();
            if i > 0 {
                parts.push(Span::styled("  ", Theme::dim()));
            }
            if *tab == current {
                parts.push(Span::styled(
                    s.genre_detail_sub_tab_label(*tab),
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
            } else {
                parts.push(Span::styled(
                    s.genre_detail_sub_tab_label(*tab),
                    Theme::dim(),
                ));
            }
            parts
        })
        .collect();

    let menu = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    frame.render_widget(menu, area);
}

fn draw_content(
    frame: &mut Frame,
    detail: &deezer_core::api::models::GenreDetail,
    sub_tab: GenreDetailSubTab,
    selected: usize,
    area: Rect,
) {
    let s = t();

    let (rows, header_cells, widths, count): (Vec<Row>, Vec<Cell>, Vec<Constraint>, usize) =
        match sub_tab {
            GenreDetailSubTab::Tracks => {
                let rows: Vec<Row> = detail
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let dur = t.duration_secs();
                        Row::new(vec![
                            Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                            Cell::from(Span::styled(t.title.clone(), Theme::text())),
                            Cell::from(Span::styled(t.artist.clone(), Theme::text())),
                            Cell::from(Span::styled(t.album.clone(), Theme::dim())),
                            Cell::from(Span::styled(
                                format!("{}:{:02}", dur / 60, dur % 60),
                                Theme::dim(),
                            )),
                        ])
                    })
                    .collect();
                let header = vec![
                    Cell::from(Span::styled("#", Theme::dim())),
                    Cell::from(Span::styled(s.header_title, Theme::dim())),
                    Cell::from(Span::styled(s.header_artist, Theme::dim())),
                    Cell::from(Span::styled(s.header_album, Theme::dim())),
                    Cell::from(Span::styled(s.header_duration, Theme::dim())),
                ];
                let widths = vec![
                    Constraint::Length(4),
                    Constraint::Percentage(35),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Length(6),
                ];
                (rows, header, widths, detail.tracks.len())
            }
            GenreDetailSubTab::Albums => {
                let rows: Vec<Row> = detail
                    .albums
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        Row::new(vec![
                            Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                            Cell::from(Span::styled(a.title.clone(), Theme::text())),
                            Cell::from(Span::styled(a.artist.clone(), Theme::text())),
                        ])
                    })
                    .collect();
                let header = vec![
                    Cell::from(Span::styled("#", Theme::dim())),
                    Cell::from(Span::styled(s.header_album, Theme::dim())),
                    Cell::from(Span::styled(s.header_artist, Theme::dim())),
                ];
                let widths = vec![
                    Constraint::Length(4),
                    Constraint::Percentage(55),
                    Constraint::Percentage(45),
                ];
                (rows, header, widths, detail.albums.len())
            }
            GenreDetailSubTab::Artists => {
                let rows: Vec<Row> = detail
                    .artists
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        Row::new(vec![
                            Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                            Cell::from(Span::styled(a.name.clone(), Theme::text())),
                        ])
                    })
                    .collect();
                let header = vec![
                    Cell::from(Span::styled("#", Theme::dim())),
                    Cell::from(Span::styled(s.header_artist, Theme::dim())),
                ];
                let widths = vec![Constraint::Length(4), Constraint::Fill(1)];
                (rows, header, widths, detail.artists.len())
            }
            GenreDetailSubTab::Playlists => {
                let rows: Vec<Row> = detail
                    .playlists
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        Row::new(vec![
                            Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                            Cell::from(Span::styled(p.title.clone(), Theme::text())),
                            Cell::from(Span::styled(p.author.clone(), Theme::dim())),
                            Cell::from(Span::styled(format!("{}", p.nb_songs), Theme::dim())),
                        ])
                    })
                    .collect();
                let header = vec![
                    Cell::from(Span::styled("#", Theme::dim())),
                    Cell::from(Span::styled(s.header_playlist, Theme::dim())),
                    Cell::from(Span::styled(s.header_author, Theme::dim())),
                    Cell::from(Span::styled(s.header_tracks, Theme::dim())),
                ];
                let widths = vec![
                    Constraint::Length(4),
                    Constraint::Percentage(45),
                    Constraint::Percentage(35),
                    Constraint::Length(6),
                ];
                (rows, header, widths, detail.playlists.len())
            }
            GenreDetailSubTab::Radios => {
                let rows: Vec<Row> = detail
                    .radios
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        Row::new(vec![
                            Cell::from(Span::styled(format!("{:>3}", i + 1), Theme::dim())),
                            Cell::from(Span::styled(r.title.clone(), Theme::text())),
                        ])
                    })
                    .collect();
                let header = vec![
                    Cell::from(Span::styled("#", Theme::dim())),
                    Cell::from(Span::styled(s.header_radio, Theme::dim())),
                ];
                let widths = vec![Constraint::Length(4), Constraint::Fill(1)];
                (rows, header, widths, detail.radios.len())
            }
        };

    if count == 0 {
        let empty = Paragraph::new(Span::styled(s.genre_detail_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let title = format!(" {} ({}) ", s.genre_detail_sub_tab_label(sub_tab), count);
    let header_row = Row::new(header_cells).height(1);
    let table = Table::new(rows, widths)
        .header(header_row)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(title)
                .title_style(Theme::title()),
        )
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut state = TableState::default().with_selected(Some(selected.min(count - 1)));
    frame.render_stateful_widget(table, area, &mut state);
}
