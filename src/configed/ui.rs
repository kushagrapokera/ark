//! Config editor UI: header, status, `.env` form and the textarea raster /
//! hit-testing. Interactions mutate the caller-owned editor; Save/Reload are
//! reported as effects so the App performs the fs boundary after the frame.

use cosmic_text::Weight;

use super::env::{self};
use super::{Body, ConfigEditor, EnvEditor};
use crate::highlight::{lang_for_source, Highlighter, Lang};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::ui::{draw_button, Pointer, Press, Rect, TextInput};
use crate::viewer::LINE_H;

mod text;

const HEADER_H: f64 = 34.0;
const STATUS_H: f64 = 18.0;
pub(super) const PAD: f64 = 8.0;
const BTN_W: f64 = 62.0;
const BTN_H: f64 = 22.0;
const ENV_ROW_H: f64 = 20.0;
const FIELD_H: f64 = 24.0;
pub(super) const GUTTER_W: f64 = 44.0;

/// What one frame's header buttons requested. Body edits are applied
/// directly (the click already schedules full damage).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigEffects {
    pub save: bool,
    pub reload: bool,
    /// The body changed (env add/remove/apply or text pointer movement).
    pub changed: bool,
}

struct HeaderLayout {
    save: Rect,
    reload: Rect,
    status: Rect,
    body: Rect,
}

fn layout(panel: Rect) -> HeaderLayout {
    let save = Rect {
        x: panel.x + panel.w - PAD - BTN_W,
        y: panel.y + PAD,
        w: BTN_W,
        h: BTN_H,
    };
    let reload = Rect { x: save.x - 6.0 - BTN_W, y: save.y, w: BTN_W, h: BTN_H };
    let status = Rect { x: panel.x + PAD, y: panel.y + HEADER_H, w: (panel.w - 2.0 * PAD).max(0.0), h: STATUS_H };
    let body = Rect {
        x: panel.x,
        y: panel.y + HEADER_H + STATUS_H,
        w: panel.w,
        h: (panel.h - HEADER_H - STATUS_H).max(0.0),
    };
    HeaderLayout { save, reload, status, body }
}

#[allow(clippy::too_many_arguments)] // explicit raster context matches ConfigEditor::draw
pub(crate) fn draw(
    editor: &mut ConfigEditor,
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    panel: Rect,
    pointer: Pointer,
    highlighter: &Highlighter,
) -> ConfigEffects {
    let mut effects = ConfigEffects::default();
    if panel.w <= 0.0 || panel.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return effects;
    }
    let l = layout(panel);
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance) / scale;
    let base_y = (panel.y + PAD + BTN_H / 2.0) * scale
        + f64::from(m.ascent - m.descent) / 2.0;

    // Header: file name + Save/Reload. The status line carries the state.
    text.draw_line(
        canvas,
        &editor.display_name(),
        ((panel.x + PAD) * scale) as f32,
        base_y as f32,
        TextStyle { color: theme.fg, font_size, weight: Weight::BOLD },
    );
    if draw_button(canvas, "Save", l.save, theme, text, pointer, scale) == Press::Clicked {
        effects.save = true;
    }
    if draw_button(canvas, "Reload", l.reload, theme, text, pointer, scale) == Press::Clicked {
        effects.reload = true;
    }

    let (status, color) = if let Some(err) = &editor.error {
        (err.clone(), theme.git_deleted)
    } else if editor.external_change {
        ("changed on disk — Reload to discard your edits".to_string(), theme.git_modified)
    } else if editor.dirty {
        ("modified · Ctrl+S saves".to_string(), theme.git_modified)
    } else {
        ("saved".to_string(), theme.fg_dim)
    };
    let status_baseline = (l.status.y + l.status.h / 2.0) * scale
        + f64::from(m.ascent - m.descent) / 2.0;
    text.draw_line(
        canvas,
        &status,
        ((l.status.x) * scale) as f32,
        status_baseline as f32,
        TextStyle { color, font_size, weight: Weight::NORMAL },
    );

    let changed;
    let mut error: Option<String> = None;
    let env_rows = env_list_rows(l.body);
    editor.viewport_rows = match &editor.body {
        Body::Env(_) => env_rows,
        Body::Text(_) => (l.body.h / LINE_H).floor().max(1.0) as usize,
    };
    if l.body.w <= 0.0 || l.body.h <= 0.0 {
        return effects; // header/status still paint; no body surface
    }
    let mut dragging = editor.dragging;
    let text_lang = match &editor.body {
        Body::Text(t) => lang_for_source(editor.path(), t.line(0)),
        Body::Env(_) => Lang::Plain,
    };
    match &mut editor.body {
        Body::Env(ed) => {
            changed = draw_env(ed, canvas, text, theme, scale, l.body, pointer, advance, font_size, &m);
            if let Some(e) = ed.take_form_error() {
                error = Some(e);
            }
        }
        Body::Text(t) => {
            changed = text::draw(
                t, canvas, text, theme, scale, l.body, pointer, advance, font_size, &m, &mut dragging,
                highlighter, text_lang,
            );
        }
    }
    editor.dragging = dragging;
    if changed {
        super::mark_dirty(editor);
    } else if let Some(e) = error {
        editor.error = Some(e);
    }
    effects.changed = changed;
    effects
}

