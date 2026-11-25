//! Bounded, content-sized Markdown table painting.

use cosmic_text::Weight;

use crate::render::{Canvas, Theme};
use crate::text::{MonoMetrics, TextSystem};
use crate::ui::Rect;

use super::paint::draw_inline_line;
use super::parse::{wrap_spans, InlineSpan};

const CELL_PAD_X: f64 = 8.0;
const CELL_PAD_Y: f64 = 6.0;
const NOTICE_H: f64 = 22.0;

struct CellLayout {
    lines: Vec<Vec<InlineSpan>>,
}

struct RowLayout {
    cells: Vec<CellLayout>,
    y: f64,
    height: f64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_table(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    content: Rect,
    top: f64,
    view_top: f64,
    view_bottom: f64,
    headers: &[Vec<InlineSpan>],
    rows: &[Vec<Vec<InlineSpan>>],
    truncated: bool,
    line_h: f64,
    font_size: f32,
    metrics: MonoMetrics,
) -> f64 {
    let cols = headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let col_w = (content.w / cols as f64).max(1.0);
    let inner_w = (col_w - 2.0 * CELL_PAD_X).max(1.0);
    let advance = f64::from(metrics.advance) / scale;
    let mut layout = Vec::with_capacity(rows.len() + 1);
    let mut y = 0.0;
    for cells in std::iter::once(headers).chain(rows.iter().map(Vec::as_slice)) {
        let mut laid_out = Vec::with_capacity(cols);
        let mut cell_lines = 1usize;
        for col in 0..cols {
            let spans = cells.get(col).cloned().unwrap_or_default();
            let lines = wrap_spans(spans, inner_w, advance);
            cell_lines = cell_lines.max(lines.len());
            laid_out.push(CellLayout { lines });
        }
        let height = cell_lines as f64 * line_h + 2.0 * CELL_PAD_Y;
        layout.push(RowLayout {
            cells: laid_out,
            y,
            height,
        });
        y += height;
    }
    let notice_h = if truncated { NOTICE_H } else { 0.0 };
    let height = y + notice_h;
    if top + height < view_top || top > view_bottom {
        return height;
    }

    let visible_top = top.max(view_top);
    let visible_bottom = (top + y).min(view_bottom);
    for (row_index, row) in layout.iter().enumerate() {
        let row_top = top + row.y;
        let row_bottom = row_top + row.height;
        if row_bottom < view_top || row_top > view_bottom {
            continue;
        }
        if row_index == 0 {
            canvas.fill_rect(
                content.x,
                row_top.max(view_top),
                content.w,
                (row_bottom.min(view_bottom) - row_top.max(view_top)).max(0.0),
                theme.surface_hover,
                scale,
            );
        }
        for (col, cell) in row.cells.iter().enumerate() {
            for (line_index, line) in cell.lines.iter().enumerate() {
                let line_y = row_top + CELL_PAD_Y + line_index as f64 * line_h;
                if line_y < view_top || line_y + line_h > view_bottom {
                    continue;
                }
                draw_inline_line(
                    canvas,
                    text,
                    theme,
                    scale,
                    line,
                    content.x + col as f64 * col_w + CELL_PAD_X,
                    line_y,
                    line_h,
                    font_size,
                    metrics,
                    theme.fg,
                    if row_index == 0 {
                        Weight::BOLD
                    } else {
                        Weight::NORMAL
                    },
                    inner_w,
                    false,
                );
            }
        }
        let grid_y = (row_bottom.min(view_bottom) - 1.0).max(view_top);
        if grid_y + 1.0 <= view_bottom {
            canvas.fill_rect(content.x, grid_y, content.w, 1.0, theme.border, scale);
        }
    }
    for col in 0..=cols {
        let x = content.x + col as f64 * col_w;
        canvas.fill_rect(
            x,
            visible_top,
            1.0,
            (visible_bottom - visible_top).max(0.0),
            theme.border,
            scale,
        );
    }
    if truncated {
        let notice_y = top + y;
        if notice_y >= view_top && notice_y + notice_h <= view_bottom {
            canvas.fill_rect(
                content.x,
                notice_y,
                content.w,
                notice_h,
                theme.bg_elevated,
                scale,
            );
            super::paint::draw_text(
                canvas,
                text,
                "Table truncated",
                content.x + CELL_PAD_X,
                notice_y + 4.0,
                notice_h - 8.0,
                font_size,
                metrics,
                scale,
                theme.text_muted,
                Weight::NORMAL,
            );
        }
    }
    height
}
