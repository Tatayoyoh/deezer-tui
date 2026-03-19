use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect) {
    let msg = Paragraph::new(Span::styled(t().downloads_placeholder, Theme::dim()))
        .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}
