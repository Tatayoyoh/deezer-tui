use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Table, TableState,
};

use crate::client::{fuzzy_match, ClickTarget, Overlay, PopupMenu, RowsKind, SubMenu, ViewState};
use crate::i18n::t;
use crate::theme::{Theme, ThemeId};
use crate::ui::common::{shortcut_hint, shortcut_line};

/// Draw the popup overlay if one is active.
pub fn draw(frame: &mut Frame, view: &mut ViewState) {
    // Draw toast notification (takes priority display but doesn't block popup)
    if let Some(ref toast) = view.toast {
        draw_toast(frame, &toast.message, toast.is_error);
    }

    // Overlays that render as background content (a sub-page: detail views,
    // waiting list) dim themselves once something is drawn on top, in the popup
    // section below. Everything else — returning modals (Settings, …) or a
    // popup over a tab — dims the background here.
    let is_bg_content_overlay = matches!(
        view.overlay,
        Some(Overlay::AlbumDetail { .. })
            | Some(Overlay::ArtistDetail)
            | Some(Overlay::GenreDetail { .. })
            | Some(Overlay::PlaylistDetail { .. })
            | Some(Overlay::ShowDetail { .. })
            | Some(Overlay::WaitingList { .. })
    );
    // Keep the cover image (if one was drawn this frame) out of the dim pass —
    // dimming sixel/kitty image cells corrupts the artwork. `cover_image_area`
    // is set only when the detail actually rendered a real image.
    let protect = view.cover_image_area;
    let has_modal = view.overlay.is_some() || view.popup.is_some();
    if has_modal && !is_bg_content_overlay {
        draw_backdrop(frame, protect);
    }

    // Clamp help scroll in-place so it doesn't exceed visible area
    if let Some(Overlay::Help { scroll }) = &view.overlay {
        let clamped = draw_help_overlay(frame, view, *scroll);
        if let Some(Overlay::Help { scroll }) = &mut view.overlay {
            *scroll = clamped;
        }
        return;
    }

    // Overlays take priority over track popups
    match &view.overlay {
        Some(Overlay::Settings { selected }) => {
            draw_settings_overlay(frame, view, *selected);
            return;
        }
        Some(Overlay::ThemePicker { selected }) => {
            draw_theme_picker(frame, view, *selected);
            return;
        }
        Some(Overlay::LanguagePicker { selected }) => {
            draw_language_picker(frame, view, *selected);
            return;
        }
        Some(Overlay::QualityPicker { selected }) => {
            draw_quality_picker(frame, view, *selected);
            return;
        }
        Some(Overlay::Info) => {
            draw_info_overlay(frame, view);
            return;
        }
        Some(Overlay::UpdateAvailable {
            version,
            download_url: _,
            selected,
        }) => {
            draw_update_available(frame, view, version, *selected);
            return;
        }
        Some(Overlay::Updating {
            version,
            progress_msg,
        }) => {
            draw_updating(frame, version, progress_msg);
            return;
        }
        Some(Overlay::AlbumDetail { .. })
        | Some(Overlay::ArtistDetail)
        | Some(Overlay::GenreDetail { .. }) => {
            // Detail views are rendered in the main content area
            // Don't return — let the popup (context menu) render on top if open
        }
        // A show's episode list uses the same modal as a playlist's tracks.
        Some(Overlay::PlaylistDetail { selected }) | Some(Overlay::ShowDetail { selected }) => {
            // Dim the tab behind this centered modal (like the waiting list).
            // With a popup on top, the popup section below dims once instead.
            if view.popup.is_none() {
                draw_backdrop(frame, protect);
            }
            let is_show = matches!(view.overlay, Some(Overlay::ShowDetail { .. }));
            draw_playlist_detail(frame, view, *selected, is_show);
            // Don't return — let the popup (context menu) render on top if open
        }
        Some(Overlay::WaitingList { selected }) => {
            // If a playlist or show detail is stacked underneath, render it as
            // background first
            let sel = *selected;
            let bg_ps = match view.overlay_stack.last() {
                Some(Overlay::PlaylistDetail { selected: ps }) => Some((*ps, false)),
                Some(Overlay::ShowDetail { selected: ps }) => Some((*ps, true)),
                _ => None,
            };
            if let Some((ps, is_show)) = bg_ps {
                draw_playlist_detail(frame, view, ps, is_show);
            }
            // Dim whatever is behind (the cover image, if any, is left untouched).
            // If a popup is also open, the popup section below applies a single
            // uniform dim over everything, so skip this one to avoid double-dim.
            if view.popup.is_none() {
                draw_backdrop(frame, protect);
            }
            draw_waiting_list(frame, view, sel);
            // Don't return — let the popup (context menu) render on top if open
        }
        Some(Overlay::OfflineDetail {
            playlist,
            index,
            selected,
        }) => {
            let (playlist, index, selected) = (*playlist, *index, *selected);
            if view.popup.is_none() {
                draw_backdrop(frame, protect);
            }
            draw_offline_detail(frame, view, playlist, index, selected);
            // Don't return — let the popup (context menu) render on top if open
        }
        Some(Overlay::Help { .. }) => unreachable!(),
        None => {}
    }

    let Some(ref popup) = view.popup else {
        return;
    };

    // A popup on top of a background-content overlay dims that overlay here
    // (the cover image, if any, is left untouched).
    if view.overlay.is_some() {
        draw_backdrop(frame, protect);
    }

    match &popup.sub_menu {
        Some(SubMenu::PlaylistPicker {
            playlists,
            selected,
            loading,
            filter_input,
            filter_typing,
        }) => {
            let picker_title = popup.track().map(|t| t.title.as_str()).unwrap_or("");
            draw_playlist_picker(
                frame,
                view,
                playlists,
                *selected,
                *loading,
                (filter_input, *filter_typing),
                picker_title,
            );
        }
        Some(SubMenu::TrackInfo) => {
            draw_track_info(frame, view, popup);
        }
        Some(SubMenu::CreatePlaylistInput { name, cursor }) => {
            draw_text_input_modal(frame, view, t().create_playlist_prompt, name, *cursor);
        }
        Some(SubMenu::RenamePlaylistInput { name, cursor }) => {
            draw_text_input_modal(frame, view, t().rename_playlist_prompt, name, *cursor);
        }
        Some(SubMenu::ConfirmDeletePlaylist { confirm_yes }) => {
            let (title, nb_songs) = match &popup.target {
                crate::client::PopupTarget::Playlist {
                    title, nb_songs, ..
                } => (title.as_str(), *nb_songs),
                _ => ("", 0),
            };
            draw_confirm_delete_playlist(frame, view, title, nb_songs, *confirm_yes);
        }
        None => {
            draw_main_menu(frame, view, popup);
        }
    }
}

