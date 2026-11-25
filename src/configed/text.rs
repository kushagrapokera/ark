//! Minimal multi-line text model for the YAML/TOML config editor (M9).
//!
//! Byte-indexed UTF-8 editing (all indices stay on char boundaries), a
//! normalized selection range, viewport scrolling and bounded undo/redo
//! deltas (never full-text snapshots). IME preedit text is held separately
//! until `Ime::Commit`.

mod pos;

use pos::{floor_boundary, line_starts, next_boundary, prev_boundary, truncate_to_boundary};

/// Undo depth cap (ops).
pub const MAX_UNDO_OPS: usize = 200;
/// Undo memory cap (removed + inserted bytes across undo/redo).
pub const MAX_UNDO_BYTES: usize = 512 * 1024;
/// IME preedit text cap (transient composition string).
pub const MAX_PREEDIT_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Debug)]
struct EditOp {
    start: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
}

impl EditOp {
    fn size(&self) -> usize {
        self.removed.len() + self.inserted.len()
    }
}

/// One editable text buffer. `text` itself is capped by the caller/open
/// boundary; undo deltas are capped by [`MAX_UNDO_OPS`]/[`MAX_UNDO_BYTES`].
#[derive(Clone, Debug)]
pub struct TextEditor {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    preferred_col: Option<usize>,
    scroll_line: usize,
    hscroll_col: usize,
    max_bytes: usize,
    line_starts: Vec<usize>,
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
    undo_bytes: usize,
    preedit: Option<String>,
    dirty: bool,
    caret_visible: bool,
}

impl TextEditor {
    pub fn new(text: String, max_bytes: usize) -> Self {
        let text = truncate_to_boundary(text, max_bytes);
        let mut editor = TextEditor {
            line_starts: line_starts(&text),
            text,
            cursor: 0,
            anchor: None,
            preferred_col: None,
            scroll_line: 0,
            hscroll_col: 0,
            max_bytes,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            preedit: None,
            dirty: false,
            caret_visible: true,
        };
        editor.cursor = editor.text.len();
        editor
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.line_col(self.cursor)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line(&self, index: usize) -> &str {
        let Some(&start) = self.line_starts.get(index) else {
            return "";
        };
        let end = self.line_end(index);
        &self.text[start..end]
    }

    pub fn line_start(&self, index: usize) -> usize {
        self.line_starts.get(index).copied().unwrap_or(self.text.len())
    }

    pub fn scroll_line(&self) -> usize {
        self.scroll_line
    }

    pub fn hscroll_col(&self) -> usize {
        self.hscroll_col
    }

    pub fn selection(&self) -> Option<std::ops::Range<usize>> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(if anchor < self.cursor { anchor..self.cursor } else { self.cursor..anchor })
    }

