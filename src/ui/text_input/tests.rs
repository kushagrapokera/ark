//! TextInput tests (moved verbatim with the widget from ui.rs during the M3
//! module split — zero assertion edits).

use super::*;
use crate::ui::testutil::{count_color_in, count_not_color, ink_bbox_in};
use crate::ui::TabBar;

// ---- TextInput editing model (acceptance 5) ----

#[test]
fn text_input_editing_model() {
    let mut ti = TextInput::default();
    // Every no-op at the empty-field edges reports `false`.
    assert!(!ti.backspace(), "backspace on empty is a no-op returning false");
    assert!(!ti.handle_key(TextInputKey::Left), "Left at 0 is a no-op returning false");
    assert!(!ti.handle_key(TextInputKey::Right), "Right at end is a no-op returning false");
    assert!(!ti.handle_key(TextInputKey::End), "End at end is a no-op returning false");
    assert!(!ti.handle_key(TextInputKey::Home), "Home at 0 is a no-op returning false");
    assert!(!ti.insert(""), "empty insert is a no-op");
    assert!(!ti.handle_key(TextInputKey::Char('\r')), "control chars never insert");

    assert!(ti.insert("ab"));
    assert_eq!((ti.text.as_str(), ti.cursor), ("ab", 2));
    assert!(ti.handle_key(TextInputKey::Left));
    assert_eq!(ti.cursor, 1);
    assert!(ti.insert("é"), "UTF-8 insert at the caret");
    assert_eq!((ti.text.as_str(), ti.cursor), ("aéb", 3), "cursor lands after 'é' (byte 3)");

    // Cursor moves by CHAR boundaries around 'é' (bytes 1..3).
    assert!(ti.handle_key(TextInputKey::Right));
    assert_eq!(ti.cursor, 4, "Right → end of text");
    assert!(ti.handle_key(TextInputKey::Left));
    assert_eq!(ti.cursor, 3, "boundary after 'é'");
    assert!(ti.handle_key(TextInputKey::Left));
    assert_eq!(ti.cursor, 1, "boundary before 'é' — never inside it");
    assert!(ti.handle_key(TextInputKey::Left));
    assert_eq!(ti.cursor, 0);
    assert!(!ti.handle_key(TextInputKey::Left));
    assert!(ti.handle_key(TextInputKey::End));
    assert_eq!(ti.cursor, 4);
    assert!(!ti.handle_key(TextInputKey::End));
    assert!(ti.handle_key(TextInputKey::Home));
    assert_eq!(ti.cursor, 0);
    assert!(!ti.handle_key(TextInputKey::Home));

    // Backspace removes whole chars — 'é' (2 bytes) is never split.
    assert!(ti.handle_key(TextInputKey::Right));
    assert_eq!(ti.cursor, 1, "boundary before 'é'");
    assert!(ti.handle_key(TextInputKey::Right));
    assert_eq!(ti.cursor, 3, "boundary after 'é'");
    assert!(ti.backspace());
    assert_eq!((ti.text.as_str(), ti.cursor), ("ab", 1), "backspace removed 'é' whole");
    assert!(ti.backspace());
    assert_eq!((ti.text.as_str(), ti.cursor), ("b", 0));
    assert!(!ti.backspace());

    // handle_key routes Char inserts too.
    assert!(ti.handle_key(TextInputKey::Char('x')));
    assert_eq!(ti.text, "xb");
}

#[test]
fn text_input_caps_growth() {
    // Fixed-size state rule: text cannot grow without bound.
    let mut ti = TextInput::default();
    let filler = "y".repeat(TEXT_INPUT_MAX_CHARS);
    assert!(ti.insert(&filler), "fill up to the cap");
    assert_eq!(ti.text.chars().count(), TEXT_INPUT_MAX_CHARS);
    assert!(!ti.insert("z"), "insert beyond the cap is refused");
    assert_eq!(ti.text.chars().count(), TEXT_INPUT_MAX_CHARS);
    assert!(ti.backspace(), "editing still works at the cap");
    assert!(ti.insert("z"), "room again after backspace");
    assert_eq!(ti.text.chars().count(), TEXT_INPUT_MAX_CHARS);

    // M9: caller-chosen caps (config values are longer than UI fields).
    let mut value = TextInput::default();
    assert!(value.insert_with_cap("12345", 5));
    assert!(!value.insert_with_cap("6", 5), "custom cap enforced");
    assert!(value.handle_key_with_cap(TextInputKey::Backspace, 5));
    assert!(value.handle_key_with_cap(TextInputKey::Char('6'), 5));
    assert_eq!(value.text, "12346");
}

// ---- TextInput cursor drawing (acceptance 6) ----

