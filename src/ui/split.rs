//! Vertical split divider widget: a single draggable x position with an
//! inclusive ±4 logical-px hit band; drag state stays with the scene.

use super::{renderable, Pointer, Rect};
use crate::render::{Canvas, Theme};

/// Vertical split divider: a single x position (LOGICAL px), clamped into
/// `[min, max]` by `drag_to`. The hit band is ±4 LOGICAL px around the
/// divider; the drawn strip is 2px wide, centered on `position` and spans
/// the divider's vertical span (`area`). Whether the divider is currently
/// being dragged is scene state (the scene draws the accent overlay while
/// it drags) — the widget itself stays stateless beyond `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Split {
    /// Divider x position in LOGICAL px.
    pub position: f64,
}

impl Split {
    /// True when the pointer could grab the divider: within ±4 LOGICAL px of
    /// `position` horizontally and inside the area's vertical span (strict
    /// interior, matching `Rect::contains`). The `scale` parameter is part
    /// of the frozen signature but the band is defined in logical px, so it
    /// does not affect the test.
    #[allow(dead_code)] // retained for the classic divider surface
    pub fn hit_divider(&self, area: Rect, pointer: Pointer, scale: f64) -> bool {
        let _ = scale;
        // Strict-interior vertical span, like Rect::contains; inclusive ±4
        // logical px band around the divider.
        pointer.y > area.y
            && pointer.y < area.y + area.h
            && (pointer.x - self.position).abs() <= 4.0
    }

    /// Move the divider to `x`, clamped into [min, max]. Inverted bounds
    /// (min > max, e.g. after a window shrink) are normalized so this can
    /// never panic; non-finite `x` is ignored.
    pub fn drag_to(&mut self, x: f64, min: f64, max: f64) {
        if !x.is_finite() {
            return;
        }
        let (lo, hi) = (min.min(max), max.max(min));
        self.position = x.clamp(lo, hi);
    }

    /// Draw the resting divider strip: 2px of `border` centered on
    /// `position`, spanning `area` vertically. Degenerate areas, scales or
    /// positions rasterize nothing.
    #[allow(dead_code)] // retained for the classic divider surface
    pub fn draw(&self, canvas: &mut Canvas, area: Rect, theme: &Theme, scale: f64) {
        if !renderable(area, scale) || !self.position.is_finite() {
            return;
        }
        canvas.fill_rect(self.position - 1.0, area.y, 2.0, area.h, theme.border, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{count_color_in, count_not_color};

    // ---- Split (acceptance 4) ----

    #[test]
    fn split_hit_and_clamp() {
        let area = Rect { x: 0.0, y: 100.0, w: 800.0, h: 200.0 };
        let mut split = Split { position: 400.0 };

        // Within ±4 logical px of the divider, inside the area → hit.
        assert!(split.hit_divider(area, Pointer { x: 397.0, y: 200.0, ..Pointer::default() }, 1.0));
        assert!(split.hit_divider(area, Pointer { x: 404.0, y: 200.0, ..Pointer::default() }, 1.0));
        assert!(
            split.hit_divider(area, Pointer { x: 396.0, y: 200.0, ..Pointer::default() }, 1.0),
            "the ±4 band is inclusive"
        );
        // Beyond the band → miss.
        assert!(!split.hit_divider(area, Pointer { x: 395.9, y: 200.0, ..Pointer::default() }, 1.0));
        assert!(!split.hit_divider(area, Pointer { x: 404.1, y: 200.0, ..Pointer::default() }, 1.0));
        // Outside the divider's vertical span → miss even at the exact x.
        assert!(!split.hit_divider(area, Pointer { x: 400.0, y: 99.0, ..Pointer::default() }, 1.0));
        assert!(!split.hit_divider(area, Pointer { x: 400.0, y: 301.0, ..Pointer::default() }, 1.0));

        // drag_to clamps into [min, max].
        split.drag_to(500.0, 100.0, 600.0);
        assert_eq!(split.position, 500.0);
        split.drag_to(50.0, 100.0, 600.0);
        assert_eq!(split.position, 100.0);
        split.drag_to(900.0, 100.0, 600.0);
        assert_eq!(split.position, 600.0);
        // Inverted bounds (shrunken window) must not panic.
        split.drag_to(50.0, 600.0, 100.0);
        assert_eq!(split.position, 100.0);
        // Non-finite x is ignored (position unchanged, no panic) — a mutant
        // feeding x straight into clamp would produce NaN here.
        split.drag_to(f64::NAN, 100.0, 600.0);
        assert_eq!(split.position, 100.0);
        split.drag_to(f64::INFINITY, 100.0, 600.0);
        assert_eq!(split.position, 100.0);
    }

    #[test]
    fn split_draws_2px_border_strip() {
        let theme = Theme { ..Theme::DARK };
        let mut canvas = Canvas::new(100, 60);
        canvas.fill(theme.bg);
        let area = Rect { x: 0.0, y: 10.0, w: 100.0, h: 40.0 };
        let split = Split { position: 50.0 };

        split.draw(&mut canvas, area, &theme, 1.0);
        // 2px-wide strip centered on the position, spanning the area exactly.
        assert_eq!(
            count_color_in(&canvas, theme.border, 49, 10, 51, 50),
            2 * 40,
            "strip must be exactly 2px × area height"
        );
        assert_eq!(
            count_color_in(&canvas, theme.border, 40, 10, 49, 50),
            0,
            "no strip ink left of the 2px band"
        );
        assert_eq!(
            count_color_in(&canvas, theme.border, 51, 10, 60, 50),
            0,
            "no strip ink right of the 2px band"
        );

        // Degenerate: zero-height area and zero scale rasterize nothing.
        canvas.fill(theme.bg);
        split.draw(&mut canvas, Rect { x: 0.0, y: 10.0, w: 100.0, h: 0.0 }, &theme, 1.0);
        split.draw(&mut canvas, area, &theme, 0.0);
        assert_eq!(
            count_not_color(&canvas, theme.bg),
            0,
            "degenerate split draws rasterize nothing"
        );
    }
}
