//! `.env` document model (M9): a line-preserving parser so the form editor
//! can rename/add/remove entries without disturbing comments, blank lines or
//! the entry order.
//!
//! Bounded: line count and entry key/value sizes are capped; the whole file
//! is already capped by `fs::MAX_CONFIG_BYTES` before it reaches here.

use std::fmt;

use crate::fs::EditError;

/// Cap on parsed `.env` lines (a config editor, not a data store).
pub const MAX_ENV_LINES: usize = 4000;
/// Cap on one key (`TextInput` uses the same for the form field).
pub const MAX_ENV_KEY_CHARS: usize = 128;
/// Cap on one value (secret tokens/URLs can be longer than UI fields).
pub const MAX_ENV_VALUE_CHARS: usize = 4096;

/// One preserved `.env` line: an editable entry or verbatim text (comments,
/// blanks, unsupported syntax). Unsupported lines are never lost on save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvLine {
    Raw(String),
    Entry { export: bool, key: String, value: String },
}

impl fmt::Display for EnvLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvLine::Raw(s) => write!(f, "{s}"),
            EnvLine::Entry { export, key, value } => {
                if *export {
                    write!(f, "export ")?;
                }
                write!(f, "{key}={value}")
            }
        }
    }
}

/// A parsed `.env` file. State is bounded by [`MAX_ENV_LINES`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvDoc {
    lines: Vec<EnvLine>,
    trailing_newline: bool,
}

impl EnvDoc {
    /// Parse `text`. Unsupported lines are preserved verbatim; only the line
    /// cap is an error (the caller already enforced the byte cap).
    pub fn parse(text: &str) -> Result<EnvDoc, EditError> {
        let line_count = text.lines().count();
        if line_count > MAX_ENV_LINES {
            return Err(EditError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("too many .env lines (max {MAX_ENV_LINES})"),
            )));
        }
        let mut lines = Vec::with_capacity(line_count);
        for line in text.lines() {
            lines.push(parse_line(line));
        }
        Ok(EnvDoc { lines, trailing_newline: text.ends_with('\n') })
    }

    /// Serialize back to the exact preserved order; entries are canonical
    /// `KEY=value` (or `export KEY=value`), raw lines verbatim.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&line.to_string());
        }
        if self.trailing_newline && !self.lines.is_empty() {
            out.push('\n');
        }
        out
    }

    /// Editable entries as (line index, key, value) in document order.
    pub fn entries(&self) -> Vec<(usize, &str, &str)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| match line {
                EnvLine::Entry { key, value, .. } => Some((i, key.as_str(), value.as_str())),
                EnvLine::Raw(_) => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&EnvLine> {
        self.lines.get(index)
    }

    /// Rename/update one entry (by document line index). Invalid keys and
    /// over-cap values are rejected (`false`) — the caller can show why.
    pub fn apply(&mut self, line: usize, key: &str, value: &str) -> bool {
        let key = key.trim();
        if !valid_key(key)
            || key.chars().count() > MAX_ENV_KEY_CHARS
            || value.chars().count() > MAX_ENV_VALUE_CHARS
        {
            return false;
        }
        match self.lines.get_mut(line) {
            Some(EnvLine::Entry { key: k, value: v, .. }) => {
                *k = key.to_string();
                *v = value.to_string();
                true
            }
            _ => false,
        }
    }

    /// Append a new entry and return its document line index. `NEW_KEY` is
    /// selected in the form so typing replaces it.
    pub fn append(&mut self) -> usize {
        self.lines.push(EnvLine::Entry {
            export: false,
            key: "NEW_KEY".to_string(),
            value: String::new(),
        });
        self.lines.len() - 1
    }

    /// Remove an entry line. Raw/comment lines are never removed here.
    pub fn remove(&mut self, line: usize) -> bool {
        if matches!(self.lines.get(line), Some(EnvLine::Entry { .. })) {
            self.lines.remove(line);
            true
        } else {
            false
        }
    }
}

fn parse_line(line: &str) -> EnvLine {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return EnvLine::Raw(line.to_string());
    }
    let (export, rest) = match trimmed.strip_prefix("export ") {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    let Some((key, value)) = rest.split_once('=') else {
        return EnvLine::Raw(line.to_string()); // unsupported line: keep it
    };
    let key = key.trim();
    if !valid_key(key) {
        return EnvLine::Raw(line.to_string());
    }
    EnvLine::Entry { export, key: key.to_string(), value: value.to_string() }
}

/// `.env` keys: non-empty, no whitespace, `=`, `#` (values take everything
/// after the first `=`; dots/dashes are tolerated for real-world files).
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && !key.chars().any(|c| c.is_whitespace() || c == '=' || c == '#')
}

/// Common secret-looking key names. This is display masking, not security:
/// the value is still stored/edited in plain text as `.env` requires.
pub fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    ["SECRET", "TOKEN", "PASSWORD", "PASSWD", "PRIVATE", "CREDENTIAL", "API_KEY", "ACCESS_KEY"]
        .iter()
        .any(|needle| upper.contains(needle))
}

/// Masked display for the entry list; empty values stay empty. Capped so a
/// long token does not stretch the row.
pub fn mask_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let n = value.chars().count().min(12);
    "•".repeat(n.max(4))
}

#[cfg(test)]
mod tests;
