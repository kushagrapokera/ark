use super::*;
use crate::text::{TextSystem, UI_FONT_SIZE};
use crate::zoom::{MAX_ZOOM, MIN_ZOOM};

fn zoomed_layout(nominal_w: f64, nominal_h: f64, device_scale: f64, zoom: f64) -> AppLayout {
    let scale = device_scale * zoom;
    app_layout_with_visibility_and_zoom(
        nominal_w * device_scale / scale,
        nominal_h * device_scale / scale,
        304.0,
        0.5,
        2,
        LayoutVisibility {
            explorer: true,
            terminal: true,
        },
        Some(180.0),
        zoom,
    )
}

fn assert_rect_in_bounds(name: &str, rect: Rect, w: f64, h: f64) {
    assert!(
        rect.x.is_finite() && rect.y.is_finite() && rect.w.is_finite() && rect.h.is_finite(),
        "{name} must be finite: {rect:?}"
    );
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let epsilon = 1e-9;
    assert!(
        rect.x >= -epsilon,
        "{name} starts outside the viewport: {rect:?}"
    );
    assert!(
        rect.y >= -epsilon,
        "{name} starts outside the viewport: {rect:?}"
    );
    assert!(
        rect.x + rect.w <= w + epsilon,
        "{name} exceeds viewport width {w}: {rect:?}"
    );
    assert!(
        rect.y + rect.h <= h + epsilon,
        "{name} exceeds viewport height {h}: {rect:?}"
    );
}

#[test]
fn zoom_keeps_nominal_responsive_breakpoints() {
    let visibility = LayoutVisibility {
        explorer: true,
        terminal: true,
    };
    let base = app_layout_with_visibility(1024.0, 768.0, 304.0, 0.5, 2, visibility, Some(180.0));
    let zoomed = zoomed_layout(1024.0, 768.0, 1.0, MAX_ZOOM);

    assert!(base.explorer_card.w > 0.0);
    assert!(
        zoomed.explorer_card.w > 0.0,
        "zooming in must not hide the Explorer at the same nominal window size"
    );
    assert_eq!(zoomed.ui_scale, base.ui_scale);
}

#[test]
fn zoomed_layout_stays_inside_the_effective_viewport() {
    for (nominal_w, nominal_h) in [
        (400.0, 300.0),
        (1024.0, 768.0),
        (1440.0, 900.0),
        (2048.0, 1536.0),
    ] {
        for device_scale in [1.0, 2.0] {
            for zoom in [MIN_ZOOM, 1.0, MAX_ZOOM] {
                let layout = zoomed_layout(nominal_w, nominal_h, device_scale, zoom);
                let w = nominal_w / zoom;
                let h = nominal_h / zoom;
                for (name, rect) in [
                    ("title bar", layout.title_bar),
                    ("workbench", layout.workbench),
                    ("activity bar", layout.activity_bar),
                    ("explorer card", layout.explorer_card),
                    ("explorer header", layout.explorer_header),
                    ("explorer root", layout.explorer_root),
                    ("tree panel", layout.tree_panel),
                    ("scroll track", layout.scroll_track),
                    ("split area", layout.split_area),
                    ("editor card", layout.editor_card),
                    ("editor tabs", layout.editor_tabs),
                    ("breadcrumb", layout.breadcrumb),
                    ("viewer pane", layout.viewer_pane),
                    ("editor group split", layout.editor_group_split),
                    ("terminal dock", layout.term_dock),
                    ("terminal resize", layout.terminal_resize),
                    ("terminal toolbar", layout.terminal_toolbar),
                    ("terminal grid", layout.term_grid),
                ] {
                    assert_rect_in_bounds(name, rect, w, h);
                }
                for (index, group) in layout.editor_groups.iter().enumerate() {
                    assert_rect_in_bounds(&format!("group {index} tabs"), group.tabs, w, h);
                    assert_rect_in_bounds(
                        &format!("group {index} breadcrumb"),
                        group.breadcrumb,
                        w,
                        h,
                    );
                    assert_rect_in_bounds(&format!("group {index} body"), group.body, w, h);
                }
            }
        }
    }
}

#[test]
fn terminal_grid_fits_at_every_zoom_level() {
    let mut text = TextSystem::new();
    for nominal_w in [1024.0, 1440.0] {
        for device_scale in [1.0, 2.0] {
            for zoom in [MIN_ZOOM, 1.0, MAX_ZOOM] {
                let layout = zoomed_layout(nominal_w, 900.0, device_scale, zoom);
                let scale = device_scale * zoom;
                let font_size = UI_FONT_SIZE as f64 * scale * layout.ui_scale;
                let metrics = text.mono_metrics(font_size as f32, (font_size * 1.35) as f32);
                let advance = f64::from(metrics.advance) / scale;
                let size = terminal_grid_size(&mut text, scale, layout.ui_scale, layout.term_grid)
                    .expect("visible terminal grid");
                assert!(size.cols >= 2 && size.rows >= 2);
                assert!(size.cols as f64 * advance <= layout.term_grid.w + 1e-9);
                assert!(size.rows as f64 * LINE_H <= layout.term_grid.h + 1e-9);
            }
        }
    }
}
