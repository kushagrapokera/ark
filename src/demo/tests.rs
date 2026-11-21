//! The M2 demo-scene test pins, moved with the scene code from main.rs.
//! Pixel helpers come from the shared `ui::testutil` module (the local
//! copies that used to live in main.rs's test module are gone).

use super::*;
use crate::ui::testutil::{count_color_in, ink_bbox_in};

// ---- Pure scene helpers ----

#[test]
fn demo_layout_matches_spec() {
    let l = demo_layout(1024.0, 768.0, 380.0);
    assert_eq!(l.tabs, Rect { x: 24.0, y: 88.0, w: 976.0, h: 30.0 });
    assert_eq!(l.split_area, Rect { x: 0.0, y: 126.0, w: 1024.0, h: 594.0 });
    assert_eq!(l.button, Rect { x: 24.0, y: 150.0, w: 140.0, h: 34.0 });
    assert_eq!(l.checkbox, Rect { x: 24.0, y: 210.0, w: 22.0, h: 22.0 });
    assert_eq!(l.input, Rect { x: 24.0, y: 250.0, w: 280.0, h: 34.0 });
    assert_eq!(l.content_box, Rect { x: 404.0, y: 150.0, w: 548.0, h: 200.0 });
    assert_eq!(l.scroll_track, Rect { x: 952.0, y: 150.0, w: 8.0, h: 200.0 });

    // Degenerate window sizes never produce negative extents.
    let tiny = demo_layout(400.0, 300.0, 380.0);
    assert!(tiny.tabs.w >= 0.0 && tiny.split_area.h >= 0.0);
    assert_eq!(tiny.content_box.w, 0.0, "box collapses below the split minimum");
    assert_eq!(demo_layout(1024.0, 768.0, 900.0).content_box.w, 28.0);
}

#[test]
fn demo_status_formats_state() {
    assert_eq!(
        demo_status(&DemoState::default()),
        "clicks 0 · checked false · tab \"main.rs\" · input \"edit me\" · scroll 0% · split 380px"
    );

    // A fully-populated state (the offset driven through the public API).
    let mut mid = Scrollbar::default();
    mid.scroll_by(32.0, DEMO_CONTENT_PX, DEMO_PANE_HEIGHT); // max 64 → 32 = 50%
    let demo = DemoState {
        clicks: 3,
        checked: true,
        tabs: TabBar {
            tabs: vec!["main.rs".into(), "viewer".into(), "commit panel".into()],
            active: 1,
        },
        split: Split { position: 500.4 },
        split_dragging: false,
        split_grab_dx: 0.0,
        input: TextInput { text: "hi".into(), cursor: 2, focused: false },
        scroll: mid,
    };
    assert_eq!(
        demo_status(&demo),
        "clicks 3 · checked true · tab \"viewer\" · input \"hi\" · scroll 50% · split 500px"
    );

    // Offset pins the percentage endpoints (content 264, viewport 200).
    assert!(
        demo_status(&DemoState::default()).contains("scroll 0%"),
        "offset 0 → 0%"
    );
    let mut full_sb = Scrollbar::default();
    full_sb.scroll_by(64.0, DEMO_CONTENT_PX, DEMO_PANE_HEIGHT);
    let full = DemoState { scroll: full_sb, ..DemoState::default() };
    assert!(demo_status(&full).contains("scroll 100%"), "max offset → 100%");
}

#[test]
fn keys_route_named_and_text() {
    use winit::keyboard::{Key, NamedKey};

    // Named navigation keys map even though Backspace carries text
    // "\x08" — named keys are checked first.
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::Backspace), Some("\x08")),
        vec![TextInputKey::Backspace]
    );
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::ArrowLeft), None),
        vec![TextInputKey::Left]
    );
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::ArrowRight), None),
        vec![TextInputKey::Right]
    );
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::Home), None),
        vec![TextInputKey::Home]
    );
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::End), None),
        vec![TextInputKey::End]
    );

    // Printable text feeds one Char per char.
    assert_eq!(
        keys_from_logical_and_text(&Key::Character("a".into()), Some("a")),
        vec![TextInputKey::Char('a')]
    );
    // Control characters in text are never inserted (Enter/Tab/Escape).
    assert!(keys_from_logical_and_text(&Key::Named(NamedKey::Enter), Some("\r")).is_empty());
    assert!(keys_from_logical_and_text(&Key::Named(NamedKey::Tab), Some("\t")).is_empty());
    assert!(keys_from_logical_and_text(&Key::Named(NamedKey::Escape), Some("\x1b")).is_empty());
    // Unlabeled keys produce nothing.
    assert!(keys_from_logical_and_text(&Key::Named(NamedKey::F5), None).is_empty());
    assert!(keys_from_logical_and_text(
        &Key::Unidentified(winit::keyboard::NativeKey::Windows(0x1234)),
        None
    )
    .is_empty());
    assert!(keys_from_logical_and_text(&Key::Dead(None), None).is_empty());
    // Space still types (text " " is printable).
    assert_eq!(
        keys_from_logical_and_text(&Key::Named(NamedKey::Space), Some(" ")),
        vec![TextInputKey::Char(' ')]
    );
}

