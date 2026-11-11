use super::*;

/// A plain, easily-asserted stylesheet: distinguishable colours, simple
/// glyphs, no background bands (so line content is the bare text).
fn sheet() -> StyleSheet {
    let heading = |fg: Color| HeadingStyle {
        fg: Some(fg),
        bg: None,
        bold: true,
    };
    StyleSheet {
        base: Style::default(),
        headings: [
            heading(Color::Rgb(1, 1, 1)),
            heading(Color::Rgb(2, 2, 2)),
            heading(Color::Rgb(3, 3, 3)),
            heading(Color::Rgb(4, 4, 4)),
            heading(Color::Rgb(5, 5, 5)),
            heading(Color::Rgb(6, 6, 6)),
        ],
        strong: Style::default().add_modifier(Modifier::BOLD),
        emphasis: Style::default().add_modifier(Modifier::ITALIC),
        strikethrough: Style::default().add_modifier(Modifier::CROSSED_OUT),
        inline_code: Style::default().fg(Color::Rgb(200, 0, 200)),
        code_block: CodeBlockStyle {
            fg: Some(Color::Rgb(0, 200, 200)),
            bg: Some(Color::Rgb(20, 20, 20)),
            title_fg: Some(Color::Rgb(240, 240, 130)),
        },
        quote: QuoteStyle {
            fg: Some(Color::Rgb(180, 180, 180)),
            bg: None,
            bar_fg: Some(Color::Rgb(100, 100, 100)),
            bar: "\u{258e}".to_string(),
        },
        highlight: Style::default()
            .fg(Color::Rgb(0, 0, 0))
            .bg(Color::Rgb(240, 240, 0)),
        link: Style::default()
            .fg(Color::Rgb(0, 120, 255))
            .add_modifier(Modifier::UNDERLINED),
        rule: RuleStyle {
            fg: Some(Color::Rgb(210, 140, 60)),
            glyph: "\u{2500}".to_string(),
        },
        bullet: BulletStyle {
            fg: Some(Color::Rgb(240, 240, 130)),
            glyphs: vec!["\u{25cf}".to_string(), "\u{25cb}".to_string()],
        },
        checkbox: CheckboxStyle {
            checked: "\u{2611}".to_string(),
            unchecked: "\u{2610}".to_string(),
            checked_fg: Some(Color::Rgb(150, 240, 150)),
            unchecked_fg: Some(Color::Rgb(240, 240, 130)),
        },
        callout: CalloutTheme {
            note: CalloutStyle {
                fg: Some(Color::Rgb(0, 120, 255)),
                bg: Some(Color::Rgb(10, 20, 40)),
            },
            tip: CalloutStyle {
                fg: Some(Color::Rgb(0, 200, 0)),
                bg: Some(Color::Rgb(0, 30, 10)),
            },
            important: CalloutStyle {
                fg: Some(Color::Rgb(200, 120, 255)),
                bg: Some(Color::Rgb(30, 10, 50)),
            },
            warning: CalloutStyle {
                fg: Some(Color::Rgb(255, 170, 80)),
                bg: Some(Color::Rgb(40, 25, 10)),
            },
            caution: CalloutStyle {
                fg: Some(Color::Rgb(255, 100, 120)),
                bg: Some(Color::Rgb(40, 15, 20)),
            },
        },
        table_border: Some(Color::Rgb(210, 140, 60)),
        smart_punctuation: false,
        preserve_line_breaks: false,
        html: Style::default().add_modifier(Modifier::DIM),
        ellipsis: Style::default().add_modifier(Modifier::DIM),
    }
}

