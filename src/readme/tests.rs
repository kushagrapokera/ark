use super::parse::{inline_spans, next_block, BlockKind, InlineKind, InlineSpan};
use super::*;
use crate::fs::testutil::temp_dir;
use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::Rect;
use crate::viewer::Viewer;
use std::fs;
use std::path::Path;

#[test]
fn readme_detection_is_case_insensitive_and_name_exact() {
    for name in ["README", "README.md", "readme.MARKDOWN", "Readme.txt"] {
        assert!(is_readme(Path::new(name)), "{name}");
    }
    for name in ["README.md.bak", "readme-other.md", "plan.md", "src.rs"] {
        assert!(!is_readme(Path::new(name)), "{name}");
    }
}

#[test]
fn inline_parser_strips_markers_and_marks_styles() {
    let spans = inline_spans("**bold** `code` [link](https://example.com)");
    assert_eq!(
        spans,
        vec![
            InlineSpan { text: "bold".into(), kind: InlineKind::Bold },
            InlineSpan { text: " ".into(), kind: InlineKind::Normal },
            InlineSpan { text: "code".into(), kind: InlineKind::Code },
            InlineSpan { text: " ".into(), kind: InlineKind::Normal },
            InlineSpan { text: "link".into(), kind: InlineKind::Link },
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
    assert!(matches!(block, BlockKind::Table { headers, rows } if headers.len() == 2 && rows.len() == 1));
}

#[test]
fn formatted_blocks_paint_heading_inline_code_and_fenced_code_styles() {
    let d = temp_dir("readme-styles");
    let path = d.path().join("README.md");
    fs::write(
        &path,
        "# Title\n\n**bold** and `code`\n\n```rs\nfn main() {}\n```\n",
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.bg_elevated = (10, 20, 30);
    theme.fg = (255, 255, 255);
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = ReadmeState::default();
    let rect = Rect { x: 10.0, y: 10.0, w: 580.0, h: 280.0 };

    let _ = draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, &viewer, &mut state);

    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg, 10, 10, 590, 60) > 50,
        "the H1 paints prominent title ink"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.bg_elevated, 10, 55, 590, 90) > 20,
        "inline code paints its raised background"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.bg_elevated, 10, 95, 590, 150) > 500,
        "fenced code paints a full-width raised block"
    );
}

#[test]
fn scrolling_the_preview_clears_previous_block_ink() {
    let d = temp_dir("readme-scroll-clear");
    let path = d.path().join("README.md");
    fs::write(&path, format!("# Title\n{}", "\n".repeat(200))).expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.fg = (255, 255, 255);
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = ReadmeState::default();
    let rect = Rect { x: 10.0, y: 10.0, w: 580.0, h: 280.0 };

    let first = draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, &viewer, &mut state);
    assert!(crate::ui::testutil::count_color_in(&canvas, theme.fg, 10, 10, 590, 60) > 0);
    state.scroll = first.content_h;
    let _ = draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, &viewer, &mut state);

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 10, 10, 590, 60),
        0,
        "scrolling to trailing blank space must clear the old heading"
    );
}

#[test]
fn streamed_render_clamps_scroll_and_paints_only_the_viewport() {
    let d = temp_dir("readme-render");
    let path = d.path().join("README.md");
    fs::write(&path, format!("# Title\n\n{}\n", "paragraph line\n\n".repeat(80))).expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let theme = Theme::DARK_PLUS;
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = ReadmeState { preview: true, scroll: 10_000.0 };
    let rect = Rect { x: 10.0, y: 10.0, w: 580.0, h: 280.0 };

    let drawn = draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, &viewer, &mut state);

    assert!(drawn.content_h > rect.h);
    assert!(state.scroll <= drawn.content_h - rect.h);
    assert!(!drawn.truncated);
}
