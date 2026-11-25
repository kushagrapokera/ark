//! Streamed, viewport-only README preview. Reuses the existing mmap-backed
//! `Viewer`; no full-document AST, duplicate file buffer, or persistent layout
//! index is retained.

use std::path::Path;

mod parse;
mod render;

#[cfg(test)]
mod tests;

pub use render::{draw, ReadmeDraw};

#[derive(Debug, Default)]
pub struct ReadmeState {
    pub preview: bool,
    pub scroll: f64,
}

impl ReadmeState {
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

pub fn is_readme(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    match lower.rsplit_once('.') {
        Some((stem, ext)) => {
            stem == "readme" && matches!(ext, "md" | "markdown" | "txt" | "mdown" | "mkd")
        }
        None => lower == "readme",
    }
}
