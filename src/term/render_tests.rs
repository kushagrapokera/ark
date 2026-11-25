use super::*;
use crate::fs::testutil::temp_dir;
use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::{testutil::count_color_in, Rect};

fn terminal() -> Terminal {
    let d = temp_dir("term-render");
    Terminal::spawn_program_with(
        d.path(),
        Path::new("/bin/cat"),
        "render test",
        GridSize::new(80, 24),
        || {},
    )
    .expect("spawn render terminal")
}

#[test]
fn terminal_redraw_clears_backgrounds_that_the_model_removed() {
    let mut term = terminal();
    term.processor
        .advance(&mut term.term, b"\x1b[41m     \x1b[0m");
    let theme = Theme::DARK_PLUS;
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 400.0,
        h: 100.0,
    };
    let mut canvas = Canvas::new(420, 120);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    term.draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, false);
    assert!(count_color_in(&canvas, ANSI_PALETTE[1], 10, 10, 410, 110) > 100);

    term.processor.advance(&mut term.term, b"\x1b[2J\x1b[H");
    term.draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, false);
    assert_eq!(
        count_color_in(&canvas, ANSI_PALETTE[1], 10, 10, 410, 110),
        0,
        "default-background cells must erase stale colored rows"
    );
}

#[test]
fn terminal_caret_hides_during_the_blink_off_phase() {
    let mut term = terminal();
    term.processor.advance(&mut term.term, b"abc");
    let theme = Theme::DARK_PLUS;
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 400.0,
        h: 100.0,
    };
    let mut canvas = Canvas::new(420, 120);
    let mut text = TextSystem::new();
    term.draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, true);
    assert!(count_color_in(&canvas, theme.accent, 10, 10, 410, 110) > 20);

    term.set_caret_visible(false);
    canvas.fill(theme.bg);
    term.draw(&mut canvas, &mut text, &theme, 1.0, 1.0, rect, true);
    assert_eq!(
        count_color_in(&canvas, theme.accent, 10, 10, 410, 110),
        0,
        "blink-off hides only the terminal caret"
    );
}

#[test]
fn terminal_text_uses_the_same_ui_scale_as_the_cursor_grid() {
    let mut term = terminal();
    term.processor.advance(&mut term.term, b"abc");
    let theme = Theme::DARK_PLUS;
    let rect = Rect {
        x: 10.0,
        y: 10.0,
        w: 400.0,
        h: 100.0,
    };
    let mut canvas = Canvas::new(420, 120);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let font_size = (UI_FONT_SIZE as f64 * 1.25) as f32;
    let metrics = text.mono_metrics(font_size, font_size * 1.35);
    let mut normal = Canvas::new(420, 120);
    normal.fill(theme.bg);
    term.draw(&mut normal, &mut text, &theme, 1.0, 1.0, rect, false);
    let normal_ink = crate::ui::testutil::ink_bbox_in(
        &normal,
        theme.bg,
        rect.x as u32,
        rect.y as u32,
        (rect.x + 100.0) as u32,
        (rect.y + TERM_LINE_H) as u32,
    )
    .expect("normal terminal text ink");
    term.draw(&mut canvas, &mut text, &theme, 1.0, 1.25, rect, false);

    let ink = crate::ui::testutil::ink_bbox_in(
        &canvas,
        theme.bg,
        rect.x as u32,
        rect.y as u32,
        (rect.x + 100.0) as u32,
        (rect.y + TERM_LINE_H) as u32,
    )
    .expect("terminal text ink");
    assert!(
        ink.2 > normal_ink.2.saturating_add(3),
        "text must grow with ui_scale like the cursor grid; normal={normal_ink:?}, scaled={ink:?}, advance={}",
        metrics.advance
    );
}
