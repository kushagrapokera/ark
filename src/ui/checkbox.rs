//! Checkbox widget: click-to-toggle square with a check-mark glyph, drawn
//! as a stroked path like the icons.rs glyphs.

// Retained for the future Source Control card.
#![allow(dead_code)]

use tiny_skia::PathBuilder;

use super::{renderable, stroke_path, stroke_rect_inset, Pointer, Rect};
use crate::render::{Canvas, ColorRgb, Theme};

/// Draw a checkbox: square box stroked with `border` (idle/hover) or
/// `accent` (clicked); when checked, the box is filled with `accent` and a
/// check mark (two `fg` stroke lines) is drawn inside. Returns the NEW
/// checked value — toggled iff `pointer.click` lands inside the rect.
///
/// Degenerate rects (w or h ≤ 0) or scale ≤ 0 render nothing and return the
/// unchanged `checked` value, regardless of pointer state.
pub fn draw_checkbox(
    canvas: &mut Canvas,
    rect: Rect,
    checked: bool,
    theme: &Theme,
    pointer: Pointer,
    scale: f64,
) -> bool {
    if !renderable(rect, scale) {
        return checked; // degenerate: render nothing
    }
    let clicked = pointer.click && rect.contains(pointer.x, pointer.y);
    let new_checked = if clicked { !checked } else { checked };

    let phys = rect.scaled(scale);
    if checked {
        canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.accent, scale);
    }
    // 1 logical-px box outline: accent while the click is processed here.
    let border_color = if clicked { theme.accent } else { theme.border };
    stroke_rect_inset(canvas.pixmap_mut(), phys, border_color, scale as f32);
    if checked {
        draw_check(canvas.pixmap_mut(), phys, theme.fg, scale);
    }

    new_checked
}

/// Check mark inside a filled checkbox: two fg-colored stroke lines joined
/// at the elbow, drawn as a path like the icons.rs glyphs.
fn draw_check(pixmap: &mut tiny_skia::Pixmap, box_rect: Rect, color: ColorRgb, scale: f64) {
    let mut pb = PathBuilder::new();
    pb.move_to(
        (box_rect.x + 0.30 * box_rect.w) as f32,
        (box_rect.y + 0.55 * box_rect.h) as f32,
    );
    pb.line_to(
        (box_rect.x + 0.45 * box_rect.w) as f32,
        (box_rect.y + 0.70 * box_rect.h) as f32,
    );
    pb.line_to(
        (box_rect.x + 0.72 * box_rect.w) as f32,
        (box_rect.y + 0.32 * box_rect.h) as f32,
    );
    if let Some(path) = pb.finish() {
        stroke_path(pixmap, &path, color, (1.75 * scale) as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{count_color_in, count_not_color, count_not_color_in};

    // ---- draw_checkbox (acceptance 4, 5) ----

    #[test]
    fn checkbox_toggles_only_on_inside_click() {
        let theme = Theme { ..Theme::DARK };
        let mut canvas = Canvas::new(100, 60);
        let rect = Rect { x: 10.0, y: 10.0, w: 16.0, h: 16.0 };

        let click_inside = Pointer { x: 18.0, y: 18.0, down: false, click: true };
        let click_outside = Pointer { x: 60.0, y: 45.0, down: false, click: true };
        let no_click = Pointer { x: 18.0, y: 18.0, down: true, click: false };

        // Click inside toggles in both directions.
        assert!(draw_checkbox(&mut canvas, rect, false, &theme, click_inside, 1.0));
        assert!(!draw_checkbox(&mut canvas, rect, true, &theme, click_inside, 1.0));
        // Click outside never changes the value.
        assert!(!draw_checkbox(&mut canvas, rect, false, &theme, click_outside, 1.0));
        assert!(draw_checkbox(&mut canvas, rect, true, &theme, click_outside, 1.0));
        // Pointer present without a completed click never changes the value.
        assert!(!draw_checkbox(&mut canvas, rect, false, &theme, no_click, 1.0));
        assert!(draw_checkbox(&mut canvas, rect, true, &theme, no_click, 1.0));
    }

    #[test]
    fn checkbox_paints_check_when_checked() {
        let theme = Theme { ..Theme::DARK };
        let mut canvas = Canvas::new(60, 40);
        let rect = Rect { x: 10.0, y: 10.0, w: 20.0, h: 20.0 };
        let none = Pointer::default();
        // Sample the box interior, inset to stay clear of the border stroke
        // and its anti-aliasing bleed.
        let (ix0, iy0, ix1, iy1) = (13, 13, 27, 27);

        // Unchecked: interior untouched — no accent, still bg_elevated.
        canvas.fill(theme.bg_elevated);
        draw_checkbox(&mut canvas, rect, false, &theme, none, 1.0);
        assert_eq!(
            count_color_in(&canvas, theme.accent, ix0, iy0, ix1, iy1),
            0,
            "unchecked box interior must not contain accent pixels"
        );
        assert_eq!(
            count_color_in(&canvas, theme.bg_elevated, ix0, iy0, ix1, iy1),
            ((ix1 - ix0) * (iy1 - iy0)) as usize,
            "unchecked box interior must stay bg_elevated"
        );

        // Checked: accent fill + check mark rasterized inside the box.
        canvas.fill(theme.bg_elevated);
        draw_checkbox(&mut canvas, rect, true, &theme, none, 1.0);
        let accent = count_color_in(&canvas, theme.accent, ix0, iy0, ix1, iy1);
        assert!(
            accent >= 100,
            "checked box must fill its interior with accent, got {accent}"
        );
        // Check mark ink over the accent fill: any blend coverage counts
        // (measured 39 px for the two 1.75px strokes; deleting draw_check
        // zeroes this). Measured exact-fg core pixels are 10, so the color
        // pin below uses presence rather than the 15 threshold.
        let ink = count_not_color_in(&canvas, theme.accent, ix0, iy0, ix1, iy1);
        assert!(
            ink >= 15,
            "check mark must rasterize inside the box interior, got {ink}"
        );
        assert!(
            count_color_in(&canvas, theme.fg, ix0, iy0, ix1, iy1) > 0,
            "check mark strokes must be fg-colored"
        );
    }

    // ---- Degenerate inputs (robustness contract) ----

    #[test]
    fn checkbox_degenerate_rect_or_scale_renders_nothing() {
        let theme = Theme { ..Theme::DARK };
        let mut canvas = Canvas::new(80, 60);
        canvas.fill(theme.bg);
        let click = Pointer { x: 15.0, y: 15.0, down: false, click: true };
        let zero_h = Rect { x: 10.0, y: 10.0, w: 16.0, h: 0.0 };
        let negative_w = Rect { x: 10.0, y: 10.0, w: -16.0, h: 16.0 };
        let valid_rect = Rect { x: 10.0, y: 10.0, w: 16.0, h: 16.0 };

        // No panic, value unchanged, and nothing rasterizes.
        assert!(
            draw_checkbox(&mut canvas, zero_h, true, &theme, click, 1.0),
            "zero-height rect keeps checked=true"
        );
        assert!(
            !draw_checkbox(&mut canvas, zero_h, false, &theme, click, 1.0),
            "zero-height rect keeps checked=false"
        );
        assert!(
            draw_checkbox(&mut canvas, negative_w, true, &theme, click, 1.0),
            "negative extent keeps the checked value"
        );
        assert!(
            draw_checkbox(&mut canvas, valid_rect, true, &theme, click, 0.0),
            "zero scale keeps the checked value"
        );
        assert_eq!(
            count_not_color(&canvas, theme.bg),
            0,
            "degenerate draws must rasterize nothing"
        );
    }
}
