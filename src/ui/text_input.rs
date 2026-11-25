//! Single-line text input widget: char-boundary-safe editing model with a
//! hard cap on stored text, plus the focused-state caret rendering.

use cosmic_text::Weight;

use super::{renderable, stroke_rect_inset, Rect};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};

/// Single-line text input: caller-owned `text` + byte-index `cursor` +
/// `focused`. Editing moves the cursor by CHAR boundaries (multi-byte UTF-8
/// is never split) and returns whether anything changed. Drawing paints an
/// `bg_elevated` field with a 1px border (accent while focused), the text in
/// `fg` and — only while focused — a 2px accent cursor column at the caret.
///
/// Memory discipline: text growth is capped at [`TEXT_INPUT_MAX_CHARS`]
/// chars; inserts beyond the cap are refused (`false`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInput {
    /// Field content; always valid UTF-8 (`cursor` stays on char boundaries).
    pub text: String,
    /// Caret position as a BYTE index into `text` (char-boundary aligned).
    pub cursor: usize,
    /// Whether the field has keyboard focus (only focused fields consume keys).
    pub focused: bool,
}

/// Upper bound on stored text (fixed-size state rule, AGENTS.md guardrail 3).
pub(crate) const TEXT_INPUT_MAX_CHARS: usize = 128;

/// Keyboard events a focused [`TextInput`] understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputKey {
    /// Insert a printable character at the caret.
    Char(char),
    Backspace,
    Left,
    Right,
    Home,
    End,
}

/// Previous char boundary at or before byte `i` in `s` (`i` itself counts if
/// it is a boundary and > 0 → walks left). `s` is always valid UTF-8.
fn prev_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Next char boundary strictly after byte `i` in `s`, or `None` at the end.
fn next_char_boundary(s: &str, mut i: usize) -> Option<usize> {
    if i >= s.len() {
        return None;
    }
    i += 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    Some(i)
}

impl TextInput {
    /// Map a winit logical key/text event into editing keys. Named navigation
    /// keys are handled before text so Backspace's `\x08` is not inserted.
    pub fn keys_from_logical_and_text(
        logical_key: &winit::keyboard::Key,
        text: Option<&str>,
    ) -> Vec<TextInputKey> {
        use winit::keyboard::{Key, NamedKey};
        if let Key::Named(named) = logical_key {
            match named {
                NamedKey::Backspace => return vec![TextInputKey::Backspace],
                NamedKey::ArrowLeft => return vec![TextInputKey::Left],
                NamedKey::ArrowRight => return vec![TextInputKey::Right],
                NamedKey::Home => return vec![TextInputKey::Home],
                NamedKey::End => return vec![TextInputKey::End],
                _ => {}
            }
        }
        text.map(|s| {
            s.chars()
                .filter(|c| !c.is_control())
                .map(TextInputKey::Char)
                .collect()
        })
        .unwrap_or_default()
    }

    /// Byte index of the caret, floored to a char boundary and clamped into
    /// the string (defends against a caller-set mid-char index).
    fn bounded_cursor(&self) -> usize {
        prev_char_boundary(&self.text, self.cursor.min(self.text.len()))
    }

    /// Insert `s` at the caret. Returns whether the text changed. Empty
    /// strings and inserts that would exceed [`TEXT_INPUT_MAX_CHARS`] are
    /// no-ops.
    #[allow(dead_code)] // frozen M2 surface; M8/M9 use the capped variants
    pub fn insert(&mut self, s: &str) -> bool {
        self.insert_with_cap(s, TEXT_INPUT_MAX_CHARS)
    }

    /// Insert with a caller-chosen cap (M9 config values can be longer than
    /// one-line UI fields; the cap is still enforced per insert).
    pub fn insert_with_cap(&mut self, s: &str, max_chars: usize) -> bool {
        if s.is_empty() {
            return false;
        }
        if self.text.chars().count() + s.chars().count() > max_chars {
            return false;
        }
        let cursor = self.bounded_cursor();
        self.text.insert_str(cursor, s);
        self.cursor = cursor + s.len();
        true
    }

    /// Remove the char left of the caret. `false` at position 0.
    pub fn backspace(&mut self) -> bool {
        let cursor = self.bounded_cursor();
        if cursor == 0 {
            return false;
        }
        let start = prev_char_boundary(&self.text, cursor - 1);
        self.text.replace_range(start..cursor, "");
        self.cursor = start;
        true
    }

    /// Apply one key to the field; returns whether the field changed.
    /// Control characters never insert.
    pub fn handle_key(&mut self, key: TextInputKey) -> bool {
        self.handle_key_with_cap(key, TEXT_INPUT_MAX_CHARS)
    }

