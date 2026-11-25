//! Viewport-only Markdown rasterization over an existing mmap-backed viewer.

use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::Rect;

use super::blocks::BlockPainter;
use super::MarkdownState;

const MAX_PREVIEW_LINES: usize = 20_000;
const MAX_CONTENT_WIDTH: f64 = 860.0;
const CONTENT_PAD_X: f64 = 24.0;
const CONTENT_PAD_Y: f64 = 24.0;

#[derive(Debug)]
pub struct MarkdownDraw {
    pub content_h: f64,
    pub truncated: bool,
}

#[allow(clippy::too_many_arguments)] // explicit raster context and streamed document
pub fn draw_markdown_preview(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    ui_scale: f64,
    rect: Rect,
    viewer: &crate::viewer::Viewer,
    state: &mut MarkdownState,
) -> MarkdownDraw {
    if rect.w <= 0.0 || rect.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return MarkdownDraw {
            content_h: 0.0,
            truncated: false,
        };
    }
    canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.bg, scale);
    let pad_x = CONTENT_PAD_X * ui_scale;
    let content_w = (rect.w - 2.0 * pad_x)
        .max(1.0)
        .min(MAX_CONTENT_WIDTH * ui_scale);
    let content = Rect {
        x: rect.x + (rect.w - content_w) / 2.0,
        y: rect.y,
        w: content_w,
        h: rect.h,
    };
    let base_font = (14.0_f64 * ui_scale * scale) as f32;
    let base_metrics = text.mono_metrics(base_font, base_font * 1.6);
    let base_line_h = (f64::from(base_metrics.line_height) / scale).max(1.0);
    let advance = (f64::from(base_metrics.advance) / scale).max(1.0);
    let view_bottom = rect.y + rect.h;
    let mut painter = BlockPainter {
        canvas,
        text,
        theme,
        scale,
        content,
        view_top: rect.y,
        view_bottom,
        base_font,
        base_metrics,
        base_line_h,
        advance,
    };
    let mut y = CONTENT_PAD_Y * ui_scale;
    let mut index = 0usize;
    let mut truncated = false;
    while index < viewer.line_count() && index < MAX_PREVIEW_LINES {
        if viewer
            .line_text(index)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            index += 1;
            continue;
        }
        let (block, next) = super::parse::next_block(viewer, index);
        let top = rect.y + y - state.scroll;
        y += painter.draw(&block, top);
        index = next.max(index + 1);
    }
    if index < viewer.line_count() {
        truncated = true;
    }
    let content_h = y + CONTENT_PAD_Y * ui_scale;
    let max_scroll = (content_h - rect.h).max(0.0);
    state.scroll = state.scroll.clamp(0.0, max_scroll);
    if truncated {
        let notice_h = 24.0;
        canvas.fill_rect(
            rect.x,
            view_bottom - notice_h,
            rect.w,
            notice_h,
            theme.bg_elevated,
            scale,
        );
        super::paint::draw_text(
            canvas,
            text,
            "Markdown preview truncated",
            rect.x + 8.0,
            view_bottom - notice_h + 2.0,
            notice_h - 4.0,
            base_font,
            base_metrics,
            scale,
            theme.text_muted,
            cosmic_text::Weight::NORMAL,
        );
    }
    MarkdownDraw {
        content_h,
        truncated,
    }
}
