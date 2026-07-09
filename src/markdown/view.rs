//! A scrollable Markdown view widget and a blocking viewer modal.
//!
//! [`MarkdownView`] renders a Markdown source into an area, scrolls it and lets
//! the user cycle its hyperlinks; [`viewer`] wraps it in a popup and returns the
//! link the user picked (the host opens it - the toolkit stays policy-free).

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{Link, StyleSheet};
use crate::{
    chrome,
    layout::centered_fraction,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    scroll, style,
    terminal::Tui,
    text::truncate,
    theme::Skin,
};

/// A scrollable Markdown view: renders a source wrapped to the render width,
/// keeps a scroll offset across frames and cycles the document's hyperlinks with
/// `Tab`/`BackTab`. The styling comes from an explicit [`StyleSheet`] or, by
/// default, from the skin via [`StyleSheet::from_skin`].
pub struct MarkdownView {
    source: String,
    sheet: Option<StyleSheet>,
    offset: Cell<usize>,
    total: Cell<usize>,
    viewport: Cell<usize>,
    links: Vec<Link>,
    active_link: usize,
    decor: Option<chrome::BoxDecor>,
}

impl MarkdownView {
    /// A view over `source`, scrolled to the top.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::markdown::MarkdownView;
    ///
    /// let view = MarkdownView::new("# Title\n\nSome **text**.");
    /// assert!(view.links().is_empty());
    /// ```
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let links = super::links(&source);
        Self {
            source,
            sheet: None,
            offset: Cell::new(0),
            total: Cell::new(0),
            viewport: Cell::new(1),
            links,
            active_link: 0,
            decor: None,
        }
    }

    /// Draws the view inside a rounded box with the given caption/badge.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// Renders with an explicit `sheet` instead of the skin-derived default.
    #[must_use]
    pub fn with_stylesheet(mut self, sheet: StyleSheet) -> Self {
        self.sheet = Some(sheet);
        self
    }

    /// Replaces the source, resetting the scroll position and link cursor.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
        self.links = super::links(&self.source);
        self.active_link = 0;
        self.offset.set(0);
    }

    /// The hyperlinks in the source, in document order.
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The currently highlighted link, if the source has any.
    pub fn selected_link(&self) -> Option<&Link> {
        self.links.get(self.active_link)
    }

    /// Handles a scroll or link-navigation key; returns whether it was consumed.
    ///
    /// `Up`/`k`, `Down`/`j`, `PageUp`/`PageDown`, `Home`/`End` scroll (clamped,
    /// not cyclic); `Tab`/`BackTab` cycle the active link.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let page = self.viewport.get().max(1);
        let offset = self.offset.get();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_to(offset.saturating_sub(1));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_to(offset + 1);
            }
            KeyCode::PageUp => self.scroll_to(offset.saturating_sub(page)),
            KeyCode::PageDown => self.scroll_to(offset + page),
            KeyCode::Home => self.scroll_to(0),
            KeyCode::End => self.scroll_to(usize::MAX),
            KeyCode::Tab => self.cycle_link(1),
            KeyCode::BackTab => self.cycle_link(-1),
            _ => return false,
        }
        true
    }

    /// Renders the view into `area`, scrolling so the offset stays in range, plus
    /// a scrollbar on overflow. Boxed when a decoration was set, in which case
    /// the box's bottom border carries the scroll percentage.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let inner = match &self.decor {
            Some(decor) => chrome::framed_decor(frame, area, skin, decor, ""),
            None => area,
        };
        let width = (inner.width as usize).max(1);
        let viewport = (inner.height as usize).max(1);
        let sheet = self
            .sheet
            .clone()
            .unwrap_or_else(|| StyleSheet::from_skin(skin));
        let lines = super::render_block(&self.source, width, &sheet);

        let total = lines.len();
        self.total.set(total);
        self.viewport.set(viewport);
        let offset = self.offset.get().min(total.saturating_sub(viewport));
        self.offset.set(offset);

        let visible: Vec<Line> =
            lines.into_iter().skip(offset).take(viewport).collect();
        frame.render_widget(Paragraph::new(visible), inner);
        scroll::render_scrollbar(
            frame,
            inner,
            skin,
            nav::ScrollView {
                total,
                offset,
                viewport,
            },
        );

        // The line count only exists once the source has been wrapped to the
        // render width, so an `Auto` badge is painted after the fact rather
        // than handed to `framed_decor` up front.
        if let Some(decor) = &self.decor
            && matches!(decor.badge, chrome::Badge::Auto)
        {
            chrome::render_badge(frame, area, skin, &self.percent_badge());
        }
    }

    /// The scroll position shown in a frame's bottom border, e.g. `"42%"`.
    /// Meaningful only after a render has sized the viewport.
    fn percent_badge(&self) -> String {
        let percent = nav::scroll_percent(nav::ScrollView {
            total: self.total.get(),
            offset: self.offset.get(),
            viewport: self.viewport.get(),
        });
        format!("{percent}%")
    }

    /// Clamps `offset` to the scrollable range and stores it.
    fn scroll_to(&self, offset: usize) {
        let max_offset = self.total.get().saturating_sub(self.viewport.get());
        self.offset.set(offset.min(max_offset));
    }

    /// Moves the active link by `delta` (wrapping), a no-op without links.
    fn cycle_link(&mut self, delta: isize) {
        if !self.links.is_empty() {
            self.active_link =
                nav::cycle(self.active_link, self.links.len(), delta);
        }
    }
}

