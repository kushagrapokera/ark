//! Viewport-only Markdown rasterization over an existing mmap-backed viewer.

use cosmic_text::Weight;

use crate::render::{Canvas, ColorRgb, Theme};
use crate::text::{TextStyle, TextSystem, UI_FONT_SIZE};
use crate::ui::Rect;
use crate::viewer::Viewer;

use super::parse::{next_block, wrap_spans, BlockKind, InlineKind, InlineSpan};
use super::ReadmeState;

const MAX_PREVIEW_LINES: usize = 20_000;

#[derive(Debug)]
pub struct ReadmeDraw {
    pub content_h: f64,
    pub truncated: bool,
}

#[allow(clippy::too_many_arguments)] // explicit raster context and streamed document
pub fn draw(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    ui_scale: f64,
    rect: Rect,
    viewer: &Viewer,
    state: &mut ReadmeState,
) -> ReadmeDraw {
    if rect.w <= 0.0 || rect.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return ReadmeDraw { content_h: 0.0, truncated: false };
    }
    canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.bg, scale);
    let base_font = (UI_FONT_SIZE as f64 * scale * ui_scale) as f32;
    let base_metrics = text.mono_metrics(base_font, base_font * 1.35);
    let base_line_h = (f64::from(base_metrics.line_height) / scale).max(1.0);
    let advance = (f64::from(base_metrics.advance) / scale).max(1.0);
    let view_bottom = rect.y + rect.h;
    let mut y = 0.0;
    let mut index = 0usize;
    let mut truncated = false;
    while index < viewer.line_count() && index < MAX_PREVIEW_LINES {
        let line = viewer.line_text(index).unwrap_or_default();
        if line.trim().is_empty() {
            y += base_line_h * 0.5;
            index += 1;
            continue;
        }
        let (block, next) = next_block(viewer, index);
        let top = rect.y + y - state.scroll;
        match block {
            BlockKind::Heading { level, spans } => {
                let multiplier = match level {
                    1 => 1.8,
                    2 => 1.45,
                    3 => 1.2,
                    4 => 1.05,
                    _ => 1.0,
                };
                let font_size = base_font * multiplier as f32;
                let metrics = text.mono_metrics(font_size, font_size * 1.35);
                let line_h = (f64::from(metrics.line_height) / scale).max(1.0);
                let lines = wrap_spans(spans, rect.w, f64::from(metrics.advance) / scale);
                let height = lines.len() as f64 * line_h + 8.0;
                if top + height >= rect.y && top <= view_bottom {
                    draw_wrapped(
                        canvas, text, theme, scale, &lines, rect.x, top + 4.0,
                        line_h, font_size, metrics, rect.y, view_bottom,
                        theme.fg, Weight::BOLD,
                    );
                }
                y += height;
            }
            BlockKind::Paragraph(spans) => {
                let lines = wrap_spans(spans, rect.w, advance);
                let height = lines.len() as f64 * base_line_h + 6.0;
                if top + height >= rect.y && top <= view_bottom {
                    draw_wrapped(
                        canvas, text, theme, scale, &lines, rect.x, top,
                        base_line_h, base_font, base_metrics, rect.y, view_bottom,
                        theme.fg, Weight::NORMAL,
                    );
                }
                y += height;
            }
            BlockKind::List { ordered, spans } => {
                let indent = 26.0;
                let lines = wrap_spans(spans, (rect.w - indent).max(1.0), advance);
                let height = lines.len() as f64 * base_line_h + 4.0;
                if top + height >= rect.y && top <= view_bottom {
                    if let Some(number) = ordered {
                        let label = format!("{number}.");
                        draw_text(
                            canvas, text, &label, rect.x + 2.0, top, base_line_h,
                            base_font, base_metrics, scale, theme.text_dim, Weight::NORMAL,
                        );
                    } else {
                        canvas.fill_round_rect(
                            rect.x + 6.0,
                            top + base_line_h / 2.0 - 2.0,
                            4.0,
                            4.0,
                            2.0,
                            theme.accent,
                            scale,
                        );
                    }
                    draw_wrapped(
                        canvas, text, theme, scale, &lines, rect.x + indent, top,
                        base_line_h, base_font, base_metrics, rect.y, view_bottom,
                        theme.fg, Weight::NORMAL,
                    );
                }
                y += height;
            }
            BlockKind::Quote(spans) => {
                let indent = 16.0;
                let lines = wrap_spans(spans, (rect.w - indent).max(1.0), advance);
                let height = lines.len() as f64 * base_line_h + 8.0;
                if top + height >= rect.y && top <= view_bottom {
                    canvas.fill_rect(rect.x, top, 2.0, height - 4.0, theme.accent, scale);
                    draw_wrapped(
                        canvas, text, theme, scale, &lines, rect.x + indent, top + 4.0,
                        base_line_h, base_font, base_metrics, rect.y, view_bottom,
                        theme.text_dim, Weight::NORMAL,
                    );
                }
                y += height;
            }
            BlockKind::Code(lines) => {
                let height = lines.len() as f64 * base_line_h + 16.0;
                if top + height >= rect.y && top <= view_bottom {
                    canvas.fill_round_rect(rect.x, top, rect.w, height, 4.0, theme.bg_elevated, scale);
                    for (row, line) in lines.iter().enumerate() {
                        let line_y = top + 8.0 + row as f64 * base_line_h;
                        if line_y + base_line_h < rect.y || line_y > view_bottom {
                            continue;
                        }
                        draw_text(
                            canvas, text, line, rect.x + 12.0, line_y, base_line_h,
                            base_font, base_metrics, scale, theme.fg, Weight::NORMAL,
                        );
                    }
                }
                y += height;
            }
            BlockKind::Rule => {
                let height = base_line_h;
                if top + height >= rect.y && top <= view_bottom {
                    canvas.fill_rect(
                        rect.x,
                        top + base_line_h / 2.0,
                        rect.w,
                        1.0,
                        theme.border,
                        scale,
                    );
                }
                y += height;
            }
            BlockKind::Table { headers, rows } => {
                let cols = headers.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
                let col_w = (rect.w / cols as f64).max(1.0);
                let row_h = base_line_h + 8.0;
                let height = (rows.len() + 1) as f64 * row_h + 4.0;
                if top + height >= rect.y && top <= view_bottom {
                    canvas.fill_rect(rect.x, top, rect.w, row_h, theme.surface_hover, scale);
                    for (row_index, cells) in std::iter::once(&headers).chain(rows.iter()).enumerate() {
                        let row_y = top + row_index as f64 * row_h;
                        if row_y + row_h < rect.y || row_y > view_bottom {
                            continue;
                        }
                        for (col, spans) in cells.iter().take(cols).enumerate() {
                            let lines = wrap_spans(spans.clone(), col_w - 12.0, advance);
                            if let Some(line) = lines.first() {
                                draw_inline_line(
                                    canvas, text, theme, scale, line,
                                    rect.x + col as f64 * col_w + 6.0,
                                    row_y + 4.0,
                                    row_h - 8.0,
                                    base_font,
                                    base_metrics,
                                    theme.fg,
                                    if row_index == 0 { Weight::BOLD } else { Weight::NORMAL },
                                    col_w - 12.0,
                                );
                            }
                        }
                    }
                    for col in 0..=cols {
                        let x = rect.x + col as f64 * col_w;
                        canvas.fill_rect(x, top, 1.0, height - 4.0, theme.border, scale);
                    }
                    canvas.fill_rect(rect.x, top + height - 5.0, rect.w, 1.0, theme.border, scale);
                }
                y += height;
            }
        }
        index = next.max(index + 1);
    }
    if index < viewer.line_count() {
        truncated = true;
    }
    let max_scroll = (y - rect.h).max(0.0);
    state.scroll = state.scroll.clamp(0.0, max_scroll);
    if truncated {
        let notice_h = 24.0;
        canvas.fill_rect(rect.x, view_bottom - notice_h, rect.w, notice_h, theme.bg_elevated, scale);
        draw_text(
            canvas, text, "README preview truncated", rect.x + 8.0,
            view_bottom - notice_h + 2.0, notice_h - 4.0, base_font, base_metrics,
            scale, theme.text_muted, Weight::NORMAL,
        );
    }
    ReadmeDraw { content_h: y, truncated }
}

