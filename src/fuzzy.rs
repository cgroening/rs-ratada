//! Fuzzy matching and match highlighting, built on `nucleo-matcher`.
//!
//! A match is a *subsequence*: `"rm"` matches `readme.md`, but also
//! `Ca`**r**`go.to`**m**`l`. Matching therefore ranks rather than filters -
//! [`score`] orders candidates, it does not narrow them to one. [`Fuzzy`] is
//! the matcher to reuse when a whole corpus is scored on every keystroke;
//! [`score_indices`] hands back the score and the matched positions in one
//! pass, for a view that ranks *and* highlights.

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

/// A reusable fuzzy matcher.
///
/// The free [`score`]/[`match_indices`]/[`score_indices`] functions build the
/// matcher, parse the pattern and encode the haystack on every call. That is
/// fine for a handful of candidates; scoring a whole corpus on each keystroke,
/// it dominates the work. `Fuzzy` keeps the scratch buffers alive and caches the
/// last parsed pattern, so a loop over many haystacks with the same query pays
/// for the setup once.
///
/// # Examples
///
/// Scoring a corpus and ranking it best-first. `"rm"` matches every candidate
/// here - it is a subsequence of all three - so the score, not the match, is
/// what separates them.
///
/// ```
/// use ratada::fuzzy::Fuzzy;
///
/// let mut fuzzy = Fuzzy::new();
/// let names = ["readme.md", "Cargo.toml", "src/main.rs"];
/// let mut ranked: Vec<_> = names
///     .iter()
///     .filter_map(|name| Some((fuzzy.score(name, "rm")?, *name)))
///     .collect();
/// ranked.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
///
/// let best: Vec<_> = ranked.iter().map(|&(_, name)| name).collect();
/// assert_eq!(best, ["readme.md", "src/main.rs", "Cargo.toml"]);
/// ```
pub struct Fuzzy {
    matcher: Matcher,
    buffer: Vec<char>,
    cached: Option<(String, Pattern)>,
}

impl Fuzzy {
    /// A matcher with empty scratch buffers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buffer: Vec::new(),
            cached: None,
        }
    }

    /// The match score of `query` against `haystack`, or `None` when it does
    /// not match. An empty query matches everything with score `0`.
    pub fn score(&mut self, haystack: &str, query: &str) -> Option<u32> {
        if query.trim().is_empty() {
            return Some(0);
        }
        self.score_indices(haystack, query).map(|(score, _)| score)
    }

    /// The match score **and** the matched char positions, or `None` when
    /// `query` does not match `haystack`. An empty query never matches.
    pub fn score_indices(
        &mut self,
        haystack: &str,
        query: &str,
    ) -> Option<(u32, Vec<u32>)> {
        if query.trim().is_empty() {
            return None;
        }
        if self.cached.as_ref().is_none_or(|(seen, _)| seen != query) {
            let pattern = Pattern::parse(
                query,
                CaseMatching::Ignore,
                Normalization::Smart,
            );
            self.cached = Some((query.to_string(), pattern));
        }
        // Distinct fields, so the three borrows below do not overlap.
        let (_, pattern) = self.cached.as_ref()?;
        let utf32 = Utf32Str::new(haystack, &mut self.buffer);
        let mut indices = Vec::new();
        let score = pattern.indices(utf32, &mut self.matcher, &mut indices)?;
        indices.sort_unstable();
        indices.dedup();
        Some((score, indices))
    }
}

impl Default for Fuzzy {
    fn default() -> Self {
        Self::new()
    }
}

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
    score_indices(haystack, query)
        .map(|(_, indices)| indices)
        .unwrap_or_default()
}

/// Returns the match score **and** the matched char positions in one pass, or
/// `None` when `query` does not match `haystack`. An empty query never matches.
///
/// Callers that rank *and* highlight - a search view listing scored hits with
/// their matched characters - want both. Asking [`score`] and [`match_indices`]
/// separately builds the matcher and re-encodes `haystack` twice.
///
/// Scoring a whole corpus? Reuse a [`Fuzzy`] instead: this rebuilds the matcher
/// and re-parses the pattern on every call.
///
/// # Examples
///
/// ```
/// use ratada::fuzzy::score_indices;
///
/// let (score, indices) = score_indices("readme.md", "rm").unwrap();
/// assert!(score > 0);
/// // The 'r' of "readme" and the 'm' of ".md" - the highest-scoring match.
/// assert_eq!(indices, vec![0, 7]);
/// assert!(score_indices("readme.md", "xyz").is_none());
/// ```
#[must_use]
pub fn score_indices(haystack: &str, query: &str) -> Option<(u32, Vec<u32>)> {
    Fuzzy::new().score_indices(haystack, query)
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
    fn a_reused_matcher_agrees_with_the_one_shot_calls() {
        let mut fuzzy = Fuzzy::new();
        // Several haystacks, and a query change in between, to exercise the
        // cached pattern being reused and then invalidated.
        for query in ["rm", "rm", "car", "rm"] {
            for haystack in ["readme.md", "Cargo.toml", "src/main.rs"] {
                assert_eq!(
                    fuzzy.score_indices(haystack, query),
                    score_indices(haystack, query),
                    "{haystack:?} vs {query:?}",
                );
                assert_eq!(
                    fuzzy.score(haystack, query),
                    score(haystack, query)
                );
            }
        }
        assert_eq!(fuzzy.score("anything", ""), Some(0));
        assert_eq!(fuzzy.score_indices("anything", ""), None);
    }

    #[test]
    fn score_indices_agrees_with_the_separate_calls() {
        let (score_of, indices) = score_indices("readme.md", "rm").unwrap();
        assert_eq!(score_of, score("readme.md", "rm").unwrap());
        assert_eq!(indices, match_indices("readme.md", "rm"));
        assert_eq!(score_indices("readme.md", "xyz"), None);
        // An empty query ranks everything equally, so it highlights nothing.
        assert_eq!(score_indices("readme.md", ""), None);
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
