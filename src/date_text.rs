//! The typed date forms the calendar pickers accept.
//!
//! A day is `YYYY-MM-DD`, `DD.MM.YYYY`, or `DD.MM.` / `DD.MM` without a year;
//! a month is `YYYY-MM`, `MM/YYYY` or `MM.YYYY`; a range is two days joined by
//! `..`. Day and month take one or two digits. A year takes four digits,
//! read literally, or two, read as `20YY` - in every form, ISO included. Any
//! other digit count, a sign and any whitespace make the text unparseable, so
//! a typo never lands silently in the year 26 or 202.
//!
//! The text is parsed by hand rather than through chrono's `%Y`, which takes a
//! one- to three-digit year literally. Each picker keeps its own prefill and
//! header; this module only reads what was typed.

use chrono::{Datelike, NaiveDate};

/// The separator between the two ends of a typed range.
pub(crate) const RANGE_SEPARATOR: &str = "..";

const ISO_SEPARATOR: char = '-';
const GERMAN_SEPARATOR: char = '.';
const SLASH_SEPARATOR: char = '/';
/// The century a two-digit year belongs to.
const CENTURY: i32 = 2000;
const SHORT_YEAR_DIGITS: usize = 2;
const FULL_YEAR_DIGITS: usize = 4;
/// The most digits a day or a month takes.
const MAX_COMPONENT_DIGITS: usize = 2;

/// Parses a typed day. A day typed without a year falls in `default_year` -
/// the calendar year the field opened over, so it does not drift with the
/// live preview.
pub(crate) fn parse_date(text: &str, default_year: i32) -> Option<NaiveDate> {
    parse_date_text(text)?.resolve(default_year)
}

/// Parses a typed month into `(year, month)`, the month 1-12.
pub(crate) fn parse_month(text: &str) -> Option<(i32, u32)> {
    let (year, month) = split_month(text)?;
    let year = parse_year(year)?;
    let month = parse_component(month)?;
    (1..=12).contains(&month).then_some((year, month))
}

/// Parses a typed range into its two days in typed order - ordering is the
/// caller's.
///
/// An end without a year takes the other end's year, or `default_year` when
/// neither has one, and then moves one year if that put it on the wrong side:
/// `20.12..5.1` runs into the next year, `20.12..5.1.2031` starts in 2030.
pub(crate) fn parse_range(
    text: &str,
    default_year: i32,
) -> Option<(NaiveDate, NaiveDate)> {
    let (begin, end) = split_range(text)?;
    let (begin, end) = (parse_date_text(begin)?, parse_date_text(end)?);
    match (begin.year, end.year) {
        (_, None) => {
            let begin = begin.resolve(default_year)?;
            Some((begin, end.on_or_after(begin)?))
        }
        (None, Some(_)) => {
            let end = end.resolve(default_year)?;
            Some((begin.on_or_before(end)?, end))
        }
        (Some(_), Some(_)) => {
            Some((begin.resolve(default_year)?, end.resolve(default_year)?))
        }
    }
}

/// A typed day whose year may be missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateText {
    day: u32,
    month: u32,
    year: Option<i32>,
}

impl DateText {
    /// The day in its own year, or in `default_year` when it has none.
    fn resolve(self, default_year: i32) -> Option<NaiveDate> {
        self.in_year(self.year.unwrap_or(default_year))
    }

    /// The day in `anchor`'s year, or the year after if that lies before it.
    fn on_or_after(self, anchor: NaiveDate) -> Option<NaiveDate> {
        let same_year = self.in_year(anchor.year())?;
        if same_year >= anchor {
            return Some(same_year);
        }
        self.in_year(anchor.year().checked_add(1)?)
    }

    /// The day in `anchor`'s year, or the year before if that lies after it.
    fn on_or_before(self, anchor: NaiveDate) -> Option<NaiveDate> {
        let same_year = self.in_year(anchor.year())?;
        if same_year <= anchor {
            return Some(same_year);
        }
        self.in_year(anchor.year().checked_sub(1)?)
    }

    fn in_year(self, year: i32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(year, self.month, self.day)
    }
}

