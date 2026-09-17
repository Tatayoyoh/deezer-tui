use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use std::sync::LazyLock;
use std::time::Instant;

use crate::client::{ClickTarget, ViewState};
use crate::theme::Theme;
use deezer_core::player::state::PlaybackStatus;

/// Blank columns drawn between two category labels.
const CATEGORY_GAP: u16 = 2;

/// Draw the centered row of category chips shared by the tab pages, and record
/// each label as clickable.
pub fn draw_category_menu(
    frame: &mut Frame,
    view: &ViewState,
    area: Rect,
    labels: &[&str],
    current: usize,
) {
    draw_chip_menu(frame, view, area, labels, current, ClickTarget::Category);
}

/// Draw a centered row of chips, recording each one as clickable. `target` maps
/// a chip's index to the click it stands for.
pub fn draw_chip_menu(
    frame: &mut Frame,
    view: &ViewState,
    area: Rect,
    labels: &[&str],
    current: usize,
    target: fn(usize) -> ClickTarget,
) {
    let active = Style::default()
        .fg(Theme::primary())
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    let mut spans = Vec::with_capacity(labels.len() * 2);
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " ".repeat(CATEGORY_GAP as usize),
                Theme::dim(),
            ));
        }
        spans.push(Span::styled(
            *label,
            if i == current { active } else { Theme::dim() },
        ));
    }

    let line = Line::from(spans);
    let total = line.width() as u16;
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);

    // Mirror the centering ratatui applies, so the recorded rects line up with
    // what was drawn. A menu too wide for the area is truncated, not centered.
    if total > area.width {
        return;
    }
    let mut x = area.x + (area.width - total) / 2;
    for (i, label) in labels.iter().enumerate() {
        let width = Span::raw(*label).width() as u16;
        let rect = Rect {
            x,
            y: area.y,
            width,
            height: 1,
        };
        view.record_click(rect, target(i));
        x += width + CATEGORY_GAP;
    }
}

/// Clickable rects of a `Tabs` widget's titles, in render order: ratatui pads
/// every title with one space on each side and separates them by a one-cell
/// divider.
pub fn tab_rects(area: Rect, titles: &[&str]) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(titles.len());
    let mut x = area.x;
    for title in titles {
        if x >= area.right() {
            break;
        }
        // A tab clipped by the area's edge stays clickable over what shows of it.
        let width = (Span::raw(*title).width() as u16 + 2).min(area.right() - x);
        rects.push(Rect {
            x,
            y: area.y,
            width,
            height: 1,
        });
        x += width + 1; // divider
    }
    rects
}

/// Marker for the loaded-but-not-running track. Deliberately unlike the `>`
/// selection cursor — the two used to be `▶` and `>`, which read as the same
/// arrow at a glance.
const PAUSED_MARKER: &str = "⏸";

/// Frames of the playing-row marker: two audio bars pulsing on a beat. A
/// braille cell is 2 columns of 4 dots, so one character holds both bars —
/// each frame is a left height plus a right height, filled from the bottom
/// (dots 7,3,2,1 on the left, 8,6,5,4 on the right).
///
/// Heights, two beats per loop: `11 32 44 34 23 12 | 11 23 44 43 32 21`.
const PULSE: [&str; 12] = ["⣀", "⣦", "⣿", "⣾", "⣴", "⣠", "⣀", "⣴", "⣿", "⣷", "⣦", "⣄"];

/// 12 frames of two beats: 100 ms reads as a beat rather than a flicker.
const PULSE_INTERVAL_MS: u128 = 100;

/// Width of the status column: selection cursor + playback marker + heart.
pub const STATUS_WIDTH: u16 = 3;

/// Wall-clock origin of the animation, so every row pulses in phase regardless
/// of when it was first drawn.
static PULSE_ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Marker for the current track: pulsing while it plays, a static pause glyph
/// otherwise — a stopped animation would read as a frozen UI.
fn current_track_marker(status: PlaybackStatus) -> &'static str {
    if status == PlaybackStatus::Playing {
        let step = PULSE_ORIGIN.elapsed().as_millis() / PULSE_INTERVAL_MS;
        PULSE[(step % PULSE.len() as u128) as usize]
    } else {
        PAUSED_MARKER
    }
}

