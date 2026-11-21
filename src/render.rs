//! Software canvas: tiny-skia rasterization target, damage tracking and the
//! present blit into softbuffer's `0x00RRGGBB` pixel format.
//!
//! The canvas is *persistent* between frames: damaged regions are re-rastered
//! in place, then presented via `present_with_damage` (zero-copy partial
//! presents on Wayland). Nothing here grows with UI content.

use std::num::NonZeroU32;
use std::sync::Arc;

use log::error;
use softbuffer::Surface;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 8-bit RGB color token (alpha is always opaque on the canvas).
pub type ColorRgb = (u8, u8, u8);

/// Theme tokens (Catppuccin Mocha-inspired). All UI color flows through this
/// struct so the visual language stays consistent. The syntax tokens are the
/// generic token palette the M4 regex highlighter maps to.
pub struct Theme {
    pub bg: ColorRgb,
    pub bg_elevated: ColorRgb,
    pub fg: ColorRgb,
    pub fg_dim: ColorRgb,
    pub accent: ColorRgb,
    pub border: ColorRgb,
    pub selection: ColorRgb,
    /// Keywords / control flow (mauve).
    pub syntax_keyword: ColorRgb,
    /// String + char literals (green).
    pub syntax_string: ColorRgb,
    /// Numeric literals (peach).
    pub syntax_number: ColorRgb,
    /// Comments (overlay0 — dimmer than fg_dim).
    pub syntax_comment: ColorRgb,
    /// Git tree-badge / gutter-mark colors (M7): modified, added, deleted,
    /// untracked.
    pub git_modified: ColorRgb,
    pub git_added: ColorRgb,
    pub git_deleted: ColorRgb,
    pub git_untracked: ColorRgb,
}

impl Theme {
    pub const DARK: Theme = Theme {
        bg: (0x1E, 0x1E, 0x2E),
        bg_elevated: (0x24, 0x24, 0x37),
        fg: (0xCD, 0xD6, 0xF4),
        fg_dim: (0x93, 0x99, 0xB2),
        accent: (0x89, 0xB4, 0xFA),
        border: (0x31, 0x32, 0x44),
        selection: (0x45, 0x47, 0x5A),
        syntax_keyword: (0xCB, 0xA6, 0xF7),
        syntax_string: (0xA6, 0xE3, 0xA1),
        syntax_number: (0xFA, 0xB3, 0x87),
        syntax_comment: (0x6C, 0x70, 0x86),
        git_modified: (0xF9, 0xE2, 0xAF),
        git_added: (0xA6, 0xE3, 0xA1),
        git_deleted: (0xF3, 0x8B, 0xA8),
        git_untracked: (0x94, 0xE2, 0xD5),
    };
}

/// Region of the canvas that needs re-rasterization (physical pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Damage accumulator. `full` short-circuits to whole-canvas rasterization.
/// Memory: bounded by the number of rects we choose to track (capped).
pub struct Damage {
    full: bool,
    rects: Vec<DamageRect>,
    max_rects: usize,
}

impl Damage {
    pub fn new(max_rects: usize) -> Self {
        Self {
            full: true, // first frame is always full
            rects: Vec::new(),
            max_rects,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full && self.rects.is_empty()
    }

    pub fn mark_full(&mut self) {
        self.full = true;
        self.rects.clear();
    }

    /// Add a rect; degrades to full damage when tracking would exceed the cap.
    pub fn mark(&mut self, rect: DamageRect) {
        if self.full {
            return;
        }
        if self.rects.len() >= self.max_rects {
            self.mark_full();
            return;
        }
        self.rects.push(rect);
    }

    /// Take the pending damage, clearing the accumulator.
    /// Returns `None` when nothing was damaged.
    pub fn take(&mut self, canvas_w: u32, canvas_h: u32) -> Option<Vec<DamageRect>> {
        if self.is_empty() {
            return None;
        }
        if self.full {
            self.full = false;
            return Some(vec![DamageRect {
                x: 0,
                y: 0,
                width: canvas_w,
                height: canvas_h,
            }]);
        }
        Some(std::mem::take(&mut self.rects))
    }
}

/// Persistent raster target. Pixels are premultiplied RGBA8 (tiny-skia
/// layout); always opaque where drawn.
pub struct Canvas {
    pixmap: Pixmap,
    width: u32,
    height: u32,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width.max(1), height.max(1)).expect("valid canvas size");
        Self {
            pixmap,
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if (width, height) == (self.width, self.height) {
            return;
        }
        // Recreating drops old content; callers must mark full damage.
        self.pixmap = Pixmap::new(width, height).expect("valid canvas size");
        self.width = width;
        self.height = height;
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn fill(&mut self, color: ColorRgb) {
        self.pixmap.fill(tiny_skia::Color::from_rgba8(
            color.0, color.1, color.2, 255,
        ));
    }

    /// Fill a logical-space rect (scaled internally) with an opaque color.
    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: ColorRgb, scale: f64) {
        let rect = tiny_skia::Rect::from_xywh(
            (x * scale) as f32,
            (y * scale) as f32,
            (w * scale) as f32,
            (h * scale) as f32,
        )
        .expect("valid rect");
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color.0, color.1, color.2, 255);
        paint.anti_alias = false;
        self.pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    /// 1-logical-px border outline of a logical-space rect via four un-AA'd
    /// fill strips — exact pixels with no AA seam against blitted interiors.
    pub fn stroke_rect_strips(&mut self, x: f64, y: f64, w: f64, h: f64, color: ColorRgb, scale: f64) {
        if scale > 0.0 && w >= 2.0 && h >= 2.0 {
            self.fill_rect(x, y, w, 1.0, color, scale);
            self.fill_rect(x, y + h - 1.0, w, 1.0, color, scale);
            self.fill_rect(x, y, 1.0, h, color, scale);
            self.fill_rect(x + w - 1.0, y, 1.0, h, color, scale);
        }
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }
}

