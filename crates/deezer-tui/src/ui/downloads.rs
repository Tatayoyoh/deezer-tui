use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect) {
    let msg = Paragraph::new(Span::styled(
        "Downloads — coming soon",
        Theme::dim(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}
