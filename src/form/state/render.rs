//! Drawing a [`FormState`] from the [`field`](crate::form::field) building
//! blocks: the label column as wide as the longest label, the `*` of a
//! changed field in its own column, the focused row tinted, the picker
//! fields with the pointer glyph, the stepper, a problem in red under its
//! field, side information right-aligned, and two fixed hint rows - the
//! focused field's keys above the form's - so the box keeps its height while
//! the focus moves.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{Editing, Editor, FieldKind, FormField, FormState, hints, is_yes};
use crate::{
    autocomplete::PopupStyle,
    chrome::{self, MODAL_CHROME_HEIGHT, MODAL_CHROME_WIDTH, MODAL_PADDING_X},
    form::field::{self, FieldStyles, LabelMarks},
    layout::centered_rect,
    nav, overlay, scroll, style, text,
    theme::Skin,
};

/// The most rows a multi-line field's text takes beside its label; it grows
/// with its text up to them.
pub const MULTILINE_ROWS: usize = 4;
/// The columns a multi-line field's text asks for, however long it is.
const MULTILINE_WIDTH: usize = 40;
/// The fewest columns a value gets, however short the values are.
const MIN_VALUE_WIDTH: usize = 28;
/// What a mixed value shows.
const MIXED: &str = "(mixed)";

/// Where the field being typed into sits on screen.
#[derive(Debug, Clone, Copy)]
struct EditSpot {
    /// The column its value starts in.
    column: u16,
    /// Its first row.
    y: u16,
    /// The rows it takes.
    rows: usize,
}

/// What every row shares in one draw.
struct RowCtx<'a> {
    skin: &'a Skin,
    styles: FieldStyles,
    label_width: usize,
    width: usize,
}

