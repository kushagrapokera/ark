//! Crate-root test module (declared by main.rs): the app-level event
//! routing pins that need main.rs's private helpers.

use super::*;
use crate::fs::testutil::temp_dir;
use std::fs;
use std::thread;
use std::time::Duration;

/// The M5 fs pipeline at the App level (no window needed): one coalesced
/// signal refreshes the OPEN viewer immediately (leading-edge stat check,
/// read position carried, flash armed) and flags the tree re-list, which
/// runs at its debounce due point.
#[test]
fn fs_change_refreshes_viewer_and_flags_tree() {
    let d = temp_dir("fs-change");
    let p = d.path().join("open.txt");
    fs::write(&p, "one\ntwo\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    // Simulate a double-click open (the cache holds the viewer; the frame
    // would render it).
    app.buffers.get_or_open(&p).expect("open");
    app.open_path = Some(p.clone());
    {
        let v = app.buffers.get_or_open(&p).expect("open");
        assert!(v.set_offset_lines(1, 1), "read position into range");
    }
    assert_eq!(app.tree.visible_count(), 1);

    // fs activity: rewrite the open file (mtime+len change) and add a file.
    thread::sleep(Duration::from_millis(30));
    fs::write(&p, "one\ntwo\nthree\n").expect("rewrite");
    thread::sleep(Duration::from_millis(30));
    fs::File::create(d.path().join("new.txt")).expect("create new file");

    // One signal: the viewer refreshes NOW; the tree is flagged for its
    // debounced re-list.
    assert!(app.process_fs_change(), "the open viewer changed on disk");
    {
        let v = app.buffers.get_or_open(&p).expect("refreshed viewer");
        assert_eq!(v.line_count(), 3, "content re-read from disk");
        assert_eq!(v.offset_lines(), 1, "read position carried across the refresh");
        assert!(v.selection().is_none(), "selection dropped with the old view");
    }
    assert!(app.flash_until.is_some(), "change flash armed");
    assert!(app.fs_dirty, "tree re-list flagged");

    // At the debounce due point the tree re-lists and the flag clears.
    assert!(app.refresh_tree_if_due(Instant::now() + TREE_REFRESH_DEBOUNCE));
    assert_eq!(app.tree.visible_count(), 2, "new file appears in the tree");
    assert!(!app.fs_dirty);
    // A second due-point check without new activity does nothing.
    assert!(!app.refresh_tree_if_due(Instant::now() + 2 * TREE_REFRESH_DEBOUNCE));

    // No viewer open → the same signal only flags the tree.
    let d2 = temp_dir("fs-change-noviewer");
    let mut app2 = App::new(d2.path().to_path_buf(), false, None, None);
    fs::File::create(d2.path().join("x.txt")).expect("create");
    assert!(!app2.process_fs_change(), "no open viewer → no viewer change");
    assert!(app2.refresh_tree_if_due(Instant::now()));
    assert_eq!(app2.tree.visible_count(), 1);
}

/// The M4 viewer wheel contract: pixel deltas convert to whole lines, a
/// moving scroll marks ONLY the viewer pane rect, and a clamped scroll marks
/// nothing.
#[test]
fn viewer_wheel_marks_pane_damage_only() {
    let d = temp_dir("wheel-viewer");
    let p = d.path().join("lines.txt");
    fs::write(&p, (0..40).map(|i| format!("line {i}\n")).collect::<String>()).expect("write");
    let mut viewer = Viewer::open(&p).expect("opens");
    let layout = frame::app_layout(1024.0, 768.0, 280.0); // pane 720×632
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // One notch (40px) → round(40/18.9) = 2 lines; damage = the pane rect.
    assert!(route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(viewer.offset_lines(), 2);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(rects, vec![damage_rect(layout.viewer_pane, 1.0, 1024, 768)]);

    // Notch to the clamp (40 lines − 33 viewport = 7), then one more notch
    // moves nothing → no damage at all.
    while route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(viewer.offset_lines(), 7, "max offset = line_count − viewport_lines");
    assert!(!route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert!(damage.take(canvas.width(), canvas.height()).is_none(), "clamped: nothing pending");
}

#[test]
fn wheel_routes_line_delta_only() {
    use winit::dpi::PhysicalPosition;

    // Wheel down (winit y = −1) → offset increases by 40 logical px.
    assert_eq!(wheel_scroll_px(&MouseScrollDelta::LineDelta(0.0, -1.0)), Some(WHEEL_PX));
    // Wheel up → offset decreases.
    assert_eq!(wheel_scroll_px(&MouseScrollDelta::LineDelta(0.0, 2.0)), Some(-2.0 * WHEEL_PX));
    // PixelDelta (touchpads) is not handled in M3.
    assert_eq!(
        wheel_scroll_px(&MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 40.0))),
        None
    );
}

/// The M3 damage contract at the routing level: a tree scroll marks the
/// panel + status rects ONLY (never full-frame), and a clamped scroll
/// marks nothing.
#[test]
fn wheel_scroll_marks_panel_damage_only() {
    let d = temp_dir("wheel-damage");
    for i in 0..40 {
        fs::File::create(d.path().join(format!("f{i:02}.txt"))).expect("create file");
    }
    let mut tree = FileTree::new(d.path());
    let layout = frame::app_layout(1024.0, 768.0, 280.0); // viewport 632px
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // First notch scrolls → exactly two rects, matching the layout.
    assert!(route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(tree.offset, WHEEL_PX);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(
        rects,
        vec![
            damage_rect(layout.tree_panel, 1.0, 1024, 768),
            damage_rect(layout.status, 1.0, 1024, 768),
        ]
    );

    // Notch to the max offset through the router: every moving notch marks
    // the same two rects.
    while route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(tree.offset, 248.0, "max offset = content (40×22) − viewport (632)");

    // One notch past the clamp moves nothing → no damage at all.
    assert!(!route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert!(
        damage.take(canvas.width(), canvas.height()).is_none(),
        "clamped: nothing pending"
    );

    // Scale 2 doubles the rect and clamps to the canvas (632×2 extends past
    // 768), never yielding out-of-bounds damage.
    assert_eq!(
        damage_rect(layout.tree_panel, 2.0, 1024, 768),
        DamageRect { x: 16, y: 176, width: 528, height: 592 }
    );
}
