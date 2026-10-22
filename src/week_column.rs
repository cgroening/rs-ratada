//! The ISO calendar-week column of the calendar pickers, and the switch a host
//! hides it with.
//!
//! The setting is public through [`crate::date_picker`]; this module also owns
//! how the column is drawn, so `date_picker` and `date_range_picker` render it
//! identically.

use std::cell::Cell;

use chrono::{Datelike, NaiveDate};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use serde::{Deserialize, Serialize};

use crate::{
    style,
    theme::{GlyphVariant, Skin},
};

/// The label above the week numbers.
const LABEL: &str = "Wk";
/// The columns the week column adds to a calendar row: the two-digit week, a
/// space and the separator, and the space that then leads the day grid.
const EXTRA_COLUMNS: u16 = 5;

/// Whether the calendar pickers show the ISO calendar week left of each row.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum WeekColumn {
    /// The week column is drawn (the default).
    #[default]
    Shown,
    /// The calendar looks exactly as it did before the column existed.
    Hidden,
}

impl WeekColumn {
    /// The columns this setting adds to a calendar picker's box.
    pub(crate) fn extra_width(self) -> u16 {
        match self {
            WeekColumn::Shown => EXTRA_COLUMNS,
            WeekColumn::Hidden => 0,
        }
    }

    /// The column to draw for one frame, or `None` while it is hidden.
    pub(crate) fn gutter(
        self,
        skin: &Skin,
        today: NaiveDate,
    ) -> Option<WeekGutter> {
        match self {
            WeekColumn::Shown => Some(WeekGutter::new(skin, today)),
            WeekColumn::Hidden => None,
        }
    }
}

thread_local! {
    /// The active setting. Per thread, like the quit policy: a TUI owns one
    /// terminal and drives it from one thread.
    static WEEK_COLUMN: Cell<WeekColumn> = const {
        Cell::new(WeekColumn::Shown)
    };
}

/// The active week-column setting.
pub fn week_column() -> WeekColumn {
    WEEK_COLUMN.with(Cell::get)
}

/// Sets whether the calendar pickers show the ISO calendar week.
///
/// It reaches every calendar picker opened afterwards, including the one a
/// [`crate::form`] date field opens. A picker that is already open keeps the
/// setting it opened with, so its size never changes under the user.
///
/// # Examples
///
/// ```
/// use ratada::date_picker::{WeekColumn, set_week_column, week_column};
///
/// assert_eq!(week_column(), WeekColumn::Shown);
/// set_week_column(WeekColumn::Hidden);
/// assert_eq!(week_column(), WeekColumn::Hidden);
/// ```
pub fn set_week_column(column: WeekColumn) {
    WEEK_COLUMN.with(|setting| setting.set(column));
}

/// The week column as one draw of a calendar picker renders it.
pub(crate) struct WeekGutter {
    separator: &'static str,
    plain: Style,
    rule: Style,
    cursor_week: Style,
    today_week: Style,
    today: NaiveDate,
}

impl WeekGutter {
    /// Resolves the column's glyph and styles from `skin`; `today` is the day
    /// whose week is marked.
    pub(crate) fn new(skin: &Skin, today: NaiveDate) -> Self {
        let palette = &skin.palette;
        Self {
            separator: separator(skin.glyphs.variant),
            plain: style::muted(palette),
            rule: style::fg(palette.border),
            cursor_week: style::fg(palette.accent).add_modifier(Modifier::BOLD),
            today_week: style::secondary(palette),
            today,
        }
    }

    /// The label and separator above the week numbers.
    pub(crate) fn header(&self) -> [Span<'static>; 2] {
        [Span::styled(LABEL, self.plain), self.rule_span()]
    }

