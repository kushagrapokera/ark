use std::path::Path;

use crate::render::{ColorRgb, Theme};

mod draw;
pub use draw::draw_with_palette as draw;

/// Two-color procedural icon palette. Both values are existing theme tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilePalette {
    pub main: ColorRgb,
    pub glyph: ColorRgb,
}

/// Procedural explorer glyphs. Classification borrows the path and returns a
/// Copy enum: no per-file strings, maps, bitmap assets, or retained icon cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    Rust,
    Python,
    Html,
    Css,
    JavaScript,
    TypeScript,
    Json,
    Markdown,
    Env,
    Toml,
    Yaml,
    Text,
    Csv,
    Image,
    Archive,
    Generic,
}

impl FileKind {
    pub fn from_path(path: &Path, is_dir: bool) -> Self {
        if is_dir {
            return Self::Folder;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Self::Generic;
        };
        let bytes = name.as_bytes();
        if bytes.eq_ignore_ascii_case(b".env")
            || (bytes.len() > 4 && bytes[..4].eq_ignore_ascii_case(b".env") && bytes[4] == b'.')
        {
            return Self::Env;
        }
        if bytes.eq_ignore_ascii_case(b"readme") {
            return Self::Markdown;
        }
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            return Self::Generic;
        };
        if extension.eq_ignore_ascii_case("rs") {
            Self::Rust
        } else if extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyw") {
            Self::Python
        } else if extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm") {
            Self::Html
        } else if ["css", "scss", "sass", "less"]
            .iter()
            .any(|kind| extension.eq_ignore_ascii_case(kind))
        {
            Self::Css
        } else if ["js", "jsx", "mjs", "cjs"]
            .iter()
            .any(|kind| extension.eq_ignore_ascii_case(kind))
        {
            Self::JavaScript
        } else if extension.eq_ignore_ascii_case("ts") || extension.eq_ignore_ascii_case("tsx") {
            Self::TypeScript
        } else if extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("jsonc")
        {
            Self::Json
        } else if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        {
            Self::Markdown
        } else if extension.eq_ignore_ascii_case("toml") {
            Self::Toml
        } else if extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml") {
            Self::Yaml
        } else if extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("log") {
            Self::Text
        } else if extension.eq_ignore_ascii_case("csv") || extension.eq_ignore_ascii_case("tsv") {
            Self::Csv
        } else if ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"]
            .iter()
            .any(|kind| extension.eq_ignore_ascii_case(kind))
        {
            Self::Image
        } else if ["zip", "tar", "gz", "bz2", "xz", "7z", "rar"]
            .iter()
            .any(|kind| extension.eq_ignore_ascii_case(kind))
        {
            Self::Archive
        } else {
            Self::Generic
        }
    }

    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::Rust => "Rust File",
            Self::Python => "Python File",
            Self::Html => "HTML File",
            Self::Css => "CSS File",
            Self::JavaScript => "JavaScript File",
            Self::TypeScript => "TypeScript File",
            Self::Json => "JSON File",
            Self::Markdown => "Markdown File",
            Self::Env => "Environment File",
            Self::Toml => "TOML File",
            Self::Yaml => "YAML File",
            Self::Text => "Text File",
            Self::Csv => "CSV File",
            Self::Image => "Image File",
            Self::Archive => "Archive File",
            Self::Generic => "File",
        }
    }

    pub const fn color(self, theme: &Theme) -> ColorRgb {
        match self {
            Self::Folder => theme.accent,
            Self::Rust => theme.syntax_escape,
            Self::Python => theme.syntax_constant,
            Self::Html => theme.syntax_regex,
            Self::Css => theme.accent,
            Self::JavaScript => theme.syntax_number,
            Self::TypeScript => theme.syntax_keyword,
            Self::Json => theme.syntax_property,
            Self::Markdown => theme.syntax_control,
            Self::Env => theme.git_added,
            Self::Toml => theme.syntax_string,
            Self::Yaml => theme.syntax_type,
            Self::Text => theme.fg_dim,
            Self::Csv => theme.git_added,
            Self::Image => theme.syntax_decorator,
            Self::Archive => theme.git_modified,
            Self::Generic => theme.text_muted,
        }
    }

    /// Pick a high-contrast theme token for the badge's foreground.
    pub fn palette(self, theme: &Theme) -> FilePalette {
        let main = self.color(theme);
        // The blue accent is the only dark badge in either bundled theme;
        // every other badge color is light enough for the window background.
        let glyph = if self == Self::Css {
            theme.on_accent
        } else {
            theme.bg
        };
        FilePalette { main, glyph }
    }
}

#[cfg(test)]
fn contrast_ratio(left: ColorRgb, right: ColorRgb) -> f64 {
    let left = relative_luminance(left);
    let right = relative_luminance(right);
    let (bright, dim) = if left > right {
        (left, right)
    } else {
        (right, left)
    };
    (bright + 0.05) / (dim + 0.05)
}

#[cfg(test)]
fn relative_luminance(color: ColorRgb) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

#[cfg(test)]
mod tests;
