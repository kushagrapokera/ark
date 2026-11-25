//! Clipboard insertion and search-range selection for config editors.

use super::{Body, ConfigEditor, ENV_KEY_CAP, ENV_VALUE_CAP};

impl ConfigEditor {
    /// Insert clipboard text into the active editable surface. Text configs
    /// keep multi-line content; `.env` fields receive normalized single-line
    /// text. The Rust whitelist remains the write boundary on save.
    pub fn paste(&mut self, text: &str) -> bool {
        let changed = match &mut self.body {
            Body::Env(form) => {
                if form.key.focused {
                    let pasted = crate::clipboard::single_line(text, usize::MAX);
                    form.key.insert_with_cap(&pasted, ENV_KEY_CAP)
                } else if form.value.focused {
                    let pasted = crate::clipboard::single_line(text, usize::MAX);
                    form.value.insert_with_cap(&pasted, ENV_VALUE_CAP)
                } else {
                    false
                }
            }
            Body::Text(editor) => editor.insert(text),
        };
        if changed && matches!(self.body, Body::Text(_)) {
            self.dirty = true;
            self.error = None;
        }
        changed
    }

    /// Select the first case-insensitive literal search hit on a 1-based line.
    /// `.env` forms have no source-range surface and return false.
    pub fn select_search_match(&mut self, line_1_based: usize, query: &str) -> bool {
        match &mut self.body {
            Body::Env(_) => false,
            Body::Text(editor) => editor.select_search_match(line_1_based, query),
        }
    }
}
