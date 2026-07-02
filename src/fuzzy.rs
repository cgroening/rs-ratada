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

/// Returns the fuzzy match score of `query` against `haystack`, or `None` when
/// it does not match. An empty query matches everything with score `0`.
pub fn score(haystack: &str, query: &str) -> Option<u32> {
    if query.trim().is_empty() {
        return Some(0);
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern =
        Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();
    let utf32 = Utf32Str::new(haystack, &mut buffer);
    pattern.score(utf32, &mut matcher)
}

/// Returns the char positions in `haystack` that `query` matches.
pub fn match_indices(haystack: &str, query: &str) -> Vec<u32> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern =
        Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();
    let utf32 = Utf32Str::new(haystack, &mut buffer);
    let mut indices = Vec::new();
    pattern.indices(utf32, &mut matcher, &mut indices);
    indices.sort_unstable();
    indices.dedup();
    indices
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
}
