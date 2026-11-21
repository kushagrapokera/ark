//! Viewer panel pixel pins (chrome-invisible theme technique) and pointer
//! interaction routing.

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use crate::ui::testutil::{count_color_in, ink_bbox_in};
use std::fs;
use std::io::Write;

const PANEL: Rect = Rect { x: 10.0, y: 10.0, w: 300.0, h: 57.0 }; // ~3 lines

/// Theme with the tokens under pin distinct; the rest sit on bg.
fn panel_theme() -> Theme {
    let bg = (0, 0, 0);
    Theme {
        bg,
        bg_elevated: (10, 20, 30),
        fg: (255, 255, 255),
        fg_dim: (90, 90, 90),
        accent: bg,
        border: bg,
        selection: (40, 50, 60),
        syntax_keyword: (203, 166, 247),
        syntax_string: (166, 227, 161),
        syntax_number: bg,
        syntax_comment: bg,
    }
}

/// Fixture: 40 lines of strictly growing length ("a", "aa", …), the last one
/// a string literal so token colors can be pinned on the deepest line.
fn lines_fixture(label: &str) -> (TempDir, PathBuf) {
    let d = temp_dir(label);
    let p = d.path().join("lines.txt");
    let mut f = fs::File::create(&p).expect("create");
    for i in 0..39 {
        writeln!(f, "{}", "a".repeat(i + 1)).expect("write");
    }
    writeln!(f, "let s = \"tok\";").expect("write"); // line 40 (index 39)
    (d, p)
}

fn draw(theme: &Theme, panel: Rect, viewer: &Viewer, canvas: &mut Canvas) -> crate::text::MonoMetrics {
    let mut text = TextSystem::new();
    let font_size = (UI_FONT_SIZE as f64 * 1.0) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    canvas.fill(theme.bg);
    draw_viewer_panel(canvas, &mut text, theme, 1.0, panel, viewer, &Highlighter::new());
    m
}

// ---- viewport-only rendering ----

#[test]
fn viewport_only_rendering() {
    let (_d, p) = lines_fixture("viewport");
    let mut v = Viewer::open(&p).expect("opens");
    assert_eq!(v.line_count(), 40);
    let theme = panel_theme();
    let mut canvas = Canvas::new(320, 80);

    draw(&theme, PANEL, &v, &mut canvas);
    // Ink confined to the 3 visible line bands [10, 10 + 3×18.9 ≈ 67).
    let bbox = ink_bbox_in(&canvas, theme.bg, 0, 0, 320, 80).expect("viewer ink");
    assert!(bbox.1 >= 10 && bbox.3 < 68, "ink confined to the visible bands, got {bbox:?}");
    // Band 3 [10 + 2×18.9, 10 + 3×18.9) shows line 3 = "aaa" (3 chars + gutter).
    let (_, short) = band_ink(&canvas, theme.bg, 47, 67);

    // Scroll 10 lines: band 3 shows line 13 = 13 a's → far wider ink.
    assert!(v.scroll_by_lines(10, 3));
    draw(&theme, PANEL, &v, &mut canvas);
    let bbox = ink_bbox_in(&canvas, theme.bg, 0, 0, 320, 80).expect("scrolled ink");
    assert!(bbox.1 >= 10 && bbox.3 < 68, "scrolled ink still viewport-only");
    let (_, long) = band_ink(&canvas, theme.bg, 47, 67);
    assert!(long > short + 20, "band 3 shows the longer line after scrolling ({short} → {long})");
}

/// Ink extent (min_x, max_x) within a horizontal pixel band.
fn band_ink(canvas: &Canvas, bg: crate::render::ColorRgb, y0: u32, y1: u32) -> (u32, u32) {
    let bbox = ink_bbox_in(canvas, bg, 0, y0, canvas.width(), y1).expect("band must contain ink");
    (bbox.0, bbox.2)
}

// ---- gutter ----

#[test]
fn gutter_renders_numbers_and_reserved_mark_strip() {
    let (_d, p) = lines_fixture("gutter");
    let v = Viewer::open(&p).expect("opens");
    let theme = panel_theme();
    let mut canvas = Canvas::new(320, 80);
    let m = draw(&theme, PANEL, &v, &mut canvas);
    let advance = f64::from(m.advance);

    // Line-number ink (fg_dim) inside the gutter, right of the mark strip.
    let gutter_w = gutter_width(v.line_count(), advance);
    assert!(
        count_color_in(&canvas, theme.fg_dim, 14, 10, 10 + gutter_w as u32, 29) > 0,
        "line-number ink in the gutter"
    );
    // The reserved mark strip [panel.x, +MARK_W) is painted bg_elevated and
    // carries no text ink (M7 paints git marks into it).
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 10, 10, 14, 67) >= 4 * 57 - 4,
        "mark strip painted across the panel height"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 10, 10, 14, 67) == 0,
        "no line-number ink inside the reserved strip"
    );
    // Numbers are right-aligned: row-0 ink inside the gutter ends within
    // ~2px of text_edge − GUT_PAD_R (band limited to row 0 and the gutter
    // x-range so text ink never leaks into the pin).
    let bbox = ink_bbox_in(&canvas, theme.bg, 14, 10, (10.0 + gutter_w) as u32, 28)
        .expect("row-0 gutter ink");
    let text_edge = 10.0 + gutter_w;
    assert!(
        (text_edge - GUT_PAD_R - f64::from(bbox.2)).abs() <= 3.0,
        "numbers right-aligned against the text edge ({} vs {})",
        bbox.2,
        text_edge - GUT_PAD_R
    );
}

