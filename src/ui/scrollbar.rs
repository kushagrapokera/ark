//! Vertical scrollbar widget: retained offset, drag interaction, and the
//! pure thumb-layout kernel shared by every scrollable surface.

use super::{renderable, stroke_rect_inset, Pointer, Rect};
use crate::render::{Canvas, Theme};

/// Vertical scrollbar: retained offset + private drag state, with all math
/// derived from (`track_h`, `content`, `viewport`) per call so nothing
/// stores geometry. All coordinates are LOGICAL px.
///
/// - `thumb_geometry` is the pure layout kernel (thumb_y from track top).
/// - `scroll_by` moves the offset by a delta and reports change.
/// - `on_pointer` drives thumb dragging from the per-frame pointer.
/// - `draw` paints track + thumb; the thumb turns accent while dragged.
///
/// Degenerate inputs (non-positive track/viewport, content ≤ viewport,
/// non-finite offsets) are explicit no-ops — the offset is forced to 0 and
/// no thumb rasterizes; nothing panics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scrollbar {
    /// Scroll position in content px; 0 = top of content. Kept clamped to
    /// [0, content − viewport] (forced 0 when there is no overflow).
    pub offset: f64,
    /// While dragging: the grab offset (`pointer.y − thumb top`). Private
    /// interaction state; never grows.
    drag: Option<f64>,
}

/// Pure scrollbar layout: returns `(thumb_y, thumb_h)` in LOGICAL px, with
/// `thumb_y` measured from the top of the track. The thumb height is
/// `viewport/content × track_h` and its position is proportional to the
/// clamped offset. `content ≤ viewport` means no overflow: the thumb fills
/// the track. Non-positive track/viewport yields no thumb.
pub fn thumb_geometry(track_h: f64, content: f64, viewport: f64, offset: f64) -> (f64, f64) {
    // Degenerate geometry: no thumb for a non-positive or non-finite
    // track/viewport/content (a NaN input must not produce a NaN thumb).
    if track_h <= 0.0
        || viewport <= 0.0
        || !track_h.is_finite()
        || !viewport.is_finite()
        || !content.is_finite()
    {
        return (0.0, 0.0);
    }
    if content <= viewport {
        return (0.0, track_h); // no overflow: the thumb fills the track
    }
    let thumb_h = viewport / content * track_h;
    let max_offset = content - viewport;
    let offset = if offset.is_finite() { offset.clamp(0.0, max_offset) } else { 0.0 };
    let thumb_y = offset / max_offset * (track_h - thumb_h);
    (thumb_y, thumb_h)
}

impl Scrollbar {
    /// Force the offset back into range: clamped to [0, content − viewport],
    /// forced to 0 when there is no overflow (or degenerate/NaN inputs).
    pub fn clamp(&mut self, content: f64, viewport: f64) {
        let max = content - viewport;
        if !max.is_finite() || max <= 0.0 {
            self.offset = 0.0; // no overflow (or degenerate input)
        } else {
            self.offset = if self.offset.is_finite() {
                self.offset.clamp(0.0, max)
            } else {
                0.0
            };
        }
    }

    /// Scroll the offset by `dy` content px (positive = toward the end),
    /// clamped; returns whether the offset changed.
    #[allow(dead_code)] // exercised by the demo scene (test builds); later scrollable surfaces use it
    pub fn scroll_by(&mut self, dy: f64, content: f64, viewport: f64) -> bool {
        if !dy.is_finite() {
            return false;
        }
        let before = self.offset;
        self.offset += dy;
        self.clamp(content, viewport);
        self.offset != before
    }

    /// Feed the per-frame pointer to the scrollbar: pressing the thumb
    /// starts a drag, dragging moves the offset proportionally, releasing
    /// ends it. Returns whether the offset changed this frame.
    pub fn on_pointer(&mut self, track: Rect, pointer: Pointer, content: f64, viewport: f64) -> bool {
        let before = self.offset;
        self.clamp(content, viewport);
        if !renderable(track, 1.0) {
            self.drag = None;
            return self.offset != before;
        }
        let (_, thumb_h) = thumb_geometry(track.h, content, viewport, self.offset);
        match self.drag {
            // Active drag: the thumb top follows pointer.y − grab offset,
            // converted back into content offset proportionally. The drag
            // keeps working outside the track (standard scrollbar grab).
            Some(grab) if pointer.down => {
                let free = track.h - thumb_h;
                if free > 0.0 {
                    let thumb_top = (pointer.y - grab - track.y).clamp(0.0, free);
                    let max_offset = content - viewport;
                    let target = thumb_top / free * max_offset.max(0.0);
                    // A non-finite pointer (NaN y) must not corrupt the
                    // offset — the move is ignored, the last valid offset
                    // stands.
                    if target.is_finite() {
                        self.offset = target;
                    }
                } else {
                    self.offset = 0.0;
                }
            }
            Some(_) => self.drag = None, // released
            // Not dragging: a press on the thumb starts one. The grab offset
            // is remembered so the thumb doesn't jump to the pointer.
            None if pointer.down && thumb_h > 0.0 && content > viewport => {
                let (thumb_y, _) = thumb_geometry(track.h, content, viewport, self.offset);
                let top = track.y + thumb_y;
                let bottom = top + thumb_h;
                if pointer.x > track.x
                    && pointer.x < track.x + track.w
                    && pointer.y > top
                    && pointer.y < bottom
                {
                    self.drag = Some(pointer.y - top);
                }
            }
            None => {}
        }
        self.offset != before
    }

    /// Draw the track (bg_elevated fill, 1px border outline) and the thumb
    /// (fg_dim fill inset 1px from the track edges; accent while dragged).
    /// Degenerate tracks or scales rasterize nothing.
    pub fn draw(&self, canvas: &mut Canvas, track: Rect, content: f64, viewport: f64, theme: &Theme, scale: f64) {
        if !renderable(track, scale) {
            return;
        }
        canvas.fill_rect(track.x, track.y, track.w, track.h, theme.bg_elevated, scale);
        stroke_rect_inset(canvas.pixmap_mut(), track.scaled(scale), theme.border, scale as f32);
        let (thumb_y, thumb_h) = thumb_geometry(track.h, content, viewport, self.offset);
        if thumb_h > 0.0 {
            let color = if self.drag.is_some() { theme.accent } else { theme.fg_dim };
            canvas.fill_rect(track.x + 1.0, track.y + thumb_y, track.w - 2.0, thumb_h, color, scale);
        }
    }
}

// The widget's test module is large (all scrollbar pins traveled here from
// ui.rs) and would push this file past the 400-line guidance, so it lives in
// the widget's submodule directory.
#[cfg(test)]
mod tests;
