//! Read-only viewer (M4): mmap-backed line view with a bounded line index,
//! viewport-only rendering support, text selection and copy.
//!
//! Memory rules (AGENTS.md guardrail 3):
//! - the viewed region is capped at [`VIEW_CAP_BYTES`]; larger files show a
//!   preview with `truncated` set (no chunked paging UI in M4);
//! - the line index is `Vec<u32>` of line-start offsets — ~4 bytes per line,
//!   bounded by the view cap;
//! - the mmap grows RSS only by the pages actually touched (viewport reads);
//! - binaries are refused (NUL sniff in the first [`BINARY_SNIFF_BYTES`]).
//!
//! Scrolling is LINE-based (integer lines, no partial rows) — the
//! monospace-grid choice that avoids all text-clipping machinery. Copy
//! normalizes line endings to `\n` (trailing `\r` is stripped for both
//! display and extraction).

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cosmic_text::Weight;
use memmap2::Mmap;

use crate::git::{FileDiff, GutterMark};
use crate::highlight::{Highlighter, Token};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::ui::{Pointer, Rect};

/// Cap on the viewed region of a file (larger files render a preview).
pub const VIEW_CAP_BYTES: u64 = 64 * 1024 * 1024;
/// NUL sniff window for binary refusal.
pub const BINARY_SNIFF_BYTES: usize = 8 * 1024;
/// Logical px per viewer text line (UI_FONT_SIZE × 1.35).
pub const LINE_H: f64 = 18.9;
/// Width of the gutter strip reserved for git marks (M7 paints into it).
pub const MARK_W: f64 = 4.0;
/// Gutter padding around the line-number digits.
const GUT_PAD_L: f64 = 6.0;
const GUT_PAD_R: f64 = 10.0;

/// Viewer open failures. `Binary` is a refusal, not a silent fallback.
#[derive(Debug)]
pub enum ViewerError {
    Binary { path: PathBuf },
    Io(std::io::Error),
}

impl fmt::Display for ViewerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewerError::Binary { path } => write!(f, "refusing binary file {}", path.display()),
            ViewerError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ViewerError {}

/// One selected span: anchor and head in (line, char-col) coordinates, in
/// DISPLAY space (lossy text, `\r`-stripped). Zero-length when equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    anchor: (usize, usize),
    head: (usize, usize),
}

impl Selection {
    /// Normalized (start, end) with start ≤ end.
    fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor.0 < self.head.0
            || (self.anchor.0 == self.head.0 && self.anchor.1 <= self.head.1)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// A read-only view of one file. Fixed-size state: mmap + `Vec<u32>` line
/// index + selection; nothing grows with interaction.
pub struct Viewer {
    path: PathBuf,
    map: Mmap,
    /// Full file size at open time (disk-change detection compares it).
    file_len: u64,
    /// File mtime at open time (disk-change detection compares it).
    modified: SystemTime,
    /// Bytes of the file visible through this view (≤ file len, ≤ cap).
    view_len: u64,
    truncated: bool,
    /// Line-start offsets within the viewed region; `starts[0] == 0`.
    starts: Vec<u32>,
    /// Scroll position in lines (kept clamped by the scroll API).
    offset_lines: usize,
    sel: Option<Selection>,
    /// Selection-drag latch: started on a press inside the panel, extends on
    /// every down frame (panel-clamped), ends on release.
    dragging: bool,
}

impl Viewer {
    /// Open a file with the default view cap. See `open_with_cap`.
    pub fn open(path: &Path) -> Result<Viewer, ViewerError> {
        Self::open_with_cap(path, VIEW_CAP_BYTES)
    }