// ---- token colors + selection ----

#[test]
fn token_colors_and_selection_render() {
    let (_d, p) = lines_fixture("tokens");
    let mut v = Viewer::open(&p).expect("opens");
    let theme = panel_theme();
    let mut canvas = Canvas::new(320, 80);
    let m = draw(&theme, PANEL, &v, &mut canvas);
    let advance = f64::from(m.advance);

    // Scroll to the last line (index 39: `let s = "tok";`) — gutter digits
    // grow to two chars; scroll to its top band.
    v.set_offset_lines(37, 3);
    draw(&theme, PANEL, &v, &mut canvas);
    // Line 39 (third visible row, band [47, 67)) has the string "tok":
    let gutter_w = gutter_width(v.line_count(), advance);
    let text_x = 10.0 + gutter_w;
    assert!(
        count_color_in(&canvas, theme.syntax_string, text_x as u32, 47, 320, 67) > 0,
        "string literal paints the syntax_string color"
    );
    assert!(
        count_color_in(&canvas, theme.syntax_keyword, text_x as u32, 47, 320, 67) > 0,
        "`let` paints the syntax_keyword color"
    );

    // Selection: select chars 2..5 on the FIRST visible line (index 37 =
    // "aaaa…"): selection bg paints over exactly 3 advances.
    v.select_at(37, 2);
    v.extend_select(37, 5);
    draw(&theme, PANEL, &v, &mut canvas);
    let sel_x0 = (text_x + 2.0 * advance) as u32;
    let sel_x1 = (text_x + 5.0 * advance) as u32;
    assert!(
        count_color_in(&canvas, theme.selection, sel_x0, 10, sel_x1, 29) > 50,
        "selection background paints over the selected span"
    );
    assert!(
        count_color_in(&canvas, theme.selection, sel_x1, 10, 320, 29) == 0,
        "nothing beyond the span end"
    );
}

// ---- pointer interaction ----

#[test]
fn handle_pointer_drags_selection() {
    let (_d, p) = lines_fixture("drag");
    let mut v = Viewer::open(&p).expect("opens");
    let mut text = TextSystem::new();
    let font_size = UI_FONT_SIZE;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance);
    let gutter_w = gutter_width(v.line_count(), advance);
    let text_x = PANEL.x + gutter_w;

    // Press at line 0 col 1, drag to line 2 col 3, release.
    let down = Pointer { x: text_x + 1.5 * advance, y: PANEL.y + 5.0, down: true, click: false };
    assert!(handle_pointer(&mut v, PANEL, down, advance), "press starts a selection");
    let drag = Pointer { x: text_x + 3.5 * advance, y: PANEL.y + 2.0 * LINE_H + 5.0, down: true, click: false };
    assert!(handle_pointer(&mut v, PANEL, drag, advance), "drag extends");
    let up = Pointer { x: drag.x, y: drag.y, down: false, click: true };
    assert!(!handle_pointer(&mut v, PANEL, up, advance), "release changes nothing");
    assert_eq!(v.selection(), Some(((0, 1), (2, 3))), "drag spans three lines");

    // A fresh press without movement leaves a zero-length selection.
    let click = Pointer { x: text_x + 2.5 * advance, y: PANEL.y + 5.0, down: true, click: false };
    handle_pointer(&mut v, PANEL, click, advance);
    let up2 = Pointer { x: click.x, y: click.y, down: false, click: true };
    handle_pointer(&mut v, PANEL, up2, advance);
    assert_eq!(v.selection(), None, "click without drag → zero-length → None");

    // Pointer outside the panel never starts a selection.
    let outside = Pointer { x: 500.0, y: PANEL.y + 5.0, down: true, click: false };
    assert!(!handle_pointer(&mut v, PANEL, outside, advance));
}

// ---- degenerate frames ----

#[test]
fn panel_draw_degenerates_safely() {
    let (_d, p) = lines_fixture("degenerate");
    let v = Viewer::open(&p).expect("opens");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(320, 80);
    canvas.fill(theme.bg);

    for panel in [
        Rect { x: 10.0, y: 10.0, w: 0.0, h: 57.0 },
        Rect { x: 10.0, y: 10.0, w: 300.0, h: 0.0 },
        Rect { x: 10.0, y: 10.0, w: 300.0, h: -5.0 },
    ] {
        draw_viewer_panel(&mut canvas, &mut text, &theme, 1.0, panel, &v, &Highlighter::new());
    }
    draw_viewer_panel(&mut canvas, &mut text, &theme, 0.0, PANEL, &v, &Highlighter::new());
    draw_viewer_panel(&mut canvas, &mut text, &theme, f64::NAN, PANEL, &v, &Highlighter::new());
    assert_eq!(
        count_color_in(&canvas, theme.bg, 0, 0, 320, 80),
        320 * 80,
        "degenerate frames rasterize nothing"
    );

    // Empty file renders the dim "(empty file)" note instead of a grid.
    let d = temp_dir("empty-pane");
    let e = d.path().join("empty.txt");
    fs::File::create(&e).expect("create");
    let empty = Viewer::open(&e).expect("opens");
    draw(&theme, PANEL, &empty, &mut canvas);
    assert!(
        count_color_in(&canvas, theme.fg_dim, 10, 10, 310, 67) > 0,
        "(empty file) note ink"
    );
}