/// Status column of a track row:
/// - 1st character: `>` if selected, else ` `
/// - 2nd character: pulse (playing) / `⏸` (paused) for the current track, else ` `
/// - 3rd character: `♥` if favorite, else ` `
pub fn track_status(
    is_selected: bool,
    is_playing: bool,
    is_favorite: bool,
    status: PlaybackStatus,
) -> Line<'static> {
    let cursor = if is_selected { ">" } else { " " };
    let bullet = if is_playing {
        current_track_marker(status)
    } else {
        " "
    };
    let heart = if is_favorite { "♥" } else { " " };
    Line::from(vec![
        // Bold only: no explicit colors, so the cell inherits the row style and
        // does not paint a colored background down the left edge of the table.
        Span::styled(cursor, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            bullet,
            if is_playing {
                Style::default()
                    .fg(Theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            },
        ),
        Span::styled(
            heart,
            if is_favorite {
                Style::default()
                    .fg(Color::Rgb(255, 75, 100))
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::dim()
            },
        ),
    ])
}

/// Render a `[key] label` (or bare `key label`) hint string with the key on a
/// "chip" (brackets stripped) and the label dimmed. Keeps inline shortcut hints
/// consistent with the rest of the app.
pub fn shortcut_hint(text: &'static str) -> Line<'static> {
    // "[key] label": chip the bracket contents, dim the rest.
    if let Some(close) = text.find(']') {
        let open = text.find('[').map_or(0, |i| i + 1);
        return Line::from(vec![
            Span::styled(&text[open..close], Theme::shortcut_key()),
            Span::styled(&text[close + 1..], Theme::dim()),
        ]);
    }
    // Bare "key label": chip up to the first space.
    if let Some(sp) = text.find(' ') {
        return Line::from(vec![
            Span::styled(&text[..sp], Theme::shortcut_key()),
            Span::styled(&text[sp..], Theme::dim()),
        ]);
    }
    Line::from(Span::styled(text, Theme::shortcut_key()))
}

/// Render a string with one or more `[key]` segments as a Line: each bracketed
/// key becomes a chip (brackets stripped), surrounding text is dimmed. For
/// border titles and hints that embed several keys.
pub fn shortcut_line(text: &'static str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        if open > 0 {
            spans.push(Span::styled(&rest[..open], Theme::dim()));
        }
        spans.push(Span::styled(&rest[open + 1..close], Theme::shortcut_key()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest, Theme::dim()));
    }
    Line::from(spans)
}

/// Deezer logo in pixel art using Unicode block characters.
/// Rendered in Deezer purple.
// pub fn deezer_logo() -> Paragraph<'static> {
//     let logo = vec![
//         Line::from(Span::styled(
//             "     \u{2593}\u{2593}       \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "   \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::primary()),
//         )),
//         Line::from(Span::styled(
//             "\u{2593}\u{2593} \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593} \u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "   \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "     \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "       \u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//         Line::from(Span::styled(
//             "         \u{2593}\u{2593}\u{2593}",
//             Style::default().fg(Theme::secondary()),
//         )),
//     ];

//     Paragraph::new(logo)
// }

pub fn deezer_logo() -> Paragraph<'static> {
    let logo = vec![
        Line::from(Span::styled(
            r"  ____                                   ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" |  _ \  ___  ___ _______ _ __           ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" | | | |/ _ \/ _ \_  / _ \ '__|          ",
            Style::default().fg(Theme::primary()),
        )),
        Line::from(Span::styled(
            r" | |_| |  __/  __// /  __/ |             ",
            Style::default().fg(Theme::secondary()),
        )),
        Line::from(Span::styled(
            r" |____/ \___|\___/___\___|_|  TUI        ",
            Style::default().fg(Theme::secondary()),
        )),
    ];

    Paragraph::new(logo).alignment(Alignment::Center)
}