    /// [`handle_key`](Self::handle_key) with a caller-chosen char cap.
    pub fn handle_key_with_cap(&mut self, key: TextInputKey, max_chars: usize) -> bool {
        match key {
            TextInputKey::Char(c) => {
                if c.is_control() {
                    return false;
                }
                let mut buf = [0u8; 4];
                self.insert_with_cap(c.encode_utf8(&mut buf), max_chars)
            }
            TextInputKey::Backspace => self.backspace(),
            TextInputKey::Left => {
                let cursor = self.bounded_cursor();
                if cursor == 0 {
                    return false;
                }
                self.cursor = prev_char_boundary(&self.text, cursor - 1);
                true
            }
            TextInputKey::Right => {
                let cursor = self.bounded_cursor();
                match next_char_boundary(&self.text, cursor) {
                    Some(next) => {
                        self.cursor = next;
                        true
                    }
                    None => false,
                }
            }
            TextInputKey::Home => {
                if self.cursor == 0 {
                    return false;
                }
                self.cursor = 0;
                true
            }
            TextInputKey::End => {
                if self.cursor == self.text.len() {
                    return false;
                }
                self.cursor = self.text.len();
                true
            }
        }
    }

    /// Draw the field. Degenerate rects or scales rasterize nothing.
    pub fn draw(&self, canvas: &mut Canvas, rect: Rect, theme: &Theme, text: &mut TextSystem, scale: f64) {
        if !renderable(rect, scale) {
            return;
        }
        canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.bg_elevated, scale);
        // 1px border ring: accent while focused, border otherwise.
        let ring = if self.focused { theme.accent } else { theme.border };
        stroke_rect_inset(canvas.pixmap_mut(), rect.scaled(scale), ring, scale as f32);
        self.draw_contents(canvas, rect, theme, text, scale);
    }

    /// Draw the field with a rounded border, used by inline tree-row prompts.
    pub fn draw_rounded(
        &self,
        canvas: &mut Canvas,
        rect: Rect,
        theme: &Theme,
        text: &mut TextSystem,
        scale: f64,
        radius: f64,
    ) {
        if !renderable(rect, scale) {
            return;
        }
        let ring = if self.focused { theme.accent } else { theme.border };
        let radius = radius.clamp(0.0, rect.w.min(rect.h) / 2.0);
        canvas.fill_round_rect(rect.x, rect.y, rect.w, rect.h, radius, ring, scale);
        canvas.fill_round_rect(
            rect.x + 1.0,
            rect.y + 1.0,
            rect.w - 2.0,
            rect.h - 2.0,
            (radius - 1.0).max(0.0),
            theme.bg_elevated,
            scale,
        );
        self.draw_contents(canvas, rect, theme, text, scale);
    }

    fn draw_contents(
        &self,
        canvas: &mut Canvas,
        rect: Rect,
        theme: &Theme,
        text: &mut TextSystem,
        scale: f64,
    ) {
        let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
        let metrics = text.mono_metrics(font_size, (font_size * 1.35).round());
        let pad = 8.0; // logical px of horizontal text padding
        let advance_logical = f64::from(metrics.advance) / scale;
        // draw_line takes PHYSICAL px: scale the logical center; the
        // metrics term is already physical (measured at the scaled font
        // size) and must NOT be scaled again.
        let baseline = ((rect.y + rect.h / 2.0) * scale
            + f64::from(metrics.ascent - metrics.descent) / 2.0) as f32;

        // Only the chars that fit the field are drawn — glyph blitting has
        // no clip support, and ink must never leave the layout bounds. The
        // full text still edits; M9 brings the real editing model.
        let chars: Vec<char> = self.text.chars().collect();
        let inner_w = rect.w - 2.0 * pad;
        let max_fit = (inner_w / advance_logical).floor().max(0.0) as usize;
        if max_fit > 0 {
            let shown: String = chars.iter().take(max_fit).collect();
            text.draw_line(
                canvas,
                &shown,
                ((rect.x + pad) * scale) as f32,
                baseline,
                TextStyle { color: theme.fg, font_size, weight: Weight::NORMAL },
            );
        }

        // Accent cursor column, only while focused, only when the caret sits
        // inside the visible region.
        if self.focused {
            // bounded_cursor is a BYTE index — count the chars BEFORE it,
            // never `take(byte_index)` on the char iterator (multi-byte
            // text would shift the caret right by one char per extra byte).
            let n_before = self.text[..self.bounded_cursor()].chars().count();
            let caret = rect.x + pad + n_before as f64 * advance_logical;
            if caret >= rect.x + pad && caret <= rect.x + rect.w - pad {
                canvas.fill_rect(caret - 1.0, rect.y + 4.0, 2.0, rect.h - 8.0, theme.accent, scale);
            }
        }
    }
}

// The widget's test module (editing model + raster pins) traveled here from
// ui.rs and would push this file past the 400-line guidance, so it lives in
// the widget's submodule directory.
#[cfg(test)]
mod tests;
