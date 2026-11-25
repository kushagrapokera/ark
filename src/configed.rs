//! Config editors (M9): one open `.env` form or YAML/TOML textarea.
//!
//! Only files matching the `ark.toml` whitelist can be opened here, and every
//! save goes through [`crate::fs::write_editable_atomic`] — the Rust write
//! boundary, not this UI, is the enforcement point.
//!
//! Memory: one editor open at a time; text ≤ [`crate::fs::MAX_CONFIG_BYTES`],
//! env lines ≤ [`env::MAX_ENV_LINES`], undo deltas ≤ 512 KiB.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use winit::keyboard::{Key, ModifiersState};

use crate::config;
use crate::fs::{self, EditError, MAX_CONFIG_BYTES};
use crate::highlight::Highlighter;
use crate::ui::{Rect, TextInput};

pub mod env;
pub mod text;
mod input;
mod keys;
mod ui;

pub use ui::ConfigEffects;

use env::EnvDoc;
use text::TextEditor;

/// `.env*` gets the key/value form; every other whitelisted config gets the
/// minimal textarea (YAML/TOML and custom patterns).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigKind {
    Env,
    Text,
}

pub fn kind_for(path: &Path) -> ConfigKind {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) if name.starts_with(".env") => ConfigKind::Env,
        _ => ConfigKind::Text,
    }
}

/// Form field caps (the list itself is bounded by `MAX_ENV_LINES`).
const ENV_KEY_CAP: usize = env::MAX_ENV_KEY_CHARS;
const ENV_VALUE_CAP: usize = env::MAX_ENV_VALUE_CHARS;

#[derive(Debug)]
struct EnvEditor {
    doc: EnvDoc,
    /// Selected entry as a document line index.
    selected: Option<usize>,
    key: TextInput,
    value: TextInput,
    /// First visible entry-list row (index into `doc.entries()`).
    scroll: usize,
    /// Form validation message surfaced in the header.
    form_error: Option<String>,
}

impl EnvEditor {
    fn new(doc: EnvDoc) -> Self {
        EnvEditor {
            doc,
            selected: None,
            key: TextInput::default(),
            value: TextInput::default(),
            scroll: 0,
            form_error: None,
        }
    }

    fn fields_for_selected(&self) -> Option<(&str, &str)> {
        let line = self.selected?;
        match self.doc.line(line) {
            Some(env::EnvLine::Entry { key, value, .. }) => Some((key, value)),
            _ => None,
        }
    }

    fn select(&mut self, line: usize) {
        self.selected = Some(line);
        if let Some(env::EnvLine::Entry { key, value, .. }) = self.doc.line(line) {
            set_field(&mut self.key, key);
            set_field(&mut self.value, value);
            self.key.focused = false;
            self.value.focused = true;
        }
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.key.focused = false;
        self.value.focused = false;
    }

    fn scroll_by(&mut self, rows: i64, viewport: usize) -> bool {
        let entries = self.doc.entries().len();
        let max = entries.saturating_sub(viewport.max(1));
        let before = self.scroll;
        let next = if rows >= 0 {
            self.scroll.saturating_add(rows as usize)
        } else {
            self.scroll.saturating_sub(rows.unsigned_abs() as usize)
        };
        self.scroll = next.min(max);
        self.scroll != before
    }

    /// Apply the form fields to the selected entry. Empty/invalid keys are
    /// rejected with a header error.
    fn apply_form(&mut self) -> Result<bool, String> {
        let Some(line) = self.selected else {
            return Ok(false);
        };
        let key = self.key.text.trim();
        if !env::valid_key(key) {
            return Err("invalid .env key".into());
        }
        if self.doc.apply(line, key, &self.value.text) {
            // Re-read the canonical key/value (trimmed).
            let (k, v) = self
                .fields_for_selected()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .unwrap_or_default();
            set_field(&mut self.key, &k);
            set_field(&mut self.value, &v);
            Ok(true)
        } else {
            Err("invalid .env key or value too long".into())
        }
    }

    fn commit_form(&mut self) -> Result<(), String> {
        self.apply_form().map(|_| ())
    }
}

fn set_field(input: &mut TextInput, text: &str) {
    input.text = text.to_string();
    input.cursor = input.text.len();
}

#[derive(Debug)]
enum Body {
    Env(EnvEditor),
    Text(TextEditor),
}

/// One open config editor. Fixed-size state (one file); body variants carry
/// their own bounded documents.
#[derive(Debug)]
pub struct ConfigEditor {
    root: PathBuf,
    path: PathBuf,
    patterns: Vec<String>,
    kind: ConfigKind,
    body: Body,
    dirty: bool,
    error: Option<String>,
    external_change: bool,
    /// Disk stamp captured at open/save/reload for external-change checks.
    stamp: Option<(u64, SystemTime)>,
    /// Rows visible in the last draw (wheel scrolling clamps to this).
    viewport_rows: usize,
    /// Textarea drag-selection latch.
    dragging: bool,
}

