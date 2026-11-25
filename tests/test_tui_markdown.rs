use lontar::tui::{parse_inline_spans, render_assistant_markdown};
use ratatui::style::{Color, Modifier, Style};

#[test]
fn test_parse_inline_spans_bold_and_code() {
    let input = "**Core LLM** · `SentenceTransformer` & `OpenAI GPT-4` (fallback)";
    let spans = parse_inline_spans(input, Style::default(), Color::Cyan);

    // Ensure raw asterisks and backticks are concealed
    for span in &spans {
        assert!(!span.content.contains("**"), "Raw ** should be stripped");
        assert!(!span.content.contains('`'), "Raw ` should be stripped");
    }

    // Verify bold span exists
    let bold_span = spans.iter().find(|s| s.content == "Core LLM");
    assert!(bold_span.is_some(), "Core LLM should be parsed as a distinct span");
    assert!(bold_span.unwrap().style.add_modifier.contains(Modifier::BOLD));

    // Verify code span exists
    let code_span = spans.iter().find(|s| s.content == "SentenceTransformer");
    assert!(code_span.is_some(), "SentenceTransformer should be parsed as a code span");
}

#[test]
fn test_render_assistant_markdown_table() {
    let markdown_table = r#"### Model yang saya gunakan

| Komponen | Teknologi | Deskripsi Singkat |
|---|---|---|
| **Core LLM** | `SentenceTransformer` | Model dasar pemahaman teks. |
| **Tokenization** | `tiktoken` | Menghitung token model. |
"#;

    let lines = render_assistant_markdown(markdown_table, Color::Cyan, 80);
    assert!(!lines.is_empty());

    // Headings must strip ###
    let heading_line = lines.iter().find(|line| {
        line.spans.iter().any(|s| s.content.contains("Model yang saya gunakan"))
    });
    assert!(heading_line.is_some());
    for span in &heading_line.unwrap().spans {
        assert!(!span.content.contains("###"), "Heading should not contain raw ###");
    }

    // Table should render with Unicode box borders
    let has_top_border = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains('┌') && s.content.contains('┬'))
    });
    assert!(has_top_border, "Table should have a clean Unicode top border");

    let has_separator = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains('├') && s.content.contains('┼'))
    });
    assert!(has_separator, "Table should have a clean Unicode header separator");

    let has_bottom_border = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains('└') && s.content.contains('┴'))
    });
    assert!(has_bottom_border, "Table should have a clean Unicode bottom border");
}

#[test]
fn test_render_assistant_markdown_lists_and_dividers() {
    let markdown = r#"
---
- First bullet item with **bold** text
- Second bullet item
1. Step one
2. Step two
"#;

    let lines = render_assistant_markdown(markdown, Color::Cyan, 80);

    // Divider check (─ instead of raw ---)
    let has_divider = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains("────") && !s.content.contains("---"))
    });
    assert!(has_divider, "Divider should be rendered as a horizontal line");

    // Bullet items check
    let has_bullet = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains("• "))
    });
    assert!(has_bullet, "List item should render with a bullet point");

    // Numbered item check
    let has_num = lines.iter().any(|line| {
        line.spans.iter().any(|s| s.content.contains("1. "))
    });
    assert!(has_num, "Numbered list should render with number prefix");
}
