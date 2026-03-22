use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
};

use crate::client::{Overlay, PopupMenu, SubMenu, ViewState};
use crate::i18n::t;
use crate::theme::{Theme, ThemeId};

/// Draw the popup overlay if one is active.
pub fn draw(frame: &mut Frame, view: &ViewState) {
    // Draw toast notification (takes priority display but doesn't block popup)
    if let Some(ref toast) = view.toast {
        draw_toast(frame, &toast.message);
    }

    let has_modal = view.overlay.is_some() || view.popup.is_some();

    // Dim backdrop behind modals
    if has_modal {
        draw_backdrop(frame);
    }

    // Overlays take priority over track popups
    match &view.overlay {
        Some(Overlay::Help) => {
            draw_help_overlay(frame);
            return;
        }
        Some(Overlay::Settings { selected }) => {
            draw_settings_overlay(frame, *selected);
            return;
        }
        Some(Overlay::ThemePicker { selected }) => {
            draw_theme_picker(frame, *selected);
            return;
        }
        Some(Overlay::LanguagePicker { selected }) => {
            draw_language_picker(frame, *selected);
            return;
        }
        Some(Overlay::Info) => {
            draw_info_overlay(frame);
            return;
        }
        Some(Overlay::AlbumDetail) => {
            // Album detail is rendered in the main content area, not as a popup
            return;
        }
        Some(Overlay::PlaylistDetail { selected }) => {
            draw_playlist_detail(frame, view, *selected);
            // Don't return — let the popup (context menu) render on top if open
        }
        Some(Overlay::WaitingList { selected }) => {
            draw_waiting_list(frame, view, *selected);
            // Don't return — let the popup (context menu) render on top if open
        }
        None => {}
    }

    let Some(ref popup) = view.popup else {
        return;
    };

    // When a popup opens on top of an overlay, add a second backdrop
    if view.overlay.is_some() {
        draw_backdrop(frame);
    }

    match &popup.sub_menu {
        Some(SubMenu::PlaylistPicker {
            playlists,
            selected,
            loading,
        }) => {
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
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Build list items
    let items: Vec<ListItem> = popup
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if item.is_header {
                ListItem::new(Line::from(Span::styled(
                    &item.label,
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD),
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
        })
        .collect();

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
    let height = if loading {
        5
    } else {
        (playlists.len() as u16).min(20) + 4
    };
    let popup_area = centered_rect(45, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(t().add_to_playlist_fmt(track_title))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if loading {
        let loading_text = Paragraph::new(t().loading_playlists)
            .style(Theme::dim())
            .alignment(Alignment::Center);
        frame.render_widget(loading_text, inner);
        return;
    }

    if playlists.is_empty() {
        let empty_text = Paragraph::new(t().no_playlists)
            .style(Theme::dim())
            .alignment(Alignment::Center);
        frame.render_widget(empty_text, inner);
        return;
    }

    let s = t();
    let items: Vec<ListItem> = playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let prefix = if i == selected { " > " } else { "   " };
            let text = format!("{}{}", prefix, s.playlist_item(&pl.title, pl.nb_songs));
            ListItem::new(Line::from(Span::styled(
                text,
                if i == selected {
                    Theme::highlight()
                } else {
                    Theme::text()
                },
            )))
        })
        .collect();

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
        .style(Style::default().bg(Theme::surface()))
        .title(format!(" {} ", t().track_info))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let s = t();
    let track = &popup.track;
    let dur = track.duration_secs();

    let info_lines = vec![
        Line::from(vec![
            Span::styled(s.info_title, Style::default().fg(Theme::text_dim_color())),
            Span::styled(&track.title, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled(s.info_artist, Style::default().fg(Theme::text_dim_color())),
            Span::styled(&track.artist, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled(s.info_album, Style::default().fg(Theme::text_dim_color())),
            Span::styled(&track.album, Theme::text()),
        ]),
        Line::from(vec![
            Span::styled(
                s.info_duration,
                Style::default().fg(Theme::text_dim_color()),
            ),
            Span::styled(format!("{}:{:02}", dur / 60, dur % 60), Theme::text()),
        ]),
        Line::from(vec![
            Span::styled(
                s.info_track_id,
                Style::default().fg(Theme::text_dim_color()),
            ),
            Span::styled(&track.track_id, Theme::text()),
        ]),
        Line::from(""),
        Line::from(Span::styled(s.press_esc_close, Theme::dim())),
    ];

    let paragraph = Paragraph::new(info_lines);
    frame.render_widget(paragraph, inner);
}

/// Draw the help overlay showing all keyboard shortcuts.
fn draw_help_overlay(frame: &mut Frame) {
    let s = t();
    let shortcuts: Vec<(&str, &str)> = vec![
        ("Tab / Shift+Tab", s.help_switch_tabs),
        ("/ or Ctrl+F", s.help_search),
        ("Enter", s.help_play_submit),
        ("Esc", s.help_settings_back),
        ("j/k or Up/Down", s.help_navigate_list),
        ("h/l or Left/Right", s.help_navigate_categories),
        ("Space", s.help_play_pause),
        ("n", s.help_next_track),
        ("b", s.help_prev_track),
        ("s", s.help_toggle_shuffle),
        ("r", s.help_cycle_repeat),
        ("+/-", s.help_volume),
        ("a", s.help_album_detail),
        ("w", s.help_waiting_list),
        ("m", s.help_context_menu),
        ("Ctrl+P", s.help_playing_menu),
        ("g", s.help_shuffle_favorites),
        ("?", s.help_this_help),
        ("Ctrl+O", s.help_settings),
        ("i", s.help_info),
        ("q", s.help_quit),
        ("Ctrl+Z", s.help_detach),
    ];

    let area = frame.area();
    let height = shortcuts.len() as u16 + 4;
    let popup_area = centered_rect(60, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.keyboard_shortcuts)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|(key, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<22}", key),
                    Style::default()
                        .fg(Theme::primary())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Theme::text()),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the application info modal.
fn draw_info_overlay(frame: &mut Frame) {
    let s = t();

    let version = env!("CARGO_PKG_VERSION");
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    let github_url = "https://github.com/Tatayoyoh/deezer-tui";
    let license_url = "https://en.wikipedia.org/wiki/WTFPL";

    let link_style = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::UNDERLINED);
    let label_style = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = vec![
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<16}", s.about_version), label_style),
            Span::styled(version, Theme::text()),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<16}", s.about_architecture), label_style),
            Span::styled(format!("{os}/{arch}"), Theme::text()),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<16}", s.about_author), label_style),
            Span::styled("Tatayoyoh", Theme::text()),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<16}", s.about_github), label_style),
            Span::styled(github_url, link_style),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<16}", s.about_license), label_style),
            Span::styled("WTFPL", Theme::text()),
            Span::styled("  ", Theme::text()),
            Span::styled(license_url, link_style),
        ])),
    ];

    let area = frame.area();
    let height = items.len() as u16 + 4;
    let popup_area = centered_rect(60, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.about_title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the settings overlay with selectable entries.
fn draw_settings_overlay(frame: &mut Frame, selected: usize) {
    let s = t();
    let entries = [
        s.settings_shortcuts,
        s.settings_themes,
        s.settings_language,
        s.settings_logout,
    ];

    let area = frame.area();
    let height = entries.len() as u16 + 4;
    let popup_area = centered_rect(40, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.settings)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let prefix = if i == selected { " > " } else { "   " };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}", prefix, entry),
                if i == selected {
                    Theme::highlight()
                } else {
                    Theme::text()
                },
            )))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the theme picker overlay.
