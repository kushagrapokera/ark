//! Viewport rasterization: syntax spans, selection and editor decorations.

use cosmic_text::Weight;

use crate::git::{FileDiff, GutterMark};
use crate::highlight::{token_color, Highlighter};
use crate::render::{Canvas, Theme};
use crate::text::{char_range, TextStyle, TextSystem, UI_FONT_SIZE};
use crate::ui::Rect;

use super::decor;
use super::{gutter_width, Viewer, GUT_PAD_R, LINE_H, MARK_W};

/// Render the visible lines of `viewer` into `panel` (LOGICAL px, rastered
/// at `scale`). Viewport-only: only the offset-exposed lines are shaped.
/// Gutter: reserved git-mark strip (M7 paints it) + right-aligned dim line
/// numbers. Selection backgrounds paint under the text; text draws in
/// `highlighter` token colors over `fg`.
#[allow(clippy::too_many_arguments)] // highlighter + diff join raster borrows; explicit beats bundling
pub fn draw_viewer_panel(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    panel: Rect,
    viewer: &Viewer,
    highlighter: &Highlighter,
    file_diff: Option<&FileDiff>,
) {
    if panel.w <= 0.0 || panel.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance) / scale;

    if viewer.line_count() == 0 {
        canvas.fill_rect(panel.x, panel.y, panel.w, panel.h, theme.bg, scale);
        let cy = (panel.y + panel.h / 2.0) * scale;
        text.draw_line(
            canvas,
            "(empty file)",
            ((panel.x + panel.w / 2.0) * scale - f64::from(m.advance) * 6.0) as f32,
            (cy + f64::from(m.ascent - m.descent) / 2.0) as f32,
            TextStyle {
                color: theme.fg_dim,
                font_size,
                weight: Weight::NORMAL,
            },
        );
        return;
    }

    let gutter_w = gutter_width(viewer.line_count(), advance);
    let text_x = panel.x + gutter_w;
    let content_w = (panel.x + panel.w - crate::tree::TRACK_W - text_x - 4.0).max(0.0);
    let content_px = viewer.max_line_chars() as f64 * advance;
    let horizontal_overflow = content_w > 0.0 && content_px > content_w;
    let track = super::horizontal_track(panel);
    let code_bottom = if horizontal_overflow {
        track.y
    } else {
        panel.y + panel.h
    };
    let code_h = (code_bottom - panel.y).max(0.0);
    let rows = ((code_h / LINE_H).floor() as usize).max(1);
    let viewport_cols = ((content_w / advance).floor().max(1.0)) as usize;
    let start_col = viewer
        .horizontal_offset()
        .min(viewer.max_horizontal_offset(viewport_cols));
    let end_col = start_col.saturating_add(viewport_cols);

    // Reserved git-mark strip and diff marks.
    canvas.fill_rect(panel.x, panel.y, MARK_W, code_h, theme.bg_elevated, scale);
    let offset_lines = viewer.offset_lines();
    if let Some(diff) = file_diff {
        for (line, kind) in &diff.marks {
            let row_i = *line as i64 - 1 - offset_lines as i64;
            if row_i < 0 || row_i as usize >= rows {
                continue;
            }
            let y = panel.y + row_i as f64 * LINE_H;
            let color = match kind {
                GutterMark::Added => theme.git_added,
                GutterMark::Modified => theme.git_modified,
            };
            canvas.fill_rect(
                panel.x + 1.0,
                y + 1.0,
                MARK_W - 2.0,
                LINE_H - 2.0,
                color,
                scale,
            );
        }
    }

    let lang = viewer.language();
    let first = viewer.offset_lines();
    let mut highlight_state = highlighter.state_before(lang, first, |i| viewer.line_text(i));
    let baseline_off = f64::from(m.ascent - m.descent) / 2.0;
    let current_line = viewer.sel.as_ref().map(|s| s.head.0).unwrap_or(first);
    let guide_text_x = text_x - start_col as f64 * advance;
    let mut bracket_state = decor::BracketState::default();

    for i in first..(first + rows).min(viewer.line_count()) {
        let row_y = panel.y + (i - first) as f64 * LINE_H;
        let Some(line) = viewer.line_text(i) else {
            continue;
        };
        let is_current = i == current_line;

        if is_current {
            decor::draw_current_line(canvas, theme, scale, text_x, row_y, content_w);
        }
        decor::draw_indent_guides(
            canvas,
            theme,
            scale,
            guide_text_x,
            row_y,
            &line,
            advance,
            is_current,
            content_w,
        );

        // Selection background, clipped to the visible column window.
        if let Some(((sl, sc), (el, ec))) = viewer.selection() {
            if i >= sl && i <= el {
                let len = line.chars().count();
                let (a, b) = if sl == el {
                    (sc, ec)
                } else if i == sl {
                    (sc, len)
                } else if i == el {
                    (0, ec)
                } else {
                    (0, len)
                };
                let a = a.max(start_col);
                let b = b.min(end_col);
                if b > a {
                    canvas.fill_rect(
                        text_x + (a - start_col) as f64 * advance,
                        row_y,
                        (b - a) as f64 * advance,
                        LINE_H,
                        theme.selection,
                        scale,
                    );
                }
            }
        }

        // Gutter line number.
        let num = (i + 1).to_string();
        let num_x = panel.x + gutter_w - GUT_PAD_R - num.chars().count() as f64 * advance;
        let baseline = (row_y + LINE_H / 2.0) * scale + baseline_off;
        text.draw_line(
            canvas,
            &num,
            (num_x * scale) as f32,
            baseline as f32,
            TextStyle {
                color: theme.fg_dim,
                font_size,
                weight: Weight::NORMAL,
            },
        );

        // Syntax segments, intersected with the visible char range.
        let spans = highlighter.line_spans_with_state(&line, lang, &mut highlight_state);
        let char_len = line.chars().count();
        let mut segs: Vec<(usize, usize, crate::render::ColorRgb)> = Vec::new();
        let mut pos = 0usize;
        for span in &spans {
            if span.start > pos {
                segs.push((pos, span.start, theme.fg));
            }
            if span.end > span.start {
                segs.push((span.start, span.end, token_color(theme, span.token)));
            }
            pos = span.end.max(pos);
        }
        if pos < char_len {
            segs.push((pos, char_len, theme.fg));
        }
        for (a, b, color) in segs {
            let a = a.max(start_col);
            let b = b.min(end_col);
            if b <= a {
                continue;
            }
            text.draw_line(
                canvas,
                char_range(&line, a, b),
                ((text_x + (a - start_col) as f64 * advance) * scale) as f32,
                baseline as f32,
                TextStyle {
                    color,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
        }
        for (a, b, color) in decor::bracket_spans(&line, &spans, &mut bracket_state, theme) {
            let a = a.max(start_col);
            let b = b.min(end_col);
            if b <= a {
                continue;
            }
            text.draw_line(
                canvas,
                char_range(&line, a, b),
                ((text_x + (a - start_col) as f64 * advance) * scale) as f32,
                baseline as f32,
                TextStyle {
                    color,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
        }
    }

    if horizontal_overflow {
        let mut scrollbar = viewer.hscroll;
        scrollbar.offset = start_col as f64 * advance;
        scrollbar.clamp(content_px, content_w);
        scrollbar.draw(canvas, track, content_px, content_w, theme, scale);
    }
}
