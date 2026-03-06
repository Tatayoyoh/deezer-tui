use std::cell::Cell;

use ratatui::style::{Color, Modifier, Style};

/// Available themes inspired by Deezer's official app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Crimson,
    Emerald,
    Amber,
    Magenta,
    Halloween,
    DarkPurple,
    DarkOrange,
    DarkPink,
    DarkRed,
    DarkYellow,
    DarkBlue,
}

impl ThemeId {
    pub const ALL: &[ThemeId] = &[
        ThemeId::Crimson,
        ThemeId::Emerald,
        ThemeId::Amber,
        ThemeId::Magenta,
        ThemeId::Halloween,
        ThemeId::DarkPurple,
        ThemeId::DarkOrange,
        ThemeId::DarkPink,
        ThemeId::DarkRed,
        ThemeId::DarkYellow,
        ThemeId::DarkBlue,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Crimson => "Crimson",
            ThemeId::Emerald => "Emerald",
            ThemeId::Amber => "Amber",
            ThemeId::Magenta => "Magenta",
            ThemeId::Halloween => "Halloween",
            ThemeId::DarkPurple => "Dark Purple",
            ThemeId::DarkOrange => "Dark Orange",
            ThemeId::DarkPink => "Dark Pink",
            ThemeId::DarkRed => "Dark Red",
            ThemeId::DarkYellow => "Dark Yellow",
            ThemeId::DarkBlue => "Dark Blue",
        }
    }

    /// Serialization key for config persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeId::Crimson => "crimson",
            ThemeId::Emerald => "emerald",
            ThemeId::Amber => "amber",
            ThemeId::Magenta => "magenta",
            ThemeId::Halloween => "halloween",
            ThemeId::DarkPurple => "dark_purple",
            ThemeId::DarkOrange => "dark_orange",
            ThemeId::DarkPink => "dark_pink",
            ThemeId::DarkRed => "dark_red",
            ThemeId::DarkYellow => "dark_yellow",
            ThemeId::DarkBlue => "dark_blue",
        }
    }

    /// Parse from config string. Returns None for unknown values.
    pub fn from_str(s: &str) -> Option<ThemeId> {
        match s {
            "crimson" => Some(ThemeId::Crimson),
            "emerald" => Some(ThemeId::Emerald),
            "amber" => Some(ThemeId::Amber),
            "magenta" => Some(ThemeId::Magenta),
            "halloween" => Some(ThemeId::Halloween),
            "dark_purple" => Some(ThemeId::DarkPurple),
            "dark_orange" => Some(ThemeId::DarkOrange),
            "dark_pink" => Some(ThemeId::DarkPink),
            "dark_red" => Some(ThemeId::DarkRed),
            "dark_yellow" => Some(ThemeId::DarkYellow),
            "dark_blue" => Some(ThemeId::DarkBlue),
            _ => None,
        }
    }
}

thread_local! {
    static CURRENT_THEME: Cell<ThemeId> = const { Cell::new(ThemeId::DarkPurple) };
}

/// Deezer-inspired color palette with dynamic theme support.
pub struct Theme;

impl Theme {
    /// Set the active theme.
    pub fn set(id: ThemeId) {
        CURRENT_THEME.with(|c| c.set(id));
    }

    /// Get the active theme id.
    pub fn current() -> ThemeId {
        CURRENT_THEME.with(|c| c.get())
    }

    // ── Color accessors ──────────────────────────────────────────

    pub fn primary() -> Color {
        match Self::current() {
            ThemeId::Crimson    => Color::Rgb(220, 40, 60),
            ThemeId::Emerald    => Color::Rgb(46, 204, 113),
            ThemeId::Amber      => Color::Rgb(240, 165, 0),
            ThemeId::Magenta    => Color::Rgb(255, 0, 200),
            // Halloween: orange-amber primary on purple background
            ThemeId::Halloween  => Color::Rgb(255, 140, 0),
            ThemeId::DarkPurple => Color::Rgb(162, 0, 255),
            ThemeId::DarkOrange => Color::Rgb(255, 140, 0),
            ThemeId::DarkPink   => Color::Rgb(255, 20, 147),
            ThemeId::DarkRed    => Color::Rgb(220, 20, 60),
            ThemeId::DarkYellow => Color::Rgb(240, 200, 0),
            ThemeId::DarkBlue   => Color::Rgb(60, 120, 255),
        }
    }