    /// Open `path` read-only: mmap it, sniff for binaries, index line starts
    /// within the view cap. Missing/unreadable files surface as
    /// `ViewerError::Io`, detected binaries as `ViewerError::Binary`.
    pub fn open_with_cap(path: &Path, cap: u64) -> Result<Viewer, ViewerError> {
        let file = std::fs::File::open(path).map_err(ViewerError::Io)?;
        let meta = file.metadata().map_err(ViewerError::Io)?;
        let len = meta.len();
        let modified = meta.modified().map_err(ViewerError::Io)?;
        let view_len = len.min(cap);
        if view_len > u32::MAX as u64 {
            return Err(ViewerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} exceeds the indexable view size", path.display()),
            )));
        }
        // Safety: the mapping is read-only and the file is not mutated by us;
        // external truncation (SIGBUS) is M5's watcher-refresh concern.
        let map = unsafe { Mmap::map(&file).map_err(ViewerError::Io)? };
        let view = &map[..view_len as usize];
        if view[..BINARY_SNIFF_BYTES.min(view.len())].contains(&0) {
            return Err(ViewerError::Binary { path: path.to_path_buf() });
        }
        let mut starts = Vec::new();
        if view_len > 0 {
            starts.push(0u32);
            for (i, b) in view.iter().enumerate() {
                if *b == b'\n' && (i as u64) + 1 < view_len {
                    starts.push((i + 1) as u32);
                }
            }
        }
        Ok(Viewer {
            path: path.to_path_buf(),
            map,
            file_len: len,
            modified,
            view_len,
            truncated: len > view_len,
            starts,
            offset_lines: 0,
            sel: None,
            dragging: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// True when the file on disk differs from what this view holds (len or
    /// mtime changed, or the stat fails — e.g. the file vanished).
    pub fn changed_on_disk(&self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(m) => {
                m.len() != self.file_len
                    || m.modified().map(|t| t != self.modified).unwrap_or(true)
            }
            Err(_) => true,
        }
    }

    /// Raw offset restore for cache refresh (clamped at the next draw's
    /// self-heal); carries the read position across a disk refresh.
    pub fn restore_offset(&mut self, v: usize) {
        self.offset_lines = v;
    }

    /// Number of lines in the viewed region (an empty file has 0 lines).
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Byte range of line `i` excluding its `\n` and any trailing `\r`
    /// (display/extraction space). `None` out of range.
    fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)? as usize;
        let mut end = if line + 1 < self.starts.len() {
            self.starts[line + 1] as usize
        } else {
            self.view_len as usize
        };
        // Exclude the newline and a trailing CR (display normalization).
        while end > start && (self.map[end - 1] == b'\n' || self.map[end - 1] == b'\r') {
            end -= 1;
        }
        Some(start..end)
    }

    /// Display text of line `i`: lossy UTF-8, `\r` stripped. `None` out of
    /// range.
    pub fn line_text(&self, line: usize) -> Option<Cow<'_, str>> {
        let range = self.line_range(line)?;
        Some(String::from_utf8_lossy(&self.map[range]))
    }

    pub fn offset_lines(&self) -> usize {
        self.offset_lines
    }

    /// Max scroll offset (lines) for a viewport of `viewport_lines` rows.
    /// A zero-height (collapsed) viewport forces offset 0 — nothing is
    /// visible, so nothing scrolls (same discipline as `ui::Scrollbar`).
    pub fn max_offset_lines(&self, viewport_lines: usize) -> usize {
        if viewport_lines == 0 {
            return 0;
        }
        self.line_count().saturating_sub(viewport_lines)
    }

    /// Scroll by `dy` lines (positive = toward the end), clamped; returns
    /// whether the offset moved.
    pub fn scroll_by_lines(&mut self, dy: i64, viewport_lines: usize) -> bool {
        self.set_offset_lines(
            (self.offset_lines as i64).saturating_add(dy).max(0) as usize,
            viewport_lines,
        )
    }

    /// Set the offset (clamped like `scroll_by_lines`); returns whether it
    /// moved.
    pub fn set_offset_lines(&mut self, v: usize, viewport_lines: usize) -> bool {
        let before = self.offset_lines;
        self.offset_lines = v.min(self.max_offset_lines(viewport_lines));
        self.offset_lines != before
    }

    /// Selection as ordered (start_line, start_col, end_line, end_col);
    /// `None` when nothing is selected or the span is zero-length.
    pub fn selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let sel = self.sel.as_ref()?;
        let ((sl, sc), (el, ec)) = sel.ordered();
        if (sl, sc) == (el, ec) {
            None
        } else {
            Some(((sl, sc), (el, ec)))
        }
    }

    /// Selected char count (0 when nothing selected).
    pub fn selection_len(&self) -> usize {
        let Some(((sl, sc), (el, ec))) = self.selection() else {
            return 0;
        };
        let mut n = 0;
        for line in sl..=el {
            let len = self.line_text(line).map(|t| t.chars().count()).unwrap_or(0);
            let (a, b) = if sl == el {
                (sc, ec)
            } else if line == sl {
                (sc, len)
            } else if line == el {
                (0, ec)
            } else {
                (0, len)
            };
            n += b.saturating_sub(a);
            if line != el {
                n += 1; // the newline joining lines
            }
        }
        n
    }

    /// The selected text: lines joined with `\n`; full lines come verbatim
    /// from the file bytes (normalized `\r`-stripped), partial lines map
    /// char columns through the lossy display text. `None` when nothing is
    /// selected.
    pub fn selection_text(&self) -> Option<String> {
        let ((sl, sc), (el, ec)) = self.selection()?;
        let mut out = String::new();
        for line in sl..=el {
            if line > sl {
                out.push('\n');
            }
            let Some(text) = self.line_text(line) else { break };
            let len = text.chars().count();
            let (a, b) = if sl == el {
                (sc, ec)
            } else if line == sl {
                (sc, len)
            } else if line == el {
                (0, ec)
            } else {
                (0, len)
            };
            for (ci, ch) in text.chars().enumerate() {
                if ci >= a && ci < b {
                    out.push(ch);
                }
            }
        }
        Some(out)
    }

    /// Clamp a (line, col) pointer position to the content.
    pub fn pos_at(&self, line: usize, col: usize) -> (usize, usize) {
        if self.line_count() == 0 {
            return (0, 0);
        }
        let line = line.min(self.line_count() - 1);
        let col = self
            .line_text(line)
            .map(|t| t.chars().count())
            .unwrap_or(0)
            .min(col);
        (line, col)
    }

    /// Start (or collapse) a selection at the clamped position.
    pub fn select_at(&mut self, line: usize, col: usize) {
        let pos = self.pos_at(line, col);
        self.sel = Some(Selection { anchor: pos, head: pos });
    }

    /// Move the selection head (starting a selection if none exists).
    pub fn extend_select(&mut self, line: usize, col: usize) {
        let pos = self.pos_at(line, col);
        match self.sel.as_mut() {
            Some(sel) => sel.head = pos,
            None => self.sel = Some(Selection { anchor: pos, head: pos }),
        }
    }

    /// Drop the selection.
    #[allow(dead_code)] // frozen-interface surface; a keyboard "Esc clears selection" affordance consumes it
    pub fn collapse_selection(&mut self) {
        self.sel = None;
    }

    /// Direct assertion surface for the LRU eviction test.
    #[cfg(test)]
    pub(crate) fn is_open_for(&self, path: &Path) -> bool {
        self.path == path
    }
}