impl FormState {
    /// Draws the form as a modal box centred in `area`, as big as its rows
    /// and keys: the frame with its title and the focus badge, the rows
    /// (scrolling with a scrollbar on a short screen), the hint rows and,
    /// while a field is typed into, its suggestions - drawn last, over
    /// everything else, and free to run past the box to the screen's edge.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let width = u16::try_from(self.content_width())
            .unwrap_or(u16::MAX)
            .saturating_add(MODAL_CHROME_WIDTH);
        let inner_width =
            width.min(area.width).saturating_sub(MODAL_CHROME_WIDTH);
        let rows = self.rows_at(usize::from(inner_width));
        let hint_rows = hints::hint_rows(usize::from(inner_width));
        let height = u16::try_from(rows)
            .unwrap_or(u16::MAX)
            .saturating_add(hint_rows + MODAL_CHROME_HEIGHT);
        let rect = centered_rect(width, height, area);
        frame.render_widget(Clear, rect);
        let inner = overlay::framed(frame, rect, skin, &self.title);
        let (position, total) = self.focus_position();
        chrome::render_badge(
            frame,
            rect,
            skin,
            &chrome::position_badge(position, total),
        );
        let rows_area = Rect {
            height: inner.height.saturating_sub(hint_rows),
            ..inner
        };
        let hints_area = Rect {
            y: rows_area.bottom(),
            height: inner.height.min(hint_rows),
            ..inner
        };
        self.draw_rows(frame, rows_area, skin, MODAL_PADDING_X);
        frame.render_widget(
            Paragraph::new(self.hint_lines(usize::from(inner.width), skin)),
            hints_area,
        );
        self.render_suggestions(frame, rows_area, skin, frame.area());
    }

    /// Draws the form inline in a host's content area instead of as a
    /// modal: the title as the first row (in the accent and bold, in line
    /// with the labels), a blank row, then the fields, the focused row's
    /// tint inside `area`. The keys belong in the host's footer
    /// ([`FormState::hint_groups`]), the question or notice in its status
    /// band ([`FormState::prompt`]). The suggestions stay inside `area`.
    pub fn render_inline(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        if area.height == 0 {
            return;
        }
        let title_style = style::accent(&skin.palette)
            .add_modifier(ratatui::style::Modifier::BOLD);
        let title = Line::from(vec![
            Span::raw(" ".repeat(field::MARK_WIDTH)),
            Span::styled(self.title.trim().to_string(), title_style),
        ]);
        frame.render_widget(Paragraph::new(title), Rect { height: 1, ..area });
        let rows = Rect {
            y: area.y.saturating_add(2),
            height: area.height.saturating_sub(2),
            ..area
        };
        self.draw_rows(frame, rows, skin, 0);
        self.render_suggestions(frame, rows, skin, area);
    }

    /// Draws the rows into `area` (a host layout or the modal's inside):
    /// scrolled so the focused field shows, its row tinted through the
    /// modal padding, a scrollbar while they overflow, the live editor of a
    /// multi-line field and the suggestions of the field being typed into,
    /// which may run past `area` down to the screen's edge.
    pub fn render_rows(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        self.draw_rows(frame, area, skin, MODAL_PADDING_X);
        self.render_suggestions(frame, area, skin, frame.area());
    }

    /// [`FormState::render_rows`] without the suggestions, with the focus
    /// tint reaching `bleed` columns past `area` on each side (the modal
    /// padding, or none).
    fn draw_rows(
        &self,
        frame: &mut Frame,
        area: Rect,
        skin: &Skin,
        bleed: u16,
    ) {
        let width = usize::from(area.width);
        let lines = self.lines(width, skin);
        let viewport = usize::from(area.height);
        let focus_row = self.row_of(self.focus, width);
        let focus_rows = self.field_rows(self.focus, width);
        let offset = self.scroll_offset(area);
        if let Some(row) = focus_row.checked_sub(offset)
            && row < viewport
        {
            let y = area.y + u16::try_from(row).unwrap_or(u16::MAX);
            let height =
                u16::try_from(focus_rows.min(viewport - row)).unwrap_or(1);
            let tint = Rect::new(
                area.x.saturating_sub(bleed),
                y,
                area.width + 2 * bleed,
                height,
            )
            .intersection(frame.area());
            let styles = FieldStyles::from_palette(&skin.palette);
            frame
                .render_widget(Block::default().style(styles.focus_tint), tint);
        }
        let shown: Vec<Line> =
            lines.into_iter().skip(offset).take(viewport).collect();
        frame.render_widget(Paragraph::new(shown), area);
        scroll::render_scrollbar(
            frame,
            area,
            skin,
            nav::ScrollView {
                total: self.rows_at(width),
                offset,
                viewport,
            },
        );
        self.render_editor(frame, area, skin);
    }

    /// The first row shown in `area`: scrolled just far enough that the
    /// focused field shows.
    fn scroll_offset(&self, area: Rect) -> usize {
        let width = usize::from(area.width);
        nav::keep_visible(
            nav::ScrollView {
                total: self.rows_at(width),
                offset: 0,
                viewport: usize::from(area.height),
            },
            self.row_of(self.focus, width),
        )
    }

    /// The rows of the fields for `width` columns, for a host that lays the
    /// form out itself (the focused row's tint covers the rows' width).
    pub fn lines(&self, width: usize, skin: &Skin) -> Vec<Line<'static>> {
        let ctx = RowCtx {
            skin,
            styles: FieldStyles::from_palette(&skin.palette),
            label_width: self.label_width(),
            width,
        };
        let mut lines = Vec::with_capacity(self.row_count());
        for (index, field) in self.fields.iter().enumerate() {
            lines.extend(self.field_lines(index, field, &ctx));
            if let Some((faulty, message)) = &self.error
                && *faulty == index
            {
                lines.push(field::problem_line(
                    message,
                    field::value_column(ctx.label_width),
                    width,
                    &ctx.styles,
                ));
            }
        }
        lines
    }

    /// The columns the rows and keys need: the label column and the widest
    /// value with its side information, never narrower than a hint row.
    pub fn content_width(&self) -> usize {
        let values = self
            .fields
            .iter()
            .map(|field| {
                let value = match &field.kind {
                    FieldKind::Stepper(options)
                    | FieldKind::Choice(options) => {
                        options.iter().map(|o| o.width()).max().unwrap_or(0) + 4
                    }
                    FieldKind::Multiline => MULTILINE_WIDTH,
                    _ => {
                        field
                            .value
                            .width()
                            .max(field::PICKER_PLACEHOLDER.width())
                            + 2
                    }
                };
                let side = side_reserve(field);
                value + if side > 0 { side + 2 } else { 0 }
            })
            .max()
            .unwrap_or(0)
            .max(MIN_VALUE_WIDTH);
        let rows = field::value_column(self.label_width()) + values + 1;
        let sections = self
            .fields
            .iter()
            .filter(|f| f.kind == FieldKind::Section)
            .map(|f| f.label.width())
            .max()
            .unwrap_or(0);
        let hints = hints::min_width();
        rows.max(sections).max(hints)
    }

    /// The number of rows the fields take in the modal (problem line
    /// included).
    pub fn row_count(&self) -> usize {
        self.rows_at(self.content_width())
    }

    /// The number of rows the fields take at `width` columns.
    fn rows_at(&self, width: usize) -> usize {
        (0..self.fields.len())
            .map(|index| self.field_rows(index, width))
            .sum::<usize>()
            + usize::from(self.error.is_some())
    }

    /// The row the field at `index` starts on at `width` columns.
    fn row_of(&self, index: usize, width: usize) -> usize {
        let before: usize = (0..index).map(|i| self.field_rows(i, width)).sum();
        let error_above = self.error.as_ref().is_some_and(|(i, _)| *i < index);
        before + usize::from(error_above)
    }

    /// The rows the field at `index` takes at `width` columns: one, or a
    /// multi-line field's wrapped text (its editor's while typed into),
    /// between one and [`MULTILINE_ROWS`].
    fn field_rows(&self, index: usize, width: usize) -> usize {
        let Some(field) = self.fields.get(index) else {
            return 0;
        };
        if field.kind != FieldKind::Multiline {
            return 1;
        }
        let wrap = self.wrap_width(field, width);
        let rows = match self.editing.as_ref() {
            Some(Editing {
                index: editing,
                editor: Editor::Area(area),
                ..
            }) if *editing == index => area.rows(wrap),
            _ => text::wrap(&field.value, wrap).len(),
        };
        rows.clamp(1, MULTILINE_ROWS)
    }

    /// The columns a multi-line field's text wraps at: the value column
    /// minus the side column it keeps free, so the text does not rewrap
    /// when its side information comes and goes.
    fn wrap_width(&self, field: &FormField, width: usize) -> usize {
        let value =
            width.saturating_sub(field::value_column(self.label_width()));
        let side = side_reserve(field);
        value
            .saturating_sub(if side > 0 { side + 1 } else { 0 })
            .max(1)
    }

    /// The side information the field at `index` shows: its lock reason,
    /// its own side information, or its `used/max` counter (counting the
    /// text being typed).
    fn side_text(&self, index: usize, field: &FormField) -> String {
        if !field.lock.is_empty() {
            return field.lock.clone();
        }
        if !field.side_info.is_empty() {
            return field.side_info.clone();
        }
        let Some(max) = field.max_chars else {
            return String::new();
        };
        let live = match self.editing.as_ref() {
            Some(editing) if editing.index == index => editing.editor.text(),
            _ => field.value.as_str(),
        };
        format!("{}/{max}", live.chars().count())
    }

    /// The width of the label column: the longest label.
    fn label_width(&self) -> usize {
        field::label_column_width(
            self.fields
                .iter()
                .filter(|f| f.kind != FieldKind::Section)
                .map(|f| f.label.as_str()),
        )
    }

    /// The rows of one field: a section bar, a field row, or a multi-line
    /// field's label row and its text rows.
    fn field_lines(
        &self,
        index: usize,
        field: &FormField,
        ctx: &RowCtx,
    ) -> Vec<Line<'static>> {
        if field.kind == FieldKind::Section {
            let title = style::accent(&ctx.skin.palette)
                .add_modifier(ratatui::style::Modifier::BOLD);
            return vec![Line::styled(field.label.clone(), title)];
        }
        let focused = index == self.focus;
        let marks = LabelMarks {
            changed: self.is_changed(index),
            focused,
            problem: self.error.as_ref().is_some_and(|(i, _)| *i == index),
        };
        if field.kind == FieldKind::Multiline {
            return self.multiline_lines(index, field, marks, ctx);
        }
        let mut spans = field::label_cell(
            &field.label,
            ctx.label_width,
            marks,
            &ctx.styles,
        );
        let value_width = ctx
            .width
            .saturating_sub(field::value_column(ctx.label_width));
        spans.extend(self.value_spans(index, field, value_width, ctx));
        let spans = if focused {
            field::tint_focused(spans, ctx.width, ctx.styles.focus_tint)
        } else {
            spans
        };
        vec![Line::from(spans)]
    }

    /// A multi-line field: its wrapped text beside the label, as many rows
    /// as it needs up to [`MULTILINE_ROWS`] (`…` when cut), the side
    /// information right-aligned in the first row.
    fn multiline_lines(
        &self,
        index: usize,
        field: &FormField,
        marks: LabelMarks,
        ctx: &RowCtx,
    ) -> Vec<Line<'static>> {
        let rows = self.field_rows(index, ctx.width);
        let mut texts = if field.is_mixed {
            vec![MIXED.to_string()]
        } else if field.value.is_empty() {
            vec![field::EMPTY_VALUE.to_string()]
        } else {
            text::wrap(&field.value, self.wrap_width(field, ctx.width))
        };
        if texts.len() > rows {
            texts.truncate(rows.saturating_sub(1));
            texts.push("\u{2026}".to_string());
        }
        texts.resize(rows, String::new());
        let value_style = if field.value.is_empty() || field.is_mixed {
            ctx.styles.muted
        } else if field.lock.is_empty() {
            field.value_style
        } else {
            ctx.styles.muted
        };
        let indent = field::value_column(ctx.label_width);
        let info = self.side_text(index, field);
        texts
            .into_iter()
            .enumerate()
            .map(|(row, text)| {
                let mut spans = if row == 0 {
                    field::label_cell(
                        &field.label,
                        ctx.label_width,
                        marks,
                        &ctx.styles,
                    )
                } else {
                    vec![Span::raw(" ".repeat(indent))]
                };
                spans.push(Span::styled(text, value_style));
                if row == 0 && !info.is_empty() {
                    spans = field::with_side_info(
                        spans,
                        &info,
                        ctx.width,
                        &side_styles(field, &ctx.styles),
                    );
                }
                let spans = if marks.focused {
                    field::tint_focused(spans, ctx.width, ctx.styles.focus_tint)
                } else {
                    spans
                };
                Line::from(spans)
            })
            .collect()
    }

    /// The value part of a single-row field, `width` columns wide, with its
    /// side information (or lock reason) right-aligned.
    fn value_spans(
        &self,
        index: usize,
        field: &FormField,
        width: usize,
        ctx: &RowCtx,
    ) -> Vec<Span<'static>> {
        let typing = self
            .editing
            .as_ref()
            .filter(|editing| editing.index == index)
            .and_then(|editing| match &editing.editor {
                Editor::Line(input) => Some(input),
                Editor::Area(_) => None,
            });
        let info = self.side_text(index, field);
        let room = if info.is_empty() {
            width
        } else {
            width.saturating_sub(info.width() + 1)
        };
        let value = match typing {
            Some(input) => {
                input.render_line(&ctx.skin.palette, room, true).spans
            }
            None => Self::shown_value(field, room, ctx),
        };
        if info.is_empty() {
            value
        } else {
            field::with_side_info(
                value,
                &info,
                width,
                &side_styles(field, &ctx.styles),
            )
        }
    }

    /// How a field's value reads while it is not typed into.
    fn shown_value(
        field: &FormField,
        width: usize,
        ctx: &RowCtx,
    ) -> Vec<Span<'static>> {
        let styles = &ctx.styles;
        let locked = !field.lock.is_empty();
        let value_style = if locked {
            styles.muted
        } else {
            field.value_style
        };
        match &field.kind {
            FieldKind::Toggle => {
                let mark = if field.is_mixed {
                    "[-]"
                } else if is_yes(&field.value) {
                    "[x]"
                } else {
                    "[ ]"
                };
                vec![Span::styled(mark, value_style)]
            }
            FieldKind::Stepper(_) => {
                let value = if field.is_mixed { MIXED } else { &field.value };
                vec![Span::styled(
                    text::truncate(&ctx.skin.glyphs.stepper(value), width),
                    value_style,
                )]
            }
            FieldKind::Choice(_)
            | FieldKind::Picker
            | FieldKind::TextPicker => {
                let value = if field.is_mixed { MIXED } else { &field.value };
                let mut spans = field::picker_value_spans(
                    value,
                    width,
                    ctx.skin.glyphs.pointer,
                    styles,
                );
                if !value.is_empty()
                    && let Some(first) = spans.first_mut()
                {
                    first.style = value_style;
                }
                spans
            }
            _ if field.is_mixed => vec![Span::styled(MIXED, styles.muted)],
            _ if field.value.is_empty() => {
                vec![Span::styled(field::EMPTY_VALUE, styles.muted)]
            }
            _ => vec![Span::styled(
                text::truncate(&field.value, width),
                value_style,
            )],
        }
    }

    /// Draws the live editor of a multi-line field over its text rows, when
    /// they are on screen.
    fn render_editor(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let Some(editing) = &self.editing else {
            return;
        };
        let Editor::Area(text_area) = &editing.editor else {
            return;
        };
        let (Some(field), Some(spot)) =
            (self.fields.get(editing.index), self.edit_spot(area))
        else {
            return;
        };
        let rect = Rect::new(
            spot.column,
            spot.y,
            u16::try_from(self.wrap_width(field, usize::from(area.width)))
                .unwrap_or(1),
            u16::try_from(spot.rows).unwrap_or(1),
        )
        .intersection(area);
        if rect.height == 0 {
            return;
        }
        frame.render_widget(Clear, rect);
        text_area.render(frame, rect, skin, true);
    }

    /// Draws the suggestions of the field being typed into (its rows laid
    /// out in `area`) under its last row, inside `menu_bounds`. Called after
    /// everything else of the form, so nothing paints over the menu.
    fn render_suggestions(
        &self,
        frame: &mut Frame,
        area: Rect,
        skin: &Skin,
        menu_bounds: Rect,
    ) {
        let (Some(editing), Some(spot)) = (&self.editing, self.edit_spot(area))
        else {
            return;
        };
        let rows = u16::try_from(spot.rows.saturating_sub(1)).unwrap_or(0);
        let last = spot.y.saturating_add(rows);
        if last >= area.bottom() {
            return; // the field's last row is scrolled out of `area`
        }
        editing.complete.render_popup(
            frame,
            Position::new(spot.column, last),
            menu_bounds,
            &PopupStyle::from_palette(&skin.palette),
        );
    }

    /// Where the field being typed into sits in `area`, or `None` while no
    /// field is typed into or it is scrolled off above `area`.
    fn edit_spot(&self, area: Rect) -> Option<EditSpot> {
        let editing = self.editing.as_ref()?;
        let width = usize::from(area.width);
        let start = self.row_of(editing.index, width);
        let row = start.checked_sub(self.scroll_offset(area))?;
        let indent = field::value_column(self.label_width());
        Some(EditSpot {
            column: area
                .x
                .saturating_add(u16::try_from(indent).unwrap_or(u16::MAX)),
            y: area
                .y
                .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
            rows: self.field_rows(editing.index, width),
        })
    }
}

/// The columns a field keeps for its side information: its lock reason,
/// its own side information or its widest counter.
fn side_reserve(field: &FormField) -> usize {
    let counter = field
        .max_chars
        .map_or(0, |max| format!("{max}/{max}").width());
    field.side_info.width().max(field.lock.width()).max(counter)
}

/// The styles of a field's side information: the error colour for a
/// warning, else muted.
fn side_styles(field: &FormField, styles: &FieldStyles) -> FieldStyles {
    if field.is_warning && field.lock.is_empty() {
        FieldStyles {
            muted: styles.error,
            ..*styles
        }
    } else {
        *styles
    }
}