/// Splits a range at its separator. A German day may end in a dot that runs
/// into the separator (`1.2...5.2.`); that dot belongs to the first day, so
/// the separator is the last two dots of the run.
fn split_range(text: &str) -> Option<(&str, &str)> {
    let mut at = text.find(RANGE_SEPARATOR)?;
    let after = &text[at + RANGE_SEPARATOR.len()..];
    if after.starts_with(GERMAN_SEPARATOR) {
        at += GERMAN_SEPARATOR.len_utf8();
    }
    Some((&text[..at], &text[at + RANGE_SEPARATOR.len()..]))
}

/// Splits a month into `(year, month)`: ISO leads with the year, the German
/// and slash forms with the month.
fn split_month(text: &str) -> Option<(&str, &str)> {
    if let Some(iso) = text.split_once(ISO_SEPARATOR) {
        return Some(iso);
    }
    let (month, year) = text.split_once([GERMAN_SEPARATOR, SLASH_SEPARATOR])?;
    Some((year, month))
}

/// Reads one day in either notation; the separator decides which.
fn parse_date_text(text: &str) -> Option<DateText> {
    if text.contains(ISO_SEPARATOR) {
        parse_iso(text)
    } else {
        parse_german(text)
    }
}

/// `YYYY-MM-DD` or `YY-MM-DD`.
fn parse_iso(text: &str) -> Option<DateText> {
    let parts: Vec<&str> = text.split(ISO_SEPARATOR).collect();
    let [year, month, day] = parts.as_slice() else {
        return None;
    };
    Some(DateText {
        day: parse_component(day)?,
        month: parse_component(month)?,
        year: Some(parse_year(year)?),
    })
}

/// `DD.MM.YYYY`, `DD.MM.YY`, or `DD.MM.` / `DD.MM` without a year.
fn parse_german(text: &str) -> Option<DateText> {
    let parts: Vec<&str> = text.split(GERMAN_SEPARATOR).collect();
    let (day, month, year) = match parts.as_slice() {
        [day, month] | [day, month, ""] => (day, month, None),
        [day, month, year] => (day, month, Some(parse_year(year)?)),
        _ => return None,
    };
    Some(DateText {
        day: parse_component(day)?,
        month: parse_component(month)?,
        year,
    })
}

/// A day or month: one or two ASCII digits.
fn parse_component(text: &str) -> Option<u32> {
    if text.len() > MAX_COMPONENT_DIGITS || !is_ascii_digits(text) {
        return None;
    }
    text.parse().ok()
}

/// A four-digit year as typed, or a two-digit one in [`CENTURY`].
fn parse_year(text: &str) -> Option<i32> {
    if !is_ascii_digits(text) {
        return None;
    }
    let value: i32 = text.parse().ok()?;
    match text.len() {
        SHORT_YEAR_DIGITS => Some(CENTURY + value),
        FULL_YEAR_DIGITS => Some(value),
        _ => None,
    }
}

