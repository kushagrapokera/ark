//! Scrollbar tests (moved verbatim with the widget from ui.rs during the M3
//! module split — zero assertion edits).

use super::*;
use crate::ui::testutil::{count_color_in, count_not_color};

// ---- Scrollbar (acceptance 1) ----

#[test]
fn thumb_geometry_proportional_and_clamped() {
    // No overflow: the thumb fills the track regardless of offset.
    assert_eq!(thumb_geometry(200.0, 100.0, 200.0, 42.0), (0.0, 200.0));

    // Proportional thumb: thumb_h = viewport/content × track_h.
    assert_eq!(thumb_geometry(200.0, 400.0, 200.0, 0.0), (0.0, 100.0));
    let (y, h) = thumb_geometry(200.0, 400.0, 200.0, 100.0);
    assert_eq!(h, 100.0);
    assert!((y - 50.0).abs() < 1e-9, "half the max offset → thumb centered, got {y}");

    // Offset clamps to [0, content − viewport] on both ends.
    let (y, h) = thumb_geometry(200.0, 400.0, 200.0, 999.0);
    assert_eq!(h, 100.0);
    assert!((y - 100.0).abs() < 1e-9, "max offset → thumb at track bottom, got {y}");
    let (y, _) = thumb_geometry(200.0, 400.0, 200.0, -5.0);
    assert!(y.abs() < 1e-9, "negative offset clamps to top, got {y}");

    // Degenerate track/viewport: no thumb, no panic.
    assert_eq!(thumb_geometry(0.0, 400.0, 200.0, 50.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(-5.0, 400.0, 200.0, 50.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(200.0, 400.0, 0.0, 50.0), (0.0, 0.0));

    // clamp() forces the offset into range.
    let mut sb = Scrollbar { offset: 42.0, drag: None };
    sb.clamp(100.0, 200.0);
    assert_eq!(sb.offset, 0.0, "no overflow forces offset 0");
    sb.offset = 500.0;
    sb.clamp(400.0, 200.0);
    assert_eq!(sb.offset, 200.0, "offset clamps to content − viewport");
    sb.offset = -10.0;
    sb.clamp(400.0, 200.0);
    assert_eq!(sb.offset, 0.0);
    sb.offset = 50.0;
    sb.clamp(400.0, 200.0);
    assert_eq!(sb.offset, 50.0, "in-range offset untouched");
}

// ---- Scrollbar dragging + wheel path (acceptance 2) ----

#[test]
fn scrollbar_drag_updates_offset() {
    // Track (0,0,8,200), content 264, viewport 200: max offset 64,
    // thumb_h = 200/264×200 ≈ 151.52, draggable span ≈ 48.48.
    let track = Rect { x: 0.0, y: 0.0, w: 8.0, h: 200.0 };
    let (content, viewport) = (264.0, 200.0);
    let mut sb = Scrollbar::default();
    assert_eq!(sb.offset, 0.0);

    let (_, thumb_h) = thumb_geometry(track.h, content, viewport, sb.offset);
    let down_on_thumb = Pointer { x: 4.0, y: 75.0, down: true, click: false };
    let (ty, th) = thumb_geometry(track.h, content, viewport, sb.offset);
    assert!(
        down_on_thumb.y > ty && down_on_thumb.y < ty + th,
        "grab point must sit on the thumb (thumb spans {ty}..{} at offset 0)",
        ty + th
    );
    assert!(
        !sb.on_pointer(track, down_on_thumb, content, viewport),
        "grabbing the thumb doesn't move it yet"
    );
    assert_eq!(sb.offset, 0.0);

    // Drag 25px down: offset moves proportionally along the free span.
    let moved = Pointer { x: 4.0, y: 100.0, down: true, click: false };
    assert!(sb.on_pointer(track, moved, content, viewport), "drag must change the offset");
    let expected = (100.0 - 75.0) / (track.h - thumb_h) * (content - viewport);
    assert!(
        (sb.offset - expected).abs() < 1e-9,
        "offset {} must be proportional {}",
        sb.offset,
        expected
    );

    // Release ends the drag…
    let up = Pointer { x: 4.0, y: 100.0, down: false, click: true };
    assert!(!sb.on_pointer(track, up, content, viewport));
    // …and later moves without a button don't move it again.
    let drift = Pointer { x: 4.0, y: 150.0, down: false, click: false };
    assert!(!sb.on_pointer(track, drift, content, viewport));
    assert!((sb.offset - expected).abs() < 1e-9, "offset stable after release");

    // Wheel path: scroll_by clamps and reports change.
    assert!(sb.scroll_by(100.0, content, viewport), "overshoot still counts as change");
    assert_eq!(sb.offset, 64.0, "clamped to content − viewport");
    assert!(sb.scroll_by(-100.0, content, viewport));
    assert_eq!(sb.offset, 0.0);
    assert!(!sb.scroll_by(0.0, content, viewport), "zero delta → no change");
    assert!(!sb.scroll_by(40.0, 100.0, 200.0), "no overflow → no change");
    assert_eq!(sb.offset, 0.0, "no-overflow scroll leaves offset 0");
}

#[test]
fn scrollbar_draws_track_and_thumb() {
    let theme = Theme { ..Theme::DARK };
    let mut canvas = Canvas::new(40, 220);
    canvas.fill(theme.bg);
    let track = Rect { x: 10.0, y: 10.0, w: 8.0, h: 200.0 };
    // content 400, viewport 200 → thumb_h 100; offset 100 → thumb_y 50.
    let sb = Scrollbar { offset: 100.0, drag: None };

    sb.draw(&mut canvas, track, 400.0, 200.0, &theme, 1.0);
    // Thumb: fg_dim fill inset 1px, rows 60..160.
    let thumb = count_color_in(&canvas, theme.fg_dim, 11, 60, 17, 160);
    assert_eq!(thumb, 6 * 100, "thumb must be an exact fg_dim 6×100 fill");
    assert_eq!(
        count_color_in(&canvas, theme.fg_dim, 11, 11, 17, 60),
        0,
        "no thumb ink above the thumb row"
    );
    // Track: bg_elevated interior around the thumb, border outline.
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 12, 11, 16, 59) >= 150,
        "track interior must be bg_elevated above the thumb"
    );
    assert!(
        count_color_in(&canvas, theme.border, 10, 10, 18, 210) > 0,
        "track must carry a border outline"
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, 0, 0, 40, 220),
        0,
        "idle scrollbar must not paint accent"
    );

    // While dragged the thumb turns accent.
    canvas.fill(theme.bg);
    let dragged = Scrollbar { offset: 100.0, drag: Some(0.0) };
    dragged.draw(&mut canvas, track, 400.0, 200.0, &theme, 1.0);
    let accent_thumb = count_color_in(&canvas, theme.accent, 11, 60, 17, 160);
    assert_eq!(accent_thumb, 6 * 100, "dragged thumb must be accent-filled");
    assert_eq!(
        count_color_in(&canvas, theme.fg_dim, 0, 0, 40, 220),
        0,
        "dragged scrollbar must not paint fg_dim thumb"
    );

    // Degenerate: zero track and zero scale rasterize nothing.
    canvas.fill(theme.bg);
    sb.draw(&mut canvas, Rect { x: 10.0, y: 10.0, w: 0.0, h: 200.0 }, 400.0, 200.0, &theme, 1.0);
    sb.draw(&mut canvas, track, 400.0, 200.0, &theme, 0.0);
    assert_eq!(
        count_not_color(&canvas, theme.bg),
        0,
        "degenerate scrollbar draws must rasterize nothing"
    );
}