/// Fix-round-2 audit: draw_content_box had the same baseline bug class —
/// the physical metrics term was multiplied by scale along with the
/// logical row position, sinking the text (metrics/2)×(scale−1) px too
/// low at scale > 1. A 24px-high box shows only tree row 0, and the
/// x-region skips the icon cell, isolating the text ink.
#[test]
fn content_box_text_baseline_scales_with_dpi() {
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
    let mut scratch = Canvas::new(1, 1);
    let box_r = Rect { x: 10.0, y: 10.0, w: 120.0, h: 24.0 };

    let mut c1 = Canvas::new(140, 40);
    c1.fill(theme.bg);
    draw_content_box(&mut c1, &mut text, &theme, 1.0, box_r, 0.0, &mut scratch);
    let b1 = ink_bbox_in(&c1, theme.bg, 24, 0, 140, 40)
        .expect("scale-1 tree text must rasterize");

    let mut c2 = Canvas::new(280, 80);
    c2.fill(theme.bg);
    draw_content_box(&mut c2, &mut text, &theme, 2.0, box_r, 0.0, &mut scratch);
    let b2 = ink_bbox_in(&c2, theme.bg, 48, 0, 280, 80)
        .expect("scale-2 tree text must rasterize");

    for (axis, a, b) in [(1, b1.1, b2.1), (3, b1.3, b2.3)] {
        assert!(
            (f64::from(b) - 2.0 * f64::from(a)).abs() <= 4.0,
            "tree text bbox edge {axis}: scale-2 {b} must be ~2× scale-1 {a}"
        );
    }
    let h1 = f64::from(b1.3 - b1.1) + 1.0;
    let h2 = f64::from(b2.3 - b2.1) + 1.0;
    let ratio = h2 / h1;
    assert!(
        (1.6..=2.4).contains(&ratio),
        "tree text ink height ratio {ratio:.2} must be ~2× ({h1} → {h2})"
    );
}

// ---- Scene rendering + interactions ----

#[test]
fn m2_scene_widgets_render_at_spec_positions() {
    let theme = Theme { ..Theme::DARK };
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut scratch = Canvas::new(1, 1);
    let mut demo = DemoState::default();

    draw_scene(&mut canvas, &mut text, &theme, 1.0, &mut demo, Pointer::default(), &mut scratch);

    // Header + info line.
    assert!(count_color_in(&canvas, theme.accent, 0, 20, 400, 50) > 0, "header ink");
    assert!(count_color_in(&canvas, theme.fg_dim, 0, 50, 700, 70) > 0, "info line ink");
    // Tab bar: accent underline on tab 0; dim label on tab 1.
    assert!(
        count_color_in(&canvas, theme.accent, 24, 114, 184, 118) >= 300,
        "active tab underline (tab rect y 88..118, underline 116..118)"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 184, 88, 344, 116) > 0,
        "inactive tab label ink"
    );
    // Split strip: 2px border column at x=380 spanning y 126..720.
    assert!(
        count_color_in(&canvas, theme.border, 379, 126, 381, 720) >= 500,
        "split divider strip"
    );
    // Button: bg_elevated interior fill.
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 26, 152, 162, 182) >= 2000,
        "button fill"
    );
    // Checkbox ring + label ink.
    assert!(count_color_in(&canvas, theme.border, 24, 210, 46, 232) > 0, "checkbox ring");
    assert!(count_color_in(&canvas, theme.fg, 54, 205, 220, 235) > 0, "checkbox label ink");
    // Input ring + text ink; unfocused → no accent anywhere in it.
    assert!(count_color_in(&canvas, theme.border, 24, 250, 304, 284) > 0, "input ring");
    assert!(count_color_in(&canvas, theme.fg, 30, 252, 300, 282) > 0, "input text ink");
    assert_eq!(
        count_color_in(&canvas, theme.accent, 24, 250, 304, 284),
        0,
        "unfocused input paints no accent"
    );
    // Content box: border outline + demo tree ink inside the interior.
    assert!(count_color_in(&canvas, theme.border, 404, 150, 952, 350) > 0, "box outline");
    assert!(count_color_in(&canvas, theme.fg, 410, 152, 950, 348) > 0, "demo tree ink");
    // Scrollbar thumb ink in the track.
    assert!(
        count_color_in(&canvas, theme.fg_dim, 953, 151, 959, 349) >= 100,
        "scrollbar thumb"
    );
    // Status line: dim ink near baseline h−28 = 740.
    assert!(count_color_in(&canvas, theme.fg_dim, 0, 724, 900, 748) > 0, "status line ink");
}

