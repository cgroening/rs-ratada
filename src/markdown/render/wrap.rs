//! Wrapping, clipping and aligning cell runs, and turning them into spans.

use pulldown_cmark::Alignment;
use ratatui::{style::Style, text::Span};

use super::{Cell, cells_width, ch_width};

/// Clips a cell to `width` display columns and pads it per the column alignment.
pub(super) fn align_cell(
    cell: &[Cell],
    width: usize,
    align: Alignment,
) -> Vec<Cell> {
    let mut clipped: Vec<Cell> = Vec::new();
    let mut used = 0usize;
    for &c in cell {
        let cw = ch_width(c.0);
        if used + cw > width {
            break;
        }
        clipped.push(c);
        used += cw;
    }
    let pad = width.saturating_sub(used);
    let (left, right) = match align {
        Alignment::Right => (pad, 0),
        Alignment::Center => (pad / 2, pad - pad / 2),
        _ => (0, pad),
    };
    let mut out = Vec::with_capacity(width);
    out.extend(std::iter::repeat_n((' ', Style::default()), left));
    out.extend(clipped);
    out.extend(std::iter::repeat_n((' ', Style::default()), right));
    out
}

/// Greedy word-wrap over styled cells: breaks at the last space, hard-splits an
/// over-long word, and treats an embedded `\n` (a hard break) as a forced
/// break. Each output line is a cell run with its styles preserved.
pub(super) fn wrap_cells(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Cell>> = Vec::new();
    let mut cur: Vec<Cell> = Vec::new();
    let mut cur_w = 0usize;
    let mut last_space: Option<usize> = None;
    for &(ch, style) in cells {
        if ch == '\n' {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
            last_space = None;
            continue;
        }
        let cw = ch_width(ch);
        if cur_w + cw > width && !cur.is_empty() {
            match last_space {
                Some(sp) => {
                    let tail = cur.split_off(sp);
                    lines.push(std::mem::take(&mut cur));
                    cur = tail.into_iter().skip(1).collect();
                    cur_w = cells_width(&cur);
                }
                None => {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                }
            }
            last_space = None;
        }
        if ch == ' ' {
            last_space = Some(cur.len());
        }
        cur.push((ch, style));
        cur_w += cw;
    }
    lines.push(cur);
    lines
}

/// Hard-wraps cells purely by display width (for code blocks, which must keep
/// their literal spacing rather than wrap at word boundaries).
pub(super) fn hard_wrap(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    if cells.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines: Vec<Vec<Cell>> = Vec::new();
    let mut cur: Vec<Cell> = Vec::new();
    let mut cur_w = 0usize;
    for &cell in cells {
        let cw = ch_width(cell.0);
        if cur_w + cw > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push(cell);
        cur_w += cw;
    }
    lines.push(cur);
    lines
}

/// Coalesces a cell run into spans, merging adjacent cells of equal style.
pub(super) fn cells_to_spans(cells: &[Cell]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut current: Option<Style> = None;
    for &(ch, style) in cells {
        if current != Some(style) {
            if let Some(prev) = current
                && !buf.is_empty()
            {
                spans.push(Span::styled(std::mem::take(&mut buf), prev));
            }
            current = Some(style);
        }
        buf.push(ch);
    }
    if let (Some(style), false) = (current, buf.is_empty()) {
        spans.push(Span::styled(buf, style));
    }
    spans
}

/// Clips a cell run to `width` columns (single-line), appending the
/// `ellipsis`-styled `…` on overflow.
pub(super) fn clip_cells(
    cells: &[Cell],
    width: usize,
    ellipsis: Style,
) -> Vec<Span<'static>> {
    let total = cells_width(cells);
    if total <= width {
        return cells_to_spans(cells);
    }
    let budget = width.saturating_sub(1);
    let mut kept: Vec<Cell> = Vec::new();
    let mut used = 0usize;
    for &cell in cells {
        let cw = ch_width(cell.0);
        if used + cw > budget {
            break;
        }
        kept.push(cell);
        used += cw;
    }
    let mut spans = cells_to_spans(&kept);
    spans.push(Span::styled("\u{2026}".to_string(), ellipsis));
    spans
}
