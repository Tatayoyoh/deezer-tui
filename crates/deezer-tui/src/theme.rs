use ratatui::style::{Color, Modifier, Style};

/// Deezer brand-inspired color palette.
pub struct Theme;

impl Theme {
    // Deezer brand colors
    pub const PRIMARY: Color = Color::Rgb(162, 0, 255);      // Deezer purple
    pub const SECONDARY: Color = Color::Rgb(239, 84, 105);   // Deezer pink/coral
    pub const ACCENT: Color = Color::Rgb(40, 40, 40);        // Dark background accent

    // UI colors
    pub const BG: Color = Color::Rgb(18, 18, 18);            // Near-black background
    pub const SURFACE: Color = Color::Rgb(30, 30, 30);       // Card/panel background
    pub const BORDER: Color = Color::Rgb(60, 60, 60);        // Border color
    pub const BORDER_FOCUSED: Color = Self::PRIMARY;

    // Text
    pub const TEXT: Color = Color::Rgb(230, 230, 230);       // Primary text
    pub const TEXT_DIM: Color = Color::Rgb(140, 140, 140);   // Secondary text
    pub const TEXT_ACCENT: Color = Self::PRIMARY;

    // Player
    pub const PROGRESS_FILL: Color = Self::PRIMARY;
    pub const PROGRESS_BG: Color = Color::Rgb(50, 50, 50);
    pub const VOLUME_FILL: Color = Self::SECONDARY;

    // Tabs
    pub const TAB_ACTIVE: Color = Self::PRIMARY;
    pub const TAB_INACTIVE: Color = Self::TEXT_DIM;

    // Styles
    pub fn title() -> Style {
        Style::default().fg(Self::TEXT).add_modifier(Modifier::BOLD)
    }

    pub fn text() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn dim() -> Style {
        Style::default().fg(Self::TEXT_DIM)
    }

    pub fn highlight() -> Style {
        Style::default()
            .fg(Self::TEXT)
            .bg(Self::PRIMARY)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::BORDER_FOCUSED)
    }

    pub fn tab_active() -> Style {
        Style::default()
            .fg(Self::TAB_ACTIVE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::default().fg(Self::TAB_INACTIVE)
    }
}