    pub fn secondary() -> Color {
        match Self::current() {
            ThemeId::Crimson    => Color::Rgb(255, 107, 107),
            ThemeId::Emerald    => Color::Rgb(0, 210, 255),
            ThemeId::Amber      => Color::Rgb(255, 209, 102),
            ThemeId::Magenta    => Color::Rgb(255, 105, 180),
            // Halloween: purple secondary to complement the orange
            ThemeId::Halloween  => Color::Rgb(162, 0, 255),
            ThemeId::DarkPurple => Color::Rgb(239, 84, 105),
            ThemeId::DarkOrange => Color::Rgb(255, 165, 0),
            ThemeId::DarkPink   => Color::Rgb(255, 105, 180),
            ThemeId::DarkRed    => Color::Rgb(255, 68, 68),
            ThemeId::DarkYellow => Color::Rgb(255, 230, 100),
            ThemeId::DarkBlue   => Color::Rgb(100, 160, 255),
        }
    }

    pub fn success() -> Color {
        Color::Rgb(0, 204, 0)
    }

    pub fn bg() -> Color {
        match Self::current() {
            ThemeId::Crimson    => Color::Rgb(46, 10, 10),
            ThemeId::Emerald    => Color::Rgb(10, 36, 24),
            ThemeId::Amber      => Color::Rgb(36, 22, 8),
            ThemeId::Magenta    => Color::Rgb(36, 10, 30),
            // Halloween: deep dark purple background
            ThemeId::Halloween  => Color::Rgb(30, 10, 50),
            ThemeId::DarkPurple => Color::Rgb(18, 18, 18),
            ThemeId::DarkOrange => Color::Rgb(18, 18, 18),
            ThemeId::DarkPink   => Color::Rgb(18, 18, 18),
            ThemeId::DarkRed    => Color::Rgb(18, 18, 18),
            ThemeId::DarkYellow => Color::Rgb(18, 18, 18),
            ThemeId::DarkBlue   => Color::Rgb(14, 16, 22),
        }
    }

    pub fn surface() -> Color {
        match Self::current() {
            ThemeId::Crimson    => Color::Rgb(58, 20, 20),
            ThemeId::Emerald    => Color::Rgb(20, 48, 34),
            ThemeId::Amber      => Color::Rgb(50, 34, 16),
            ThemeId::Magenta    => Color::Rgb(50, 20, 42),
            // Halloween: slightly lighter purple surface
            ThemeId::Halloween  => Color::Rgb(42, 18, 65),
            ThemeId::DarkPurple => Color::Rgb(30, 30, 30),
            ThemeId::DarkOrange => Color::Rgb(30, 30, 30),
            ThemeId::DarkPink   => Color::Rgb(30, 30, 30),
            ThemeId::DarkRed    => Color::Rgb(30, 30, 30),
            ThemeId::DarkYellow => Color::Rgb(30, 30, 30),
            ThemeId::DarkBlue   => Color::Rgb(22, 26, 36),
        }
    }

    pub fn border_color() -> Color {
        match Self::current() {
            ThemeId::Crimson    => Color::Rgb(90, 58, 58),
            ThemeId::Emerald    => Color::Rgb(58, 90, 74),
            ThemeId::Amber      => Color::Rgb(90, 74, 58),
            ThemeId::Magenta    => Color::Rgb(90, 58, 80),
            ThemeId::Halloween  => Color::Rgb(80, 50, 90),
            ThemeId::DarkPurple
            | ThemeId::DarkOrange
            | ThemeId::DarkPink
            | ThemeId::DarkRed
            | ThemeId::DarkYellow => Color::Rgb(60, 60, 60),
            ThemeId::DarkBlue   => Color::Rgb(50, 55, 75),
        }
    }

    pub fn border_focused_color() -> Color {
        Self::primary()
    }

    pub fn text_color() -> Color {
        Color::Rgb(230, 230, 230)
    }

    pub fn text_dim_color() -> Color {
        Color::Rgb(140, 140, 140)
    }

    pub fn progress_fill() -> Color {
        Self::primary()
    }

    pub fn progress_bg() -> Color {
        Color::Rgb(50, 50, 50)
    }

    pub fn tab_active_color() -> Color {
        Self::primary()
    }

    pub fn tab_inactive_color() -> Color {
        Self::text_dim_color()
    }

    // ── Style helpers (unchanged API) ────────────────────────────

    pub fn title() -> Style {
        Style::default().fg(Self::text_color()).add_modifier(Modifier::BOLD)
    }

    pub fn text() -> Style {
        Style::default().fg(Self::text_color())
    }

    pub fn dim() -> Style {
        Style::default().fg(Self::text_dim_color())
    }

    pub fn highlight() -> Style {
        Style::default()
            .fg(Self::text_color())
            .bg(Self::primary())
            .add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Self::border_color())
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::border_focused_color())
    }

    pub fn tab_active() -> Style {
        Style::default()
            .fg(Self::tab_active_color())
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::default().fg(Self::tab_inactive_color())
    }
}