/// Opens `source` in a blocking Markdown viewer over `render_bg`. `Tab` cycles
/// the document's links (shown in the footer); `Enter`/`o` returns the
/// highlighted link, `Esc` cancels, the global quit chord yields `Quit`.
///
/// # Errors
///
/// Returns an I/O error if drawing or reading terminal events fails.
pub fn viewer(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    source: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Link>> {
    let mut view = MarkdownView::new(source);
    popup(
        tui,
        &mut view,
        |area, _| centered_fraction(area, 3, 4, 40, 8),
        |frame, _| render_bg(frame),
        |frame, rect, view: &MarkdownView| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_viewer_body(frame, inner, skin, view);
            // The body has just wrapped the source, so the percentage matches
            // what is on screen.
            chrome::render_badge(frame, rect, skin, &view.percent_badge());
        },
        |view, key| match key.code {
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Enter | KeyCode::Char('o') => match view.selected_link() {
                Some(link) => PopupFlow::Done(link.clone()),
                None => PopupFlow::Continue,
            },
            _ => {
                view.handle_key(key);
                PopupFlow::Continue
            }
        },
    )
}

/// Splits `inner` into the scrolling body and, when the source has links, a
/// footer showing the active link.
fn render_viewer_body(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    view: &MarkdownView,
) {
    let footer = u16::from(!view.links().is_empty());
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(footer)])
        .split(inner);
    view.render(frame, rows[0], skin);
    if footer > 0 {
        let width = rows[1].width as usize;
        frame.render_widget(
            Paragraph::new(link_hint_line(view, skin, width)),
            rows[1],
        );
    }
}

/// A dim footer line for the active link (`› text · url`), clipped to `width`.
fn link_hint_line(
    view: &MarkdownView,
    skin: &Skin,
    width: usize,
) -> Line<'static> {
    match view.selected_link() {
        Some(link) => {
            let text = truncate(
                &format!(" \u{203a} {} \u{b7} {} ", link.text, link.url),
                width,
            );
            Line::from(Span::styled(text, style::secondary(&skin.palette)))
        }
        None => Line::from(""),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn scroll_is_clamped_to_the_content() {
        let mut view = MarkdownView::new("a\n\nb\n\nc");
        view.total.set(3);
        view.viewport.set(2);
        // Down once moves to offset 1 (max = total - viewport = 1).
        assert!(view.handle_key(press(KeyCode::Down)));
        assert_eq!(view.offset.get(), 1);
        // End clamps to the max, not past it.
        view.handle_key(press(KeyCode::End));
        assert_eq!(view.offset.get(), 1);
        view.handle_key(press(KeyCode::Home));
        assert_eq!(view.offset.get(), 0);
    }

    #[test]
    fn tab_cycles_the_links() {
        let mut view = MarkdownView::new("[one](http://a) and [two](http://b)");
        assert_eq!(view.links().len(), 2);
        assert_eq!(
            view.selected_link().map(|l| l.url.as_str()),
            Some("http://a")
        );
        view.handle_key(press(KeyCode::Tab));
        assert_eq!(
            view.selected_link().map(|l| l.url.as_str()),
            Some("http://b")
        );
        view.handle_key(press(KeyCode::Tab));
        assert_eq!(
            view.selected_link().map(|l| l.url.as_str()),
            Some("http://a")
        );
    }

    #[test]
    fn set_source_rebuilds_links_and_resets_scroll() {
        let mut view = MarkdownView::new("no links here");
        view.offset.set(5);
        view.set_source("see [x](http://x)");
        assert_eq!(view.offset.get(), 0);
        assert_eq!(view.links().len(), 1);
    }
}
