//! Widget kit: shared primitives (logical-px geometry `Rect`, per-frame
//! pointer state `Pointer`, interaction outcome `Press`), the shared raster
//! helpers, and one submodule per widget with `pub use` re-exports that keep
//! every `crate::ui::{...}` path identical across the M3 module split.
//!
//! Design contract (M2 plan, frozen):
//! - Every widget coordinate is LOGICAL px; `Rect::scaled` converts to
//!   physical px at raster time. Widgets never store scale.
//! - `Pointer.click` is `true` only on the frame processing a completed
//!   press→release at that position. Widgets decide "clicked" purely from
//!   `pointer` + containment — stateless drawing, caller-owned state.
//! - All colors flow through `Theme` tokens; nothing is hardcoded here.
//! - Degenerate frames — non-positive rect extents or a non-positive scale —
//!   render nothing: no panic, no pixels. `draw_button` reports `Idle`,
//!   `draw_checkbox` returns the unchanged value (see `renderable`).

// Widgets with no production consumer yet are test-gated: the demo scene
// exercises them in test builds, and the milestones that wire them (M9
// config editor) un-gate the declarations. M8's commit panel consumes the
// checkbox + single-line text input, so both are live now. Gating keeps dead
// code out of the production binary instead of allowing it.
mod button;
mod checkbox;
mod scrollbar;
mod split;
#[cfg(test)]
mod tabbar;
mod text_input;

// The re-export block preserves the pre-split `crate::ui::{...}` paths for
// every call site.
pub use scrollbar::Scrollbar;
pub use split::Split;
pub use button::{draw_button, Press};
pub use checkbox::draw_checkbox;
#[cfg(test)]
pub use tabbar::{TabBar, TAB_HEIGHT};
pub use text_input::{TextInput, TextInputKey};

use tiny_skia::{LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::render::ColorRgb;

/// Logical-px rectangle. `w`/`h` are extents (never negative in practice);
/// `scaled` produces the physical-px rect used at raster time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// Strict-interior containment: a point exactly on any edge is outside,
    /// so adjacent widgets never both claim a boundary point.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x > self.x && x < self.x + self.w && y > self.y && y < self.y + self.h
    }

    /// Multiply every component by `scale` (logical px → physical px).
    /// A scale of 0 collapses the rect to zero size; the widget draw
    /// functions treat that envelope as explicitly degenerate (`renderable`).
    pub fn scaled(&self, scale: f64) -> Rect {
        Rect {
            x: self.x * scale,
            y: self.y * scale,
            w: self.w * scale,
            h: self.h * scale,
        }
    }
}

/// Pointer state for the current frame, in logical px.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub down: bool,
    /// `true` ONLY on the frame processing a completed press→release here.
    pub click: bool,
}

/// Widget render envelope: positive extents and a positive scale. Anything
/// else is degenerate — the draw functions return before touching the
/// raster stack (which would panic on negative extents or a zero font
/// size). Also rejects NaN extents/scale, since `NaN > 0.0` is false.
fn renderable(rect: Rect, scale: f64) -> bool {
    rect.w > 0.0 && rect.h > 0.0 && scale > 0.0
}

/// Stroke the outline of a physical-px rect with the stroke kept inside the
/// rect (path inset by half the stroke width), so widgets never paint
/// outside their layout bounds.
fn stroke_rect_inset(pixmap: &mut Pixmap, rect: Rect, color: ColorRgb, width: f32) {
    let inset = f64::from(width) / 2.0;
    let mut pb = PathBuilder::new();
    pb.move_to((rect.x + inset) as f32, (rect.y + inset) as f32);
    pb.line_to((rect.x + rect.w - inset) as f32, (rect.y + inset) as f32);
    pb.line_to((rect.x + rect.w - inset) as f32, (rect.y + rect.h - inset) as f32);
    pb.line_to((rect.x + inset) as f32, (rect.y + rect.h - inset) as f32);
    pb.close();
    if let Some(path) = pb.finish() {
        stroke_path(pixmap, &path, color, width);
    }
}

