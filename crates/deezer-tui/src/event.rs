use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Application-level events.
#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

/// Poll for terminal events with a tick rate.
/// Only key-press events are forwarded (ignore Release/Repeat).
pub fn poll(tick_rate: Duration) -> Result<AppEvent> {
    if event::poll(tick_rate)? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return Ok(AppEvent::Key(key));
            }
        }
    }
    Ok(AppEvent::Tick)
}

/// Check if a key event is the quit shortcut (q or Ctrl+C).
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } | KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}