fn env_list_rows(body: Rect) -> usize {
    let buttons_y = body.y + body.h - PAD - BTN_H;
    let fields_y = buttons_y - 8.0 - FIELD_H;
    let list_h = (fields_y - 8.0 - body.y).max(0.0);
    (list_h / ENV_ROW_H).floor() as usize
}

#[allow(clippy::too_many_arguments)]
fn draw_env(
    ed: &mut EnvEditor,
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    body: Rect,
    pointer: Pointer,
    advance: f64,
    font_size: f32,
    m: &crate::text::MonoMetrics,
) -> bool {
    let mut changed = false;
    let buttons_y = body.y + body.h - PAD - BTN_H;
    let fields_y = buttons_y - 8.0 - FIELD_H;
    let list_h = (fields_y - 8.0 - body.y).max(0.0);
    let rows = (list_h / ENV_ROW_H).floor() as usize;
    // Owned snapshot: selection mutates the model while rows are drawn.
    let entries: Vec<(usize, String, String)> = ed
        .doc
        .entries()
        .into_iter()
        .map(|(line, k, v)| (line, k.to_string(), v.to_string()))
        .collect();
    let max_scroll = entries.len().saturating_sub(rows.max(1));
    if ed.scroll > max_scroll {
        ed.scroll = max_scroll;
    }
    // Keep the selected entry visible (Add appends at the end).
    if let Some(line) = ed.selected {
        if let Some(idx) = entries.iter().position(|(l, _, _)| *l == line) {
            let rows = rows.max(1);
            if idx < ed.scroll {
                ed.scroll = idx;
            } else if idx >= ed.scroll + rows {
                ed.scroll = idx + 1 - rows;
            }
            ed.scroll = ed.scroll.min(max_scroll);
        }
    }
    if pointer.click {
        ed.key.focused = false;
        ed.value.focused = false;
    }
    let row_baseline_off = (ENV_ROW_H / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
    for slot in 0..rows.min(entries.len().saturating_sub(ed.scroll)) {
        let (line, key, value) = &entries[ed.scroll + slot];
        let row_y = body.y + slot as f64 * ENV_ROW_H;
        let row = Rect { x: body.x, y: row_y, w: body.w, h: ENV_ROW_H };
        let selected = ed.selected == Some(*line);
        if selected {
            canvas.fill_rect(row.x, row.y, row.w, row.h, theme.selection, scale);
        } else if row.contains(pointer.x, pointer.y) {
            canvas.fill_rect(row.x, row.y, row.w, row.h, theme.bg_elevated, scale);
        }
        if pointer.click && row.contains(pointer.x, pointer.y) {
            // Selection/caret moves are redraws, not document edits.
            ed.select(*line);
        }
        let shown_value = if env::is_secret_key(key) { env::mask_value(value) } else { value.clone() };
        let display = truncate_right(
            &format!("{key}={shown_value}"),
            (((body.w - 2.0 * PAD) / advance).floor() as usize).max(1),
        );
        let row_baseline = row_y * scale + row_baseline_off;
        text.draw_line(
            canvas,
            &display,
            ((body.x + PAD) * scale) as f32,
            row_baseline as f32,
            TextStyle { color: theme.fg, font_size, weight: Weight::NORMAL },
        );
    }

    // Form: key + value fields, then Add / Apply / Remove.
    let inner_w = (body.w - 2.0 * PAD).max(0.0);
    let key_w = ((inner_w * 0.38).floor()).max(0.0);
    let key_rect = Rect { x: body.x + PAD, y: fields_y, w: key_w, h: FIELD_H };
    let value_rect = Rect {
        x: key_rect.x + key_w + 6.0,
        y: fields_y,
        w: (inner_w - key_w - 6.0).max(0.0),
        h: FIELD_H,
    };
    if pointer.click {
        if key_rect.contains(pointer.x, pointer.y) {
            ed.key.focused = true;
            ed.value.focused = false;
        } else if value_rect.contains(pointer.x, pointer.y) {
            ed.value.focused = true;
            ed.key.focused = false;
        }
    }
    ed.key.draw(canvas, key_rect, theme, text, scale);
    ed.value.draw(canvas, value_rect, theme, text, scale);
    draw_placeholder(canvas, text, theme, scale, key_rect, &ed.key, "key", font_size, m);
    draw_placeholder(canvas, text, theme, scale, value_rect, &ed.value, "value", font_size, m);

    let gap = 6.0;
    let btn_w = ((inner_w - 2.0 * gap) / 3.0).max(0.0);
    let add = Rect { x: body.x + PAD, y: buttons_y, w: btn_w, h: BTN_H };
    let apply = Rect { x: add.x + btn_w + gap, y: buttons_y, w: btn_w, h: BTN_H };
    let remove = Rect { x: apply.x + btn_w + gap, y: buttons_y, w: btn_w, h: BTN_H };
    if draw_button(canvas, "Add", add, theme, text, pointer, scale) == Press::Clicked {
        // Commit the pending form first so Add never discards typed edits.
        let mut can_add = true;
        if ed.selected.is_some() {
            match ed.apply_form() {
                Ok(_) => changed = true,
                Err(e) => {
                    ed.set_form_error(e);
                    can_add = false;
                }
            }
        }
        if can_add {
            let line = ed.doc.append();
            ed.select(line);
            ed.key.focused = true;
            ed.value.focused = false;
            changed = true;
        }
    }
    if draw_button(canvas, "Apply", apply, theme, text, pointer, scale) == Press::Clicked {
        match ed.apply_form() {
            Ok(applied) => changed |= applied,
            Err(e) => ed.set_form_error(e),
        }
    }
    if draw_button(canvas, "Remove", remove, theme, text, pointer, scale) == Press::Clicked {
        if let Some(line) = ed.selected {
            if ed.doc.remove(line) {
                ed.clear_selection();
                ed.key.text.clear();
                ed.key.cursor = 0;
                ed.value.text.clear();
                ed.value.cursor = 0;
                changed = true;
            }
        }
    }
    changed
}

#[allow(clippy::too_many_arguments)] // raster borrows; explicit beats bundling
fn draw_placeholder(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    rect: Rect,
    input: &TextInput,
    label: &str,
    font_size: f32,
    m: &crate::text::MonoMetrics,
) {
    if !input.text.is_empty() || input.focused {
        return;
    }
    let baseline = (rect.y + rect.h / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
    text.draw_line(
        canvas,
        label,
        ((rect.x + 8.0) * scale) as f32,
        baseline as f32,
        TextStyle { color: theme.fg_dim, font_size, weight: Weight::NORMAL },
    );
}

fn truncate_right(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

impl EnvEditor {
    fn set_form_error(&mut self, e: String) {
        self.form_error = Some(e);
    }

    fn take_form_error(&mut self) -> Option<String> {
        self.form_error.take()
    }
}
