//! Bounded, viewport-only fenced-code painting. Long lines hard-wrap inside the
//! padded block; no document-wide wrapped-line buffer is retained.

use cosmic_text::Weight;

use crate::text::char_range;

use super::blocks::BlockPainter;
use super::paint::draw_text;

const CODE_PAD: f64 = 12.0;
const MAX_CODE_ROWS: usize = 4096;
const NOTICE_H: f64 = 22.0;

pub(super) fn draw(painter: &mut BlockPainter<'_>, lines: &[String], top: f64) -> f64 {
    let pad = CODE_PAD.min(painter.content.w / 4.0);
    let max_chars = ((painter.content.w - 2.0 * pad) / painter.advance)
        .floor()
        .max(1.0) as usize;
    let mut rows = 0usize;
    let mut truncated = false;
    for line in lines {
        let len = line.chars().count();
        let count = len.div_ceil(max_chars).max(1);
        if rows.saturating_add(count) > MAX_CODE_ROWS {
            truncated = true;
            rows = MAX_CODE_ROWS;
            break;
        }
        rows += count;
    }
    let notice_h = if truncated { NOTICE_H } else { 0.0 };
    let height = rows as f64 * painter.base_line_h + 2.0 * pad + notice_h;
    if top + height < painter.view_top || top > painter.view_bottom {
        return height;
    }

    let y0 = top.max(painter.view_top);
    let y1 = (top + height).min(painter.view_bottom);
    painter.canvas.fill_round_rect(
        painter.content.x,
        y0,
        painter.content.w,
        (y1 - y0).max(0.0),
        5.0,
        painter.theme.bg_elevated,
        painter.scale,
    );

    let text_top = top + pad;
    let first_row = ((painter.view_top - text_top) / painter.base_line_h)
        .floor()
        .max(0.0) as usize;
    let last_row = (((painter.view_bottom - text_top) / painter.base_line_h)
        .ceil()
        .max(0.0) as usize)
        .min(rows);
    let mut row_index = 0usize;
    'source: for line in lines {
        if row_index >= rows {
            break;
        }
        let len = line.chars().count();
        let count = len.div_ceil(max_chars).max(1);
        if row_index.saturating_add(count) <= first_row {
            row_index += count;
            continue;
        }
        let first_chunk = first_row.saturating_sub(row_index);
        let end_chunk = last_row
            .saturating_sub(row_index)
            .min(count)
            .min(rows.saturating_sub(row_index));
        for chunk in first_chunk..end_chunk {
            let y = text_top + (row_index + chunk) as f64 * painter.base_line_h;
            if y < painter.view_top || y + painter.base_line_h > painter.view_bottom {
                continue;
            }
            let start = chunk * max_chars;
            let end = start.saturating_add(max_chars).min(len);
            draw_text(
                painter.canvas,
                painter.text,
                char_range(line, start, end),
                painter.content.x + pad,
                y,
                painter.base_line_h,
                painter.base_font,
                painter.base_metrics,
                painter.scale,
                painter.theme.fg,
                Weight::NORMAL,
            );
        }
        row_index += count;
        if row_index >= last_row {
            break 'source;
        }
    }

    if truncated {
        let notice_y = top + pad + rows as f64 * painter.base_line_h;
        if notice_y >= painter.view_top && notice_y + NOTICE_H <= painter.view_bottom {
            painter.canvas.fill_rect(
                painter.content.x,
                notice_y,
                painter.content.w,
                NOTICE_H,
                painter.theme.bg_elevated,
                painter.scale,
            );
            draw_text(
                painter.canvas,
                painter.text,
                "Code block truncated",
                painter.content.x + pad,
                notice_y + 4.0,
                NOTICE_H - 8.0,
                painter.base_font,
                painter.base_metrics,
                painter.scale,
                painter.theme.text_muted,
                Weight::NORMAL,
            );
        }
    }
    height
}
