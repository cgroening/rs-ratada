//! Text helpers for terminal rendering.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates `text` to `width` display columns, appending '…' when clipped.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(1);
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > budget {
            break;
        }
        result.push(ch);
        used += char_width;
    }
    result.push('\u{2026}');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_text_untouched() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis_when_clipped() {
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
    }

    #[test]
    fn truncate_to_zero_is_empty() {
        assert_eq!(truncate("hello", 0), "");
    }
}
