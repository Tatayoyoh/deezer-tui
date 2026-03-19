use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::client::ViewState;
use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, _view: &ViewState, area: Rect) {
    let s = t();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .title(s.tab_radios)
        .title_style(Theme::title());

    let content = Paragraph::new(Span::styled(s.radios_placeholder, Theme::dim()))
        .alignment(Alignment::Center)
        .block(block);

    frame.render_widget(content, area);
}
