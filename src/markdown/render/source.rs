//! Where shown text came from: the source position of each rendered cell.
//!
//! Rendering drops Markdown markers (`**`, `[`, `](url)`, list bullets) and
//! reflows soft breaks, so a shown character has no fixed distance to its
//! place in the source. The positions are found by walking the source forward
//! once per block and matching each shown character in order.

/// The byte offset in `src` of each shown character, matched in order
/// within the block's source `from..to`.
///
/// A character matches the same character, and any whitespace matches any
/// whitespace (a soft break shows as a space). A character the block's
/// source does not hold from the current position on (smart punctuation, a
/// decoded entity's glyph) takes the current position without moving it,
/// so one such character never pulls the rest of the block out of place;
/// the search never leaves the block.
pub(super) fn byte_offsets(
    shown: impl IntoIterator<Item = char>,
    src: &str,
    from: usize,
    to: usize,
) -> Vec<usize> {
    let to = floor_char_boundary(src, to.min(src.len()));
    let block = &src[..to];
    let mut at = floor_char_boundary(src, from.min(to));
    let mut offsets = Vec::new();
    for ch in shown {
        match find_from(block, at, ch) {
            Some(found) => {
                offsets.push(found);
                at = found
                    + src[found..].chars().next().map_or(0, char::len_utf8);
            }
            None => offsets.push(at.min(to)),
        }
    }
    offsets
}

/// The char index of each byte offset in `src` (an offset inside a character
/// counts as that character).
pub(super) fn char_indices_of(
    src: &str,
    bytes: &[Option<usize>],
) -> Vec<Option<usize>> {
    let starts: Vec<usize> = src.char_indices().map(|(at, _)| at).collect();
    let char_index = |byte: usize| {
        if byte >= src.len() {
            return starts.len();
        }
        match starts.binary_search(&byte) {
            Ok(index) => index,
            // The first start is 0, so a miss is past at least one start.
            Err(index) => index - 1,
        }
    };
    bytes.iter().map(|byte| byte.map(char_index)).collect()
}

/// The byte offset of the next character from `at` that matches `ch`.
fn find_from(src: &str, at: usize, ch: char) -> Option<usize> {
    src.get(at..)?
        .char_indices()
        .find(|(_, candidate)| is_match(*candidate, ch))
        .map(|(offset, _)| at + offset)
}

/// Whether a source character can be the shown one.
fn is_match(source: char, shown: char) -> bool {
    source == shown || (source.is_whitespace() && shown.is_whitespace())
}

/// The largest char boundary at or below `at`.
fn floor_char_boundary(src: &str, mut at: usize) -> usize {
    while at > 0 && !src.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_skipped_and_soft_breaks_match_a_space() {
        let src = "a **bold**\nnext";
        let offsets = byte_offsets("a bold next".chars(), src, 0, src.len());
        assert_eq!(offsets, vec![0, 1, 4, 5, 6, 7, 10, 11, 12, 13, 14]);
    }

    #[test]
    fn a_character_the_source_lacks_keeps_the_position() {
        let src = "a -- b";
        let offsets = byte_offsets("a \u{2013} b".chars(), src, 0, src.len());
        assert_eq!(offsets[2], 2, "the dash stands where the source is");
        assert_eq!(offsets[4], 5, "the rest still matches");
    }

    #[test]
    fn byte_offsets_become_char_indices() {
        let src = "über x";
        let chars = char_indices_of(src, &[Some(0), Some(3), None]);
        assert_eq!(chars, vec![Some(0), Some(2), None]);
    }
}
