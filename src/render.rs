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
    /// Window chrome background (`#181818` in Dark+).
    pub surface_chrome: ColorRgb,
    /// Card/editor surface (`#1F1F1F`).
    pub surface: ColorRgb,
    pub bg_elevated: ColorRgb,
    /// Hover/current-line surface (`#2A2D2E`).
    pub surface_hover: ColorRgb,
    pub fg: ColorRgb,
    pub fg_dim: ColorRgb,
    /// Muted chrome text (`#9D9D9D`).
    pub text_muted: ColorRgb,
    /// Dim code text (`#8B949E`).
    pub text_dim: ColorRgb,
    pub accent: ColorRgb,
    pub border: ColorRgb,
    /// Subtle card border (`#2B2B2B`).
    pub border_subtle: ColorRgb,
    pub selection: ColorRgb,
    /// Keywords / control flow (mauve).
    pub syntax_keyword: ColorRgb,
    /// Control-flow keywords (VS Code Dark+ orchid).
    pub syntax_control: ColorRgb,
    /// String + char literals (green).
    pub syntax_string: ColorRgb,
    /// Numeric literals (peach).
    pub syntax_number: ColorRgb,
    /// Comments (overlay0 — dimmer than fg_dim).
    pub syntax_comment: ColorRgb,
    /// Type/class names (M12 all-language highlighting).
    pub syntax_type: ColorRgb,
    /// Function/method names (VS Code Dark+ yellow).
    pub syntax_function: ColorRgb,
    /// Constants / all-caps identifiers (VS Code Dark+ light blue).
    pub syntax_constant: ColorRgb,
    /// Object properties / keys / HTML attributes (VS Code Dark+ pale blue).
    pub syntax_property: ColorRgb,
    /// Variables / parameters (VS Code Dark+ pale blue).
    pub syntax_variable: ColorRgb,
    /// Regular-expression literals (VS Code Dark+ red).
    pub syntax_regex: ColorRgb,
    /// String escape sequences (VS Code Dark+ amber).
    pub syntax_escape: ColorRgb,
    /// Decorators / annotations (VS Code Dark+ yellow).
    pub syntax_decorator: ColorRgb,
    /// CSS selectors / class and id names (VS Code Dark+ amber).
    pub syntax_selector: ColorRgb,
    /// Text painted over an `accent` fill (status bar, M12).
    pub on_accent: ColorRgb,
    /// Current editor line background.
    pub editor_current_line: ColorRgb,
    /// Search navigation match background.
    pub editor_search_match: ColorRgb,
    /// Indent-guide colors, inactive and active.
    pub editor_indent_guide: ColorRgb,
    pub editor_indent_guide_active: ColorRgb,
    /// Three bracket-pair colors, matching VS Code Dark+.
    pub editor_bracket_1: ColorRgb,
    pub editor_bracket_2: ColorRgb,
    pub editor_bracket_3: ColorRgb,
    /// Git tree-badge / gutter-mark colors (M7): modified, added, deleted,
    /// untracked.
    pub git_modified: ColorRgb,
    pub git_added: ColorRgb,
    pub git_deleted: ColorRgb,
    pub git_untracked: ColorRgb,
}

