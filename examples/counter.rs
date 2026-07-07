//! A minimal `ratada` app: a counter driven through the `Screen` trait.
//!
//! Run it with `cargo run --example counter`. `Space` increments, `q` quits,
//! and the global `Ctrl+Q` chord quits from anywhere.

use crossterm::event::{KeyCode, KeyEvent};
use ratada::prelude::*;
use ratatui::{
    Frame,
    layout::Alignment,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// The whole application state: a single counter.
struct Counter {
    count: u32,
}

impl Screen for Counter {
    type Error = std::io::Error;

    fn render(&self, frame: &mut Frame) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" ratada counter ");
        let body = Paragraph::new(vec![
            Line::from(Span::raw("")),
            Line::from(format!("  count: {}", self.count)),
            Line::from(Span::raw("")),
            Line::from("  space +1   ·   q quit"),
        ])
        .alignment(Alignment::Left)
        .block(block);
        frame.render_widget(body, frame.area());
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        _tui: &mut Tui,
    ) -> std::io::Result<Flow> {
        match key.code {
            KeyCode::Char('q') => Ok(Flow::Quit),
            KeyCode::Char(' ') => {
                self.count += 1;
                Ok(Flow::Continue)
            }
            _ => Ok(Flow::Continue),
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut tui = Tui::new()?;
    run(&mut tui, &mut Counter { count: 0 })
}
