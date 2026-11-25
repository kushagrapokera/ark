//! Inline/text painting primitives for the streamed Markdown preview.

use cosmic_text::Weight;

use crate::render::{Canvas, ColorRgb, Theme};
use crate::text::{TextStyle, TextSystem};

use super::parse::{InlineKind, InlineSpan};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_wrapped(
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
    strike_all: bool,
) {
    for (index, line) in lines.iter().enumerate() {
        let y = top + index as f64 * line_h;
        if y < view_top || y + line_h > view_bottom {
            continue;
        }
        draw_inline_line(
            canvas,
            text,
            theme,
            scale,
            line,
            x,
            y,
            line_h,
            font_size,
            metrics,
            color,
            weight,
            f64::INFINITY,
            strike_all,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_inline_line(
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
    strike_all: bool,
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
        let span_weight = if span.kind == InlineKind::Bold {
            Weight::BOLD
        } else {
            weight
        };
        let span_color = if span.kind == InlineKind::Link {
            theme.accent
        } else {
            color
        };
        if span.kind == InlineKind::Code {
            let chip_w = shown.chars().count() as f64 * advance;
            let chip_y = y + 1.0;
            let chip_h = (line_h - 2.0).max(1.0);
            canvas.fill_round_rect(
                cursor,
                chip_y,
                chip_w,
                chip_h,
                3.0,
                theme.surface_hover,
                scale,
            );
            canvas.fill_rect(cursor, chip_y, chip_w, 1.0, theme.border, scale);
            canvas.fill_rect(
                cursor,
                chip_y + chip_h - 1.0,
                chip_w,
                1.0,
                theme.border,
                scale,
            );
            canvas.fill_rect(cursor, chip_y, 1.0, chip_h, theme.border, scale);
            canvas.fill_rect(
                cursor + chip_w - 1.0,
                chip_y,
                1.0,
                chip_h,
                theme.border,
                scale,
            );
        }
        let link_start = cursor;
        let width = draw_text(
            canvas,
            text,
            &shown,
            cursor,
            y,
            line_h,
            font_size,
            metrics,
            scale,
            span_color,
            span_weight,
        );
        cursor += width;
        if span.kind == InlineKind::Link {
            canvas.fill_rect(
                link_start,
                y + line_h - 3.0,
                width,
                1.0,
                theme.accent,
                scale,
            );
        }
        if strike_all || span.kind == InlineKind::Strike {
            canvas.fill_rect(link_start, y + line_h / 2.0, width, 1.0, span_color, scale);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_text(
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
        TextStyle {
            color,
            font_size,
            weight,
        },
    );
    f64::from(width) / scale
}