/// Renders the Deezer logo centered within area.
pub fn render_logo(frame: &mut Frame, area: Rect) {
    // for pixel logo
    // const LOGO_W: u16 = 21;
    // const LOGO_H: u16 = 9;
    const LOGO_W: u16 = 44;
    const LOGO_H: u16 = 7;
    let logo_area = Rect {
        x: area.x + (area.width.saturating_sub(LOGO_W)) / 2 + 2,
        y: area.y + (area.height.saturating_sub(LOGO_H)) / 2 - 1,
        width: LOGO_W.min(area.width),
        height: LOGO_H.min(area.height),
    };
    frame.render_widget(deezer_logo(), logo_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(line: &Line<'static>) -> String {
        line.spans.iter().map(|sp| sp.content.as_ref()).collect()
    }

    /// Paused swaps the pulse for a static marker.
    #[test]
    fn track_status_displays_expected_characters() {
        let paused = PlaybackStatus::Paused;
        assert_eq!(
            cells(&track_status(true, true, true, paused)),
            format!(">{PAUSED_MARKER}♥")
        );
        assert_eq!(cells(&track_status(false, false, false, paused)), "   ");
        assert_eq!(cells(&track_status(true, false, true, paused)), "> ♥");
        assert_eq!(
            cells(&track_status(false, true, false, paused)),
            format!(" {PAUSED_MARKER} ")
        );
    }

    /// A frame wider than one cell would make the column jitter mid-animation.
    #[test]
    fn every_marker_is_one_cell_wide() {
        for frame in PULSE {
            assert_eq!(frame.chars().count(), 1, "{frame:?}");
        }
        assert_eq!(PAUSED_MARKER.chars().count(), 1);
    }

    /// The cursor, marker and heart must fill exactly the declared column.
    #[test]
    fn status_column_matches_its_declared_width() {
        for status in [PlaybackStatus::Playing, PlaybackStatus::Paused] {
            for is_current in [true, false] {
                let line = track_status(true, is_current, true, status);
                assert_eq!(cells(&line).chars().count(), STATUS_WIDTH as usize);
            }
        }
    }

    /// Playing swaps the static marker for a pulse frame.
    #[test]
    fn track_status_pulses_while_playing() {
        let line = track_status(false, true, false, PlaybackStatus::Playing);
        let s = cells(&line);
        assert_eq!(s.chars().count(), STATUS_WIDTH as usize);
        let marker = s.chars().nth(1).unwrap().to_string();
        assert!(PULSE.contains(&marker.as_str()), "{s:?}");
    }

    /// Rows that are not the current track never show a marker, whatever the
    /// player is doing.
    #[test]
    fn track_status_leaves_other_rows_blank_while_playing() {
        assert_eq!(
            cells(&track_status(false, false, false, PlaybackStatus::Playing)),
            "   "
        );
    }

    /// Decode a braille cell into its two bar heights, bottom-up.
    fn bar_heights(frame: &str) -> (usize, usize) {
        let bits = frame.chars().next().unwrap() as u32 - 0x2800;
        let height = |dots: [u32; 4]| dots.iter().take_while(|d| bits & *d != 0).count();
        // Left column: dots 7,3,2,1. Right column: dots 8,6,5,4.
        (
            height([0x40, 0x04, 0x02, 0x01]),
            height([0x80, 0x20, 0x10, 0x08]),
        )
    }

    /// Each bar must be filled from the bottom without gaps, or a mistyped
    /// codepoint shows up as a dot floating mid-cell.
    #[test]
    fn pulse_frames_are_two_bottom_anchored_bars() {
        let expected = [
            (1, 1),
            (3, 2),
            (4, 4),
            (3, 4),
            (2, 3),
            (1, 2),
            (1, 1),
            (2, 3),
            (4, 4),
            (4, 3),
            (3, 2),
            (2, 1),
        ];
        for (frame, want) in PULSE.iter().zip(expected) {
            let bits = frame.chars().next().unwrap() as u32 - 0x2800;
            let (left, right) = bar_heights(frame);
            assert_eq!((left, right), want, "{frame:?}");
            // No stray dots above the filled part of either column.
            let filled = [0x40, 0x04, 0x02, 0x01][..left].iter().sum::<u32>()
                + [0x80, 0x20, 0x10, 0x08][..right].iter().sum::<u32>();
            assert_eq!(bits, filled, "{frame:?} has a floating dot");
        }
    }

    /// Neighbouring duplicates would stall the animation mid-loop.
    #[test]
    fn pulse_never_repeats_a_frame_back_to_back() {
        for pair in PULSE.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
        assert_ne!(PULSE[PULSE.len() - 1], PULSE[0], "loop seam");
    }
}