/// Copy `text` to the Wayland clipboard (data-control protocol). The crate
/// spawns its own serving thread and returns immediately; ownership ends
/// when something else copies. Errors surface to the caller (logged — a
/// failed copy is never silent, but it is also never fatal).
pub fn copy_to_clipboard(text: &str) -> Result<(), std::io::Error> {
    use wl_clipboard_rs::copy::{copy, MimeType, Options, Source};
    copy(
        Options::new(),
        Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
        MimeType::Text,
    )
    .map_err(|e| std::io::Error::other(format!("clipboard copy failed: {e}")))
}

// ---- Panel surface: interaction + viewport-only rendering ----

/// Gutter width for a line count at the given LOGICAL char advance: the
/// reserved git-mark strip, padded line-number digits, and right padding.
pub fn gutter_width(line_count: usize, advance: f64) -> f64 {
    let digits = if line_count == 0 { 1 } else { line_count.checked_ilog10().unwrap_or(0) as usize + 1 };
    MARK_W + GUT_PAD_L + digits as f64 * advance.max(0.0) + GUT_PAD_R
}

/// Map a pointer position to content (line, char col), clamped. Private
/// geometry shared by `handle_pointer`.
fn pos_in_panel(viewer: &Viewer, panel: Rect, x: f64, y: f64, advance: f64) -> (usize, usize) {
    let row = (((y - panel.y) / LINE_H).floor().max(0.0)) as usize;
    let line = viewer.offset_lines() + row;
    let text_x = panel.x + gutter_width(viewer.line_count(), advance);
    let col = (((x - text_x) / advance).floor().max(0.0)) as usize;
    viewer.pos_at(line, col)
}