#[test]
fn m2_scene_pointer_interactions_update_state() {
    let theme = Theme { ..Theme::DARK };
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut scratch = Canvas::new(1, 1);
    let mut demo = DemoState::default();
    let mut scene =
        |demo: &mut DemoState, pointer: Pointer, canvas: &mut Canvas, scratch: &mut Canvas| {
            draw_scene(canvas, &mut text, &theme, 1.0, demo, pointer, scratch);
        };

    // Click tab 1 → activates "viewer" (status reflects it).
    scene(&mut demo, Pointer { x: 200.0, y: 100.0, down: false, click: true }, &mut canvas, &mut scratch);
    assert_eq!(demo.tabs.active, 1);
    assert!(demo_status(&demo).contains("tab \"viewer\""));

    // Click the button → counts one click.
    scene(&mut demo, Pointer { x: 90.0, y: 165.0, down: false, click: true }, &mut canvas, &mut scratch);
    assert_eq!(demo.clicks, 1);
    assert!(demo_status(&demo).starts_with("clicks 1 ·"));

    // Click the checkbox → toggles on; again → off.
    let click_box = Pointer { x: 35.0, y: 220.0, down: false, click: true };
    scene(&mut demo, click_box, &mut canvas, &mut scratch);
    assert!(demo.checked);
    scene(&mut demo, click_box, &mut canvas, &mut scratch);
    assert!(!demo.checked);

    // Click inside the input → focused (accent ring + cursor appear);
    // click anywhere else → unfocused.
    scene(&mut demo, Pointer { x: 100.0, y: 260.0, down: false, click: true }, &mut canvas, &mut scratch);
    assert!(demo.input.focused);
    assert!(
        count_color_in(&canvas, theme.accent, 24, 250, 304, 284) > 0,
        "focused input paints accent"
    );
    scene(&mut demo, Pointer { x: 600.0, y: 500.0, down: false, click: true }, &mut canvas, &mut scratch);
    assert!(!demo.input.focused);

    // Split drag: press inside the ±4px band (no position jump), drag,
    // release. The grab offset keeps the divider under the pointer.
    scene(&mut demo, Pointer { x: 381.0, y: 400.0, down: true, click: false }, &mut canvas, &mut scratch);
    assert!(demo.split_dragging, "press on the divider starts a drag");
    assert_eq!(demo.split.position, 380.0, "grabbing doesn't jump the divider");
    scene(&mut demo, Pointer { x: 501.0, y: 400.0, down: true, click: false }, &mut canvas, &mut scratch);
    assert_eq!(demo.split.position, 500.0);
    assert!(
        count_color_in(&canvas, theme.accent, 499, 126, 501, 720) >= 500,
        "dragging paints the accent strip at the divider"
    );
    scene(&mut demo, Pointer { x: 501.0, y: 400.0, down: false, click: true }, &mut canvas, &mut scratch);
    assert!(!demo.split_dragging, "release ends the drag");
    assert_eq!(demo.split.position, 500.0, "position persists after release");
    assert!(demo_status(&demo).contains("split 500px"));

    // Scrollbar thumb drag: press the thumb (offset 0 → rows 151..302),
    // drag past the track end → offset clamps to max.
    scene(&mut demo, Pointer { x: 956.0, y: 200.0, down: true, click: false }, &mut canvas, &mut scratch);
    assert_eq!(demo.scroll.offset, 0.0, "grabbing the thumb doesn't move it yet");
    scene(&mut demo, Pointer { x: 956.0, y: 400.0, down: true, click: false }, &mut canvas, &mut scratch);
    // Deterministic clamp: grab at y=200 (thumb top, offset 0), drag to
    // y=400 → the thumb pins at the free-span end → offset = max = 64.
    assert_eq!(demo.scroll.offset, 64.0, "thumb drag past the track end clamps to max");
    assert!(demo_status(&demo).contains("scroll 100%"), "status reflects the clamped offset");
    scene(&mut demo, Pointer { x: 956.0, y: 400.0, down: false, click: true }, &mut canvas, &mut scratch);
    let after_drag = demo.scroll.offset;

    // Wheel down over the content box = scroll_by(+40) (wheel_scroll_px
    // maps LineDelta y=−1 to +40), clamped at the max offset.
    demo.scroll.scroll_by(40.0, DEMO_CONTENT_PX, DEMO_PANE_HEIGHT);
    assert_eq!(demo.scroll.offset, (after_drag + 40.0).min(64.0));
}
