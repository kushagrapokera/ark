//! Search sidebar rendering and bounded result viewport.

use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::render::{Canvas, ColorRgb, Theme};
use crate::text::{TextStyle, TextSystem};
use crate::ui::{Pointer, Rect, Scrollbar};

use super::SearchState;

const SEARCH_HEADER_H: f64 = 36.0;
const SEARCH_INPUT_H: f64 = 28.0;
const SEARCH_STATUS_H: f64 = 20.0;
const SEARCH_ROW_H: f64 = 46.0;
const SEARCH_PAD: f64 = 8.0;

#[derive(Clone, Copy, Debug)]
pub struct SearchLayout {
    pub input: Rect,
    pub status: Rect,
    pub results: Rect,
    pub scroll_track: Rect,
}

pub fn panel_layout(rect: Rect) -> SearchLayout {
    let empty = Rect { x: rect.x, y: rect.y, w: 0.0, h: 0.0 };
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return SearchLayout {
            input: empty,
            status: empty,
            results: empty,
            scroll_track: empty,
        };
    }
    let inner_w = (rect.w - 2.0 * SEARCH_PAD).max(0.0);
    let input = Rect {
        x: rect.x + SEARCH_PAD,
        y: rect.y + SEARCH_HEADER_H + SEARCH_PAD,
        w: inner_w,
        h: SEARCH_INPUT_H,
    };
    let status = Rect {
        x: rect.x + SEARCH_PAD,
        y: input.y + input.h + 6.0,
        w: inner_w,
        h: SEARCH_STATUS_H,
    };
    let results = Rect {
        x: rect.x + SEARCH_PAD,
        y: status.y + status.h + 4.0,
        w: (inner_w - 8.0).max(0.0),
        h: (rect.y + rect.h - SEARCH_PAD - (status.y + status.h + 4.0)).max(0.0),
    };
    let scroll_track = Rect {
        x: results.x + results.w,
        y: results.y,
        w: 8.0,
        h: results.h,
    };
    SearchLayout { input, status, results, scroll_track }
}

#[derive(Debug, Default)]
pub struct SearchEffects {
    pub open: Option<(PathBuf, usize)>,
}

#[allow(clippy::too_many_arguments)] // explicit raster and bounded search state
pub fn draw_search_panel(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    ui_scale: f64,
    rect: Rect,
    root: &Path,
    state: &mut SearchState,
    pointer: Pointer,
    scrollbar: &mut Scrollbar,
) -> SearchEffects {
    let mut effects = SearchEffects::default();
    if rect.w <= 0.0 || rect.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return effects;
    }
    let layout = panel_layout(rect);
    canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.surface, scale);
    canvas.fill_rect(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        SEARCH_HEADER_H - 1.0,
        theme.bg_elevated,
        scale,
    );
    draw_clipped(
        canvas, text, scale, ui_scale, "SEARCH",
        rect.x + 12.0, rect.y + 10.0, (rect.w - 24.0).max(0.0),
        theme.fg, Weight::BOLD, rect,
    );
    if pointer.click && layout.input.contains(pointer.x, pointer.y) {
        state.input.focused = true;
    }
    state.input.draw(canvas, layout.input, theme, text, scale);
    draw_clipped(
        canvas, text, scale, ui_scale, &state.status,
        layout.status.x, layout.status.y + 4.0, layout.status.w,
        theme.text_muted, Weight::NORMAL, layout.status,
    );

    let content_h = state.results.len() as f64 * SEARCH_ROW_H;
    let max_scroll = (content_h - layout.results.h).max(0.0);
    state.scroll = state.scroll.clamp(0.0, max_scroll);
    let first = (state.scroll / SEARCH_ROW_H).floor() as usize;
    let visible = ((layout.results.h / SEARCH_ROW_H).ceil() as usize).saturating_add(1);
    for (row_index, result) in state.results.iter().enumerate().skip(first).take(visible) {
        let row_y = layout.results.y + row_index as f64 * SEARCH_ROW_H - state.scroll;
        if row_y + SEARCH_ROW_H <= layout.results.y || row_y >= layout.results.y + layout.results.h {
            continue;
        }
        let visible_top = row_y.max(layout.results.y);
        let visible_bottom = (row_y + SEARCH_ROW_H).min(layout.results.y + layout.results.h);
        if visible_top >= visible_bottom {
            continue;
        }
        let row_rect = Rect {
            x: layout.results.x,
            y: visible_top,
            w: layout.results.w,
            h: visible_bottom - visible_top,
        };
        let hovered = row_rect.contains(pointer.x, pointer.y);
        if hovered {
            canvas.fill_rect(row_rect.x, row_rect.y, row_rect.w, row_rect.h, theme.selection, scale);
        }
        let relative = result.path.strip_prefix(root).unwrap_or(&result.path);
        let label = relative.to_string_lossy();
        let count = format!("{} match{}", result.matches, if result.matches == 1 { "" } else { "es" });
        let count_w = count.chars().count() as f64 * 5.4 + 8.0;
        draw_clipped(
            canvas, text, scale, ui_scale, &label,
            row_rect.x + 8.0, row_rect.y + 5.0, (row_rect.w - count_w - 16.0).max(0.0),
            theme.fg, Weight::BOLD, row_rect,
        );
        draw_clipped(
            canvas, text, scale, ui_scale, &result.snippet,
            row_rect.x + 8.0, row_rect.y + 25.0, (row_rect.w - 16.0).max(0.0),
            theme.text_dim, Weight::NORMAL, row_rect,
        );
        draw_clipped(
            canvas, text, scale, ui_scale, &count,
            row_rect.x + row_rect.w - count_w, row_rect.y + 6.0, count_w,
            theme.text_muted, Weight::NORMAL, row_rect,
        );
        if pointer.click && hovered {
            effects.open = Some((result.path.clone(), result.line));
            state.input.focused = false;
        }
    }
    if layout.results.w > 0.0 && layout.results.h > 0.0 {
        scrollbar.offset = state.scroll;
        if scrollbar.on_pointer(layout.scroll_track, pointer, content_h, layout.results.h) {
            state.scroll = scrollbar.offset;
        }
        scrollbar.draw(canvas, layout.scroll_track, content_h, layout.results.h, theme, scale);
    }
    effects
}

#[allow(clippy::too_many_arguments)] // explicit clipped text raster context
fn draw_clipped(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    scale: f64,
    ui_scale: f64,
    label: &str,
    x: f64,
    y: f64,
    width: f64,
    color: ColorRgb,
    weight: Weight,
    clip: Rect,
) {
    let font_size = (crate::frame::ui_size(9.0, ui_scale) * scale) as f32;
    let metrics = text.mono_metrics(font_size, font_size * 1.35);
    let line_h = f64::from(metrics.line_height) / scale;
    if y < clip.y || y + line_h > clip.y + clip.h {
        return;
    }
    let advance = f64::from(metrics.advance) / scale;
    let max_chars = (width / advance.max(1.0)).floor().max(0.0) as usize;
    let shown: String = label.chars().take(max_chars).collect();
    if shown.is_empty() {
        return;
    }
    let baseline = (y + crate::frame::ui_size(9.0, ui_scale) / 2.0) * scale
        + f64::from(metrics.ascent - metrics.descent) / 2.0;
    text.draw_line(
        canvas,
        &shown,
        (x * scale) as f32,
        baseline as f32,
        TextStyle { color, font_size, weight },
    );
}
