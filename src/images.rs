//! Images drawn in the terminal, with the feature `images`.
//!
//! [`ImageCache`] picks the best protocol the terminal speaks - Kitty,
//! iTerm2, Sixel, or half blocks as a fallback that works everywhere - and
//! draws image files into a [`Rect`], scaled to fit. Each file is decoded
//! once and kept; a file that cannot be read is remembered as such and drawn
//! as nothing.
//!
//! The terminal is queried once, after the alternate screen is entered and
//! before events are read ([`ImageCache::detect`]); tests and hosts that do
//! not want the query use [`ImageCache::half_blocks`].

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

// https://crates.io/crates/ratatui-image
use ratatui::{Frame, layout::Rect};
use ratatui_image::{
    StatefulImage, picker::Picker, protocol::StatefulProtocol,
};

/// Decoded images ready to draw, keyed by file.
pub struct ImageCache {
    picker: Picker,
    images: RefCell<HashMap<PathBuf, Option<StatefulProtocol>>>,
}

impl ImageCache {
    /// Asks the terminal which image protocol it supports, falling back to
    /// half blocks when it does not answer.
    ///
    /// Input that arrived while the terminal was asked - keys typed early,
    /// or a late answer to the query - is dropped afterwards, so it cannot
    /// show up as key presses.
    pub fn detect() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|error| {
            log::info!(
                "no terminal image protocol, using half blocks: {error}"
            );
            Picker::halfblocks()
        });
        drain_input();
        Self::with_picker(picker)
    }

    /// A cache drawing with half blocks, without asking the terminal.
    pub fn half_blocks() -> Self {
        Self::with_picker(Picker::halfblocks())
    }

    fn with_picker(picker: Picker) -> Self {
        Self {
            picker,
            images: RefCell::new(HashMap::new()),
        }
    }

    /// Draws the image file `path` into `area`, scaled to fit. Returns
    /// whether there was an image to draw (`false` for a file that cannot be
    /// read or decoded).
    pub fn render(&self, frame: &mut Frame, area: Rect, path: &Path) -> bool {
        let mut images = self.images.borrow_mut();
        let entry = images
            .entry(path.to_path_buf())
            .or_insert_with(|| self.load(path));
        let Some(protocol) = entry.as_mut() else {
            return false;
        };
        frame.render_stateful_widget(StatefulImage::default(), area, protocol);
        true
    }

    /// Decodes `path` into a drawable protocol, `None` (logged) on failure.
    fn load(&self, path: &Path) -> Option<StatefulProtocol> {
        let decoded = image::ImageReader::open(path)
            .and_then(image::ImageReader::with_guessed_format)
            .map_err(image::ImageError::IoError)
            .and_then(image::ImageReader::decode);
        match decoded {
            Ok(picture) => Some(self.picker.new_resize_protocol(picture)),
            Err(error) => {
                log::info!("cannot draw {}: {error}", path.display());
                None
            }
        }
    }
}

/// Drops every input event already waiting, without blocking.
fn drain_input() {
    while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
        if crossterm::event::read().is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn a_missing_file_draws_nothing_and_is_remembered() {
        let cache = ImageCache::half_blocks();
        let mut terminal =
            Terminal::new(TestBackend::new(10, 5)).expect("terminal");
        let missing = Path::new("/nonexistent/ratada.png");
        terminal
            .draw(|frame| {
                assert!(!cache.render(frame, frame.area(), missing));
            })
            .expect("draw");
        assert!(cache.images.borrow().contains_key(missing));
    }

    #[test]
    fn a_decodable_file_is_drawn_with_half_blocks() {
        let path = std::env::temp_dir()
            .join(format!("ratada-image-{}.png", std::process::id()));
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 30]))
            .save(&path)
            .expect("write png");
        let cache = ImageCache::half_blocks();
        let mut terminal =
            Terminal::new(TestBackend::new(10, 5)).expect("terminal");
        terminal
            .draw(|frame| assert!(cache.render(frame, frame.area(), &path)))
            .expect("draw");
        let _ = std::fs::remove_file(&path);
    }
}
