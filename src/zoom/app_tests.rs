use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use crate::ui::Scrollbar;
use crate::App;

fn app() -> (TempDir, App) {
    let dir = temp_dir("ui-zoom");
    let app = App::new(dir.path().to_path_buf(), false, None, None);
    (dir, app)
}

#[test]
fn app_starts_at_default_zoom() {
    let (_dir, app) = app();
    assert_eq!(app.device_scale, 1.0);
    assert_eq!(app.zoom, 1.0);
    assert_eq!(app.scale, 1.0);
}

#[test]
fn adjust_zoom_updates_effective_scale_and_preserves_pointer_position() {
    let (_dir, mut app) = app();
    app.device_scale = 2.0;
    app.scale = 2.0;
    app.pointer.x = 100.0;
    app.pointer.y = 50.0;
    app.damage.take(app.canvas.width(), app.canvas.height());

    assert!(app.adjust_zoom(1));
    assert!((app.zoom - 1.1).abs() < f64::EPSILON);
    assert!((app.scale - 2.2).abs() < f64::EPSILON);
    assert!((app.pointer.x - 200.0 / 2.2).abs() < 1e-9);
    assert!((app.pointer.y - 100.0 / 2.2).abs() < 1e-9);
    assert_eq!(app.last_click, None);
    assert!(!app.pending_dbl);
    assert!(app.damage.take(1, 1).is_some(), "zoom marks full damage");
}

#[test]
fn device_scale_changes_preserve_session_zoom() {
    let (_dir, mut app) = app();
    app.zoom = 1.5;
    app.scale = 1.5;
    app.pointer.x = 100.0;
    app.pointer.y = 50.0;

    assert!(app.set_device_scale(2.0));
    assert_eq!(app.device_scale, 2.0);
    assert_eq!(app.zoom, 1.5);
    assert!((app.scale - 3.0).abs() < f64::EPSILON);
    assert!((app.pointer.x - 50.0).abs() < 1e-9);
    assert!((app.pointer.y - 25.0).abs() < 1e-9);
    assert!(!app.set_device_scale(f64::NAN));
    assert!((app.scale - 3.0).abs() < f64::EPSILON);
}

#[test]
fn scale_changes_cancel_stale_geometry_drags() {
    let (_dir, mut app) = app();
    app.split_drag.dragging = true;
    app.split_drag.grab_dx = 12.0;
    app.group_split_drag.dragging = true;
    app.group_split_drag.grab_dx = 8.0;
    app.term_drag.dragging = true;
    app.term_drag.grab_dy = 20.0;
    app.term_drag.dirty = true;
    app.term_drag.damage_marked = true;
    app.pointer.down = true;

    assert!(app.adjust_zoom(1));
    assert!(!app.split_drag.dragging);
    assert!(!app.group_split_drag.dragging);
    assert!(!app.term_drag.dragging);
    assert!(!app.term_drag.dirty);
    assert!(!app.term_drag.damage_marked);
    assert!(!app.pointer.down);
    assert_eq!(app.scrollbar, Scrollbar::default());
    assert_eq!(app.group_scrollbars, [Scrollbar::default(); 2]);
}

#[test]
fn adjust_zoom_is_a_bounded_noop_at_limits() {
    let (_dir, mut app) = app();
    app.device_scale = 2.0;
    app.zoom = MAX_ZOOM;
    app.scale = app.device_scale * app.zoom;
    assert!(!app.adjust_zoom(1));
    assert_eq!(app.zoom, MAX_ZOOM);
    assert!((app.scale - 3.0).abs() < f64::EPSILON);

    app.zoom = MIN_ZOOM;
    app.scale = app.device_scale * app.zoom;
    assert!(!app.adjust_zoom(-1));
    assert_eq!(app.zoom, MIN_ZOOM);
    assert!((app.scale - 1.5).abs() < f64::EPSILON);
}
