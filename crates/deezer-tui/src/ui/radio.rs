use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::client::ViewState;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, _view: &ViewState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .title(" Radios / Podcasts ")
        .title_style(Theme::title());

    let content = Paragraph::new(Span::styled(
        "Radios and podcasts will appear here",
        Theme::dim(),
    ))
    .alignment(Alignment::Center)
    .block(block);

    frame.render_widget(content, area);
}
