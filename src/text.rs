//! Text system: bundled-fonts-only cosmic-text stack plus monospace metrics
//! and glyph blitting into the tiny-skia canvas.
//!
//! Memory rules honored here:
//! - `FontSystem::new_with_fonts` loads ONLY the two bundled JetBrains Mono
//!   faces (`include_bytes!`) — cosmic-text is built without the `fontconfig`
//!   feature, so system fonts are never scanned into RAM.
//! - The glyph raster cache is cosmic-text's `SwashCache` (keyed per glyph);
//!   our own cache holds only measured per-size metrics (a handful of sizes).
//! - Per-draw `Buffer`s are short-lived; the big persistent buffer strategy
//!   (one per open file viewport) arrives with M4's viewer.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, SwashImage,
    Weight,
};

/// Font family name as embedded inside the bundled TTFs. We address it by
/// name: cosmic-text's `Family::Monospace` resolves to "Noto Sans Mono",
/// which we deliberately do not bundle.
const MONO_FAMILY: &str = "JetBrains Mono";

const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");

/// Default logical (scale-independent) UI font size in px.
pub const UI_FONT_SIZE: f32 = 14.0;

/// Measured metrics of the bundled monospace face at one physical size.
#[derive(Clone, Copy, Debug)]
pub struct MonoMetrics {
    /// Advance width of every character (monospace guarantee)
    pub advance: f32,
    /// Baseline height above line top
    pub ascent: f32,
    /// Depth below baseline
    pub descent: f32,
    /// Total line height used by layout
    pub line_height: f32,
}

/// One text run's visual style (color resolved against the canvas).
#[derive(Clone, Copy)]
pub struct TextStyle {
    pub color: crate::render::ColorRgb,
    pub font_size: f32,
    pub weight: Weight,
}

pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    /// Measured metrics per physical font size. Key: font_size quantized to
    /// quarter pixels — bounded by the handful of sizes/scales ark uses.
    metrics_cache: HashMap<u32, MonoMetrics>,
}

impl TextSystem {
    /// Build the font system from bundled bytes only.
    pub fn new() -> Self {
        let font_system = FontSystem::new_with_fonts([
            fontdb::Source::Binary(Arc::new(FONT_REGULAR.to_vec())),
            fontdb::Source::Binary(Arc::new(FONT_BOLD.to_vec())),
        ]);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics_cache: HashMap::new(),
        }
    }

    /// Monospace metrics at a physical font size (measured once, cached).
    pub fn mono_metrics(&mut self, font_size: f32, line_height: f32) -> MonoMetrics {
        let key = (font_size * 4.0).round() as u32;
        if let Some(m) = self.metrics_cache.get(&key) {
            return *m;
        }
        let mut metrics = MonoMetrics {
            advance: font_size * 0.6, // sane fallback for degenerate layout
            ascent: font_size * 0.8,
            descent: font_size * 0.2,
            line_height,
        };
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics { font_size, line_height },
        );
        buffer.set_text(
            &mut self.font_system,
            "MMMMMMMMMM",
            &Self::attrs(Weight::NORMAL),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        for run in buffer.layout_runs() {
            if run.glyphs.len() >= 2 {
                let first = run.glyphs[0].x;
                let last = run.glyphs[run.glyphs.len() - 1].x;
                metrics.advance = (last - first) / (run.glyphs.len() - 1) as f32;
            }
            metrics.ascent = run.line_y - run.line_top;
            metrics.descent = run.line_height - metrics.ascent;
            metrics.line_height = run.line_height;
        }
        self.metrics_cache.insert(key, metrics);
        metrics
    }

    fn attrs(weight: Weight) -> Attrs<'static> {
        Attrs::new()
            .family(Family::Name(MONO_FAMILY))
            .weight(weight)
    }

    /// Draw one text run with its baseline at `baseline_y` (physical px).
    /// Returns the laid-out width in physical px.
    /// Coordinates must already be scaled to physical pixels.
    pub fn draw_line(
        &mut self,
        canvas: &mut crate::render::Canvas,
        text: &str,
        x: f32,
        baseline_y: f32,
        style: TextStyle,
    ) -> f32 {
        let font_size = style.font_size;
        let line_height = (font_size * 1.35).round();
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics { font_size, line_height },
        );
        buffer.set_text(
            &mut self.font_system,
            text,
            &Self::attrs(style.weight),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut width = 0.0f32;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                // Baseline lands exactly at `baseline_y`; glyph.x starts at 0.
                // LayoutGlyph.y is baseline-relative shaping output (≈0 on the
                // baseline), so `physical()` returns the glyph position
                // relative to the line top — the baseline's own offset within
                // the buffer (`run.line_y`) must be composed in, exactly like
                // cosmic-text's own draw code (buffer.rs `run.line_y +
                // physical_glyph.y`).
                let phys = glyph.physical((x, baseline_y - run.line_y), 1.0);
                let image = self
                    .swash_cache
                    .get_image(&mut self.font_system, phys.cache_key);
                if let Some(img) = image {
                    blit_glyph(
                        canvas.pixmap_mut(),
                        phys.x,
                        run.line_y as i32 + phys.y,
                        img,
                        style.color,
                    );
                }
            }
            width = width.max(run.line_w);
        }
        width
    }
}

