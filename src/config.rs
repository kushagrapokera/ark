//! Configuration (M9): `ark.toml` at the project root. The only setting
//! needed by the config editor is the human-writable whitelist:
//!
//! ```toml
//! editable = [".env*", "*.yaml", "*.yml", "*.toml"]
//! ```
//!
//! No TOML crate (frozen stack): this is a tiny, bounded reader for that one
//! array. Missing file/array → [`DEFAULT_EDITABLE`]; an explicit empty array
//! means "no files are human-writable". Malformed input is an error — the
//! caller falls back to defaults AND logs it, never silently widening.
//!
//! Memory: the config file is capped at [`MAX_CONFIG_FILE_BYTES`] and the
//! parsed list at [`MAX_EDITABLE_PATTERNS`].

use std::fmt;
use std::path::Path;

/// Default human-writable whitelist (plan.md feature 3).
pub const DEFAULT_EDITABLE: &[&str] = &[".env*", "*.yaml", "*.yml", "*.toml"];
/// `ark.toml` read cap.
pub const MAX_CONFIG_FILE_BYTES: u64 = 64 * 1024;
/// Parsed whitelist cap.
pub const MAX_EDITABLE_PATTERNS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Glob patterns (matched against file names, or relative paths when
    /// the pattern contains `/`).
    pub editable: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { editable: DEFAULT_EDITABLE.iter().map(|s| (*s).to_string()).collect() }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::Parse(e) => write!(f, "invalid ark.toml: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Load `ark.toml` from `root`. A missing file is not an error (defaults);
/// unreadable/oversized/broken files are (the App logs and keeps defaults).
pub fn load(root: &Path) -> Result<Config, ConfigError> {
    let path = root.join("ark.toml");
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(e) => return Err(ConfigError::Io(e)),
    };
    if meta.len() > MAX_CONFIG_FILE_BYTES {
        return Err(ConfigError::Parse(format!(
            "{} is larger than {} bytes",
            path.display(),
            MAX_CONFIG_FILE_BYTES
        )));
    }
    let text = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
    Ok(Config { editable: parse_editable(&text)? })
}

/// Parse the `editable` array. Absent array → defaults (see [`load`]).
pub fn parse_editable(text: &str) -> Result<Vec<String>, ConfigError> {
    Ok(parse_editable_opt(text)?.unwrap_or_else(|| Config::default().editable))
}

/// `Some` only when the key exists (an explicit `[]` yields `Some(vec![])`).
fn parse_editable_opt(text: &str) -> Result<Option<Vec<String>>, ConfigError> {
    let cleaned = strip_comments(text);
    let mut found = false;
    let mut patterns: Vec<String> = Vec::new();
    let mut in_array = false;
    let mut array_text = String::new();
    for line in cleaned.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !in_array {
            let Some(rest) = line.strip_prefix("editable") else {
                continue; // other keys/table headers are ignored in M9
            };
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                return Err(ConfigError::Parse("editable must be an array = [...]".into()));
            };
            found = true;
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix('[') else {
                return Err(ConfigError::Parse("editable must start with '['".into()));
            };
            array_text.push_str(rest);
            if array_text.contains(']') {
                collect_patterns(&array_text, &mut patterns)?;
                in_array = false;
            } else {
                in_array = true;
            }
        } else {
            array_text.push(' ');
            array_text.push_str(line);
            if array_text.contains(']') {
                collect_patterns(&array_text, &mut patterns)?;
                in_array = false;
            }
        }
        if patterns.len() > MAX_EDITABLE_PATTERNS {
            return Err(ConfigError::Parse(format!(
                "editable has more than {MAX_EDITABLE_PATTERNS} patterns"
            )));
        }
    }
    if !found {
        return Ok(None);
    }
    if in_array {
        return Err(ConfigError::Parse("unterminated editable array".into()));
    }
    Ok(Some(patterns))
}

/// Extract quoted strings from an array body (after `[`, through `]`).
fn collect_patterns(array_text: &str, out: &mut Vec<String>) -> Result<(), ConfigError> {
    let body = array_text
        .split_once(']')
        .map(|(b, _)| b)
        .ok_or_else(|| ConfigError::Parse("unterminated editable array".into()))?;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push(parse_basic_string(&mut chars)?),
            '\'' => out.push(parse_literal_string(&mut chars)?),
            ',' | ' ' | '\t' | '\n' | '\r' => {}
            other => {
                return Err(ConfigError::Parse(format!(
                    "expected a quoted string in editable, found {other:?}"
                )))
            }
        }
    }
    Ok(())
}

fn parse_basic_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ConfigError> {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    // TOML would reject unknown escapes; globs never need
                    // them, so surface the problem instead of guessing.
                    return Err(ConfigError::Parse(format!("unsupported escape \\{other}")));
                }
                None => return Err(ConfigError::Parse("unterminated string".into())),
            },
            other => out.push(other),
        }
    }
    Err(ConfigError::Parse("unterminated string".into()))
}

fn parse_literal_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ConfigError> {
    let mut out = String::new();
    for c in chars.by_ref() {
        if c == '\'' {
            return Ok(out);
        }
        out.push(c);
    }
    Err(ConfigError::Parse("unterminated string".into()))
}

/// Strip `#` comments outside strings, preserving line structure.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_basic = false;
        let mut in_literal = false;
        let mut escaped = false;
        for c in line.chars() {
            if escaped {
                out.push(c);
                escaped = false;
                continue;
            }
            match c {
                '\\' if in_basic => {
                    out.push(c);
                    escaped = true;
                }
                '"' if !in_literal => {
                    in_basic = !in_basic;
                    out.push(c);
                }
                '\'' if !in_basic => {
                    in_literal = !in_literal;
                    out.push(c);
                }
                '#' if !in_basic && !in_literal => break,
                other => out.push(other),
            }
        }
        out.push('\n');
    }
    out
}

/// Whether a file name is one of the built-in config surfaces. The
/// `ark.toml` whitelist can only NARROW this set (AGENTS.md guardrail 2: code
/// files are never human-writable through the config editor).
pub fn is_config_surface(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.starts_with(".env") {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("yaml" | "yml" | "toml")
    )
}

/// Whether `path` is human-writable under `patterns`. It must be BOTH a
/// built-in config surface and match the whitelist. `root` is the project
/// root (absolute in the App); paths outside `root` are never editable here;
/// the write boundary re-checks after canonicalization.
pub fn is_editable(root: &Path, path: &Path, patterns: &[String]) -> bool {
    let rel = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) if path.is_absolute() => return false,
        Err(_) => path,
    };
    if !is_config_surface(rel) {
        return false; // code/other files stay read-only even with `*`
    }
    let rel_path = rel.to_string_lossy().replace('\\', "/");
    let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    patterns.iter().any(|p| matches(p, name, &rel_path))
}

/// One glob: `*` any run of characters, `?` one character. Patterns with
/// `/` match the repository-relative path; everything else matches the file
/// name (so `*.yaml` works at any depth).
pub fn matches(pattern: &str, file_name: &str, rel_path: &str) -> bool {
    let text = if pattern.contains('/') { rel_path } else { file_name };
    glob_match(pattern, text)
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_t = 0usize;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            star_t = ti;
        } else if let Some(s) = star {
            pi = s + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests;
