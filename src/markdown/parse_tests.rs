use super::parse::{inline_spans, next_block, BlockKind, InlineKind, InlineSpan};
use super::*;
use crate::fs::testutil::temp_dir;
use crate::viewer::Viewer;
use std::fs;
use std::path::Path;

#[test]
fn markdown_detection_accepts_all_markdown_files() {
    for name in [
        "guide.md",
        "docs/API.MD",
        "notes.markdown",
        "notes.mdown",
        "notes.mkd",
    ] {
        assert!(is_markdown(Path::new(name)), "{name}");
    }
}

#[test]
fn legacy_readme_detection_is_case_insensitive_and_name_exact() {
    for name in ["README", "README.md", "readme.MARKDOWN", "Readme.txt"] {
        assert!(is_markdown(Path::new(name)), "{name}");
    }
    for name in ["README.md.bak", "readme-other.md.bak", "src.rs"] {
        assert!(!is_markdown(Path::new(name)), "{name}");
    }
}

#[test]
fn parser_joins_soft_lines_and_strips_task_and_strike_markers() {
    let d = temp_dir("markdown-inline-blocks");
    let path = d.path().join("guide.md");
    fs::write(
        &path,
        "first line\nsecond line\n\n- [x] done\n- [ ] todo\n\n~~gone~~\n",
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");

    let (block, next) = next_block(&viewer, 0);
    let BlockKind::Paragraph(spans) = block else {
        panic!("expected paragraph");
    };
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "first line second line"
    );
    assert_eq!(next, 2);

    for (index, expected) in [(3, "done"), (4, "todo")] {
        let BlockKind::List { spans, .. } = next_block(&viewer, index).0 else {
            panic!("expected list item");
        };
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            expected
        );
    }

    let spans = inline_spans("~~gone~~");
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "gone"
    );
}

#[test]
fn paragraph_joining_respects_line_and_character_caps() {
    let d = temp_dir("markdown-paragraph-cap");
    let path = d.path().join("guide.md");
    fs::write(&path, format!("{}\n", "x".repeat(100)).repeat(70)).expect("write");
    let viewer = Viewer::open(&path).expect("viewer");

    let (block, next) = next_block(&viewer, 0);
    let BlockKind::Paragraph(spans) = block else {
        panic!("paragraph")
    };
    let chars = spans
        .iter()
        .map(|span| span.text.chars().count())
        .sum::<usize>();
    assert!(chars <= 2000, "paragraph text is bounded, got {chars}");
    assert!(
        next > 1 && next <= 20,
        "paragraph line count is bounded, got {next}"
    );
}

#[test]
fn inline_parser_strips_markers_and_marks_styles() {
    let spans = inline_spans("**bold** `code` [link](https://example.com)");
    assert_eq!(
        spans,
        vec![
            InlineSpan {
                text: "bold".into(),
                kind: InlineKind::Bold
            },
            InlineSpan {
                text: " ".into(),
                kind: InlineKind::Normal
            },
            InlineSpan {
                text: "code".into(),
                kind: InlineKind::Code
            },
            InlineSpan {
                text: " ".into(),
                kind: InlineKind::Normal
            },
            InlineSpan {
                text: "link".into(),
                kind: InlineKind::Link
            },
        ]
    );
}

#[test]
fn block_parser_recognizes_headings_lists_quotes_fences_and_tables() {
    let d = temp_dir("readme-blocks");
    let path = d.path().join("README.md");
    fs::write(
        &path,
        "# Title\n\n- item\n\n> quote\n\n```rs\nfn main() {}\n```\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n",
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");

    let (block, next) = next_block(&viewer, 0);
    assert!(matches!(block, BlockKind::Heading { level: 1, .. }));
    assert_eq!(next, 1);
    let (block, _) = next_block(&viewer, 2);
    assert!(matches!(block, BlockKind::List { ordered: None, .. }));
    let (block, _) = next_block(&viewer, 4);
    assert!(matches!(block, BlockKind::Quote(_)));
    let (block, _) = next_block(&viewer, 6);
    assert!(matches!(block, BlockKind::Code(ref lines) if lines.len() == 1));
    let (block, _) = next_block(&viewer, 10);
    assert!(
        matches!(block, BlockKind::Table { headers, rows, .. } if headers.len() == 2 && rows.len() == 1)
    );
}

#[test]
fn table_parser_caps_rows_and_columns_and_reports_truncation() {
    let d = temp_dir("markdown-table-cap");
    let path = d.path().join("guide.md");
    let header = format!(
        "| {} |\n| {} |\n",
        (0..33)
            .map(|i| format!("head{i}"))
            .collect::<Vec<_>>()
            .join(" | "),
        (0..33).map(|_| "---").collect::<Vec<_>>().join(" | "),
    );
    let rows = "| cell |\n".repeat(65);
    fs::write(&path, format!("{header}{rows}")).expect("write");
    let viewer = Viewer::open(&path).expect("viewer");

    let (block, _) = next_block(&viewer, 0);
    let BlockKind::Table {
        headers,
        rows,
        truncated,
    } = block
    else {
        panic!("table")
    };
    assert_eq!(headers.len(), 32);
    assert_eq!(rows.len(), 64);
    assert!(truncated);
}