/// Blit one swash-rasterized glyph into the premultiplied-RGBA canvas.
/// `x`/`y` are the glyph's baseline origin (physical px); swash's placement
/// is relative to that origin with `top` measured upward.
fn blit_glyph(
    pixmap: &mut tiny_skia::Pixmap,
    x: i32,
    y: i32,
    image: &SwashImage,
    color: crate::render::ColorRgb,
) {
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    if image.placement.width == 0 || image.placement.height == 0 {
        return;
    }
    let origin_x = x + image.placement.left;
    let origin_y = y - image.placement.top;
    let img_w = image.placement.width as usize;
    let data = &image.data;

    match image.content {
        SwashContent::Mask => {
            // 1 byte per pixel: coverage. src = ink color premultiplied by
            // coverage, blended over the opaque canvas.
            for row in 0..image.placement.height as i32 {
                let dy = origin_y + row;
                if dy < 0 || dy >= height {
                    continue;
                }
                for col in 0..img_w as i32 {
                    let dx = origin_x + col;
                    if dx < 0 || dx >= width {
                        continue;
                    }
                    let cov = data[(row as usize * img_w) + col as usize] as u32;
                    if cov == 0 {
                        continue;
                    }
                    let idx = (dy as usize * width as usize + dx as usize) * 4;
                    blend_premultiplied(pixmap, idx, color, cov);
                }
            }
        }
        SwashContent::Color => {
            // 4 bytes per pixel RGBA (non-premultiplied), e.g. emoji fonts.
            for row in 0..image.placement.height as i32 {
                let dy = origin_y + row;
                if dy < 0 || dy >= height {
                    continue;
                }
                for col in 0..img_w as i32 {
                    let dx = origin_x + col;
                    if dx < 0 || dx >= width {
                        continue;
                    }
                    let i = (row as usize * img_w + col as usize) * 4;
                    let a = data[i + 3] as u32;
                    if a == 0 {
                        continue;
                    }
                    // Color glyphs carry their own RGBA (no ink color needed)
                    blend_rgba(pixmap, dx, dy, data[i], data[i + 1], data[i + 2], a as u8);
                }
            }
        }
        SwashContent::SubpixelMask => {
            // We never request subpixel rendering; skip defensively.
            log::warn!("subpixel mask glyph encountered — skipped");
        }
    }
}

/// src-over blend of a coverage-masked premultiplied source onto the canvas.
fn blend_premultiplied(
    pixmap: &mut tiny_skia::Pixmap,
    idx: usize,
    color: crate::render::ColorRgb,
    cov: u32,
) {
    let data = pixmap.data_mut();
    let inv = 255 - cov;
    let d = &mut data[idx..idx + 4];
    d[0] = ((color.0 as u32 * cov + d[0] as u32 * inv) / 255) as u8;
    d[1] = ((color.1 as u32 * cov + d[1] as u32 * inv) / 255) as u8;
    d[2] = ((color.2 as u32 * cov + d[2] as u32 * inv) / 255) as u8;
    d[3] = 255;
}