impl ConfigEditor {
    pub fn open(root: &Path, path: &Path, patterns: &[String]) -> Result<Self, EditError> {
        if !config::is_editable(root, path, patterns) {
            return Err(EditError::NotWhitelisted(path.to_path_buf()));
        }
        let kind = kind_for(path);
        let stamp = disk_stamp(path);
        let text = fs::read_bounded(path, MAX_CONFIG_BYTES)?;
        let body = match kind {
            ConfigKind::Env => Body::Env(EnvEditor::new(EnvDoc::parse(&text)?)),
            ConfigKind::Text => Body::Text(TextEditor::new(text, MAX_CONFIG_BYTES)),
        };
        Ok(ConfigEditor {
            root: root.to_path_buf(),
            path: path.to_path_buf(),
            patterns: patterns.to_vec(),
            kind,
            body,
            dirty: false,
            error: None,
            external_change: false,
            stamp,
            viewport_rows: 20,
            dragging: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> ConfigKind {
        self.kind
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn external_change(&self) -> bool {
        self.external_change
    }

    /// Display name for the header/status (root-relative when possible).
    pub fn display_name(&self) -> String {
        self.path
            .strip_prefix(&self.root)
            .unwrap_or(&self.path)
            .to_string_lossy()
            .into_owned()
    }

    pub fn changed_on_disk(&self) -> bool {
        disk_stamp(&self.path) != self.stamp
    }

    /// Reload from disk after an external edit. A dirty editor is NOT
    /// clobbered: it flags `external_change` and keeps the user's text.
    /// Returns whether editor state changed this call (reload OR the first
    /// conflict notice), so the App knows to repaint the header.
    pub fn refresh_if_clean(&mut self) -> bool {
        if !self.changed_on_disk() {
            return false;
        }
        if self.dirty {
            let first_notice = !self.external_change;
            self.external_change = true;
            return first_notice;
        }
        match self.reload() {
            Ok(()) => true,
            // The error is now in the header; signal a repaint either way.
            Err(_) => true,
        }
    }

    pub fn reload(&mut self) -> Result<(), EditError> {
        let text = match fs::read_bounded(&self.path, MAX_CONFIG_BYTES) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e.to_string());
                return Err(e);
            }
        };
        match self.kind {
            ConfigKind::Env => match EnvDoc::parse(&text) {
                Ok(doc) => self.body = Body::Env(EnvEditor::new(doc)),
                Err(e) => {
                    self.error = Some(e.to_string());
                    return Err(e);
                }
            },
            ConfigKind::Text => {
                self.body = Body::Text(TextEditor::new(text, MAX_CONFIG_BYTES));
            }
        }
        self.dirty = false;
        self.error = None;
        self.external_change = false;
        self.dragging = false;
        self.stamp = disk_stamp(&self.path);
        Ok(())
    }

    /// Serialize the current body (committing the env form first) and write
    /// through the enforced atomic boundary.
    pub fn save(&mut self) -> Result<(), EditError> {
        let contents = match &mut self.body {
            Body::Env(ed) => {
                if let Err(e) = ed.commit_form() {
                    self.error = Some(e.clone());
                    return Err(EditError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        e,
                    )));
                }
                ed.doc.render()
            }
            Body::Text(t) => t.text().to_string(),
        };
        match fs::write_editable_atomic(&self.root, &self.path, &contents, &self.patterns) {
            Ok(()) => {
                self.dirty = false;
                self.error = None;
                self.external_change = false;
                if let Body::Text(t) = &mut self.body {
                    t.mark_clean();
                }
                self.stamp = disk_stamp(&self.path);
                log::info!("config saved atomically: {}", self.path.display());
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Wheel scroll in whole rows (text) or entries (env).
    pub fn scroll_by(&mut self, dy: f64) -> bool {
        let rows = (dy / 20.0).round() as i64;
        match &mut self.body {
            Body::Env(ed) => ed.scroll_by(rows, self.viewport_rows),
            Body::Text(t) => t.scroll_by(rows, self.viewport_rows),
        }
    }

    /// Keyboard input for the active surface. Ctrl+S/W are App-level; the
    /// editor handles Ctrl+Z/Y/A/C and all editing keys.
    pub fn handle_key(&mut self, key: &Key, text: Option<&str>, mods: &ModifiersState) -> bool {
        keys::handle(self, key, text, mods)
    }

    pub fn on_ime_commit(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let changed = match &mut self.body {
            Body::Env(ed) => {
                if ed.key.focused {
                    ed.key.insert_with_cap(text, ENV_KEY_CAP)
                } else if ed.value.focused {
                    ed.value.insert_with_cap(text, ENV_VALUE_CAP)
                } else {
                    false
                }
            }
            Body::Text(t) => {
                let changed = t.insert(text);
                t.set_preedit("");
                changed
            }
        };
        if changed {
            self.dirty = true;
            self.error = None;
        }
        changed
    }

    pub fn on_ime_preedit(&mut self, text: &str) -> bool {
        match &mut self.body {
            Body::Text(t) => t.set_preedit(text),
            Body::Env(_) => false, // fields commit directly; no preview surface
        }
    }

    #[allow(clippy::too_many_arguments)] // shared raster context + editor effects
    pub fn draw(
        &mut self,
        canvas: &mut crate::render::Canvas,
        text: &mut crate::text::TextSystem,
        theme: &crate::render::Theme,
        scale: f64,
        ui_scale: f64,
        rect: Rect,
        pointer: crate::ui::Pointer,
        highlighter: &Highlighter,
    ) -> ConfigEffects {
        ui::draw(self, canvas, text, theme, scale, ui_scale, rect, pointer, highlighter)
    }
}

fn disk_stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Mark the editor dirty from the UI (env apply/add/remove).
pub(crate) fn mark_dirty(editor: &mut ConfigEditor) {
    editor.dirty = true;
    editor.error = None;
}

#[cfg(test)]
mod tests;