fn draw_theme_picker(frame: &mut Frame, selected: usize) {
    let themes = ThemeId::ALL;
    let current = Theme::current();

    let area = frame.area();
    // +1 for the header line, +1 blank line after header
    let height = themes.len() as u16 + 6;
    let popup_area = centered_rect(45, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(t().themes)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut items: Vec<ListItem> = Vec::with_capacity(themes.len() + 2);

    // Header
    items.push(ListItem::new(Line::from(Span::styled(
        t().official_deezer_themes,
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD),
    ))));
    items.push(ListItem::new(Line::from("")));

    // Theme entries
    for (i, &theme) in themes.iter().enumerate() {
        let prefix = if i == selected { " > " } else { "   " };
        let suffix = if theme == current { "  ●" } else { "" };
        let label = format!("{}{}{}", prefix, theme.label(), suffix);
        let style = if i == selected {
            Theme::highlight()
        } else if theme == current {
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Theme::text()
        };
        items.push(ListItem::new(Line::from(Span::styled(label, style))));
    }

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the language picker overlay.
fn draw_language_picker(frame: &mut Frame, selected: usize) {
    use crate::i18n::{current_locale, Locale};

    let locales = Locale::ALL;
    let current = current_locale();

    let area = frame.area();
    let height = locales.len() as u16 + 4;
    let popup_area = centered_rect(45, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(format!(" {} ", t().settings_language))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem> = locales
        .iter()
        .enumerate()
        .map(|(i, &locale)| {
            let prefix = if i == selected { " > " } else { "   " };
            let suffix = if locale == current { "  ●" } else { "" };
            let label = format!("{}{}{}", prefix, locale.label(), suffix);
            let style = if i == selected {
                Theme::highlight()
            } else if locale == current {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::text()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the playlist detail modal.
fn draw_playlist_detail(frame: &mut Frame, view: &ViewState, selected: usize) {
    let area = frame.area();

    let detail = match &view.playlist_detail {
        Some(d) => d,
        None => {
            // Loading state
            let popup_area = centered_rect(60, 5, area);
            frame.render_widget(Clear, popup_area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border_focused())
                .style(Style::default().bg(Theme::surface()))
                .title(t().playlist)
                .title_style(Theme::title());
            let msg = Paragraph::new(Span::styled(t().loading, Theme::dim()))
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(msg, popup_area);
            return;
        }
    };

    let tracks = &detail.tracks;
    let visible_count = (tracks.len() as u16).min(area.height.saturating_sub(8));
    let height = visible_count + 7; // header + title line + separator + footer + borders
    let popup_area = centered_rect(80, height, area);

    frame.render_widget(Clear, popup_area);

    let title = format!(" {} ", detail.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let s = t();
    if tracks.is_empty() {
        let empty =
            Paragraph::new(Span::styled(s.no_tracks, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    // Split inner: subtitle + list + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // subtitle
            Constraint::Min(1),    // track list
            Constraint::Length(1), // footer hints
        ])
        .split(inner);

    // Subtitle: creator + track count
    let subtitle = Line::from(vec![Span::styled(
        s.playlist_subtitle(&detail.creator, detail.nb_tracks),
        Theme::dim(),
    )]);
    frame.render_widget(
        Paragraph::new(subtitle).alignment(Alignment::Center),
        chunks[0],
    );

    // Table header
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_album, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    // Table rows
    let rows: Vec<Row> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let dur = track.duration_secs();
            let is_fav = view.favorites.iter().any(|f| f.track_id == track.track_id);
            let is_current = view
                .current_track
                .as_ref()
                .is_some_and(|ct| ct.track_id == track.track_id);

            let prefix = if is_current { "▶" } else { "" };
            let fav_marker = if is_fav { " ♥" } else { "" };

            let num_style = if is_current {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            };

            Row::new(vec![
                Cell::from(Span::styled(format!("{}{:>3}", prefix, i + 1), num_style)),
                Cell::from(Span::styled(
                    format!("{}{}", track.title, fav_marker),
                    Theme::text(),
                )),
                Cell::from(Span::styled(
                    &track.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(&track.album, Theme::dim())),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Percentage(35),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, chunks[1], &mut table_state);

    // Footer hints
    let hints = Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_play, Theme::dim()),
        Span::styled(
            "m",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_menu, Theme::dim()),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_close, Theme::dim()),
    ]);
    let footer = Paragraph::new(hints).alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

fn draw_waiting_list(frame: &mut Frame, view: &ViewState, selected: usize) {
    let s = t();
    let area = frame.area();
    let queue = &view.queue;

    let visible_count = (queue.len() as u16).min(area.height.saturating_sub(8));
    let height = visible_count + 6; // header row + footer + borders
    let popup_area = centered_rect(80, height, area);

    frame.render_widget(Clear, popup_area);

    let title = s.waiting_list_title(queue.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if queue.is_empty() {
        let empty =
            Paragraph::new(Span::styled(s.queue_empty, Theme::dim())).alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    // Split inner into table area + footer hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    // Table header
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_album, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = queue
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let dur = track.duration_secs();
            let is_current = i == view.queue_index;
            let is_fav = view.favorites.iter().any(|f| f.track_id == track.track_id);

            let prefix = if is_current { "▶" } else { "" };
            let fav_marker = if is_fav { " ♥" } else { "" };

            let num_style = if is_current {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            };

            Row::new(vec![
                Cell::from(Span::styled(format!("{}{:>3}", prefix, i + 1), num_style)),
                Cell::from(Span::styled(
                    format!("{}{}", track.title, fav_marker),
                    Theme::text(),
                )),
                Cell::from(Span::styled(
                    &track.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(&track.album, Theme::dim())),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Percentage(35),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Theme::highlight())
        .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, chunks[0], &mut table_state);

    // Footer hints
    let hints = Line::from(vec![
        Span::styled(
            "d",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_remove, Theme::dim()),
        Span::styled(
            "f",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_favorite, Theme::dim()),
        Span::styled(
            "m",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_menu, Theme::dim()),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(s.hint_close, Theme::dim()),
    ]);
    let footer = Paragraph::new(hints).alignment(Alignment::Center);
    frame.render_widget(footer, chunks[1]);
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
        .border_style(Style::default().fg(Theme::success()))
        .style(Style::default().bg(Theme::surface()));

    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    let text = Paragraph::new(message)
        .style(Theme::text())
        .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

/// Render a dimmed full-screen backdrop behind modals.
fn draw_backdrop(frame: &mut Frame) {
    let area = frame.area();
    let backdrop = Block::default().style(Style::default().bg(Theme::backdrop()));
    frame.render_widget(backdrop, area);
}

/// Create a centered rectangle with a given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_height = height.min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    Rect::new(x, y, popup_width, popup_height)
}
