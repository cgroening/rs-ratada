//! Case-insensitive substring search that reports the hit in chars.
//!
//! Lowercasing can change a string's length (`İ` becomes two chars), so the
//! search runs over the folded chars and maps the hit back onto the original
//! chars. The range is therefore always valid for the original text and never
//! splits a char, whatever the input.

use std::ops::Range;

/// Finds the first case-insensitive occurrence of `needle` in `haystack` and
/// returns its char range in `haystack`, or `None` when it does not occur. An
/// empty `needle` matches nothing.
pub(super) fn find_ci(haystack: &str, needle: &str) -> Option<Range<usize>> {
    let needle: Vec<char> =
        needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    // Each folded char remembers which original char it came from.
    let (folded, origin): (Vec<char>, Vec<usize>) = haystack
        .chars()
        .enumerate()
        .flat_map(|(index, ch)| ch.to_lowercase().map(move |low| (low, index)))
        .unzip();
    let start = folded
        .windows(needle.len())
        .position(|window| window == needle.as_slice())?;
    let last = start + needle.len() - 1;
    Some(origin[start]..origin[last] + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_case_insensitive_substring() {
        assert_eq!(find_ci("Rewe big shop", "BIG"), Some(5..8));
    }

    #[test]
    fn reports_the_first_occurrence() {
        assert_eq!(find_ci("abab", "b"), Some(1..2));
    }

    #[test]
    fn ranges_count_chars_not_bytes() {
        assert_eq!(find_ci("Müller Straße", "straße"), Some(7..13));
        assert_eq!(find_ci("ÄÖÜ", "öü"), Some(1..3));
    }

    #[test]
    fn a_char_that_folds_to_two_maps_back_onto_one() {
        // "İ" lowercases to "i̇" (two chars); the range still covers one char.
        assert_eq!(find_ci("xİy", "y"), Some(2..3));
        assert_eq!(find_ci("İstanbul", "stan"), Some(1..5));
    }

    #[test]
    fn an_exact_match_covers_the_whole_text() {
        assert_eq!(find_ci("Edeka", "edeka"), Some(0..5));
    }

    #[test]
    fn misses_and_empty_needles_yield_none() {
        assert_eq!(find_ci("Edeka", "rewe"), None);
        assert_eq!(find_ci("Edeka", ""), None);
        assert_eq!(find_ci("", "a"), None);
    }
}