/// Pointer interaction for the viewer panel: a press inside the panel starts
/// a selection (anchor = head), dragging extends the head (clamped to the
/// content, standard behavior past the panel edges), release finalizes. A
/// click without drag leaves a zero-length selection (renders nothing).
/// Returns whether the selection changed.
pub fn handle_pointer(viewer: &mut Viewer, panel: Rect, pointer: Pointer, advance: f64) -> bool {
    if !advance.is_finite() || advance <= 0.0 || panel.w <= 0.0 || panel.h <= 0.0 {
        return false;
    }
    let before = viewer.sel;
    if pointer.down {
        let inside = panel.contains(pointer.x, pointer.y);
        if viewer.dragging || inside {
            let was_dragging = viewer.dragging;
            viewer.dragging = true;
            let pos = pos_in_panel(viewer, panel, pointer.x, pointer.y, advance);
            if was_dragging {
                viewer.extend_select(pos.0, pos.1);
            } else {
                viewer.select_at(pos.0, pos.1);
            }
        }
    } else if viewer.dragging {
        viewer.dragging = false; // release finalizes; selection persists
    }
    viewer.sel != before
}

fn token_color(theme: &Theme, token: Token) -> crate::render::ColorRgb {
    match token {
        Token::Keyword => theme.syntax_keyword,
        Token::String => theme.syntax_string,
        Token::Number => theme.syntax_number,
        Token::Comment => theme.syntax_comment,
    }
}