#[test]
fn text_input_draw_cursor_when_focused() {
    let theme = Theme { ..Theme::DARK };
    let mut text = TextSystem::new();
    let rect = Rect { x: 10.0, y: 10.0, w: 200.0, h: 34.0 };
    // Interior bands inset ≥4 physical px from the rect edges so the
    // 1px accent border ring and its AA can never pollute the counts.
    let (top, bottom) = (28u32, 80u32);
    let (inner_l, inner_r) = (26u32, 213u32);

    // Focused with text "ab": accent cursor column at the caret
    // (pad + 2 × advance, in logical px).
    let mut canvas = Canvas::new(240, 100);
    canvas.fill(theme.bg);
    let ti = TextInput { text: "ab".into(), cursor: 2, focused: true };
    ti.draw(&mut canvas, rect, &theme, &mut text, 2.0);
    let m = text.mono_metrics(
        (UI_FONT_SIZE as f64 * 2.0) as f32,
        (UI_FONT_SIZE as f64 * 2.0 * 1.35).round() as f32,
    );
    let advance_logical = f64::from(m.advance) / 2.0;
    let caret_phys = (rect.x + 8.0 + 2.0 * advance_logical) * 2.0;
    let band = count_color_in(
        &canvas,
        theme.accent,
        (caret_phys - 3.0) as u32,
        top,
        (caret_phys + 4.0) as u32,
        bottom,
    );
    assert!(band >= 50, "focused input must paint the accent cursor at caret x, got {band}");
    assert_eq!(
        count_color_in(&canvas, theme.accent, inner_l, top, (caret_phys - 4.0) as u32, bottom),
        0,
        "no accent ink left of the cursor column"
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, (caret_phys + 5.0) as u32, top, inner_r, bottom),
        0,
        "no accent ink right of the cursor column"
    );
    assert!(
        count_color_in(&canvas, theme.fg, 24, 20, 215, 88) > 0,
        "input text ink must be present"
    );

    // Char-boundary caret: text "aéb" with cursor 3 (BYTE index) puts
    // the caret after 'é' = 2 CHARS before it — the same caret x as the
    // "ab" case above. A byte-counting regression in the caret math
    // would place the caret one advance right (after 'b') and find no
    // accent in this band.
    canvas.fill(theme.bg);
    let ti_utf8 = TextInput { text: "aéb".into(), cursor: 3, focused: true };
    ti_utf8.draw(&mut canvas, rect, &theme, &mut text, 2.0);
    let band_utf8 = count_color_in(
        &canvas,
        theme.accent,
        (caret_phys - 3.0) as u32,
        top,
        (caret_phys + 4.0) as u32,
        bottom,
    );
    assert!(
        band_utf8 >= 50,
        "caret after a multi-byte char must sit at char-boundary advance x, got {band_utf8}"
    );
    assert!(
        count_color_in(&canvas, theme.fg, 24, 20, 215, 88) > 0,
        "'aéb' text ink must be present"
    );

    // Unfocused: no accent anywhere in the rect (border is border-colored
    // and there is no cursor column), text ink and border still drawn.
    let unfocused = TextInput { text: "ab".into(), cursor: 2, focused: false };
    let mut canvas2 = Canvas::new(240, 100);
    canvas2.fill(theme.bg);
    unfocused.draw(&mut canvas2, rect, &theme, &mut text, 2.0);
    assert_eq!(
        count_color_in(&canvas2, theme.accent, 20, 20, 220, 88),
        0,
        "unfocused input must not paint accent"
    );
    assert!(
        count_color_in(&canvas2, theme.fg, 24, 20, 215, 88) > 0,
        "unfocused input still draws its text"
    );
    assert!(
        count_color_in(&canvas2, theme.border, 10, 10, 210, 44) > 0,
        "unfocused input still draws its border ring"
    );
}

