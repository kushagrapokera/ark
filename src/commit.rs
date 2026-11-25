//! Commit & push panel (M8): changed-file checkboxes, one bounded commit
//! message and click-only stage/commit/push.
//!
//! Memory rules (AGENTS.md guardrail 3):
//! - the list is the bounded `GitStatus` snapshot (100k cap), rendered
//!   viewport-only (`scroll` is a row index, never a per-row buffer);
//! - only user-UNCHECKED paths are stored and they are pruned to the current
//!   snapshot on every status apply;
//! - the message is one `TextInput` (128-char cap).
//!
//! Git runs out-of-process: [`CommitPanel::commit_command`] builds the
//! quoted `git add … && git commit …` line that the app streams through a
//! PTY terminal tab (M6); `git::PUSH_COMMAND` does the same for push.

//! The panel is hosted by the workbench's Source Control card in wide windows
//! and by the editor body in compact windows.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use cosmic_text::Weight;
use winit::keyboard::{Key, NamedKey};

use crate::git::{self, GitStatus};
use crate::render::{Canvas, Theme};
use crate::text::{TextStyle, TextSystem, UI_FONT_SIZE};
use crate::ui::{draw_button, draw_checkbox, Pointer, Press, Rect, TextInput};

/// Preferred panel height used by the original layout pins. The Source
/// Control card now uses all available card/body height.
#[allow(dead_code)]
pub const PANEL_H: f64 = 236.0;
/// Logical px per changed-file row.
pub const ROW_H: f64 = 20.0;

const PAD: f64 = 8.0;
const TITLE_H: f64 = 22.0;
const FIELD_H: f64 = 24.0;
const BUTTON_H: f64 = 24.0;
const BUTTON_GAP: f64 = 8.0;
const CHECKBOX: f64 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitError {
    NoSelection,
    EmptyMessage,
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommitError::NoSelection => write!(f, "select a file to commit"),
            CommitError::EmptyMessage => write!(f, "commit message required"),
        }
    }
}

impl std::error::Error for CommitError {}

/// Pure geometry of the panel (all LOGICAL px). The list gets every pixel
/// between the title and the message field; its row count is derived.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelLayout {
    pub title: Rect,
    pub list: Rect,
    pub rows: usize,
    pub field: Rect,
    pub commit_button: Rect,
    pub push_button: Rect,
}

pub fn layout(panel: Rect) -> PanelLayout {
    let compact = panel.h < 150.0;
    let pad = if compact { 4.0 } else { PAD };
    let title_h = if compact { 14.0 } else { TITLE_H };
    let field_h = if compact { 20.0 } else { FIELD_H };
    let button_h = if compact { 20.0 } else { BUTTON_H };
    let gap = if compact { 4.0 } else { BUTTON_GAP };
    let inner_w = (panel.w - 2.0 * pad).max(0.0);
    let title = Rect {
        x: panel.x + pad,
        y: panel.y + pad,
        w: inner_w,
        h: title_h,
    };
    let buttons_y = panel.y + panel.h - pad - button_h;
    let field_y = buttons_y - gap - field_h;
    let list_y = title.y + title.h;
    let list_h = (field_y - gap - list_y).max(0.0);
    let button_w = ((inner_w - gap) / 2.0).max(0.0);
    PanelLayout {
        title,
        list: Rect {
            x: panel.x,
            y: list_y,
            w: panel.w,
            h: list_h,
        },
        rows: (list_h / ROW_H).floor() as usize,
        field: Rect {
            x: panel.x + pad,
            y: field_y,
            w: inner_w,
            h: field_h,
        },
        commit_button: Rect {
            x: panel.x + pad,
            y: buttons_y,
            w: button_w,
            h: button_h,
        },
        push_button: Rect {
            x: panel.x + pad + button_w + gap,
            y: buttons_y,
            w: button_w,
            h: button_h,
        },
    }
}

/// What one frame's pointer did to the panel (the App turns the booleans
/// into git terminal tabs after the raster pass).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelEffects {
    /// A file row toggled its checkbox (checked state changed).
    pub toggled: bool,
    /// The Commit button was clicked.
    pub commit: bool,
    /// The Push button was clicked.
    pub push: bool,
}

/// Commit & push panel state. Checked is the DEFAULT (matching "stage every
/// change" muscle memory); only explicit unchecks are stored.
#[derive(Debug, Default)]
pub struct CommitPanel {
    /// Paths the user unchecked. Pruned to the current status snapshot.
    unchecked: HashSet<PathBuf>,
    /// First visible changed-file row (viewport-only list).
    pub scroll: usize,
    /// The single-line commit message (bounded by TextInput).
    pub message: TextInput,
    /// Most recent validation error, rendered in the title row.
    pub error: Option<String>,
}