    /// Alias used by the editor UI/tests (same normalized range).
    pub fn selected_range(&self) -> Option<std::ops::Range<usize>> {
        self.selection()
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|r| &self.text[r])
    }

    /// Insert text at the selection/caret. CRLF/CR normalizes to LF; NUL is
    /// refused; growth past `max_bytes` is refused. Returns whether changed.
    pub fn insert(&mut self, s: &str) -> bool {
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() || normalized.contains('\0') {
            return false;
        }
        let range = self.selection().unwrap_or(self.cursor..self.cursor);
        if self.text.len() - range.len() + normalized.len() > self.max_bytes {
            return false;
        }
        self.replace_range(range, &normalized);
        true
    }

    pub fn backspace(&mut self) -> bool {
        let range = match self.selection() {
            Some(r) => r,
            None if self.cursor == 0 => return false,
            None => prev_boundary(&self.text, self.cursor)..self.cursor,
        };
        self.replace_range(range, "");
        true
    }

    pub fn delete(&mut self) -> bool {
        let range = match self.selection() {
            Some(r) => r,
            None if self.cursor >= self.text.len() => return false,
            None => self.cursor..next_boundary(&self.text, self.cursor),
        };
        self.replace_range(range, "");
        true
    }

    /// Move the caret. `select` extends the selection from the existing
    /// anchor (Shift+arrow); otherwise the selection collapses.
    pub fn move_cursor(&mut self, motion: Motion, select: bool) -> bool {
        let before = (self.cursor, self.anchor);
        if select && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        } else if !select {
            self.anchor = None;
        }
        let (line, col) = self.line_col(self.cursor);
        match motion {
            Motion::Left => {
                self.cursor = prev_boundary(&self.text, self.cursor);
                self.preferred_col = None;
            }
            Motion::Right => {
                self.cursor = next_boundary(&self.text, self.cursor);
                self.preferred_col = None;
            }
            Motion::Home => {
                self.cursor = self.line_start(line);
                self.preferred_col = None;
            }
            Motion::End => {
                self.cursor = self.line_end(line);
                self.preferred_col = None;
            }
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown => {
                let want = self.preferred_col.unwrap_or(col);
                self.preferred_col = Some(want);
                let step = match motion {
                    Motion::PageUp => 20usize,
                    Motion::PageDown => 20,
                    _ => 1,
                };
                let target = match motion {
                    Motion::Up | Motion::PageUp => line.saturating_sub(step),
                    _ => (line + step).min(self.line_count().saturating_sub(1)),
                };
                self.cursor = self.byte_at(target, want);
            }
        }
        if !select {
            self.anchor = None;
        }
        (self.cursor, self.anchor) != before
    }

    pub fn select_all(&mut self) -> bool {
        if self.text.is_empty() {
            return false;
        }
        let before = (self.cursor, self.anchor);
        self.anchor = Some(0);
        self.cursor = self.text.len();
        self.preferred_col = None;
        (self.cursor, self.anchor) != before
    }

    /// Select the first case-insensitive literal match on a 1-based line.
    /// Missing/empty matches clear the selection without modifying the document.
    pub fn select_search_match(&mut self, line_1_based: usize, query: &str) -> bool {
        let before = (self.cursor, self.anchor);
        let line = line_1_based.saturating_sub(1).min(self.line_count().saturating_sub(1));
        let start = self.line_start(line);
        let end = self.line_end(line);
        let found = if query.is_empty() || query.chars().count() > 128 {
            None
        } else {
            regex::RegexBuilder::new(&regex::escape(query))
                .case_insensitive(true)
                .build()
                .ok()
                .and_then(|pattern| pattern.find(&self.text[start..end]))
        };
        match found {
            Some(found) => {
                self.anchor = Some(start + found.start());
                self.cursor = start + found.end();
            }
            None => {
                self.anchor = None;
                self.cursor = start;
            }
        }
        self.preferred_col = None;
        (self.cursor, self.anchor) != before
    }

    /// Set the caret from (line, char-col), clamped. `select` keeps the old
    /// caret as the anchor.
    pub fn set_cursor(&mut self, line: usize, col: usize, select: bool) {
        let old = self.cursor;
        let line = line.min(self.line_count().saturating_sub(1));
        self.cursor = self.byte_at(line, col);
        self.anchor = if select { Some(old) } else { None };
        self.preferred_col = Some(col);
    }

    /// Mouse-drag start: caret at the point with a collapsed selection.
    pub fn start_selection(&mut self, line: usize, col: usize) {
        self.set_cursor(line, col, false);
        self.anchor = Some(self.cursor);
    }

    /// Mouse-drag continuation: keeps the original anchor.
    pub fn extend_selection(&mut self, line: usize, col: usize) {
        let anchor = self.anchor.or(Some(self.cursor));
        let line = line.min(self.line_count().saturating_sub(1));
        self.cursor = self.byte_at(line, col);
        self.anchor = anchor;
        self.preferred_col = Some(col);
    }

    /// Mouse click: collapse to the point.
    pub fn collapse_to(&mut self, line: usize, col: usize) {
        self.set_cursor(line, col, false);
    }

    pub fn undo(&mut self) -> bool {
        let Some(op) = self.undo.pop() else {
            return false;
        };
        let end = op.start + op.inserted.len();
        if end > self.text.len() {
            return false; // defensive: stale op cannot corrupt the buffer
        }
        self.text.replace_range(op.start..end, &op.removed);
        self.cursor = op.cursor_before.min(self.text.len());
        self.anchor = None;
        self.preferred_col = None;
        self.dirty = true;
        self.rebuild_lines();
        self.redo.push(op);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(op) = self.redo.pop() else {
            return false;
        };
        let end = op.start + op.removed.len();
        if end > self.text.len() {
            return false;
        }
        self.text.replace_range(op.start..end, &op.inserted);
        self.cursor = op.start + op.inserted.len();
        self.anchor = None;
        self.preferred_col = None;
        self.dirty = true;
        self.rebuild_lines();
        self.undo.push(op);
        true
    }

    /// Wheel scroll in whole lines, clamped to the document.
    pub fn scroll_by(&mut self, lines: i64, viewport: usize) -> bool {
        let max = self.line_count().saturating_sub(viewport.max(1));
        let before = self.scroll_line;
        let next = if lines >= 0 {
            self.scroll_line.saturating_add(lines as usize)
        } else {
            self.scroll_line.saturating_sub(lines.unsigned_abs() as usize)
        };
        self.scroll_line = next.min(max);
        self.scroll_line != before
    }

    /// Keep the caret visible in a `rows × cols` viewport (called from draw).
    pub fn ensure_cursor_visible(&mut self, rows: usize, cols: usize) {
        let (line, col) = self.line_col(self.cursor);
        let rows = rows.max(1);
        if line < self.scroll_line {
            self.scroll_line = line;
        } else if line >= self.scroll_line + rows {
            self.scroll_line = line + 1 - rows;
        }
        let cols = cols.max(1);
        if col < self.hscroll_col {
            self.hscroll_col = col;
        } else if col >= self.hscroll_col + cols {
            self.hscroll_col = col + 1 - cols;
        }
    }

    pub fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    pub fn set_caret_visible(&mut self, visible: bool) {
        self.caret_visible = visible;
    }

    pub fn preedit(&self) -> Option<&str> {
        self.preedit.as_deref()
    }

    /// IME preedit update (empty text clears it). Capped like the document.
    pub fn set_preedit(&mut self, text: &str) -> bool {
        let capped: String = text.chars().take(MAX_PREEDIT_CHARS).collect();
        let next = if capped.is_empty() { None } else { Some(capped) };
        if self.preedit == next {
            return false;
        }
        self.preedit = next;
        true
    }

    fn replace_range(&mut self, range: std::ops::Range<usize>, inserted: &str) {
        let removed = self.text[range.clone()].to_string();
        let cursor_before = self.cursor;
        self.text.replace_range(range.clone(), inserted);
        self.cursor = range.start + inserted.len();
        self.anchor = None;
        self.preferred_col = None;
        self.dirty = true;
        self.rebuild_lines();
        self.clear_redo();
        let op = EditOp { start: range.start, removed, inserted: inserted.to_string(), cursor_before };
        self.undo_bytes += op.size();
        self.undo.push(op);
        while (self.undo.len() > MAX_UNDO_OPS || self.undo_bytes > MAX_UNDO_BYTES)
            && !self.undo.is_empty()
        {
            let removed = self.undo.remove(0);
            self.undo_bytes = self.undo_bytes.saturating_sub(removed.size());
        }
    }

    fn clear_redo(&mut self) {
        for op in self.redo.drain(..) {
            self.undo_bytes = self.undo_bytes.saturating_sub(op.size());
        }
    }

    fn rebuild_lines(&mut self) {
        self.line_starts = line_starts(&self.text);
    }

    fn line_col(&self, byte: usize) -> (usize, usize) {
        let byte = floor_boundary(&self.text, byte.min(self.text.len()));
        let line = self.line_starts.partition_point(|&start| start <= byte).saturating_sub(1);
        let start = self.line_starts[line];
        (line, self.text[start..byte].chars().count())
    }

    fn line_end(&self, line: usize) -> usize {
        self.line_starts
            .get(line + 1)
            .map(|&next| next.saturating_sub(1))
            .unwrap_or(self.text.len())
    }

    fn byte_at(&self, line: usize, col: usize) -> usize {
        let start = self.line_start(line);
        let end = self.line_end(line);
        let mut byte = start;
        for _ in 0..col {
            let next = next_boundary(&self.text, byte);
            if next > end {
                break;
            }
            byte = next;
        }
        byte.min(end)
    }
}

#[cfg(test)]
mod tests;