/// Stroke one path in physical px with the shared widget look: opaque color,
/// anti-aliased, round caps and joins (same aesthetic as icons.rs).
fn stroke_path(pixmap: &mut Pixmap, path: &Path, color: ColorRgb, width: f32) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, 255);
    paint.anti_alias = true;
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
}

/// Pixel-count helpers shared by the per-widget test modules and the
/// panel/frame pixel pins elsewhere in the crate (test-only, so this module
/// does not exist in non-test builds).
#[cfg(test)]
pub(crate) mod testutil {
    use crate::render::Canvas;

    /// Pixels in the half-open physical region [x0,x1) × [y0,y1) whose RGB
    /// equals `color` exactly (tiny-skia fills are un-AA'd, so exact matches
    /// are deterministic).
    pub(crate) fn count_color_in(
        canvas: &Canvas,
        color: crate::render::ColorRgb,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) -> usize {
        let w = canvas.width() as usize;
        let (px, _) = canvas.pixmap().data().as_chunks::<4>();
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = &px[y as usize * w + x as usize];
                if (p[0], p[1], p[2]) == color {
                    n += 1;
                }
            }
        }
        n
    }

    /// Pixels anywhere on the canvas whose RGB differs from `color`.
    pub(crate) fn count_not_color(canvas: &Canvas, color: crate::render::ColorRgb) -> usize {
        let (px, _) = canvas.pixmap().data().as_chunks::<4>();
        px.iter().filter(|p| (p[0], p[1], p[2]) != color).count()
    }

    /// Pixels in the half-open region whose RGB differs from `color` — ink
    /// coverage over a known exact fill (any AA blend level counts).
    pub(crate) fn count_not_color_in(
        canvas: &Canvas,
        color: crate::render::ColorRgb,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) -> usize {
        let w = canvas.width() as usize;
        let (px, _) = canvas.pixmap().data().as_chunks::<4>();
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = &px[y as usize * w + x as usize];
                if (p[0], p[1], p[2]) != color {
                    n += 1;
                }
            }
        }
        n
    }

    /// Inclusive bounding box of pixels whose RGB differs from `bg`, within
    /// the half-open physical region [x0,x1) × [y0,y1). With chrome tokens
    /// set to bg it isolates text ink for baseline placement pins.
    pub(crate) fn ink_bbox_in(
        canvas: &Canvas,
        bg: crate::render::ColorRgb,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        let w = canvas.width() as usize;
        let (px, _) = canvas.pixmap().data().as_chunks::<4>();
        let (mut bx0, mut by0, mut bx1, mut by1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        let mut any = false;
        for y in y0..y1.min(canvas.height()) {
            for x in x0..x1.min(canvas.width()) {
                let p = &px[y as usize * w + x as usize];
                if (p[0], p[1], p[2]) != bg {
                    any = true;
                    bx0 = bx0.min(x);
                    by0 = by0.min(y);
                    bx1 = bx1.max(x);
                    by1 = by1.max(y);
                }
            }
        }
        any.then_some((bx0, by0, bx1, by1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Rect (acceptance 1) ----

    #[test]
    fn rect_contains_and_scale() {
        let r = Rect { x: 10.0, y: 20.0, w: 30.0, h: 40.0 };
        assert!(r.contains(25.0, 40.0), "center point is inside");
        // Strict interior: a point exactly on an edge is NOT contained.
        assert!(!r.contains(10.0, 40.0), "left edge excluded");
        assert!(!r.contains(40.0, 40.0), "right edge excluded");
        assert!(!r.contains(25.0, 20.0), "top edge excluded");
        assert!(!r.contains(25.0, 60.0), "bottom edge excluded");
        assert!(!r.contains(10.0, 20.0), "corner excluded");
        // Outside on every side.
        assert!(!r.contains(9.0, 40.0), "left of rect");
        assert!(!r.contains(41.0, 40.0), "right of rect");
        assert!(!r.contains(25.0, 19.0), "above rect");
        assert!(!r.contains(25.0, 61.0), "below rect");

        // scaled() multiplies x/y/w/h by scale.
        assert_eq!(r.scaled(2.0), Rect { x: 20.0, y: 40.0, w: 60.0, h: 80.0 });
        assert_eq!(r.scaled(1.0), r, "scale 1 is the identity");
    }
}