/// src-over blend of a non-premultiplied RGBA source (color glyphs).
fn blend_rgba(
    pixmap: &mut tiny_skia::Pixmap,
    dx: i32,
    dy: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    let width = pixmap.width() as usize;
    let idx = (dy as usize * width + dx as usize) * 4;
    let (ar, ag, ab) = (r as u32 * a as u32 / 255, g as u32 * a as u32 / 255, b as u32 * a as u32 / 255);
    let a = a as u32;
    let inv = 255 - a;
    let data = pixmap.data_mut();
    let d = &mut data[idx..idx + 4];
    // src-over is bounded by 255 for premultiplied sources on opaque dst
    d[0] = (ar + d[0] as u32 * inv / 255) as u8;
    d[1] = (ag + d[1] as u32 * inv / 255) as u8;
    d[2] = (ab + d[2] as u32 * inv / 255) as u8;
    d[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Canvas;

    const EPS: f32 = 0.2;

    // ---- Monospace metrics (measured from the bundled font) ----

    #[test]
    fn jetbrains_mono_advance_is_0_6x_font_size() {
        let mut ts = TextSystem::new();
        let m = ts.mono_metrics(14.0, 18.9);
        assert!(
            (m.advance - 8.4).abs() < EPS,
            "14px × 0.6 = 8.4px, got {}",
            m.advance
        );
        // The monospace guarantee holds across sizes
        let m20 = ts.mono_metrics(20.0, 27.0);
        assert!((m20.advance - 12.0).abs() < EPS, "got {}", m20.advance);
    }

    #[test]
    fn metrics_are_consistent_and_cached() {
        let mut ts = TextSystem::new();
        let a = ts.mono_metrics(14.0, 18.9);
        let b = ts.mono_metrics(14.0, 18.9);
        assert_eq!(a.advance, b.advance, "cache must return identical metrics");
        assert!(a.ascent > 0.0 && a.descent >= 0.0);
        assert!((a.ascent + a.descent - a.line_height).abs() < EPS);
    }

    // ---- Glyph rendering into the canvas ----

    #[test]
    fn draw_line_paints_pixels_over_background() {
        let mut ts = TextSystem::new();
        let mut canvas = Canvas::new(300, 40);
        canvas.fill((0, 0, 0));
        let bg_before = count_non_black(&canvas);

        ts.draw_line(
            &mut canvas,
            "MMMM",
            10.0,
            30.0,
            TextStyle { color: (255, 255, 255), font_size: 14.0, weight: Weight::NORMAL },
        );
        let bg_after = count_non_black(&canvas);
        assert!(
            bg_after > bg_before + 50,
            "glyph raster should cover many pixels ({bg_before} → {bg_after})"
        );
    }

    #[test]
    fn draw_line_places_ink_around_the_requested_baseline() {
        let mut ts = TextSystem::new();
        let mut canvas = Canvas::new(64, 64);
        canvas.fill((0, 0, 0));
        ts.draw_line(
            &mut canvas,
            "M",
            10.0,
            30.0,
            TextStyle { color: (255, 255, 255), font_size: 14.0, weight: Weight::NORMAL },
        );

        // Ink rows = rows containing any changed pixel (AA-tolerant: any
        // blend coverage marks the row, not just full-coverage rows).
        let w = canvas.width() as usize;
        let (px, _) = canvas.pixmap().data().as_chunks::<4>();
        let (mut ink_min, mut ink_max, mut n) = (usize::MAX, 0usize, 0usize);
        for y in 0..canvas.height() as usize {
            for x in 0..w {
                let p = &px[y * w + x];
                if (p[0], p[1], p[2]) != (0, 0, 0) {
                    n += 1;
                    ink_min = ink_min.min(y);
                    ink_max = ink_max.max(y);
                }
            }
        }
        assert!(n > 0, "glyph must rasterize");

        // The cap-height ink of "M" must overlap the band around the
        // requested baseline: [baseline - ascent - 1, baseline + descent + 1]
        // (metrics measured for the same font size and line height that
        // draw_line uses internally). A composition bug that drops the
        // run.line_y term shifts the whole run up by line_y and puts the ink
        // entirely above this band.
        let m = ts.mono_metrics(14.0, (14.0f32 * 1.35).round());
        let band_min = 30.0 - m.ascent - 1.0;
        let band_max = 30.0 + m.descent + 1.0;
        assert!(
            (ink_min as f32) <= band_max && (ink_max as f32) >= band_min,
            "ink rows {ink_min}..{ink_max} must overlap baseline band {band_min}..{band_max}"
        );
    }

    #[test]
    fn draw_empty_line_is_a_noop() {
        let mut ts = TextSystem::new();
        let mut canvas = Canvas::new(100, 30);
        canvas.fill((10, 20, 30));
        let w = ts.draw_line(
            &mut canvas,
            "",
            0.0,
            20.0,
            TextStyle { color: (255, 0, 0), font_size: 14.0, weight: Weight::NORMAL },
        );
        assert_eq!(w, 0.0);
        // Nothing was painted (all pixels still the bg color)
        let (px_chunks, _) = canvas.pixmap().data().as_chunks::<4>();
        assert!(px_chunks.iter().all(|px| px[0] == 10 && px[1] == 20 && px[2] == 30));
    }

    // ---- Blending math (premultiplied src-over on opaque canvas) ----

    #[test]
    fn blend_half_coverage_of_black_over_white_gives_mid_grey() {
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).expect("pixmap");
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        let (px_x, px_y) = (1usize, 1usize);
        let idx = (px_y * 4 + px_x) * 4; // pixel (1,1)
        blend_premultiplied(&mut pixmap, idx, (0, 0, 0), 128);
        let px = pixmap.pixel(1, 1).expect("pixel");
        assert!(
            (px.red() as i32 - 127).abs() <= 1,
            "255 × (1 − 128/255) ≈ 127, got {}",
            px.red()
        );
        assert_eq!(px.alpha(), 255, "canvas stays opaque");
    }

    #[test]
    fn blend_full_coverage_replaces_channel() {
        let mut pixmap = tiny_skia::Pixmap::new(2, 2).expect("pixmap");
        pixmap.fill(tiny_skia::Color::from_rgba8(10, 20, 30, 255));
        blend_premultiplied(&mut pixmap, 0, (200, 100, 50), 255);
        let px = pixmap.pixel(0, 0).expect("pixel");
        assert_eq!((px.red(), px.green(), px.blue()), (200, 100, 50));
    }

    fn count_non_black(canvas: &Canvas) -> usize {
        let (px_chunks, _) = canvas.pixmap().data().as_chunks::<4>();
        px_chunks
            .iter()
            .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
            .count()
    }
}