/// The auditor's Fix-round-2 finding: TabBar::draw and TextInput::draw
/// passed the text BASELINE in logical px to draw_line (which takes
/// physical px) while mixing in the already-physical metrics term — at
/// scale 2 the text landed at ~half its y. Chrome-invisible custom theme
/// (every token except fg == bg) isolates the text ink, so the same
/// logical widget on same-logical-size canvases must produce an ink box
/// at ~2× the position with ~2× the height.
#[test]
fn tab_and_input_text_baseline_scales_with_dpi() {
    let bg = (1, 2, 3);
    let fg = (200, 100, 50);
    let theme = Theme {
        bg,
        bg_elevated: bg,
        fg,
        fg_dim: bg,
        accent: bg,
        border: bg,
        selection: bg,
        syntax_keyword: bg,
        syntax_string: bg,
        syntax_number: bg,
        syntax_comment: bg,
        git_modified: bg,
        git_added: bg,
        git_deleted: bg,
        git_untracked: bg,
    };
    let mut text = TextSystem::new();

    // ---- TabBar: same logical bar on same-logical-size canvases ----
    let bar = Rect { x: 0.0, y: 0.0, w: 300.0, h: 30.0 };
    let tabs = TabBar { tabs: vec!["main.rs".into()], active: 0 };
    let mut c1 = Canvas::new(300, 40);
    c1.fill(theme.bg);
    tabs.draw(&mut c1, bar, &theme, &mut text, 1.0);
    let b1 = ink_bbox_in(&c1, theme.bg, 0, 0, 300, 40)
        .expect("scale-1 tab label must rasterize");
    let mut c2 = Canvas::new(600, 80);
    c2.fill(theme.bg);
    tabs.draw(&mut c2, bar, &theme, &mut text, 2.0);
    let b2 = ink_bbox_in(&c2, theme.bg, 0, 0, 600, 80)
        .expect("scale-2 tab label must rasterize");
    for (axis, a, b) in [(0, b1.0, b2.0), (1, b1.1, b2.1), (2, b1.2, b2.2), (3, b1.3, b2.3)] {
        assert!(
            (f64::from(b) - 2.0 * f64::from(a)).abs() <= 4.0,
            "tab label bbox edge {axis}: scale-2 {b} must be ~2× scale-1 {a}"
        );
    }
    let h1 = f64::from(b1.3 - b1.1) + 1.0;
    let h2 = f64::from(b2.3 - b2.1) + 1.0;
    let ratio = h2 / h1;
    assert!(
        (1.6..=2.4).contains(&ratio),
        "tab label ink height ratio {ratio:.2} must be ~2× ({h1} → {h2})"
    );

    // ---- TextInput: focused, same logical rect ----
    let rect = Rect { x: 10.0, y: 10.0, w: 200.0, h: 34.0 };
    let ti = TextInput { text: "ab".into(), cursor: 2, focused: true };
    let mut i1 = Canvas::new(240, 60);
    i1.fill(theme.bg);
    ti.draw(&mut i1, rect, &theme, &mut text, 1.0);
    let t1 = ink_bbox_in(&i1, theme.bg, 0, 0, 240, 60)
        .expect("scale-1 input text must rasterize");
    let mut i2 = Canvas::new(480, 120);
    i2.fill(theme.bg);
    ti.draw(&mut i2, rect, &theme, &mut text, 2.0);
    let t2 = ink_bbox_in(&i2, theme.bg, 0, 0, 480, 120)
        .expect("scale-2 input text must rasterize");
    for (axis, a, b) in [(0, t1.0, t2.0), (1, t1.1, t2.1), (2, t1.2, t2.2), (3, t1.3, t2.3)] {
        assert!(
            (f64::from(b) - 2.0 * f64::from(a)).abs() <= 4.0,
            "input text bbox edge {axis}: scale-2 {b} must be ~2× scale-1 {a}"
        );
    }
    let th1 = f64::from(t1.3 - t1.1) + 1.0;
    let th2 = f64::from(t2.3 - t2.1) + 1.0;
    let tratio = th2 / th1;
    assert!(
        (1.6..=2.4).contains(&tratio),
        "input text ink height ratio {tratio:.2} must be ~2× ({th1} → {th2})"
    );
}

#[test]
fn text_input_draw_degenerates_safely() {
    let theme = Theme { ..Theme::DARK };
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(120, 60);
    let rect = Rect { x: 10.0, y: 10.0, w: 100.0, h: 34.0 };

    // Empty focused field: no text ink, cursor column at the start.
    canvas.fill(theme.bg);
    let ti = TextInput { text: String::new(), cursor: 0, focused: true };
    ti.draw(&mut canvas, rect, &theme, &mut text, 1.0);
    assert_eq!(
        count_color_in(&canvas, theme.fg, 0, 0, 120, 60),
        0,
        "empty field must not paint text ink"
    );
    assert!(
        count_color_in(&canvas, theme.accent, 15, 12, 23, 42) >= 5,
        "focused empty field paints the cursor at the text start"
    );

    // Zero rect and zero scale: nothing rasterizes.
    canvas.fill(theme.bg);
    ti.draw(&mut canvas, Rect { x: 10.0, y: 10.0, w: 0.0, h: 34.0 }, &theme, &mut text, 1.0);
    ti.draw(&mut canvas, rect, &theme, &mut text, 0.0);
    assert_eq!(
        count_not_color(&canvas, theme.bg),
        0,
        "degenerate input draws rasterize nothing"
    );
}