    /// The week number of the row containing `day`, and the separator.
    ///
    /// The cursor's week is emphasised over today's, so the row the user is
    /// on always reads as the active one.
    pub(crate) fn row(
        &self,
        day: NaiveDate,
        cursor: NaiveDate,
    ) -> [Span<'static>; 2] {
        let week = day.iso_week();
        let style = if week == cursor.iso_week() {
            self.cursor_week
        } else if week == self.today.iso_week() {
            self.today_week
        } else {
            self.plain
        };
        let number = format!("{:>2}", week.week());
        [Span::styled(number, style), self.rule_span()]
    }

    /// An empty week cell, for a row without any day.
    pub(crate) fn blank(&self) -> [Span<'static>; 2] {
        [Span::raw(" ".repeat(LABEL.len())), self.rule_span()]
    }

    fn rule_span(&self) -> Span<'static> {
        Span::styled(format!(" {}", self.separator), self.rule)
    }
}

/// The vertical separator for `variant`.
fn separator(variant: GlyphVariant) -> &'static str {
    match variant {
        GlyphVariant::Unicode => "\u{2502}",
        GlyphVariant::Ascii => "|",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ColorOverrides, Glyphs, Palette, ThemeRegistry};

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
    }

    fn skin(variant: GlyphVariant) -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(variant),
        )
    }

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    /// One test owns the thread-local, so it is set and read in one place.
    #[test]
    fn week_column_defaults_to_shown_and_round_trips() {
        assert_eq!(week_column(), WeekColumn::Shown);
        set_week_column(WeekColumn::Hidden);
        assert_eq!(week_column(), WeekColumn::Hidden);
        set_week_column(WeekColumn::Shown);
        assert_eq!(week_column(), WeekColumn::Shown);
    }

    #[test]
    fn a_hidden_column_adds_no_width() {
        assert_eq!(WeekColumn::Hidden.extra_width(), 0);
        assert_eq!(WeekColumn::Shown.extra_width(), EXTRA_COLUMNS);
    }

    #[test]
    fn the_separator_follows_the_glyph_variant() {
        let today = date(2026, 6, 15);
        let unicode = WeekGutter::new(&skin(GlyphVariant::Unicode), today);
        let ascii = WeekGutter::new(&skin(GlyphVariant::Ascii), today);
        assert_eq!(text(&unicode.header()), "Wk \u{2502}");
        assert_eq!(text(&ascii.header()), "Wk |");
    }

    /// ISO weeks cross the year edge: the row of Monday 2025-12-29 is week 1,
    /// the row of Friday 2027-01-01 is week 53.
    #[test]
    fn rows_show_the_plain_iso_week_across_year_edges() {
        let gutter =
            WeekGutter::new(&skin(GlyphVariant::Ascii), date(2026, 6, 15));
        let cursor = date(2026, 6, 15);
        assert_eq!(text(&gutter.row(date(2025, 12, 29), cursor)), " 1 |");
        assert_eq!(text(&gutter.row(date(2027, 1, 1), cursor)), "53 |");
    }

    #[test]
    fn the_cursor_week_is_accent_bold_and_today_week_one_step_brighter() {
        let skin = skin(GlyphVariant::Unicode);
        let palette = &skin.palette;
        let today = date(2026, 6, 3);
        let cursor = date(2026, 6, 15);
        let gutter = WeekGutter::new(&skin, today);

        let [cursor_number, _] = gutter.row(cursor, cursor);
        assert_eq!(
            cursor_number.style,
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        );
        let [today_number, _] = gutter.row(today, cursor);
        assert_eq!(today_number.style, style::secondary(palette));
        let [other_number, _] = gutter.row(date(2026, 6, 22), cursor);
        assert_eq!(other_number.style, style::muted(palette));
    }

    /// When today and the cursor share a row, the cursor's emphasis wins.
    #[test]
    fn the_cursor_week_wins_over_today_week() {
        let skin = skin(GlyphVariant::Unicode);
        let day = date(2026, 6, 15);
        let gutter = WeekGutter::new(&skin, day);
        let [number, _] = gutter.row(day, day);
        assert_eq!(
            number.style,
            style::fg(skin.palette.accent).add_modifier(Modifier::BOLD),
        );
    }
}
