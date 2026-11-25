use super::*;
use crate::fs::testutil::temp_dir;
use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::Rect;
use crate::viewer::Viewer;
use std::fs;

#[test]
fn long_code_lines_wrap_inside_the_padded_block() {
    let d = temp_dir("markdown-code-wrap");
    let path = d.path().join("guide.md");
    fs::write(&path, format!("```js\n{}\n```\n", "x".repeat(120))).expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.bg_elevated = (10, 20, 30);
    theme.fg = (255, 255, 255);
    let mut canvas = Canvas::new(240, 300);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut state = MarkdownState::default();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 240.0,
        h: 300.0,
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

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 216, 0, 240, 300),
        0,
        "code ink must stay inside the centered block"
    );
    assert!(
        drawn.content_h > 70.0,
        "the long line wraps onto multiple rows"
    );
}

#[test]
fn code_wraps_inside_very_narrow_viewports() {
    let d = temp_dir("markdown-code-narrow");
    let path = d.path().join("guide.md");
    fs::write(&path, "```\nabcdefghijklmnopqrstuvwxyz\n```\n").expect("write");
    let viewer = Viewer::open(&path).expect("viewer");
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.bg_elevated = (10, 20, 30);
    theme.fg = (255, 255, 255);
    let mut canvas = Canvas::new(60, 300);
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
            w: 60.0,
            h: 300.0,
        },
        &viewer,
        &mut state,
    );

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg, 36, 0, 60, 300),
        0,
        "narrow code blocks reduce padding instead of overflowing"
    );
}
