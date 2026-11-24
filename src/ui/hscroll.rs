//! Horizontal scrollbar widget: retained pixel offset and thumb drag state.
//! Geometry is derived per call; no track or content size is stored.

use super::{renderable, stroke_rect_inset, Pointer, Rect};
use crate::render::{Canvas, Theme};

/// Logical height of a horizontal scrollbar track.
pub const HSCROLL_H: f64 = 8.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HScrollbar {
    /// Horizontal content offset in logical px.
    pub offset: f64,
    /// Pointer.x minus thumb.x while dragging.
    drag: Option<f64>,
}

/// Return `(thumb_x, thumb_w)` relative to the track's left edge.
pub fn thumb_geometry(track_w: f64, content: f64, viewport: f64, offset: f64) -> (f64, f64) {
    if track_w <= 0.0
        || viewport <= 0.0
        || !track_w.is_finite()
        || !viewport.is_finite()
        || !content.is_finite()
    {
        return (0.0, 0.0);
    }
    if content <= viewport {
        return (0.0, track_w);
    }
    let thumb_w = (viewport / content * track_w).max(20.0).min(track_w);
    let max_offset = content - viewport;
    let offset = if offset.is_finite() {
        offset.clamp(0.0, max_offset)
    } else {
        0.0
    };
    let thumb_x = offset / max_offset * (track_w - thumb_w);
    (thumb_x, thumb_w)
}

impl HScrollbar {
    pub fn clamp(&mut self, content: f64, viewport: f64) {
        let max = content - viewport;
        if !max.is_finite() || max <= 0.0 {
            self.offset = 0.0;
        } else {
            self.offset = if self.offset.is_finite() {
                self.offset.clamp(0.0, max)
            } else {
                0.0
            };
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Drive thumb dragging. A press elsewhere on the track centers the thumb
    /// there, matching the vertical scrollbar's direct manipulation feel.
    pub fn on_pointer(
        &mut self,
        track: Rect,
        pointer: Pointer,
        content: f64,
        viewport: f64,
    ) -> bool {
        let before = self.offset;
        self.clamp(content, viewport);
        if !renderable(track, 1.0) || content <= viewport {
            self.drag = None;
            return self.offset != before;
        }
        let (thumb_x, thumb_w) = thumb_geometry(track.w, content, viewport, self.offset);
        let left = track.x + thumb_x;
        let right = left + thumb_w;
        match self.drag {
            Some(grab) if pointer.down => {
                let free = track.w - thumb_w;
                if free > 0.0 {
                    let x = (pointer.x - grab - track.x).clamp(0.0, free);
                    let target = x / free * (content - viewport);
                    if target.is_finite() {
                        self.offset = target;
                    }
                }
            }
            Some(_) => self.drag = None,
            None if pointer.down && track.contains(pointer.x, pointer.y) => {
                if pointer.x > left && pointer.x < right {
                    self.drag = Some(pointer.x - left);
                } else {
                    let free = track.w - thumb_w;
                    let target = if free > 0.0 {
                        (pointer.x - track.x - thumb_w / 2.0).clamp(0.0, free) / free
                            * (content - viewport)
                    } else {
                        0.0
                    };
                    if target.is_finite() {
                        self.offset = target;
                    }
                    self.drag = Some(thumb_w / 2.0);
                }
            }
            None => {}
        }
        self.offset != before
    }

    pub fn draw(
        &self,
        canvas: &mut Canvas,
        track: Rect,
        content: f64,
        viewport: f64,
        theme: &Theme,
        scale: f64,
    ) {
        if !renderable(track, scale) || content <= viewport {
            return;
        }
        canvas.fill_rect(track.x, track.y, track.w, track.h, theme.bg_elevated, scale);
        stroke_rect_inset(
            canvas.pixmap_mut(),
            track.scaled(scale),
            theme.border_subtle,
            scale as f32,
        );
        let (thumb_x, thumb_w) = thumb_geometry(track.w, content, viewport, self.offset);
        let color = if self.drag.is_some() {
            theme.accent
        } else {
            theme.fg_dim
        };
        canvas.fill_rect(
            track.x + thumb_x,
            track.y + 1.0,
            thumb_w,
            track.h - 2.0,
            color,
            scale,
        );
    }
}