impl Theme {
    /// Original Catppuccin Mocha palette (blue-tinted). Kept for the widget
    /// test fixtures and an easy future theme switch; the app defaults to
    /// [`Theme::DARK_PLUS`].
    #[allow(dead_code)]
    pub const DARK: Theme = Theme {
        bg: (0x1E, 0x1E, 0x2E),
        surface_chrome: (0x18, 0x18, 0x24),
        surface: (0x1E, 0x1E, 0x2E),
        bg_elevated: (0x24, 0x24, 0x37),
        surface_hover: (0x31, 0x32, 0x44),
        fg: (0xCD, 0xD6, 0xF4),
        fg_dim: (0x93, 0x99, 0xB2),
        text_muted: (0x93, 0x99, 0xB2),
        text_dim: (0x7F, 0x85, 0xA3),
        accent: (0x89, 0xB4, 0xFA),
        border: (0x31, 0x32, 0x44),
        border_subtle: (0x31, 0x32, 0x44),
        selection: (0x45, 0x47, 0x5A),
        syntax_keyword: (0xCB, 0xA6, 0xF7),
        syntax_control: (0xF5, 0xC2, 0xE7),
        syntax_string: (0xA6, 0xE3, 0xA1),
        syntax_number: (0xFA, 0xB3, 0x87),
        syntax_comment: (0x6C, 0x70, 0x86),
        syntax_type: (0x89, 0xB4, 0xFA),
        syntax_function: (0xF9, 0xE2, 0xAF),
        syntax_constant: (0x89, 0xB4, 0xFA),
        syntax_property: (0x89, 0xB4, 0xFA),
        syntax_variable: (0x89, 0xB4, 0xFA),
        syntax_regex: (0xF3, 0x8B, 0xA8),
        syntax_escape: (0xF9, 0xE2, 0xAF),
        syntax_decorator: (0xF9, 0xE2, 0xAF),
        syntax_selector: (0xF9, 0xE2, 0xAF),
        on_accent: (0x1E, 0x1E, 0x2E),
        editor_current_line: (0x31, 0x32, 0x44),
        editor_search_match: (0x5C, 0x4F, 0x2E),
        editor_indent_guide: (0x45, 0x47, 0x5A),
        editor_indent_guide_active: (0x6C, 0x70, 0x86),
        editor_bracket_1: (0xF9, 0xE2, 0xAF),
        editor_bracket_2: (0xF5, 0xC2, 0xE7),
        editor_bracket_3: (0x89, 0xB4, 0xFA),
        git_modified: (0xF9, 0xE2, 0xAF),
        git_added: (0xA6, 0xE3, 0xA1),
        git_deleted: (0xF3, 0x8B, 0xA8),
        git_untracked: (0x94, 0xE2, 0xD5),
    };