/// Joins a rendered block's line texts for content assertions.
fn texts(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

#[test]
fn heading_drops_markers_and_styles_text() {
    let lines = render_block("# Title", 40, &sheet());
    assert_eq!(texts(&lines), vec!["Title".to_string()]);
    let style = lines[0].spans[0].style;
    assert_eq!(style.fg, Some(Color::Rgb(1, 1, 1)));
    assert!(style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn emphasis_and_strong_compose_modifiers() {
    let lines = render_block("a *b* **c**", 40, &sheet());
    let spans = &lines[0].spans;
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "a b c");
    let italic = spans
        .iter()
        .find(|s| s.content.as_ref() == "b")
        .expect("emphasis span");
    assert!(italic.style.add_modifier.contains(Modifier::ITALIC));
    let bold = spans
        .iter()
        .find(|s| s.content.as_ref() == "c")
        .expect("strong span");
    assert!(bold.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn highlight_marker_is_dropped_and_styled() {
    let lines = render_block("x ==hot== y", 40, &sheet());
    let spans = &lines[0].spans;
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "x hot y");
    let hot = spans
        .iter()
        .find(|s| s.content.as_ref() == "hot")
        .expect("highlight span");
    assert_eq!(hot.style.bg, Some(Color::Rgb(240, 240, 0)));
}

#[test]
fn highlight_does_not_fire_inside_inline_code() {
    let lines = render_block("`==x==`", 40, &sheet());
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "==x==");
}

#[test]
fn highlight_requires_nonempty_marker_free_inner() {
    // Empty `====` and a `=` inside never match.
    let lines = render_block("==== a=b", 40, &sheet());
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "==== a=b");
}

#[test]
fn inline_code_uses_code_style() {
    let lines = render_block("run `cargo`", 40, &sheet());
    let code = lines[0]
        .spans
        .iter()
        .find(|s| s.content.as_ref() == "cargo")
        .expect("code span");
    assert_eq!(code.style.fg, Some(Color::Rgb(200, 0, 200)));
}

#[test]
fn bullets_render_with_glyph_by_depth() {
    let src = "- one\n- two";
    let lines = render_block(src, 40, &sheet());
    assert_eq!(
        texts(&lines),
        vec!["\u{25cf} one".to_string(), "\u{25cf} two".to_string()]
    );
}

#[test]
fn a_tight_item_keeps_its_line_above_its_nested_list() {
    let lines = render_block("- a\n  - b\n- c", 40, &sheet());
    assert_eq!(
        texts(&lines),
        vec![
            "\u{25cf} a".to_string(),
            "  \u{25cb} b".to_string(),
            "\u{25cf} c".to_string(),
        ]
    );
}

#[test]
fn ordered_list_numbers_items() {
    let lines = render_block("1. a\n2. b", 40, &sheet());
    assert_eq!(texts(&lines), vec!["1. a".to_string(), "2. b".to_string()]);
}

#[test]
fn task_list_uses_checkbox_glyphs() {
    let lines = render_block("- [ ] todo\n- [x] done", 40, &sheet());
    assert_eq!(
        texts(&lines),
        vec!["\u{2610} todo".to_string(), "\u{2611} done".to_string()]
    );
}

#[test]
fn strikethrough_sets_crossed_out() {
    let lines = render_block("~~gone~~", 40, &sheet());
    let span = &lines[0].spans[0];
    assert_eq!(span.content.as_ref(), "gone");
    assert!(span.style.add_modifier.contains(Modifier::CROSSED_OUT));
}

#[test]
fn rule_fills_width_with_glyph() {
    let lines = render_block("---", 5, &sheet());
    assert_eq!(
        texts(&lines),
        vec!["\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()]
    );
}

#[test]
fn code_block_band_fills_width_and_keeps_text() {
    let src = "```\nlet x;\n```";
    let lines = render_block(src, 10, &sheet());
    assert_eq!(lines.len(), 1);
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    // Full-width band: text padded with spaces to 10 columns.
    assert_eq!(joined, "let x;    ");
    assert_eq!(lines[0].spans[0].style.bg, Some(Color::Rgb(20, 20, 20)));
}

#[test]
fn quote_prefixes_a_bar() {
    let lines = render_block("> hi", 40, &sheet());
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "\u{258e} hi");
}

#[test]
fn block_wraps_at_width() {
    let lines = render_block("alpha beta gamma", 7, &sheet());
    assert_eq!(
        texts(&lines),
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );
}

#[test]
fn paragraphs_are_separated_by_a_blank_line() {
    let lines = render_block("a\n\nb", 40, &sheet());
    assert_eq!(
        texts(&lines),
        vec!["a".to_string(), String::new(), "b".to_string()]
    );
}

#[test]
fn render_inline_strips_inline_markers() {
    let spans = render_inline("a **b** `c`", 40, &sheet());
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "a b c");
}

#[test]
fn render_inline_keeps_block_syntax_literal() {
    let spans = render_inline("# not a heading", 40, &sheet());
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "# not a heading");
}

#[test]
fn render_inline_clips_with_ellipsis() {
    let spans = render_inline("abcdefhij", 5, &sheet());
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "abcd\u{2026}");
}