#[allow(clippy::too_many_arguments)] // explicit styled line raster context
fn draw_wrapped(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    lines: &[Vec<InlineSpan>],
    x: f64,
    top: f64,
    line_h: f64,
    font_size: f32,
    metrics: crate::text::MonoMetrics,
    view_top: f64,
    view_bottom: f64,
    color: ColorRgb,
    weight: Weight,
) {
    for (index, line) in lines.iter().enumerate() {
        let y = top + index as f64 * line_h;
        if y + line_h < view_top || y > view_bottom {
            continue;
        }
        draw_inline_line(
            canvas, text, theme, scale, line, x, y, line_h, font_size, metrics,
            color, weight, f64::INFINITY,
        );
    }
}

#[allow(clippy::too_many_arguments)] // explicit inline raster context
fn draw_inline_line(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    spans: &[InlineSpan],
    x: f64,
    y: f64,
    line_h: f64,
    font_size: f32,
    metrics: crate::text::MonoMetrics,
    color: ColorRgb,
    weight: Weight,
    max_width: f64,
) {
    let mut cursor = x;
    for span in spans {
        let remaining = (max_width - (cursor - x)).max(0.0);
        let advance = f64::from(metrics.advance) / scale;
        let max_chars = (remaining / advance.max(1.0)).floor() as usize;
        let shown: String = span.text.chars().take(max_chars).collect();
        if shown.is_empty() {
            break;
        }
        let span_weight = if span.kind == InlineKind::Bold { Weight::BOLD } else { weight };
        let span_color = if span.kind == InlineKind::Link { theme.accent } else { color };
        if span.kind == InlineKind::Code {
            canvas.fill_rect(
                cursor,
                y + 2.0,
                shown.chars().count() as f64 * advance,
                (line_h - 4.0).max(1.0),
                theme.bg_elevated,
                scale,
            );
        }
        let link_start = cursor;
        let width = draw_text(
            canvas, text, &shown, cursor, y, line_h, font_size, metrics, scale,
            span_color, span_weight,
        );
        cursor += width;
        if span.kind == InlineKind::Link {
            canvas.fill_rect(link_start, y + line_h - 3.0, width, 1.0, theme.accent, scale);
        }
    }
}

#[allow(clippy::too_many_arguments)] // explicit styled text raster context
fn draw_text(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    label: &str,
    x: f64,
    y: f64,
    line_h: f64,
    font_size: f32,
    metrics: crate::text::MonoMetrics,
    scale: f64,
    color: ColorRgb,
    weight: Weight,
) -> f64 {
    let baseline = (y + line_h / 2.0) * scale + f64::from(metrics.ascent - metrics.descent) / 2.0;
    let width = text.draw_line(
        canvas,
        label,
        (x * scale) as f32,
        baseline as f32,
        TextStyle { color, font_size, weight },
    );
    f64::from(width) / scale
}