/// Convert the premultiplied-RGBA canvas into softbuffer's `0x00RRGGBB`
/// buffer. The canvas is opaque, so premultiplied == straight channels.
pub fn blit_to_buffer(pixmap: &Pixmap, buffer: &mut [u32]) {
    let data = pixmap.data();
    debug_assert_eq!(data.len(), buffer.len() * 4);
    let (px_chunks, _) = data.as_chunks::<4>();
    for (dst, px) in buffer.iter_mut().zip(px_chunks) {
        *dst = (px[0] as u32) << 16 | (px[1] as u32) << 8 | px[2] as u32;
    }
}

/// Blit the canvas into the surface buffer and present it — partial damage
/// when `rects` doesn't cover the whole frame. Acquire/present failures are
/// logged (never silently dropped); returns whether the frame was presented.
pub fn present(
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    canvas: &Canvas,
    rects: &[DamageRect],
) -> bool {
    match surface.buffer_mut() {
        Ok(mut buffer) => {
            blit_to_buffer(canvas.pixmap(), &mut buffer);
            let sb_rects: Vec<softbuffer::Rect> = rects
                .iter()
                .filter_map(|r| {
                    Some(softbuffer::Rect {
                        x: r.x,
                        y: r.y,
                        width: NonZeroU32::new(r.width.max(1))?,
                        height: NonZeroU32::new(r.height.max(1))?,
                    })
                })
                .collect();
            // Full-frame present skips softbuffer's damage computation.
            let full = sb_rects.len() == 1
                && sb_rects[0].x == 0
                && sb_rects[0].y == 0
                && sb_rects[0].width.get() == canvas.width()
                && sb_rects[0].height.get() == canvas.height();
            let result = if full {
                buffer.present()
            } else {
                buffer.present_with_damage(&sb_rects)
            };
            if let Err(e) = result {
                error!("present failed: {e}");
                return false;
            }
            true
        }
        Err(e) => {
            error!("acquiring surface buffer failed: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Damage accumulator ----

    #[test]
    fn damage_starts_full_and_clears_on_take() {
        let mut d = Damage::new(4);
        assert!(!d.is_empty());
        let rects = d.take(800, 600).expect("first take is full damage");
        let r = rects.first().expect("full damage yields one rect");
        assert_eq!((r.x, r.y, r.width, r.height), (0, 0, 800, 600));
        assert!(d.is_empty(), "take must clear pending damage");
        assert!(d.take(800, 600).is_none(), "nothing pending after take");
    }

    #[test]
    fn damage_marked_rects_are_returned() {
        let mut d = Damage::new(4);
        d.take(800, 600); // consume initial full damage
        d.mark(DamageRect { x: 10, y: 20, width: 5, height: 6 });
        d.mark(DamageRect { x: 0, y: 0, width: 1, height: 1 });
        let rects = d.take(800, 600).expect("two marked rects");
        assert_eq!(rects.len(), 2);
        assert!(d.is_empty());
    }

    #[test]
    fn damage_degrades_to_full_at_cap() {
        let mut d = Damage::new(2);
        d.take(800, 600);
        for i in 0..3 {
            d.mark(DamageRect { x: i, y: 0, width: 1, height: 1 });
        }
        let rects = d.take(800, 600).expect("degraded to full");
        assert_eq!(rects.len(), 1, "cap exceeded → single full-canvas rect");
        assert_eq!(rects[0].width, 800);
    }

    #[test]
    fn mark_is_noop_while_full() {
        let mut d = Damage::new(4);
        d.mark(DamageRect { x: 1, y: 1, width: 1, height: 1 }); // initial state: full
        let rects = d.take(10, 10).expect("still full");
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].width, 10, "must be full-canvas, not the marked rect");
    }

    // ---- Canvas + blit ----

    #[test]
    fn canvas_fill_and_blit_pack_0xrrggbb() {
        let mut canvas = Canvas::new(2, 1);
        canvas.fill((255, 0, 0)); // pure red
        // paint pixel (1,0) green via the raw premultiplied pixel buffer
        let green = tiny_skia::PremultipliedColorU8::from_rgba(0, 255, 0, 255)
            .expect("opaque green");
        canvas.pixmap_mut().pixels_mut()[1] = green;

        let mut buffer = vec![0u32; 2];
        blit_to_buffer(canvas.pixmap(), &mut buffer);
        assert_eq!(buffer[0], 0x00FF_0000, "red packs to 0x00RRGGBB");
        assert_eq!(buffer[1], 0x0000_FF00, "green packs to 0x00RRGGBB");
    }

    #[test]
    fn canvas_fill_rect_scales_with_dpi() {
        let mut canvas = Canvas::new(10, 10);
        canvas.fill((0, 0, 0));
        // 2x2 logical rect at 2x scale → 4x4 physical pixels at (2,2)
        canvas.fill_rect(1.0, 1.0, 2.0, 2.0, (255, 255, 255), 2.0);
        let center = canvas.pixmap().pixel(4, 4).expect("inside rect");
        assert_eq!((center.red(), center.green(), center.blue()), (255, 255, 255));
        let outside = canvas.pixmap().pixel(0, 0).expect("outside rect");
        assert_eq!((outside.red(), outside.green(), outside.blue()), (0, 0, 0));
    }

    #[test]
    fn canvas_resize_changes_dimensions() {
        let mut canvas = Canvas::new(4, 4);
        canvas.resize(8, 2);
        assert_eq!((canvas.width(), canvas.height()), (8, 2));
    }
}