#[test]
fn scrollbar_nan_inf_inputs_stay_finite_and_clamped() {
    let track = Rect { x: 0.0, y: 0.0, w: 8.0, h: 200.0 };

    // thumb_geometry: non-finite OFFSET is treated as 0 — the thumb
    // stays proportional (mutation M9 target: guard removal would yield
    // a NaN thumb_y here).
    assert_eq!(thumb_geometry(200.0, 400.0, 200.0, f64::NAN), (0.0, 100.0));
    assert_eq!(thumb_geometry(200.0, 400.0, 200.0, f64::INFINITY), (0.0, 100.0));
    assert_eq!(thumb_geometry(200.0, 400.0, 200.0, f64::NEG_INFINITY), (0.0, 100.0));
    // thumb_geometry: non-finite track/viewport/content → no thumb.
    assert_eq!(thumb_geometry(f64::NAN, 400.0, 200.0, 0.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(200.0, f64::NAN, 200.0, 0.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(200.0, 400.0, f64::NAN, 0.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(f64::INFINITY, 400.0, 200.0, 0.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(200.0, f64::INFINITY, 200.0, 0.0), (0.0, 0.0));
    assert_eq!(thumb_geometry(200.0, 400.0, f64::INFINITY, 0.0), (0.0, 0.0));
    // −Inf content is the discriminating case: without the finiteness
    // guard it takes the content ≤ viewport branch and fills the track.
    assert_eq!(thumb_geometry(200.0, f64::NEG_INFINITY, 200.0, 0.0), (0.0, 0.0));

    // scroll_by: non-finite deltas are explicit no-ops (offset unchanged).
    let (content, viewport) = (400.0, 200.0);
    let mut sb = Scrollbar { offset: 50.0, drag: None };
    assert!(!sb.scroll_by(f64::NAN, content, viewport), "NaN dy is a no-op");
    assert_eq!(sb.offset, 50.0);
    assert!(!sb.scroll_by(f64::INFINITY, content, viewport), "+Inf dy is a no-op");
    assert_eq!(sb.offset, 50.0);
    assert!(!sb.scroll_by(f64::NEG_INFINITY, content, viewport), "−Inf dy is a no-op");
    assert_eq!(sb.offset, 50.0);

    // clamp: non-finite content/viewport forces offset 0 (a NaN max
    // would make f64::clamp panic without the guard).
    sb.offset = 50.0;
    sb.clamp(f64::NAN, viewport);
    assert_eq!(sb.offset, 0.0, "NaN content forces offset 0");
    sb.offset = 50.0;
    sb.clamp(f64::INFINITY, viewport);
    assert_eq!(sb.offset, 0.0, "infinite content forces offset 0");
    sb.offset = 50.0;
    sb.clamp(content, f64::NAN);
    assert_eq!(sb.offset, 0.0, "NaN viewport forces offset 0");
    sb.offset = 50.0;
    sb.clamp(content, f64::INFINITY);
    assert_eq!(sb.offset, 0.0, "infinite viewport forces offset 0");

    // clamp: non-finite OFFSET snaps to 0 (offset is a pub field —
    // callers can hand in garbage directly).
    sb.offset = f64::NAN;
    sb.clamp(content, viewport);
    assert_eq!(sb.offset, 0.0, "NaN offset → 0");
    sb.offset = f64::INFINITY;
    sb.clamp(content, viewport);
    assert_eq!(sb.offset, 0.0, "+Inf offset → 0");
    sb.offset = f64::NEG_INFINITY;
    sb.clamp(content, viewport);
    assert_eq!(sb.offset, 0.0, "−Inf offset → 0");

    // on_pointer: the entry clamp keeps a NaN offset finite, and a NaN
    // pointer never grabs the thumb (NaN comparisons are all false).
    let mut sb2 = Scrollbar { offset: f64::NAN, drag: None };
    let nan_pointer = Pointer { x: 4.0, y: f64::NAN, down: false, click: false };
    // The changed-flag is not pinned here: correcting NaN → 0 IS an
    // offset change. The invariant under test is offset finiteness.
    sb2.on_pointer(track, nan_pointer, content, viewport);
    assert_eq!(sb2.offset, 0.0, "NaN offset re-clamped by on_pointer entry");
    let nan_press = Pointer { x: 4.0, y: f64::NAN, down: true, click: false };
    sb2.on_pointer(track, nan_press, content, viewport);
    assert_eq!(sb2.offset, 0.0, "NaN pointer never grabs");

    // A NaN pointer DURING an active drag must not corrupt the offset:
    // grab normally, feed a NaN move (ignored), then keep dragging.
    let mut sb3 = Scrollbar::default();
    let (drag_content, drag_viewport) = (264.0, 200.0);
    assert!(
        !sb3.on_pointer(track, Pointer { x: 4.0, y: 75.0, down: true, click: false }, drag_content, drag_viewport),
        "grabbing doesn't move the thumb"
    );
    assert!(
        !sb3.on_pointer(track, Pointer { x: 4.0, y: f64::NAN, down: true, click: false }, drag_content, drag_viewport),
        "a NaN move changes nothing"
    );
    assert_eq!(sb3.offset, 0.0, "offset must stay finite through a NaN drag move");
    assert!(
        sb3.on_pointer(track, Pointer { x: 4.0, y: 100.0, down: true, click: false }, drag_content, drag_viewport),
        "the drag resumes with a finite move"
    );
    assert!(
        (sb3.offset - 33.0).abs() < 0.1,
        "drag math intact after the ignored NaN move: {}",
        sb3.offset
    );
}

#[test]
fn scrollbar_no_overflow_degenerates_to_filled_thumb_and_zero_offset() {
    let theme = Theme { ..Theme::DARK };
    let mut canvas = Canvas::new(40, 240);
    canvas.fill(theme.bg);
    let track = Rect { x: 10.0, y: 10.0, w: 8.0, h: 200.0 };
    let mut sb = Scrollbar::default(); // offset 0 (kept clamped by clamp())

    // on_pointer with no overflow: pressing anywhere never drags, offset
    // is forced to 0, and nothing reports change.
    let press = Pointer { x: 12.0, y: 100.0, down: true, click: false };
    assert!(!sb.on_pointer(track, press, 100.0, 200.0), "no overflow → no drag");
    let drag = Pointer { x: 12.0, y: 180.0, down: true, click: false };
    assert!(!sb.on_pointer(track, drag, 100.0, 200.0));
    assert_eq!(sb.offset, 0.0, "offset forced 0 when content ≤ viewport");

    // Draw: the thumb fills the whole track (bg_elevated+border+thumb).
    sb.draw(&mut canvas, track, 100.0, 200.0, &theme, 1.0);
    let thumb = count_color_in(&canvas, theme.fg_dim, 11, 11, 17, 209);
    assert_eq!(thumb, 6 * 198, "no-overflow thumb fills the inset track, got {thumb}");
}
