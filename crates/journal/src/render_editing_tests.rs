use super::*;
fn change(inline: &mut Inline, value: &str) {
    let units: Vec<_> = inline
        .atoms
        .iter()
        .map(|a| crate::editing::Unit {
            text: a.text.clone(),
            span: a.span.clone(),
            code: a.code,
            wrappers: a.wrappers.clone(),
        })
        .collect();
    for (a, text) in inline
        .atoms
        .iter_mut()
        .zip(crate::editing::values(&units, value))
    {
        a.text = text;
    }
    inline.value = value.into();
}
fn text(preview: &mut Preview, index: usize, value: &str) {
    let (Kind::Text(_, inline) | Kind::Media(_, inline)) = &mut preview.blocks[index].kind else {
        panic!()
    };
    change(inline, value);
    preview.changed = true;
}
#[test]
fn edits_preserve_front_matter_unicode_styles_links_and_multiple_separate_changes() {
    let source = "\u{feff}---\r\nprivate: Keep me\r\n---\r\n# Heading\r\n\r\nHello **bold** and _italic_ and [link](Note.md) and [[Target]].\r\n";
    let mut p = Preview::document(source);
    text(&mut p, 0, "Renamed heading");
    text(&mut p, 1, "Hello brave🌍 and italic and link and Target.");
    text(
        &mut p,
        1,
        "Hello brave🌍 and italic and renamed and Target.",
    );
    text(
        &mut p,
        1,
        "Hello brave🌍 and italic and renamed and Display.",
    );
    let edited = p.edited_document().unwrap();
    assert_eq!(
        edited,
        "\u{feff}---\r\nprivate: Keep me\r\n---\r\n# Renamed heading\r\n\r\nHello **brave🌍** and _italic_ and [renamed](Note.md) and [[Target|Display]].\r\n"
    );
}
#[test]
fn deleting_across_styles_removes_empty_markers_and_preserves_surviving_styles() {
    for (source, value, expected) in [
        (
            "one **bold** two _italic_ three",
            "one x three",
            "one **x** three",
        ),
        ("**bold**", "bold ", "**bold** "),
        ("**bold**", "First\n\nSecond", "**First**\n\n**Second**"),
        ("**bold**", "", ""),
        (
            "![Picture](photo.png)",
            "New description",
            "![New description](photo.png)",
        ),
        ("plain", "&amp;", "\\&amp;"),
        ("[link](Note.md)", "", ""),
        ("`code`", "a`b", "``a`b``"),
        (
            "a &amp; \\*literal\\*",
            "a & *edited*",
            "a &amp; \\*edited\\*",
        ),
        (
            "[[Target|Display]]",
            "New display",
            "[[Target|New display]]",
        ),
    ] {
        let mut p = Preview::document(source);
        text(&mut p, 0, value);
        assert_eq!(p.edited_document().unwrap(), expected, "{source}");
    }
}
#[test]
fn table_cells_checkboxes_and_fenced_code_edit_only_their_own_content() {
    let source = "- [ ] Task\n\n```rust\nlet n = 1;\n```\n\n| Name | Value |\n| --- | --- |\n| **old** |  |\n";
    let mut p = Preview::document(source);
    p.blocks[0].task.as_mut().unwrap().2 = true;
    let Kind::Code(_, value) = &mut p.blocks[1].kind else {
        panic!()
    };
    *value = "let n = 2;\n```\n🌍".into();
    let Kind::Table(_, rows) = &mut p.blocks[2].kind else {
        panic!()
    };
    change(&mut rows[1][0], "new");
    change(&mut rows[1][1], "a | b");
    p.changed = true;
    let edited = p.edited_document().unwrap();
    assert!(
        edited.starts_with("- [x] Task\n\n````rust\nlet n = 2;\n```\n🌍\n````\n\n"),
        "{edited:?}"
    );
    assert!(edited.contains("| **new** |a \\| b  |"), "{edited:?}");
    let parsed = Preview::document(&edited);
    assert_eq!(
        parsed
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, Kind::Code(_, _)))
            .count(),
        1
    );
    assert!(matches!(&parsed.blocks[2].kind, Kind::Table(_, rows) if rows[1][1].value == "a | b"));
}
#[test]
fn new_paragraphs_preserve_crlf_and_do_not_rewrite_existing_source() {
    let source = "---\r\nkey: Value\r\n---\r\nExisting **text**.\r\n";
    let mut p = Preview::document(source);
    p.append = "Next 🌍\nAnother line".into();
    p.changed = true;
    assert_eq!(
        p.edited_document().unwrap(),
        format!("{source}\r\nNext 🌍\r\nAnother line")
    );
    let mut empty = Preview::document("");
    empty.append = "First paragraph".into();
    empty.changed = true;
    assert_eq!(empty.edited_document().unwrap(), "First paragraph");
}

#[test]
fn code_edits_preserve_indented_list_and_quote_structure() {
    for source in [
        "    let n = 1;\n\nAfter\n",
        "> ```rust\n> let n = 1;\n> ```\n\nAfter\n",
        "- Example\n\n  ```rust\n  let n = 1;\n  ```\n\nAfter\n",
    ] {
        let mut p = Preview::document(source);
        let code = p
            .blocks
            .iter_mut()
            .find(|b| matches!(b.kind, Kind::Code(_, _)))
            .unwrap();
        let Kind::Code(_, value) = &mut code.kind else {
            unreachable!()
        };
        *value = "let n = 2;\nlet m = 3;".into();
        p.changed = true;
        let edited = p.edited_document().unwrap();
        let parsed = Preview::document(&edited);
        let edited_code = parsed
            .blocks
            .iter()
            .find(|b| matches!(b.kind, Kind::Code(_, _)))
            .unwrap();
        assert!(
            matches!(&edited_code.kind, Kind::Code(_, value) if value == "let n = 2;\nlet m = 3;"),
            "{edited:?}"
        );
        assert_eq!(
            edited_code.quote,
            usize::from(source.starts_with('>')),
            "{edited:?}"
        );
        assert!(edited.ends_with("\nAfter\n"), "{edited:?}");
    }
}

#[test]
fn empty_items_render_without_adding_a_spurious_item_before_a_heading_or_code_block() {
    let preview = Preview::document("- # Heading\n\n- ```rust\n  let n = 1;\n  ```\n\n- \n");
    assert_eq!(preview.blocks.len(), 3);
    assert!(preview.blocks.iter().all(|block| block.marker.is_some()));
    assert!(matches!(&preview.blocks[0].kind, Kind::Text(1, text) if text.value == "Heading"));
    assert!(matches!(&preview.blocks[1].kind, Kind::Code(language, _) if language == "rust"));
    assert!(matches!(&preview.blocks[2].kind, Kind::Text(0, text) if text.value.is_empty()));
}