#[test]
fn measure_block_matches_line_count() {
    let src = "# h\n\nbody text here";
    assert_eq!(
        measure_block(src, 40, &sheet()),
        render_block(src, 40, &sheet()).len()
    );
}

#[test]
fn links_are_extracted_in_order() {
    let src = "see [a](http://a) and [b](http://b)";
    let found = links(src);
    assert_eq!(
        found,
        vec![
            Link {
                text: "a".to_string(),
                url: "http://a".to_string()
            },
            Link {
                text: "b".to_string(),
                url: "http://b".to_string()
            },
        ]
    );
}

#[test]
fn style_overlay_keeps_markers_and_styles_emphasis() {
    // `*x*` - all three chars (markers included) carry the italic modifier.
    let styles = style_overlay("*x*", &sheet());
    assert_eq!(styles.len(), 3);
    for style in &styles {
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }
}

#[test]
fn style_overlay_styles_heading_including_hash() {
    let styles = style_overlay("# Hi", &sheet());
    assert_eq!(styles.len(), 4);
    // The `#` marker is styled with the H1 colour, like the rest.
    assert_eq!(styles[0].fg, Some(Color::Rgb(1, 1, 1)));
    assert!(styles[0].add_modifier.contains(Modifier::BOLD));
    assert_eq!(styles[3].fg, Some(Color::Rgb(1, 1, 1)));
}

#[test]
fn style_overlay_highlights_with_markers() {
    let styles = style_overlay("==hot==", &sheet());
    assert_eq!(styles.len(), 7);
    for style in &styles {
        assert_eq!(style.bg, Some(Color::Rgb(240, 240, 0)));
    }
}

#[test]
fn style_overlay_styles_inline_code_with_backticks() {
    let styles = style_overlay("`c`", &sheet());
    assert_eq!(styles.len(), 3);
    for style in &styles {
        assert_eq!(style.fg, Some(Color::Rgb(200, 0, 200)));
    }
}

#[test]
fn style_overlay_leaves_plain_text_unstyled() {
    let styles = style_overlay("plain", &sheet());
    assert!(styles.iter().all(|style| *style == Style::default()));
}

#[test]
fn style_overlay_handles_multibyte_chars() {
    // Umlaut before emphasis (`ä *x*`, 5 chars): the multi-byte char means
    // byte and char indices differ, so the byte->char mapping must align.
    let styles = style_overlay("\u{e4} *x*", &sheet());
    assert_eq!(styles.len(), 5);
    assert_eq!(styles[0], Style::default());
    assert_eq!(styles[1], Style::default());
    for style in &styles[2..5] {
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }
}

#[test]
fn callout_renders_title_and_colours() {
    let lines = render_block("> [!WARNING]\n> careful", 40, &sheet());
    let warning_fg = Color::Rgb(255, 170, 80);
    // The title line carries the kind label in the warning colour.
    let title: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(title.contains("WARNING"));
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|s| s.style.fg == Some(warning_fg))
    );
    // The body line is coloured + backgrounded with the callout colours.
    let body = lines
        .iter()
        .find(|l| {
            l.spans
                .iter()
                .any(|s| s.content.as_ref().contains("careful"))
        })
        .expect("callout body line");
    assert!(
        body.spans
            .iter()
            .any(|s| s.style.bg == Some(Color::Rgb(40, 25, 10)))
    ); // warning bg
}

#[test]
fn image_alt_text_styles_like_a_link_and_is_collectable() {
    let lines = render_block("![pic](http://img)", 40, &sheet());
    let alt = lines[0]
        .spans
        .iter()
        .find(|s| s.content.as_ref() == "pic")
        .expect("alt text span");
    assert!(alt.style.add_modifier.contains(Modifier::UNDERLINED));
    let found = links("![pic](http://img)");
    assert_eq!(found[0].url, "http://img");
}

#[test]
fn code_block_shows_language_label() {
    let src = "```rust\nlet x;\n```";
    let lines = render_block(src, 20, &sheet());
    let label: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(label.trim_end().starts_with("rust"));
    assert_eq!(lines[0].spans[0].style.fg, Some(Color::Rgb(240, 240, 130)));
    // The code line follows.
    let joined: String =
        lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(joined.starts_with("let x;"));
}

#[test]
fn raw_html_is_rendered_literally() {
    let lines = render_block("see <br> end", 40, &sheet());
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "see <br> end");
}

