//! "Press twice within a second to confirm" detection.

use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);

/// Tracks the timing of repeated presses of one key.
#[derive(Debug, Default)]
pub struct DoublePress {
    last: Option<Instant>,
}

impl DoublePress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a press at `now`. Returns `true` when it completes a
    /// double-press (two presses within [`WINDOW`]); the state then resets so a
    /// third press starts fresh.
    pub fn register(&mut self, now: Instant) -> bool {
        let doubled = matches!(
            self.last,
            Some(previous) if now.duration_since(previous) <= WINDOW
        );
        self.last = if doubled { None } else { Some(now) };
        doubled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_press_within_window_doubles_then_resets() {
        let mut press = DoublePress::new();
        let base = Instant::now();
        assert!(!press.register(base));
        assert!(press.register(base));
        // Reset after a double: the next press is single again.
        assert!(!press.register(base));
    }

    #[test]
    fn presses_too_far_apart_do_not_double() {
        let mut press = DoublePress::new();
        let base = Instant::now();
        assert!(!press.register(base));
        assert!(!press.register(base + Duration::from_secs(2)));
    }
}
