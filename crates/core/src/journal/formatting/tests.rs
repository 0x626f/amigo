use super::*;
fn whole(text: &mut String, kind: Format) -> Range<usize> {
    apply(text, 0..text.chars().count(), kind)
}
#[test]
fn empty_lines_accept_block_styles_and_heading_markers_follow_markdown_rules() {
    for (kind, expected) in [
        (Format::Heading(2), "## "),
        (Format::Bullet, "- "),
        (Format::Numbered, "1. "),
        (Format::Task, "- [ ] "),
        (Format::Quote, "> "),
    ] {
        let mut text = String::new();
        whole(&mut text, kind);
        assert_eq!(text, expected);
    }
    let mut text = "### text ###\n#hashtag\n####### literal".to_owned();
    whole(&mut text, Format::Heading(0));
    assert_eq!(text, "text\n#hashtag\n####### literal");
    let mut text = "first\nsecond".to_owned();
    apply(&mut text, 0..6, Format::Bullet);
    assert_eq!(text, "- first\nsecond");
}
#[test]
fn syntax_aware_transforms_keep_valid_unicode_selection_bounds() {
    for source in [
        "***🌍 é***",
        "~🌍~",
        "**a __é__ b**",
        "`🌍`",
        "# 🌍\r\n- é",
        "```js\n🌍\n```",
    ] {
        let length = source.chars().count();
        for a in 0..=length {
            for b in a..=length {
                for kind in [
                    Format::Bold,
                    Format::Italic,
                    Format::Strike,
                    Format::Code,
                    Format::Heading(2),
                    Format::Task,
                    Format::Quote,
                    Format::Link,
                ] {
                    let mut text = source.to_owned();
                    let selection = apply(&mut text, b..a, kind);
                    assert!(
                        selection.start <= selection.end && selection.end <= text.chars().count(),
                        "{source:?} {a}..{b} {kind:?} -> {text:?} {selection:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn headings_replace_prefixes_and_preserve_fenced_code_instead_of_formatting_its_lines() {
    let code = "```javascript\nimport { useState } from \"react\"\n\nexport default () => {}\n```";
    let mut text = format!("###### **testing**\n\n{code}\n\nTail");
    whole(&mut text, Format::Heading(3));
    assert_eq!(text, format!("### **testing**\n\n{code}\n\n### Tail"));
    let previous = text.clone();
    whole(&mut text, Format::Heading(3));
    assert_eq!(text, previous, "choosing a heading twice is idempotent");
    whole(&mut text, Format::Heading(0));
    assert_eq!(text, format!("**testing**\n\n{code}\n\nTail"));
    for format in [
        Format::Heading(2),
        Format::Bullet,
        Format::Quote,
        Format::Bold,
        Format::Code,
        Format::Link,
    ] {
        let mut text = code.to_owned();
        whole(&mut text, format);
        assert_eq!(text, code, "{format:?} must leave fenced code intact");
    }
}
#[test]
fn list_styles_toggle_and_convert_existing_items_without_stacking_prefixes() {
    for newline in ["\n", "\r\n"] {
        let mut text = format!("- One{newline}  + Two{newline}{newline}Three");
        let range = whole(&mut text, Format::Task);
        assert_eq!(
            text,
            format!("- [ ] One{newline}  - [ ] Two{newline}{newline}- [ ] Three")
        );
        let state = SelectionState::new(&text, range.clone());
        assert!(state.is_active(Format::Task));
        apply(&mut text, range, Format::Task);
        assert_eq!(text, format!("One{newline}  Two{newline}{newline}Three"));
        whole(&mut text, Format::Numbered);
        assert_eq!(
            text,
            format!("1. One{newline}  2. Two{newline}{newline}3. Three")
        );
        whole(&mut text, Format::Bullet);
        assert_eq!(
            text,
            format!("- One{newline}  - Two{newline}{newline}- Three")
        );
    }
    let mut text = "> - [x] Done\n> New".to_owned();
    whole(&mut text, Format::Task);
    assert_eq!(text, "> - [x] Done\n> - [ ] New");
    whole(&mut text, Format::Quote);
    assert_eq!(text, "- [x] Done\n- [ ] New");
    whole(&mut text, Format::Quote);
    assert_eq!(text, "> - [x] Done\n> - [ ] New");
    whole(&mut text, Format::Heading(2));
    assert_eq!(text, "> ## Done\n> ## New");
}
#[test]
fn inline_actions_recognize_existing_markers_nested_styles_and_mixed_selections() {
    for (source, kind, expected) in [
        ("**hello**", Format::Bold, "hello"),
        ("__hello__", Format::Bold, "hello"),
        ("*hello*", Format::Italic, "hello"),
        ("_hello_", Format::Italic, "hello"),
        ("~~hello~~", Format::Strike, "hello"),
        ("***hello***", Format::Bold, "*hello*"),
        ("***hello***", Format::Italic, "**hello**"),
        ("**one** two", Format::Bold, "**one two**"),
        ("**one** **two**", Format::Bold, "one two"),
        ("**one _two_**", Format::Bold, "one _two_"),
        ("`hello`", Format::Code, "hello"),
        ("`` `tick` ``", Format::Code, "`tick`"),
    ] {
        let mut text = source.to_owned();
        let selection = whole(&mut text, kind);
        assert_eq!(text, expected, "{kind:?} on {source}");
        assert!(selection.end <= text.chars().count());
    }
    let mut text = "before **🌍 hello** after".to_owned();
    let range = apply(&mut text, 9..10, Format::Bold);
    assert_eq!(text, "before 🌍 **hello** after");
    assert_eq!(selected_text(&text, range), "🌍");
    let mut text = "## heading".to_owned();
    whole(&mut text, Format::Bold);
    assert_eq!(text, "## **heading**");
    let mut text = "one\n\n- two\n\n```js\nthree\n```".to_owned();
    whole(&mut text, Format::Bold);
    assert_eq!(text, "**one**\n\n- **two**\n\n```js\nthree\n```");
    let mut text = "**one\ntwo**".to_owned();
    whole(&mut text, Format::Bold);
    assert_eq!(text, "one\ntwo");
    let mut text = "## **one**\n\n- two".to_owned();
    let range = whole(&mut text, Format::Bold);
    assert_eq!(text, "## **one**\n\n- **two**");
    assert!(SelectionState::new(&text, range.clone()).is_active(Format::Bold));
    apply(&mut text, range, Format::Bold);
    assert_eq!(text, "## one\n\n- two");
}
#[test]
fn formatting_state_respects_actual_markdown_and_existing_links_edit_the_destination() {
    for (text, active, inactive) in [
        ("***hello***", Format::Bold, Format::Strike),
        ("__hello__", Format::Bold, Format::Italic),
        ("_hello_", Format::Italic, Format::Bold),
        ("~~hello~~", Format::Strike, Format::Code),
        ("`**hello**`", Format::Code, Format::Bold),
        (r"\*\*hello\*\*", Format::Heading(0), Format::Bold),
        ("## hello", Format::Heading(2), Format::Heading(0)),
        ("#hashtag", Format::Heading(0), Format::Heading(1)),
    ] {
        let state = SelectionState::new(text, 0..text.chars().count());
        assert!(state.is_active(active), "{text}: {active:?}");
        assert!(!state.is_active(inactive), "{text}: {inactive:?}");
    }
    let text = "```javascript\nconst x = 1;\n```";
    let state = SelectionState::new(text, 18..20);
    assert!(state.literal);
    assert!(!state.can_apply(Format::Bold));
    assert_eq!(state.language.as_deref(), Some("javascript"));
    let mut text = "[🌍 site](https://example.com)".to_owned();
    for selection in [0..29, 1..5, 12..16] {
        let original = text.clone();
        let range = apply(&mut text, selection, Format::Link);
        assert_eq!(text, original);
        assert_eq!(selected_text(&text, range), "https://example.com");
    }
}
fn selected_text(text: &str, range: Range<usize>) -> String {
    text.chars().skip(range.start).take(range.len()).collect()
}
#[test]
fn code_blocks_keep_unicode_and_separate_fences_from_surrounding_prose() {
    let mut text = "Before 🌍 after".to_owned();
    let range = apply(&mut text, 7..8, Format::CodeBlock("python"));
    assert_eq!(text, "Before \n```python\n🌍\n```\n after");
    assert_eq!(selected_text(&text, range.clone()), "🌍");
    assert_eq!(code_block_language(&text, range), Some("python"));
    let mut text = String::new();
    let range = apply(&mut text, 0..0, Format::CodeBlock("rust"));
    assert_eq!(text, "```rust\ncode\n```");
    assert_eq!(selected_text(&text, range), "code");
}
#[test]
fn changing_language_reuses_fences_and_preserves_code_and_line_endings() {
    for newline in ["\n", "\r\n"] {
        for marker in ["```", "~~~~"] {
            let mut text = format!(
                "Before{newline}  {marker}rust{newline}🌍 let n = 1;{newline}  {marker}{newline}After"
            );
            let body = text[..text.find('🌍').unwrap()].chars().count();
            let range = apply(&mut text, body..body + 1, Format::CodeBlock("python"));
            assert_eq!(
                text,
                format!(
                    "Before{newline}  {marker}python{newline}🌍 let n = 1;{newline}  {marker}{newline}After"
                )
            );
            assert_eq!(selected_text(&text, range.clone()), "🌍 let n = 1;");
            apply(&mut text, range, Format::CodeBlock(""));
            assert!(text.contains(&format!("  {marker}{newline}🌍")));
        }
    }
    let mut text = "```rust\nlet n = 1;".to_owned();
    apply(&mut text, 10..10, Format::CodeBlock("javascript"));
    assert_eq!(text, "```javascript\nlet n = 1;\n```");
}
#[test]
fn fences_outgrow_embedded_backticks_and_whole_block_selection_updates_language() {
    let mut text = "a ````` b\n``` c\n".to_owned();
    let end = text.chars().count();
    let range = apply(&mut text, 0..end, Format::CodeBlock("markdown"));
    assert_eq!(text, "``````markdown\na ````` b\n``` c\n``````");
    assert_eq!(selected_text(&text, range), "a ````` b\n``` c\n");
    let end = text.chars().count();
    apply(&mut text, end..0, Format::CodeBlock("text"));
    assert_eq!(text, "``````text\na ````` b\n``` c\n``````");
    let mut text = "```rust\nlet n = 1;\n```\n".to_owned();
    let end = text.chars().count();
    apply(&mut text, 0..end, Format::CodeBlock("python"));
    assert_eq!(text, "```python\nlet n = 1;\n```\n");
}
#[test]
fn inline_code_escapes_backticks_and_toggles_without_losing_spaces() {
    for body in ["🌍", "a `` b", " leading and trailing ", "   "] {
        let mut text = body.to_owned();
        let range = apply(&mut text, 0..body.chars().count(), Format::Code);
        assert_eq!(selected_text(&text, range.clone()), body);
        apply(&mut text, range, Format::Code);
        assert_eq!(text, body);
    }
}
#[test]
fn selection_formatting_preserves_unicode_and_toggles_without_losing_text() {
    let mut text = "Hi 🌍 Україна!".to_owned();
    let selected = apply(&mut text, 3..12, Format::Bold);
    assert_eq!(text, "Hi **🌍 Україна**!");
    apply(&mut text, selected, Format::Bold);
    assert_eq!(text, "Hi 🌍 Україна!");
}
#[test]
fn block_formatting_keeps_unselected_lines_and_links_select_the_url() {
    let mut text = "First\nSecond\nThird".to_owned();
    apply(&mut text, 6..13, Format::Task);
    assert_eq!(text, "First\n- [ ] Second\nThird");
    let mut text = "Journal".into();
    let range = apply(&mut text, 0..7, Format::Link);
    assert_eq!(text, "[Journal](https://)");
    assert_eq!(
        text.chars()
            .skip(range.start)
            .take(range.len())
            .collect::<String>(),
        "https://"
    );
}