#[test]
fn smart_punctuation_replaces_when_enabled() {
    let mut sheet = sheet();
    sheet.smart_punctuation = true;
    let lines = render_block("a -- b ...", 40, &sheet);
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(!joined.contains("--"));
    assert!(joined.contains('\u{2026}')); // …
}

#[test]
fn smart_punctuation_off_keeps_raw() {
    let lines = render_block("a -- b ...", 40, &sheet());
    let joined: String =
        lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "a -- b ...");
}

#[test]
fn preserve_line_breaks_keeps_soft_breaks_as_lines() {
    let mut sheet = sheet();
    sheet.preserve_line_breaks = true;
    let lines = render_block("XXX\nXXX", 40, &sheet);
    assert_eq!(texts(&lines), vec!["XXX".to_string(), "XXX".to_string()]);
}

#[test]
fn soft_breaks_collapse_to_a_space_by_default() {
    let lines = render_block("XXX\nXXX", 40, &sheet());
    assert_eq!(texts(&lines), vec!["XXX XXX".to_string()]);
}

#[test]
fn table_renders_columns_and_header_rule() {
    let src = "| a | b |\n|---|---|\n| 1 | 2 |";
    let lines = render_block(src, 40, &sheet());
    let all: String = lines
        .iter()
        .flat_map(|l| l.spans.iter())
        .map(|s| s.content.as_ref())
        .collect();
    assert!(all.contains('\u{2502}')); // │ column border
    assert!(all.contains('a') && all.contains('1'));
    // The header separator line is present.
    assert!(
        lines.iter().any(|l| {
            l.spans.iter().any(|s| s.content.contains('\u{251c}'))
        })
    );
}

#[test]
fn body_text_uses_the_configured_base_colour() {
    let mut sheet = sheet();
    sheet.base = Style::default().fg(Color::Rgb(10, 20, 30));
    let lines = render_block("plain text", 40, &sheet);
    assert_eq!(lines[0].spans[0].style.fg, Some(Color::Rgb(10, 20, 30)));
}

#[test]
fn raw_html_uses_the_configured_html_style() {
    let mut sheet = sheet();
    sheet.html = Style::default().fg(Color::Rgb(90, 90, 90));
    let lines = render_block("a <br> b", 40, &sheet);
    let html = lines[0]
        .spans
        .iter()
        .find(|s| s.content.as_ref() == "<br>")
        .expect("html span");
    assert_eq!(html.style.fg, Some(Color::Rgb(90, 90, 90)));
}

#[test]
fn ellipsis_uses_the_configured_style() {
    let mut sheet = sheet();
    sheet.ellipsis = Style::default().fg(Color::Rgb(200, 50, 50));
    let spans = render_inline("abcdefghij", 4, &sheet);
    let ell = spans.last().expect("ellipsis span");
    assert_eq!(ell.content.as_ref(), "\u{2026}");
    assert_eq!(ell.style.fg, Some(Color::Rgb(200, 50, 50)));
}

#[test]
fn clip_spans_truncates_to_budget() {
    let spans = vec![Span::raw("hello world")];
    let clipped = clip_spans(spans, 4, Style::default());
    let joined: String = clipped.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(joined, "hel\u{2026}");
}

/// The char index of `needle` in `src`, plus `skip` chars.
fn char_at(src: &str, needle: &str, skip: usize) -> usize {
    let byte = src.find(needle).expect("the needle is in the source");
    src[..byte].chars().count() + skip
}

/// The shown text of a line without its decoration (bullets, bars, the
/// table's borders), so its first char is the first char of its text.
fn shown_text(line: &Line<'static>) -> String {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    text.trim_start_matches(|ch: char| !ch.is_alphanumeric())
        .to_string()
}

#[test]
fn mapped_lines_start_where_their_text_starts() {
    let src = "Intro **bold** text\n\n### Head\n\n- a\n  - b\n- c\n\n\
               ```\nfn x\nlet y\n```\n\nA long paragraph that wraps \
               over several lines in a narrow box.";
    let mapped = render_block_mapped(src, 20, &sheet());
    let chars: Vec<char> = src.chars().collect();
    for line in &mapped {
        let Some(source) = line.source else {
            continue;
        };
        let shown = shown_text(&line.line);
        let first = shown.chars().next().expect("a mapped line shows text");
        assert_eq!(chars[source], first, "line {shown:?} maps to {source}");
    }
    let sources: Vec<Option<usize>> =
        mapped.iter().map(|line| line.source).collect();
    for (needle, skip) in [
        ("Intro", 0),
        ("Head", 0),
        ("- a", 2),
        ("- b", 2),
        ("- c", 2),
        ("fn x", 0),
        ("let y", 0),
        ("A long", 0),
        ("that wraps", 0),
        ("several", 0),
        ("narrow", 0),
    ] {
        let expected = Some(char_at(src, needle, skip));
        assert!(sources.contains(&expected), "{needle} has a line");
    }
}

