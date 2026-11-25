//! Streamed, viewport-only Markdown preview. Reuses the existing mmap-backed
//! `Viewer`; no full-document AST, duplicate file buffer, or persistent layout
//! index is retained.

use std::path::Path;

mod blocks;
mod code;
mod paint;
mod parse;
mod render;
mod table;

#[cfg(test)]
mod code_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod tests;

pub use render::{draw_markdown_preview, MarkdownDraw};

#[derive(Debug, Default)]
pub struct MarkdownState {
    pub preview: bool,
    pub scroll: f64,
}

impl MarkdownState {
    pub fn scroll_by(&mut self, dy: f64) -> bool {
        if !dy.is_finite() {
            return false;
        }
        let next = (self.scroll + dy).max(0.0);
        if (next - self.scroll).abs() <= f64::EPSILON {
            return false;
        }
        self.scroll = next;
        true
    }
}

pub fn is_markdown(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    match lower.rsplit_once('.') {
        Some((stem, ext)) => {
            matches!(ext, "md" | "markdown" | "mdown" | "mkd") || (stem == "readme" && ext == "txt")
        }
        None => lower == "readme",
    }
}