/// Substring of `s` between CHAR indices [a, b).
fn char_range(s: &str, a: usize, b: usize) -> &str {
    let start = s.char_indices().nth(a).map(|(i, _)| i).unwrap_or(s.len());
    let end = s.char_indices().nth(b).map(|(i, _)| i).unwrap_or(s.len());
    &s[start..end]
}

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
        return; // degenerate panel or scale: nothing to raster
    }
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance) / scale; // logical px per char

    if viewer.line_count() == 0 {
        canvas.fill_rect(panel.x, panel.y, panel.w, panel.h, theme.bg, scale);
        let cy = (panel.y + panel.h / 2.0) * scale;
        text.draw_line(
            canvas,
            "(empty file)",
            ((panel.x + panel.w / 2.0) * scale - f64::from(m.advance) * 6.0) as f32,
            (cy + f64::from(m.ascent - m.descent) / 2.0) as f32,
            TextStyle { color: theme.fg_dim, font_size, weight: Weight::NORMAL },
        );
        return;
    }

    // Reserved git-mark strip: the M7 diff marks paint into it.
    canvas.fill_rect(panel.x, panel.y, MARK_W, panel.h, theme.bg_elevated, scale);
    let offset_lines = viewer.offset_lines();
    let visible_rows = ((panel.h / LINE_H).floor() as usize).max(1);
    if let Some(diff) = file_diff {
        for (line, kind) in &diff.marks {
            // line is 1-based; the strip bar spans the line's row band.
            let row_i = *line as i64 - 1 - offset_lines as i64;
            if row_i < 0 || row_i as usize >= visible_rows {
                continue;
            }
            let y = panel.y + row_i as f64 * LINE_H;
            let color = match kind {
                GutterMark::Added => theme.git_added,
                GutterMark::Modified => theme.git_modified,
            };
            canvas.fill_rect(panel.x + 1.0, y + 1.0, MARK_W - 2.0, LINE_H - 2.0, color, scale);
        }
    }

    let gutter_w = gutter_width(viewer.line_count(), advance);
    let text_x = panel.x + gutter_w;
    let first = viewer.offset_lines();
    // Whole rows only: a partial row at the bottom would paint ink past the
    // panel (there is no clip machinery) — the sliver stays background.
    let rows = ((panel.h / LINE_H).floor() as usize).max(1);
    let baseline_off = f64::from(m.ascent - m.descent) / 2.0;

    for i in first..(first + rows).min(viewer.line_count()) {
        let row_y = panel.y + (i - first) as f64 * LINE_H;
        // Selection background: intersection of the selection span with
        // this line, in char columns → x/w.
        if let Some(((sl, sc), (el, ec))) = viewer.selection() {
            if i >= sl && i <= el {
                let len = viewer.line_text(i).map(|t| t.chars().count()).unwrap_or(0);
                let (a, b) = if sl == el {
                    (sc, ec)
                } else if i == sl {
                    (sc, len)
                } else if i == el {
                    (0, ec)
                } else {
                    (0, len)
                };
                if b > a {
                    canvas.fill_rect(
                        text_x + a as f64 * advance,
                        row_y,
                        (b - a) as f64 * advance,
                        LINE_H,
                        theme.selection,
                        scale,
                    );
                }
            }
        }

        let Some(line) = viewer.line_text(i) else { continue };

        // Gutter line number, right-aligned against the text edge.
        let num = (i + 1).to_string();
        let num_x = panel.x + gutter_w - GUT_PAD_R - num.chars().count() as f64 * advance;
        let baseline = (row_y + LINE_H / 2.0) * scale + baseline_off;
        text.draw_line(
            canvas,
            &num,
            (num_x * scale) as f32,
            baseline as f32,
            TextStyle { color: theme.fg_dim, font_size, weight: Weight::NORMAL },
        );

        // Text segments: plain fg runs between token-colored spans. Drawing
        // per segment segments the shaping (no ligatures across colors) and
        // keeps each run's x predictable: char col × advance.
        let spans = highlighter.line_spans(&line);
        let char_len = line.chars().count();
        let mut segs: Vec<(usize, usize, crate::render::ColorRgb)> = Vec::new();
        let mut pos = 0usize;
        for s in &spans {
            if s.start > pos {
                segs.push((pos, s.start, theme.fg));
            }
            if s.end > s.start {
                segs.push((s.start, s.end, token_color(theme, s.token)));
            }
            pos = s.end.max(pos);
        }
        if pos < char_len {
            segs.push((pos, char_len, theme.fg));
        }
        for (a, b, color) in segs {
            let seg = char_range(&line, a, b);
            text.draw_line(
                canvas,
                seg,
                ((text_x + a as f64 * advance) * scale) as f32,
                baseline as f32,
                TextStyle { color, font_size, weight: Weight::NORMAL },
            );
        }
        // NOTE: text that crosses the panel's right edge is left unclipped
        // (no horizontal scroll in M4, same as the tree panel).
    }
}

// Model tests and panel pixel pins live in the module subdirectory (impl +
// all tests together would bloat this file).
#[cfg(test)]
mod tests;
#[cfg(test)]
mod panel_tests;