fn draw_main_menu(frame: &mut Frame, view: &ViewState, popup: &PopupMenu) {
    let area = frame.area();
    let popup_area = centered_rect(40, popup.items.len() as u16 + 4, area);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Build title
    let title = if let Some(ref t) = popup.title {
        format!(" {} ", t)
    } else {
        match &popup.target {
            crate::client::PopupTarget::Track(track) => {
                format!(" {} — {} ", track.title, track.artist)
            }
            crate::client::PopupTarget::Artist { name, .. } => {
                format!(" {} ", name)
            }
            crate::client::PopupTarget::Album { title, artist, .. } => {
                format!(" {} — {} ", title, artist)
            }
            crate::client::PopupTarget::Playlist { title, .. } => {
                format!(" {} ", title)
            }
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    view.record_modal(popup_area);
    view.record_rows(inner, 0, 0, popup.items.len(), RowsKind::Modal);

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
    view: &ViewState,
    playlists: &[deezer_core::api::models::PlaylistData],
    selected: usize,
    loading: bool,
    filter: (&str, bool),
    track_title: &str,
) {
    let (filter_input, filter_typing) = filter;
    let area = frame.area();
    let max_height = area.height.saturating_sub(4).max(10);
    let filtered: Vec<&deezer_core::api::models::PlaylistData> = if filter_input.is_empty() {
        playlists.iter().collect()
    } else {
        let query = filter_input.to_lowercase();
        playlists
            .iter()
            .filter(|pl| fuzzy_match(&query, &pl.title.to_lowercase()))
            .collect()
    };
    // Height is based on the unfiltered count so the modal doesn't resize as the
    // user types — only the row list inside it shrinks/grows with the filter.
    let total_rows = (playlists.len() as u16) + 1; // +1 for "create" entry
    let height = if loading {
        5
    } else {
        (total_rows + 7).min(max_height) // +4 for borders/margins, +3 for filter box
    };
    let width = 70u16.min(area.width.saturating_sub(4));
    let popup_area = centered_rect_abs(width, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(t().add_to_playlist_fmt(track_title))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    view.record_modal(popup_area);

    if loading {
        let loading_text = Paragraph::new(t().loading_playlists)
            .style(Theme::dim())
            .alignment(Alignment::Center);
        frame.render_widget(loading_text, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(inner);

    draw_playlist_filter_input(frame, filter_input, filter_typing, chunks[0]);
    view.record_click(chunks[0], ClickTarget::PlaylistFilterInput);

    let s = t();
    let mut rows: Vec<Row> = Vec::with_capacity(filtered.len() + 1);
    // First row: "+ Create new playlist..." entry.
    rows.push(Row::new(vec![
        Cell::from(Span::styled(
            s.create_new_playlist,
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::raw("")),
    ]));
    rows.extend(filtered.iter().map(|pl| {
        let kind = if pl.collaborative {
            s.playlist_kind_collaborative
        } else {
            s.playlist_kind_personal
        };
        Row::new(vec![
            Cell::from(Span::styled(
                s.playlist_item(&pl.title, pl.nb_songs),
                Theme::text(),
            )),
            Cell::from(Span::styled(kind, Theme::dim())),
        ])
    }));

    let widths = [Constraint::Min(20), Constraint::Length(16)];
    let table = Table::new(rows, widths)
        .row_highlight_style(Theme::highlight())
        .highlight_symbol(" > ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, chunks[1], &mut table_state);
    view.record_rows(
        chunks[1],
        0,
        table_state.offset(),
        filtered.len() + 1,
        RowsKind::Modal,
    );

    let total = filtered.len() + 1;
    if total as u16 > chunks[1].height {
        let max_scroll = total.saturating_sub(chunks[1].height as usize);
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(selected);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Theme::primary()));
        frame.render_stateful_widget(scrollbar, chunks[1], &mut scrollbar_state);
    }
}

fn draw_playlist_filter_input(frame: &mut Frame, filter_input: &str, is_typing: bool, area: Rect) {
    let s = t();
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(shortcut_line(if is_typing {
            s.playlist_filter_typing
        } else {
            s.playlist_filter_normal
        }))
        .title_style(Theme::title());

    let input_text = if filter_input.is_empty() && !is_typing {
        Span::styled(s.playlist_filter_placeholder, Theme::dim())
    } else {
        Span::styled(filter_input, Theme::text())
    };

    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, area);

    if is_typing {
        let cursor_x = area.x + 1 + filter_input.chars().count() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

/// Render a yes/no confirmation modal for deleting a non-empty playlist.
fn draw_confirm_delete_playlist(
    frame: &mut Frame,
    view: &ViewState,
    title: &str,
    nb_songs: u64,
    confirm_yes: bool,
) {
    let s = t();
    let area = frame.area();
    let width = 60u16.min(area.width.saturating_sub(4));
    let popup_area = centered_rect_abs(width, 8, area);

    frame.render_widget(Clear, popup_area);
    view.record_modal(popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.confirm_delete_playlist_title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let prompt = s.confirm_delete_playlist_body(title, nb_songs);
    let para = Paragraph::new(prompt)
        .style(Theme::text())
        .wrap(ratatui::widgets::Wrap { trim: true })
        .alignment(Alignment::Center);
    frame.render_widget(para, chunks[0]);

    let yes_style = if confirm_yes {
        Theme::highlight()
    } else {
        Theme::dim()
    };
    let no_style = if !confirm_yes {
        Theme::highlight()
    } else {
        Theme::dim()
    };
    let yes_label = format!("  [ {} ]  ", s.yes);
    let no_label = format!("  [ {} ]  ", s.no);
    let yes_width = Span::raw(yes_label.as_str()).width() as u16;
    let no_width = Span::raw(no_label.as_str()).width() as u16;
    let buttons = Line::from(vec![
        Span::styled(yes_label, yes_style),
        Span::styled(no_label, no_style),
    ]);
    let para = Paragraph::new(buttons).alignment(Alignment::Center);
    frame.render_widget(para, chunks[1]);

    // Mirror the centering so each button is clickable where it was drawn.
    let total = yes_width + no_width;
    if total <= chunks[1].width {
        let x = chunks[1].x + (chunks[1].width - total) / 2;
        let button = |x: u16, width: u16| Rect {
            x,
            y: chunks[1].y,
            width,
            height: 1,
        };
        view.record_click(button(x, yes_width), ClickTarget::ConfirmChoice(true));
        view.record_click(
            button(x + yes_width, no_width),
            ClickTarget::ConfirmChoice(false),
        );
    }
}

/// Render a small modal with a single-line text input and a title.
fn draw_text_input_modal(
    frame: &mut Frame,
    view: &ViewState,
    title: &str,
    value: &str,
    _cursor: usize,
) {
    let area = frame.area();
    let width = 60u16.min(area.width.saturating_sub(4));
    let popup_area = centered_rect_abs(width, 5, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    view.record_modal(popup_area);

    // Rough cursor representation: append "│" at end of input.
    let display = format!(" {}│", value);
    let para = Paragraph::new(display).style(Theme::text());
    frame.render_widget(para, inner);
}

fn draw_track_info(frame: &mut Frame, view: &ViewState, popup: &PopupMenu) {
    let area = frame.area();
    let popup_area = centered_rect(50, 12, area);

    frame.render_widget(Clear, popup_area);
    view.record_modal(popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(format!(" {} ", t().track_info))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let s = t();
    let Some(track) = popup.track() else {
        return;
    };
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
        Line::from(vec![
            Span::styled("Esc", Theme::shortcut_key()),
            Span::styled(s.hint_close, Theme::dim()),
        ]),
    ];

    let paragraph = Paragraph::new(info_lines);
    frame.render_widget(paragraph, inner);
}

/// Draw the help overlay showing all keyboard shortcuts grouped by section.
fn draw_help_overlay(frame: &mut Frame, view: &ViewState, scroll: usize) -> usize {
    let s = t();
    // (Option<key>, label) — None key = section header
    let shortcuts: Vec<(Option<&str>, &str)> = vec![
        // Navigation
        (None, s.help_section_navigation),
        (Some("Tab / Shift+Tab"), s.help_switch_tabs),
        (Some("Ctrl+F or /"), s.help_search),
        (Some("Enter"), s.help_play_submit),
        (Some("Esc"), s.help_settings_back),
        (Some("j/k or Up/Down"), s.help_navigate_list),
        (Some("h/l or Left/Right"), s.help_navigate_categories),
        // Playback
        (None, s.help_section_playback),
        (Some("Space"), s.help_play_pause),
        (Some("n"), s.help_next_track),
        (Some("b"), s.help_prev_track),
        (Some("Ctrl+→"), s.help_seek_forward),
        (Some("Ctrl+←"), s.help_seek_backward),
        (Some("s"), s.help_toggle_shuffle),
        (Some("r"), s.help_cycle_repeat),
        (Some("+/-"), s.help_volume),
        // Menus
        (None, s.help_section_menus),
        (Some("a"), s.help_album_detail),
        (Some("t"), s.help_artist_detail),
        (Some("w"), s.help_waiting_list),
        (Some("x"), s.help_context_menu),
        (Some("Ctrl+Space"), s.help_playing_menu),
        (Some("?"), s.help_this_help),
        (Some("Ctrl+O"), s.help_settings),
        (Some("i"), s.help_info),
        // Actions
        (None, s.help_section_actions),
        (Some("f"), s.help_start_flow),
        (Some("g"), s.help_shuffle_favorites),
        (Some("Ctrl+Q"), s.help_quit),
        (Some("Ctrl+Z"), s.help_detach),
    ];

    let area = frame.area();
    let ideal_height = shortcuts.len() as u16 + 4;
    let popup_area = centered_rect(60, ideal_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.keyboard_shortcuts)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    view.record_modal(popup_area);

    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|(key, desc)| match key {
            Some(k) => {
                // Chip only the key text (not the alignment padding). All keys
                // use the same style here — Flow's special chip is footer-only.
                let key_style = Theme::shortcut_key();
                let pad = 22usize.saturating_sub(k.chars().count());
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(*k, key_style),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(*desc, Theme::text()),
                ]))
            }
            None => ListItem::new(Line::from(Span::styled(
                format!("  {}", desc),
                Theme::dim(),
            ))),
        })
        .collect();

    let item_count = items.len();
    let max_scroll = item_count.saturating_sub(inner.height as usize);
    let scroll = scroll.min(max_scroll);
    let mut state = ListState::default().with_offset(scroll);
    let list = List::new(items);
    frame.render_stateful_widget(list, inner, &mut state);

    // Scrollbar (only when content overflows)
    if item_count as u16 > inner.height {
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Theme::primary()));
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }

    scroll
}

/// Draw the application info modal.
fn draw_info_overlay(frame: &mut Frame, view: &ViewState) {
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
    view.record_modal(popup_area);

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
fn draw_settings_overlay(frame: &mut Frame, view: &ViewState, selected: usize) {
    let s = t();
    let entries: &[(&str, &str)] = &[
        (s.settings_shortcuts, "?"),
        (s.settings_themes, ""),
        (s.settings_quality, ""),
        (s.settings_language, ""),
        (s.settings_logout, ""),
        (s.settings_background, "Ctrl+Z"),
        (s.settings_quit, "Ctrl+Q"),
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

    view.record_modal(popup_area);
    view.record_rows(inner, 0, 0, entries.len(), RowsKind::Modal);

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, (label, shortcut))| {
            let prefix = if i == selected { " > " } else { "   " };
            let style = if i == selected {
                Theme::highlight()
            } else {
                Theme::text()
            };
            if shortcut.is_empty() {
                ListItem::new(Line::from(Span::styled(format!("{prefix}{label}"), style)))
            } else {
                let text_part = format!("{prefix}{label}");
                // Count characters, not bytes: accented labels (e.g. "arrière")
                // are multi-byte in UTF-8 but one column wide.
                let used = (text_part.chars().count() + shortcut.chars().count()) as u16;
                let pad = inner.width.saturating_sub(used) as usize;
                ListItem::new(Line::from(vec![
                    Span::styled(text_part, style),
                    Span::styled(" ".repeat(pad), style),
                    Span::styled(
                        *shortcut,
                        if i == selected {
                            style
                        } else {
                            Theme::shortcut_key()
                        },
                    ),
                ]))
            }
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the theme picker overlay.
fn draw_theme_picker(frame: &mut Frame, view: &ViewState, selected: usize) {
    let s = t();
    let themes = ThemeId::ALL;
    let current = Theme::current();
    let transparent = Theme::is_transparent();

    let area = frame.area();
    // transparency (1) + blank (1) + header (1) + blank (1) + themes + blank (1) + hints (1) + borders (2)
    let height = themes.len() as u16 + 8;
    let popup_area = centered_rect(45, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.themes)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner: slider (1) | blank (1) | theme list (fill) | blank (1) | hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // transparency slider row
            Constraint::Length(1), // blank separator
            Constraint::Min(0),    // theme list
            Constraint::Length(1), // blank before hints
            Constraint::Length(1), // shortcut hints
        ])
        .split(inner);

    // ── Transparency toggle ──────────────────────────────────────
    // "Transparency <status> <toggle>": ON/OFF (untranslated, left-padded to 3
    // so the knob stays put) then a knob switch (knob right = on, left = off).
    // Left/Right flip it. Indent matches the theme list below.
    let (status, switch, switch_style) = if transparent {
        (
            "ON",
            "[ ●]",
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        ("OFF", "[● ]", Theme::dim())
    };
    let toggle_line = Line::from(vec![
        Span::styled(format!("   {} ", s.transparency_label), Theme::text()),
        Span::styled(format!("{status:<3} "), switch_style),
        Span::styled(switch, switch_style),
    ]);
    frame.render_widget(Paragraph::new(toggle_line), chunks[0]);

    // ── Theme list ───────────────────────────────────────────────
    let mut items: Vec<ListItem> = Vec::with_capacity(themes.len() + 2);

    items.push(ListItem::new(Line::from(Span::styled(
        s.official_deezer_themes,
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD),
    ))));
    items.push(ListItem::new(Line::from("")));

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

    frame.render_widget(List::new(items), chunks[2]);
    view.record_modal(popup_area);
    view.record_rows(
        chunks[2],
        2, // section header + blank line
        0,
        themes.len(),
        RowsKind::Modal,
    );
    view.record_click(chunks[0], ClickTarget::TransparencyToggle);

    // ── Shortcut hints ───────────────────────────────────────────
    let mut hint_spans = shortcut_hint(s.theme_picker_hint_transparency).spans;
    hint_spans.push(Span::styled("  ", Theme::dim()));
    hint_spans.extend(shortcut_hint(s.theme_picker_hint_navigate).spans);
    let hint_line = Line::from(hint_spans);
    frame.render_widget(Paragraph::new(hint_line), chunks[4]);
}

/// Draw the language picker overlay.
fn draw_language_picker(frame: &mut Frame, view: &ViewState, selected: usize) {
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

    view.record_modal(popup_area);
    view.record_rows(inner, 0, 0, locales.len(), RowsKind::Modal);

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

/// Draw the audio quality picker overlay.
fn draw_quality_picker(frame: &mut Frame, view: &ViewState, selected: usize) {
    use deezer_core::api::models::AudioQuality;

    let qualities = AudioQuality::ALL;

    let area = frame.area();
    let height = qualities.len() as u16 + 4;
    let popup_area = centered_rect(50, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(format!(" {} ", t().settings_quality))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    view.record_modal(popup_area);
    view.record_rows(inner, 0, 0, qualities.len(), RowsKind::Modal);

    let items: Vec<ListItem> = qualities
        .iter()
        .enumerate()
        .map(|(i, &q)| {
            let prefix = if i == selected { " > " } else { "   " };
            let label = format!("{}{}", prefix, q.label());
            let style = if i == selected {
                Theme::highlight()
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
/// Renders the tracks of a playlist, or the episodes of a podcast show when
/// `is_show` is set — both live in `view.playlist_detail`.
fn draw_playlist_detail(frame: &mut Frame, view: &ViewState, selected: usize, is_show: bool) {
    let area = frame.area();

    let detail = match &view.playlist_detail {
        Some(d) => d,
        None => {
            // Loading state
            let popup_area = centered_rect(60, 5, area);
            frame.render_widget(Clear, popup_area);
            view.record_modal(popup_area);
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

    view.record_modal(popup_area);

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

    // Subtitle: creator + track count (episode count for a show, which has no creator)
    let subtitle_text = if is_show {
        format!("{} {}", detail.nb_tracks, s.cat_episodes)
    } else {
        s.playlist_subtitle(&detail.creator, detail.nb_tracks)
    };
    let subtitle = Line::from(vec![Span::styled(subtitle_text, Theme::dim())]);
    frame.render_widget(
        Paragraph::new(subtitle).alignment(Alignment::Center),
        chunks[0],
    );

    // Table header
    let (col_title, col_author) = if is_show {
        (s.header_episode, s.header_podcast)
    } else {
        (s.header_title, s.header_artist)
    };
    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(col_title, Theme::dim())),
        Cell::from(Span::styled(col_author, Theme::dim())),
        Cell::from(Span::styled(
            if is_show { "" } else { s.header_album },
            Theme::dim(),
        )),
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
    view.record_rows(
        chunks[1],
        1, // header
        table_state.offset(),
        tracks.len(),
        RowsKind::PlaylistDetail,
    );

    // Footer hints — a show has no context menu and no offline download
    let mut hint_spans = vec![
        Span::styled("Enter", Theme::shortcut_key()),
        Span::styled(s.hint_play, Theme::dim()),
    ];
    if !is_show {
        hint_spans.extend([
            Span::styled("x", Theme::shortcut_key()),
            Span::styled(s.hint_menu, Theme::dim()),
            Span::styled("d", Theme::shortcut_key()),
            Span::styled(s.hint_download_album, Theme::dim()),
        ]);
    }
    hint_spans.extend([
        Span::styled("Esc", Theme::shortcut_key()),
        Span::styled(s.hint_close, Theme::dim()),
    ]);
    let footer = Paragraph::new(Line::from(hint_spans)).alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

fn draw_waiting_list(frame: &mut Frame, view: &ViewState, selected: usize) {
    let s = t();
    let area = frame.area();
    let queue = &view.queue;

    let visible_count = (queue.len() as u16).min(area.height.saturating_sub(8));
    let height = if queue.is_empty() {
        8
    } else {
        visible_count + 6
    }; // min height for empty msg
    let popup_area = centered_rect(80, height, area);

    frame.render_widget(Clear, popup_area);

    view.record_modal(popup_area);

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
        let empty = Paragraph::new(vec![
            Line::from(Span::styled(
                s.queue_empty_title,
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(s.queue_empty_subtitle, Theme::dim())),
        ])
        .alignment(Alignment::Center);
        // Vertically center the 3 lines within inner
        let top_pad = inner.height.saturating_sub(3) / 2;
        let centered = Rect {
            y: inner.y + top_pad,
            height: inner.height.saturating_sub(top_pad),
            ..inner
        };
        frame.render_widget(empty, centered);
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
    view.record_rows(
        chunks[0],
        1, // header
        table_state.offset(),
        queue.len(),
        RowsKind::WaitingList,
    );

    // Footer hints
    let hints = Line::from(vec![
        Span::styled("d", Theme::shortcut_key()),
        Span::styled(s.hint_remove, Theme::dim()),
        Span::styled("f", Theme::shortcut_key()),
        Span::styled(s.hint_favorite, Theme::dim()),
        Span::styled("x", Theme::shortcut_key()),
        Span::styled(s.hint_menu, Theme::dim()),
        Span::styled("Esc", Theme::shortcut_key()),
        Span::styled(s.hint_close, Theme::dim()),
    ]);
    let footer = Paragraph::new(hints).alignment(Alignment::Center);
    frame.render_widget(footer, chunks[1]);
}

/// Draw the track list of a downloaded album or playlist, with its own filter.
fn draw_offline_detail(
    frame: &mut Frame,
    view: &ViewState,
    playlist: bool,
    index: usize,
    selected: usize,
) {
    let s = t();
    let area = frame.area();
    let tracks = view.offline_detail_tracks(playlist, index);

    let visible_count = (tracks.len() as u16).min(area.height.saturating_sub(11));
    let height = visible_count + 9; // borders + title + filter box + header + footer
    let popup_area = centered_rect(80, height, area);

    frame.render_widget(Clear, popup_area);
    view.record_modal(popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(view.offline_detail_title(playlist, index))
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Filter input
            Constraint::Min(1),    // Track table
            Constraint::Length(1), // Footer hints
        ])
        .split(inner);

    // Filter input
    let is_typing = view.offline_detail_filter_typing;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_typing {
            Theme::border_focused()
        } else {
            Theme::border()
        })
        .title(shortcut_line(if is_typing {
            s.favorites_filter_typing
        } else {
            s.favorites_filter_normal
        }))
        .title_style(Theme::title());
    let input_text = if view.offline_detail_filter_input.is_empty() && !is_typing {
        Span::styled(s.offline_filter_placeholder, Theme::dim())
    } else {
        Span::styled(&view.offline_detail_filter_input, Theme::text())
    };
    frame.render_widget(Paragraph::new(input_text).block(input_block), chunks[0]);
    view.record_click(chunks[0], crate::client::ClickTarget::FilterInput);
    if is_typing {
        let cursor_x = chunks[0].x + 1 + view.offline_detail_filter_input.len() as u16;
        frame.set_cursor_position(Position::new(cursor_x, chunks[0].y + 1));
    }

    if tracks.is_empty() {
        let empty = Paragraph::new(Span::styled(s.offline_empty, Theme::dim()))
            .alignment(Alignment::Center);
        frame.render_widget(empty, chunks[1]);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("#", Theme::dim())),
        Cell::from(Span::styled(s.header_title, Theme::dim())),
        Cell::from(Span::styled(s.header_artist, Theme::dim())),
        Cell::from(Span::styled(s.header_duration, Theme::dim())),
    ])
    .height(1);

    let rows: Vec<Row> = tracks
        .iter()
        .map(|(track_index, track)| {
            let dur = track.duration_secs();
            let is_current = view
                .current_track
                .as_ref()
                .is_some_and(|ct| ct.track_id == track.track_id);
            let num_style = if is_current {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            };
            let prefix = if is_current { "▶" } else { "" };

            Row::new(vec![
                Cell::from(Span::styled(
                    format!("{}{:>3}", prefix, track_index + 1),
                    num_style,
                )),
                Cell::from(Span::styled(&track.title, Theme::text())),
                Cell::from(Span::styled(
                    &track.artist,
                    Style::default().fg(Theme::primary()),
                )),
                Cell::from(Span::styled(
                    format!("{}:{:02}", dur / 60, dur % 60),
                    Theme::dim(),
                )),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(45),
            Constraint::Percentage(35),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .row_highlight_style(Theme::highlight())
    .highlight_symbol("> ");

    let mut table_state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, chunks[1], &mut table_state);
    view.record_rows(
        chunks[1],
        1, // header
        table_state.offset(),
        tracks.len(),
        RowsKind::OfflineDetail,
    );

    let hints = Line::from(vec![
        Span::styled("Enter", Theme::shortcut_key()),
        Span::styled(s.hint_play, Theme::dim()),
        Span::styled("/", Theme::shortcut_key()),
        Span::styled(s.hint_filter, Theme::dim()),
        Span::styled("x", Theme::shortcut_key()),
        Span::styled(s.hint_menu, Theme::dim()),
        Span::styled("Esc", Theme::shortcut_key()),
        Span::styled(s.hint_close, Theme::dim()),
    ]);
    frame.render_widget(
        Paragraph::new(hints).alignment(Alignment::Center),
        chunks[2],
    );
}

/// Draw a temporary toast notification at the bottom center of the screen.
fn draw_toast(frame: &mut Frame, message: &str, is_error: bool) {
    let area = frame.area();
    let width = (message.len() as u16 + 6).min(area.width.saturating_sub(4));
    let height = 3;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + area.height.saturating_sub(height + 5); // Above the player bar

    let toast_area = Rect::new(x, y, width, height);
    frame.render_widget(Clear, toast_area);

    let border_color = if is_error {
        Color::Red
    } else {
        Theme::success()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Theme::surface()));

    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    let text = Paragraph::new(message)
        .style(Theme::text())
        .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

/// Dim the background behind modals, btop-style: darken the colors of the cells
/// already drawn in the content area (excluding the bottom player bar) so the
/// background stays visible but grayed, instead of covering it with a layer.
/// With a transparent background the solid cells (text) dim while the terminal
/// still shows through the `Reset` cells. `protect` (e.g. a cover image) is
/// left untouched.
fn draw_backdrop(frame: &mut Frame, protect: Option<Rect>) {
    /// Percent of each RGB channel kept when dimming (lower = darker).
    const KEEP: u16 = 40;
    fn dim(c: Color) -> Color {
        match c {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as u16 * KEEP / 100) as u8,
                (g as u16 * KEEP / 100) as u8,
                (b as u16 * KEEP / 100) as u8,
            ),
            // Reset / indexed / named colors can't be scaled; leave them as-is.
            other => other,
        }
    }
    let full = frame.area();
    let y_end = full.bottom().saturating_sub(super::PLAYER_BAR_HEIGHT);
    let buf = frame.buffer_mut();
    for y in full.top()..y_end {
        for x in full.left()..full.right() {
            // Leave the cover image untouched — dimming its cells corrupts it.
            if protect.is_some_and(|p| p.contains(Position::new(x, y))) {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.fg = dim(cell.fg);
                cell.bg = dim(cell.bg);
            }
        }
    }
}

/// Area available to modals: the screen minus the bottom player bar. Keeping
/// modals out of the player-bar rows keeps that bar visible below them, and
/// stops the dim pass (which excludes those rows) from leaving a modal's bottom
/// rows undimmed.
fn modal_area(area: Rect) -> Rect {
    Rect {
        height: area.height.saturating_sub(super::PLAYER_BAR_HEIGHT),
        ..area
    }
}

/// Create a centered rectangle with a given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let area = modal_area(area);
    let popup_width = area.width * percent_x / 100;
    let popup_height = height.min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    Rect::new(x, y, popup_width, popup_height)
}

/// Create a centered rectangle with absolute width and height.
fn centered_rect_abs(width: u16, height: u16, area: Rect) -> Rect {
    let area = modal_area(area);
    let popup_width = width.min(area.width);
    let popup_height = height.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    Rect::new(x, y, popup_width, popup_height)
}

/// Draw the "Update Available" overlay with version info and 3 options.
fn draw_update_available(frame: &mut Frame, view: &ViewState, version: &str, selected: usize) {
    let s = t();
    let area = frame.area();
    let popup_area = centered_rect(50, 10, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(s.update_available_title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let current = env!("CARGO_PKG_VERSION");
    let options = [s.update_now, s.update_later, s.update_never];

    view.record_modal(popup_area);
    view.record_rows(
        inner,
        3, // two version lines + separator
        0,
        options.len(),
        RowsKind::Modal,
    );

    let mut lines: Vec<Line> = Vec::new();

    // Version info
    lines.push(Line::from(Span::styled(
        s.fmt_update_new_version(version),
        Style::default()
            .fg(Theme::success())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        s.fmt_update_current_version(current),
        Theme::dim(),
    )));
    lines.push(Line::from("")); // separator

    // Options
    for (i, &option) in options.iter().enumerate() {
        let prefix = if i == selected { " > " } else { "   " };
        let style = if i == selected {
            Theme::highlight()
        } else {
            Theme::text()
        };
        lines.push(Line::from(Span::styled(format!("{prefix}{option}"), style)));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Draw the "Updating..." progress overlay.
fn draw_updating(frame: &mut Frame, version: &str, progress_msg: &str) {
    let s = t();
    let area = frame.area();
    let popup_area = centered_rect(50, 5, area);

    frame.render_widget(Clear, popup_area);

    let title = format!(" {} v{} ", s.update_available_title.trim(), version);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border_focused())
        .style(Style::default().bg(Theme::surface()))
        .title(title)
        .title_style(Theme::title());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let text = Paragraph::new(progress_msg)
        .style(Style::default().fg(Theme::primary()))
        .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}