    /// Neutral black palette from the shared design mock (VS Code Dark+).
    /// Kept separate from Catppuccin so the blue-tinted theme remains
    /// available and tests can pin either.
    pub const DARK_PLUS: Theme = Theme {
        bg: (0x1E, 0x1E, 0x1E),
        surface_chrome: (0x18, 0x18, 0x18),
        surface: (0x1F, 0x1F, 0x1F),
        bg_elevated: (0x25, 0x25, 0x26),
        surface_hover: (0x2A, 0x2D, 0x2E),
        fg: (0xD4, 0xD4, 0xD4),
        fg_dim: (0x96, 0x96, 0x96),
        text_muted: (0x9D, 0x9D, 0x9D),
        text_dim: (0x8B, 0x94, 0x9E),
        accent: (0x00, 0x7A, 0xCC),
        border: (0x3C, 0x3C, 0x3C),
        border_subtle: (0x2B, 0x2B, 0x2B),
        selection: (0x26, 0x4F, 0x78),
        syntax_keyword: (0x56, 0x9C, 0xD6),
        syntax_control: (0xC5, 0x86, 0xC0),
        syntax_string: (0xCE, 0x91, 0x78),
        syntax_number: (0xB5, 0xCE, 0xA8),
        syntax_comment: (0x6A, 0x99, 0x55),
        syntax_type: (0x4E, 0xC9, 0xB0),
        syntax_function: (0xDC, 0xDC, 0xAA),
        syntax_constant: (0x4F, 0xC1, 0xFF),
        syntax_property: (0x9C, 0xDC, 0xFE),
        syntax_variable: (0x9C, 0xDC, 0xFE),
        syntax_regex: (0xD1, 0x69, 0x69),
        syntax_escape: (0xD7, 0xBA, 0x7D),
        syntax_decorator: (0xDC, 0xDC, 0xAA),
        syntax_selector: (0xD7, 0xBA, 0x7D),
        on_accent: (0xFF, 0xFF, 0xFF),
        editor_current_line: (0x2A, 0x2D, 0x2E),
        editor_search_match: (0x61, 0x54, 0x1D),
        editor_indent_guide: (0x40, 0x40, 0x40),
        editor_indent_guide_active: (0x70, 0x70, 0x70),
        editor_bracket_1: (0xFF, 0xD7, 0x00),
        editor_bracket_2: (0xDA, 0x70, 0xD6),
        editor_bracket_3: (0x17, 0x9F, 0xFF),
        git_modified: (0xE2, 0xC0, 0x8D),
        git_added: (0x73, 0xC9, 0x91),
        git_deleted: (0xF1, 0x4C, 0x4C),
        git_untracked: (0x73, 0xC9, 0x91),
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
        self.pixmap
            .fill(tiny_skia::Color::from_rgba8(color.0, color.1, color.2, 255));
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
        self.pixmap
            .fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    /// Fill an anti-aliased rounded rectangle in logical space. The path is
    /// temporary; no geometry or cache is retained by the canvas.
    #[allow(clippy::too_many_arguments)] // explicit rounded-rect geometry
    pub fn fill_round_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        radius: f64,
        color: ColorRgb,
        scale: f64,
    ) {
        if !(scale > 0.0 && w > 0.0 && h > 0.0) {
            return;
        }
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        let x0 = (x * scale) as f32;
        let y0 = (y * scale) as f32;
        let x1 = ((x + w) * scale) as f32;
        let y1 = ((y + h) * scale) as f32;
        let rr = (r * scale) as f32;
        let mut path = tiny_skia::PathBuilder::new();
        path.move_to(x0 + rr, y0);
        path.line_to(x1 - rr, y0);
        path.quad_to(x1, y0, x1, y0 + rr);
        path.line_to(x1, y1 - rr);
        path.quad_to(x1, y1, x1 - rr, y1);
        path.line_to(x0 + rr, y1);
        path.quad_to(x0, y1, x0, y1 - rr);
        path.line_to(x0, y0 + rr);
        path.quad_to(x0, y0, x0 + rr, y0);
        path.close();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color.0, color.1, color.2, 255);
        paint.anti_alias = true;
        self.pixmap.fill_path(
            &path.finish().expect("valid rounded rect"),
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    /// 1-logical-px border outline of a logical-space rect via four un-AA'd
    /// fill strips — exact pixels with no AA seam against blitted interiors.
    #[cfg(test)]
    pub fn stroke_rect_strips(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: ColorRgb,
        scale: f64,
    ) {
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
        d.mark(DamageRect {
            x: 10,
            y: 20,
            width: 5,
            height: 6,
        });
        d.mark(DamageRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
        let rects = d.take(800, 600).expect("two marked rects");
        assert_eq!(rects.len(), 2);
        assert!(d.is_empty());
    }

    #[test]
    fn damage_degrades_to_full_at_cap() {
        let mut d = Damage::new(2);
        d.take(800, 600);
        for i in 0..3 {
            d.mark(DamageRect {
                x: i,
                y: 0,
                width: 1,
                height: 1,
            });
        }
        let rects = d.take(800, 600).expect("degraded to full");
        assert_eq!(rects.len(), 1, "cap exceeded → single full-canvas rect");
        assert_eq!(rects[0].width, 800);
    }

    #[test]
    fn mark_is_noop_while_full() {
        let mut d = Damage::new(4);
        d.mark(DamageRect {
            x: 1,
            y: 1,
            width: 1,
            height: 1,
        }); // initial state: full
        let rects = d.take(10, 10).expect("still full");
        assert_eq!(rects.len(), 1);
        assert_eq!(
            rects[0].width, 10,
            "must be full-canvas, not the marked rect"
        );
    }

    // ---- Canvas + blit ----

    #[test]
    fn canvas_fill_and_blit_pack_0xrrggbb() {
        let mut canvas = Canvas::new(2, 1);
        canvas.fill((255, 0, 0)); // pure red
                                  // paint pixel (1,0) green via the raw premultiplied pixel buffer
        let green =
            tiny_skia::PremultipliedColorU8::from_rgba(0, 255, 0, 255).expect("opaque green");
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
        assert_eq!(
            (center.red(), center.green(), center.blue()),
            (255, 255, 255)
        );
        let outside = canvas.pixmap().pixel(0, 0).expect("outside rect");
        assert_eq!((outside.red(), outside.green(), outside.blue()), (0, 0, 0));
    }

    #[test]
    fn canvas_round_rect_keeps_corners_clear() {
        let mut canvas = Canvas::new(20, 20);
        canvas.fill((0, 0, 0));
        canvas.fill_round_rect(2.0, 2.0, 16.0, 16.0, 4.0, (255, 255, 255), 1.0);
        let center = canvas.pixmap().pixel(10, 10).expect("center");
        assert_eq!(
            (center.red(), center.green(), center.blue()),
            (255, 255, 255)
        );
        let corner = canvas.pixmap().pixel(2, 2).expect("corner");
        assert_eq!((corner.red(), corner.green(), corner.blue()), (0, 0, 0));
    }

    #[test]
    fn canvas_resize_changes_dimensions() {
        let mut canvas = Canvas::new(4, 4);
        canvas.resize(8, 2);
        assert_eq!((canvas.width(), canvas.height()), (8, 2));
    }
}