#[test]
fn mapped_blank_lines_and_rules_are_told_apart() {
    let src = "One\n\n---\n\nTwo";
    let mapped = render_block_mapped(src, 10, &sheet());
    let sources: Vec<Option<usize>> =
        mapped.iter().map(|line| line.source).collect();
    assert_eq!(
        sources,
        vec![Some(0), None, Some(5), None, Some(10)],
        "text, separator, the rule's own line, separator, text"
    );
}

#[test]
fn mapped_table_rows_start_at_their_row() {
    let src = "| x | y |\n|---|---|\n| 1 | 2 |";
    let mapped = render_block_mapped(src, 30, &sheet());
    let sources: Vec<Option<usize>> =
        mapped.iter().map(|line| line.source).collect();
    assert_eq!(sources, vec![Some(0), None, Some(20)]);
}

#[test]
fn mapped_lines_match_render_block() {
    let src = "# T\n\n> [!NOTE]\n> quoted\n\n- [ ] task\n\n```rust\nx\n```";
    let plain = render_block(src, 30, &sheet());
    let mapped: Vec<Line<'static>> = render_block_mapped(src, 30, &sheet())
        .into_iter()
        .map(|line| line.line)
        .collect();
    assert_eq!(plain, mapped);
}

#[test]
fn mapped_offsets_count_chars_not_bytes() {
    let src = "Über\n\nÄpfel";
    let mapped = render_block_mapped(src, 10, &sheet());
    let sources: Vec<Option<usize>> =
        mapped.iter().map(|line| line.source).collect();
    assert_eq!(sources, vec![Some(0), None, Some(6)]);
}

#[test]
fn an_entity_never_pulls_the_mapping_into_a_later_block() {
    let src = "a &lt; b\n\nc < d";
    let mapped = render_block_mapped(src, 3, &sheet());
    // At width 3 the second line is `< b`; its `<` is the entity.
    let b_line = mapped
        .iter()
        .find(|line| shown_text(&line.line).starts_with('b'))
        .expect("`< b` has its own line at width 3");
    assert_eq!(b_line.source, Some(char_at(src, "&lt;", 0)));
    let sources: Vec<usize> =
        mapped.iter().filter_map(|line| line.source).collect();
    assert!(
        sources.windows(2).all(|pair| pair[0] <= pair[1]),
        "{sources:?}"
    );
}

#[test]
fn a_smart_quote_never_pulls_the_mapping_into_a_later_block() {
    let mut smart = sheet();
    smart.smart_punctuation = true;
    let src = "\"Zitat\" here\n\nlater \u{201c}echt\u{201d}";
    let mapped = render_block_mapped(src, 40, &smart);
    assert_eq!(mapped[0].source, Some(0));
    assert_eq!(mapped[2].source, Some(char_at(src, "later", 0)));
}

#[test]
fn a_list_after_a_table_maps_to_itself() {
    let src = "| a | b |\n|---|---|\n| c | d |\n\n-\n- x";
    let mapped = render_block_mapped(src, 30, &sheet());
    let x_line = mapped
        .iter()
        .find(|line| shown_text(&line.line).starts_with('x'))
        .expect("the x item");
    assert_eq!(x_line.source, Some(char_at(src, "- x", 2)));
    let table_end = char_at(src, "| c | d |", 9);
    for line in &mapped {
        let is_after_table = shown_text(&line.line).is_empty()
            && !line.line.spans.is_empty()
            && line.source.is_some_and(|start| start < table_end);
        assert!(!is_after_table, "an empty item maps into the table");
    }
}

#[test]
fn an_item_holding_only_a_list_renders_as_before() {
    let lines = render_block("- - b", 40, &sheet());
    assert_eq!(texts(&lines), vec!["  \u{25cb} b".to_string()]);
}