impl CommitPanel {
    /// Whether a path is staged by the panel (default: yes).
    pub fn is_checked(&self, path: &Path) -> bool {
        !self.unchecked.contains(path)
    }

    /// Toggle one path's checked state.
    pub fn toggle(&mut self, path: &Path) {
        if !self.unchecked.remove(path) {
            self.unchecked.insert(path.to_path_buf());
        }
        self.error = None;
    }

    /// The repo-relative paths the Commit action will stage, in status order.
    /// Bounded by the status snapshot (no copy of the snapshot is retained).
    pub fn selected_paths(&self, status: &GitStatus) -> Vec<PathBuf> {
        status
            .entries
            .iter()
            .filter(|e| self.is_checked(&e.path))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Drop unchecked paths that vanished from the snapshot and re-clamp
    /// the scroll. Called on every applied status.
    pub fn retain(&mut self, status: &GitStatus) {
        if !self.unchecked.is_empty() {
            let keep: HashSet<PathBuf> = status
                .entries
                .iter()
                .filter(|e| self.unchecked.contains(&e.path))
                .map(|e| e.path.clone())
                .collect();
            self.unchecked = keep;
        }
        self.scroll = self.scroll.min(status.entries.len().saturating_sub(1));
    }

    /// Move the visible row window by `rows` (positive = toward the end),
    /// clamped so the last page stays full. Returns whether it moved.
    pub fn scroll_by(&mut self, rows: i64, visible: usize, total: usize) -> bool {
        let max = total.saturating_sub(visible);
        let next = if rows >= 0 {
            self.scroll.saturating_add(rows as usize)
        } else {
            self.scroll.saturating_sub(rows.unsigned_abs() as usize)
        };
        let next = next.min(max);
        if next == self.scroll {
            return false;
        }
        self.scroll = next;
        true
    }

    /// Validate and build the Commit action's shell command.
    pub fn commit_command(&self, status: &GitStatus) -> Result<String, CommitError> {
        let paths = self.selected_paths(status);
        if paths.is_empty() {
            return Err(CommitError::NoSelection);
        }
        let message = self.message.text.trim();
        if message.is_empty() {
            return Err(CommitError::EmptyMessage);
        }
        Ok(git::stage_and_commit_command(&paths, message))
    }

    /// Feed one keyboard event to the focused message field. Escape blurs;
    /// printable text inserts at the caret; editing clears the error.
    pub fn handle_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if !self.message.focused {
            return false;
        }
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.message.focused = false;
            return true;
        }
        let mut changed = false;
        for edit in TextInput::keys_from_logical_and_text(key, text) {
            changed |= self.message.handle_key(edit);
        }
        if changed {
            self.error = None;
        }
        changed
    }

    /// Draw the panel, process the frame's click and return its effects.
    #[allow(clippy::too_many_arguments)] // raster borrows + git snapshot; explicit beats bundling
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        text: &mut TextSystem,
        theme: &Theme,
        scale: f64,
        panel: Rect,
        status: &GitStatus,
        pointer: Pointer,
    ) -> PanelEffects {
        let mut effects = PanelEffects::default();
        if panel.w <= 0.0 || panel.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
            return effects;
        }
        let l = layout(panel);
        let pad = if panel.h < 150.0 { 4.0 } else { PAD };
        // The card supplies the rounded outer surface; the panel fills only
        // its body and uses the same elevated/subtle tokens as the workbench.
        canvas.fill_rect(panel.x, panel.y, panel.w, panel.h, theme.surface, scale);
        canvas.fill_rect(panel.x, panel.y, panel.w, 1.0, theme.border_subtle, scale);
        let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
        let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
        let advance = f64::from(m.advance) / scale;

        // Focus: only a click inside the message field focuses it; any other
        // click blurs (the panel is always drawn when a repo is open).
        if pointer.click {
            self.message.focused = l.field.contains(pointer.x, pointer.y);
        }

        // Title row: validation error, or change/selection counts. A status
        // shrink re-clamps the window to a full page before drawing.
        let total = status.entries.len();
        let max_scroll = total.saturating_sub(l.rows);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        let selected = status
            .entries
            .iter()
            .filter(|e| self.is_checked(&e.path))
            .count();
        let (title, color) = match &self.error {
            Some(err) => (err.clone(), theme.git_deleted),
            None if total == 0 => ("No changes".to_string(), theme.fg_dim),
            None if selected == total => (format!("Changes {total}"), theme.fg_dim),
            None => (format!("Changes {total} · {selected} sel"), theme.fg_dim),
        };
        let baseline =
            (l.title.y + l.title.h / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
        text.draw_line(
            canvas,
            &title,
            (l.title.x * scale) as f32,
            baseline as f32,
            TextStyle {
                color,
                font_size,
                weight: Weight::BOLD,
            },
        );

        // Changed-file rows: viewport only, checkbox + badge + path. Slice
        // the window directly: `skip` on a huge snapshot walks every row.
        let visible_rows = status.entries.get(self.scroll..).unwrap_or(&[]);
        for (slot, entry) in visible_rows.iter().take(l.rows).enumerate() {
            let row_y = l.list.y + slot as f64 * ROW_H;
            let row = Rect {
                x: panel.x,
                y: row_y,
                w: panel.w,
                h: ROW_H,
            };
            let hovered = row.contains(pointer.x, pointer.y);
            if hovered {
                canvas.fill_rect(panel.x, row_y, panel.w, ROW_H, theme.surface_hover, scale);
            }
            if pointer.click && hovered {
                self.toggle(&entry.path);
                effects.toggled = true;
            }
            let checked = self.is_checked(&entry.path);
            draw_checkbox(
                canvas,
                Rect {
                    x: panel.x + pad,
                    y: row_y + (ROW_H - CHECKBOX) / 2.0,
                    w: CHECKBOX,
                    h: CHECKBOX,
                },
                checked,
                theme,
                Pointer::default(),
                scale,
            );
            let (letter, badge_color) = crate::tree::badge_glyph(entry.badge(), theme);
            let badge_x = panel.x + pad + CHECKBOX + 8.0;
            let row_baseline =
                (row_y + ROW_H / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
            text.draw_line(
                canvas,
                letter,
                (badge_x * scale) as f32,
                row_baseline as f32,
                TextStyle {
                    color: badge_color,
                    font_size,
                    weight: Weight::BOLD,
                },
            );
            let path_x = badge_x + advance + 8.0;
            let max_chars =
                (((panel.x + panel.w - PAD - path_x) / advance).floor()).max(0.0) as usize;
            let display = elide_path(&entry.path.to_string_lossy(), max_chars);
            text.draw_line(
                canvas,
                &display,
                (path_x * scale) as f32,
                row_baseline as f32,
                TextStyle {
                    color: theme.fg,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
        }

        // Message field + placeholder, then the two buttons.
        self.message.draw(canvas, l.field, theme, text, scale);
        if self.message.text.is_empty() && !self.message.focused {
            let field_baseline =
                (l.field.y + l.field.h / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
            text.draw_line(
                canvas,
                "commit message",
                ((l.field.x + pad) * scale) as f32,
                field_baseline as f32,
                TextStyle {
                    color: theme.fg_dim,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
        }
        if draw_primary_button(
            canvas,
            text,
            theme,
            scale,
            l.commit_button,
            "Commit",
            pointer,
        ) {
            effects.commit = true;
        }
        if draw_button(canvas, "Push", l.push_button, theme, text, pointer, scale) == Press::Clicked
        {
            effects.push = true;
        }
        effects
    }
}

fn draw_primary_button(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    rect: Rect,
    label: &str,
    pointer: Pointer,
) -> bool {
    if rect.w <= 0.0 || rect.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return false;
    }
    let clicked = pointer.click && rect.contains(pointer.x, pointer.y);
    canvas.fill_round_rect(rect.x, rect.y, rect.w, rect.h, 3.0, theme.accent, scale);
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35).round() as f32);
    let label_w = m.advance * label.chars().count() as f32;
    let x = rect.x * scale + (rect.w * scale - f64::from(label_w)) / 2.0;
    let baseline = (rect.y + rect.h / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
    text.draw_line(
        canvas,
        label,
        x as f32,
        baseline as f32,
        TextStyle {
            color: theme.on_accent,
            font_size,
            weight: Weight::BOLD,
        },
    );
    clicked
}

/// Truncate a repo-relative path from the LEFT so the file name stays
/// visible; `"…"` is avoided for the bundled monospace-only aesthetic.
fn elide_path(path: &str, max_chars: usize) -> String {
    let count = path.chars().count();
    if count <= max_chars {
        return path.to_string();
    }
    if max_chars <= 3 {
        return path.chars().take(max_chars).collect();
    }
    let tail: String = path.chars().skip(count - (max_chars - 3)).collect();
    format!("...{tail}")
}

#[cfg(test)]
mod tests;