/// Whether `text` is non-empty and only ASCII digits. Checked before `parse`,
/// which would also take a leading `+`.
fn is_ascii_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const YEAR: i32 = 2030;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
    }

    #[test]
    fn iso_and_german_days_parse_alike() {
        for text in ["2026-02-01", "2026-2-1", "01.02.2026", "1.2.2026"] {
            assert_eq!(parse_date(text, YEAR), Some(day(2026, 2, 1)), "{text}");
        }
    }

    #[test]
    fn a_two_digit_year_is_read_as_20yy() {
        assert_eq!(parse_date("1.2.26", YEAR), Some(day(2026, 2, 1)));
        assert_eq!(parse_date("10-11-12", YEAR), Some(day(2010, 11, 12)));
        assert_eq!(parse_month("2/26"), Some((2026, 2)));
        assert_eq!(parse_month("26-02"), Some((2026, 2)));
    }

    #[test]
    fn a_four_digit_year_is_taken_literally() {
        assert_eq!(parse_date("1.2.0026", YEAR), Some(day(26, 2, 1)));
        assert_eq!(parse_date("0026-02-01", YEAR), Some(day(26, 2, 1)));
    }

    #[test]
    fn a_one_or_three_digit_year_is_rejected() {
        for text in ["1.2.6", "1.2.026", "6-02-01", "026-02-01"] {
            assert_eq!(parse_date(text, YEAR), None, "{text}");
        }
        assert_eq!(parse_month("02/026"), None);
    }

    #[test]
    fn a_day_without_a_year_falls_in_the_default_year() {
        for text in ["1.2.", "1.2", "01.02."] {
            assert_eq!(parse_date(text, YEAR), Some(day(YEAR, 2, 1)), "{text}");
        }
    }

    #[test]
    fn a_leap_day_without_a_year_needs_a_leap_year() {
        assert_eq!(parse_date("29.2.", 2028), Some(day(2028, 2, 29)));
        assert_eq!(parse_date("29.2.", 2027), None);
    }

    #[test]
    fn a_day_that_does_not_exist_is_rejected() {
        for text in ["31.2.2026", "2026-13-01", "0.1.2026", "1.0.2026"] {
            assert_eq!(parse_date(text, YEAR), None, "{text}");
        }
    }

    #[test]
    fn signs_whitespace_and_stray_separators_are_rejected() {
        for text in [
            "",
            "+1.2.2026",
            "1.+2.2026",
            " 1.2.2026",
            "1.2.2026 ",
            "1. 2.2026",
            "1.2.2026.",
            "1.2.20.26",
            "2026-02-01-",
            "2026-02",
            "1/2/2026",
            "001.2.2026",
        ] {
            assert_eq!(parse_date(text, YEAR), None, "{text:?}");
        }
    }

    #[test]
    fn every_month_form_parses() {
        for text in ["2026-02", "2026-2", "02/2026", "2/2026", "02.2026"] {
            assert_eq!(parse_month(text), Some((2026, 2)), "{text}");
        }
        assert_eq!(parse_month("2.26"), Some((2026, 2)));
    }

    #[test]
    fn a_month_outside_1_to_12_or_malformed_is_rejected() {
        for text in ["2026-13", "2026-0", "13/2026", "02", "02/", " 02/2026"] {
            assert_eq!(parse_month(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_range_mixes_notations() {
        assert_eq!(
            parse_range("1.2.26..2026-02-05", YEAR),
            Some((day(2026, 2, 1), day(2026, 2, 5))),
        );
    }

    #[test]
    fn a_trailing_dot_before_the_separator_belongs_to_the_first_day() {
        assert_eq!(
            parse_range("1.2...5.2.", YEAR),
            Some((day(YEAR, 2, 1), day(YEAR, 2, 5))),
        );
    }

    #[test]
    fn a_yearless_end_borrows_the_other_ends_year() {
        assert_eq!(
            parse_range("1.2.2026..5.2", YEAR),
            Some((day(2026, 2, 1), day(2026, 2, 5))),
        );
        assert_eq!(
            parse_range("1.2..5.2.2026", YEAR),
            Some((day(2026, 2, 1), day(2026, 2, 5))),
        );
    }

    #[test]
    fn a_yearless_end_crosses_the_year_boundary() {
        assert_eq!(
            parse_range("20.12..5.1", YEAR),
            Some((day(YEAR, 12, 20), day(YEAR + 1, 1, 5))),
        );
        assert_eq!(
            parse_range("20.12.2030..5.1", YEAR),
            Some((day(2030, 12, 20), day(2031, 1, 5))),
        );
        assert_eq!(
            parse_range("20.12..5.1.2031", YEAR),
            Some((day(2030, 12, 20), day(2031, 1, 5))),
        );
    }

    /// Ordering an explicit pair is the picker's job, so it comes back as
    /// typed.
    #[test]
    fn an_explicit_reversed_pair_keeps_its_typed_order() {
        assert_eq!(
            parse_range("5.2.2026..1.2.2026", YEAR),
            Some((day(2026, 2, 5), day(2026, 2, 1))),
        );
    }

    #[test]
    fn a_malformed_range_is_rejected() {
        for text in [
            "1.2.2026",
            "1.2.2026..",
            "..1.2.2026",
            "1.2.2026 .. 5.2.2026",
            "1.2....5.2",
        ] {
            assert_eq!(parse_range(text, YEAR), None, "{text:?}");
        }
    }
}
