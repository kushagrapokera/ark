//! Textarea raster + pointer mapping (split from `ui.rs`).

use cosmic_text::Weight;

use super::{GUTTER_W, PAD};
use crate::configed::text::TextEditor;
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle};
use crate::ui::{Pointer, Rect};
use crate::viewer::LINE_H;

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    t: &mut TextEditor,
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    body: Rect,
    pointer: Pointer,
    advance: f64,
    font_size: f32,
    m: &crate::text::MonoMetrics,
    dragging: &mut bool,
) -> bool {
    let rows = (body.h / LINE_H).floor().max(1.0) as usize;
    let text_x = body.x + GUTTER_W;
    let cols = (((body.w - GUTTER_W - PAD) / advance).floor()).max(1.0) as usize;
    t.ensure_cursor_visible(rows, cols);

    let selection = t.selected_range();
    let (cursor_line, cursor_col) = t.cursor();
    let hscroll = t.hscroll_col();
    for slot in 0..rows {
        let line_idx = t.scroll_line() + slot;
        if line_idx >= t.line_count() {
            break;
        }
        let line = t.line(line_idx);
        let line_start = t.line_start(line_idx);
        let y = body.y + slot as f64 * LINE_H;
        let baseline = (y + LINE_H / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;

        // Line number (bright for the caret line).
        let number = format!("{}", line_idx + 1);
        let num_color = if line_idx == cursor_line { theme.fg } else { theme.fg_dim };
        text.draw_line(
            canvas,
            &number,
            ((body.x + GUTTER_W - 8.0) * scale - advance * number.chars().count() as f64 * scale) as f32,
            baseline as f32,
            TextStyle { color: num_color, font_size, weight: Weight::NORMAL },
        );

        // Selection highlight for this line (byte intersection → char cells).
        if let Some(sel) = &selection {
            let line_end = line_start + line.len();
            let a = sel.start.max(line_start).min(line_end);
            let b = sel.end.max(line_start).min(line_end);
            if a < b {
                let c0 = t.text()[line_start..a].chars().count();
                let n = t.text()[a..b].chars().count();
                let draw_c0 = c0.max(hscroll);
                let draw_n = (c0 + n).min(hscroll + cols).saturating_sub(draw_c0);
                if draw_n > 0 {
                    canvas.fill_rect(
                        text_x + (draw_c0 - hscroll) as f64 * advance,
                        y,
                        draw_n as f64 * advance,
                        LINE_H,
                        theme.selection,
                        scale,
                    );
                }
            }
        }

        let visible: String = line.chars().skip(hscroll).take(cols).collect();
        if !visible.is_empty() {
            text.draw_line(
                canvas,
                &visible,
                (text_x * scale) as f32,
                baseline as f32,
                TextStyle { color: theme.fg, font_size, weight: Weight::NORMAL },
            );
        }

        // Caret: a 2px accent column when this line holds it and it's in view.
        if line_idx == cursor_line && cursor_col >= hscroll && cursor_col <= hscroll + cols {
            canvas.fill_rect(
                text_x + (cursor_col - hscroll) as f64 * advance,
                y,
                2.0,
                LINE_H,
                theme.accent,
                scale,
            );
        }
        // IME preedit text sits at the caret (commit replaces it).
        if line_idx == cursor_line {
            if let Some(pre) = t.preedit() {
                let x = text_x + (cursor_col.saturating_sub(hscroll)) as f64 * advance;
                text.draw_line(
                    canvas,
                    pre,
                    (x * scale) as f32,
                    baseline as f32,
                    TextStyle { color: theme.syntax_string, font_size, weight: Weight::NORMAL },
                );
                canvas.fill_rect(x, y + LINE_H - 2.0, advance * pre.chars().count() as f64, 1.0, theme.syntax_string, scale);
            }
        }
    }

    // Pointer: press starts a selection, drag extends it, release finalizes
    // WITHOUT collapsing (a drag keeps its selection; a plain click was
    // already placed by the press frame).
    let in_text = pointer.x >= text_x && body.contains(pointer.x, pointer.y);
    if in_text && pointer.down {
        let line = t.scroll_line() + ((pointer.y - body.y) / LINE_H).floor().max(0.0) as usize;
        let col = hscroll + ((pointer.x - text_x) / advance).floor().max(0.0) as usize;
        if !*dragging {
            t.start_selection(line, col);
            *dragging = true;
        } else {
            t.extend_selection(line, col);
        }
    } else if *dragging {
        if in_text {
            let line = t.scroll_line() + ((pointer.y - body.y) / LINE_H).floor().max(0.0) as usize;
            let col = hscroll + ((pointer.x - text_x) / advance).floor().max(0.0) as usize;
            t.extend_selection(line, col);
        }
        *dragging = false;
    } else if in_text && pointer.click {
        // Press+release coalesced into one frame (no drag latch): place the
        // caret directly.
        let line = t.scroll_line() + ((pointer.y - body.y) / LINE_H).floor().max(0.0) as usize;
        let col = hscroll + ((pointer.x - text_x) / advance).floor().max(0.0) as usize;
        t.collapse_to(line, col);
    }
    false
}
