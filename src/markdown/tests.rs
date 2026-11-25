use super::*;
use crate::fs::testutil::temp_dir;
use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::Rect;
use crate::viewer::Viewer;
use std::fs;

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
    let mut state = MarkdownState::default();
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 580.0,
        h: 280.0,
    };

    let _ = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        rect,
        &viewer,
        &mut state,
    );

    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg, 10, 10, 590, 60) > 50,
        "the H1 paints prominent title ink"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.bg_elevated, 10, 105, 590, 150) > 20,
        "inline code paints its raised background"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.bg_elevated, 10, 150, 590, 230) > 500,
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
    let mut state = MarkdownState::default();
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 580.0,
        h: 280.0,
    };

    let first = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        rect,
        &viewer,
        &mut state,
    );
    assert!(crate::ui::testutil::count_color_in(&canvas, theme.fg, 10, 10, 590, 60) > 0);
    state.scroll = first.content_h;
    let _ = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        rect,
        &viewer,
        &mut state,
    );

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 10, 10, 590, 60),
        0,
        "scrolling to trailing blank space must clear the old heading"
    );
}

#[test]
fn markdown_content_uses_a_centered_maximum_width_column() {
    let d = temp_dir("markdown-centered");
    let path = d.path().join("guide.md");
    fs::write(
        &path,
        "# Title\n\nA paragraph that should stay in the centered reading column.\n",
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    let mut canvas = Canvas::new(1200, 400);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState::default();

    let _ = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1200.0,
            h: 400.0,
        },
        &viewer,
        &mut state,
    );

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 0, 0, 160, 400),
        0,
        "wide previews keep a clear left margin"
    );
    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 1040, 0, 1200, 400),
        0,
        "wide previews keep a clear right margin"
    );
}

#[test]
fn markdown_h1_paints_a_separator_rule() {
    let d = temp_dir("markdown-heading-rule");
    let path = d.path().join("guide.md");
    fs::write(&path, "# Title\n\nBody\n").expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.border = (0, 255, 0);
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState::default();

    let _ = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 580.0,
            h: 280.0,
        },
        &viewer,
        &mut state,
    );

    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.border, 10, 10, 590, 140) > 100,
        "H1 has the VS Code-style bottom rule"
    );
}

#[test]
fn markdown_task_items_paint_checkbox_boundaries_and_checks() {
    let d = temp_dir("markdown-tasks");
    let path = d.path().join("guide.md");
    fs::write(&path, "- [x] done\n- [ ] todo\n").expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.border = (0, 255, 0);
    theme.accent = (0, 0, 255);
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState::default();

    let _ = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 580.0,
            h: 280.0,
        },
        &viewer,
        &mut state,
    );

    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.border, 10, 10, 590, 160) > 20,
        "task checkboxes have visible boundaries"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.accent, 10, 10, 590, 160) > 5,
        "completed tasks paint a check"
    );
}

#[test]
fn markdown_table_rows_grow_for_wrapped_cells() {
    let d = temp_dir("markdown-table-wrap");
    let path = d.path().join("guide.md");
    fs::write(
        &path,
        "| Column | Value |\n| --- | --- |\n| a very long first cell | another very long second cell |\n",
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let theme = Theme::DARK_PLUS;
    let mut canvas = Canvas::new(320, 400);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState::default();

    let drawn = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 320.0,
            h: 400.0,
        },
        &viewer,
        &mut state,
    );

    assert!(
        drawn.content_h > 140.0,
        "wrapped cells need multi-line row height"
    );
}

#[test]
fn streamed_render_clamps_scroll_and_paints_only_the_viewport() {
    let d = temp_dir("readme-render");
    let path = d.path().join("README.md");
    fs::write(
        &path,
        format!("# Title\n\n{}\n", "paragraph line\n\n".repeat(80)),
    )
    .expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let theme = Theme::DARK_PLUS;
    let mut canvas = Canvas::new(600, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState {
        preview: true,
        scroll: 10_000.0,
    };
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 580.0,
        h: 280.0,
    };

    let drawn = draw_markdown_preview(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        rect,
        &viewer,
        &mut state,
    );

    assert!(drawn.content_h > rect.h);
    assert!(state.scroll <= drawn.content_h - rect.h);
    assert!(!drawn.truncated);
}
