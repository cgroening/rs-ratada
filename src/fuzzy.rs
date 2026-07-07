//! Fuzzy matching and match highlighting, built on `nucleo-matcher`.

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use super::style;
use crate::theme::Palette;

/// Runs `action` with a matcher and the parsed `query`/`haystack`, the shared
/// setup behind [`score`] and [`match_indices`].
fn with_pattern<T>(
    haystack: &str,
    query: &str,
    action: impl FnOnce(&Pattern, Utf32Str, &mut Matcher) -> T,
) -> T {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern =
        Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();
    let utf32 = Utf32Str::new(haystack, &mut buffer);
    action(&pattern, utf32, &mut matcher)
}

/// Returns the fuzzy match score of `query` against `haystack`, or `None` when
/// it does not match. An empty query matches everything with score `0`.
pub fn score(haystack: &str, query: &str) -> Option<u32> {
    if query.trim().is_empty() {
        return Some(0);
    }
    with_pattern(haystack, query, |pattern, utf32, matcher| {
        pattern.score(utf32, matcher)
    })
}

/// Returns the char positions in `haystack` that `query` matches.
pub fn match_indices(haystack: &str, query: &str) -> Vec<u32> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    with_pattern(haystack, query, |pattern, utf32, matcher| {
        let mut indices = Vec::new();
        pattern.indices(utf32, matcher, &mut indices);
        indices.sort_unstable();
        indices.dedup();
        indices
    })
}

/// Renders `text` as spans with the chars matched by `query` accented and bold
/// and the rest in `base`. Falls back to a single `base` span when nothing
/// matches.
pub fn highlight(
    text: &str,
    query: &str,
    base: Style,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let hits = match_indices(text, query);
    if hits.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let accent = style::fg(palette.accent).add_modifier(Modifier::BOLD);
    text.chars()
        .enumerate()
        .map(|(index, ch)| {
            let span_style = if hits.binary_search(&(index as u32)).is_ok() {
                accent
            } else {
                base
            };
            Span::styled(ch.to_string(), span_style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    #[test]
    fn empty_query_matches_with_zero_score() {
        assert_eq!(score("anything", ""), Some(0));
    }

    #[test]
    fn non_matching_query_scores_none() {
        assert_eq!(score("readme", "xyz"), None);
    }

    #[test]
    fn matching_query_scores_some() {
        assert!(score("readme.md", "rm").is_some());
    }

    #[test]
    fn match_indices_are_sorted_and_unique() {
        let indices = match_indices("readme", "rm");
        assert!(indices.windows(2).all(|w| w[0] < w[1]));
        assert!(!indices.is_empty());
    }

    #[test]
    fn highlight_splits_into_one_span_per_char_on_a_match() {
        let palette = ThemeRegistry::builtin().resolve("default");
        let palette = Palette::resolve(palette, &ColorOverrides::default());
        let spans = highlight("readme", "rm", Style::default(), &palette);
        // A match accents individual chars, so every char is its own span.
        assert_eq!(spans.len(), "readme".chars().count());
        let joined: String =
            spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(joined, "readme");
    }

    #[test]
    fn highlight_without_a_match_is_a_single_span() {
        let palette = ThemeRegistry::builtin().resolve("default");
        let palette = Palette::resolve(palette, &ColorOverrides::default());
        let spans = highlight("readme", "xyz", Style::default(), &palette);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "readme");
    }
}
