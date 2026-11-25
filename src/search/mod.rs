//! Bounded project keyword search. No persistent index: each query walks the
//! filesystem in a worker, reads capped text files, and returns a bounded report.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::ui::TextInput;

mod ui;

#[cfg(test)]
mod tests;

pub use ui::{draw_search_panel, SearchEffects};
#[cfg(test)]
pub use ui::{panel_layout, SearchLayout};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub path: PathBuf,
    pub matches: usize,
    pub line: usize,
    pub snippet: String,
}

#[derive(Clone, Debug)]
pub struct SearchReport {
    pub generation: u64,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub scanned_files: usize,
    pub truncated: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct SearchState {
    pub input: TextInput,
    pub results: Vec<SearchResult>,
    pub status: String,
    pub scroll: f64,
    pub running: bool,
}

impl SearchState {
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

    pub fn begin(&mut self, query: &str) {
        self.input.text = query.to_owned();
        self.input.cursor = self.input.text.len();
        self.results.clear();
        self.scroll = 0.0;
        self.running = true;
        self.status = "Searching…".into();
    }

    pub fn clear(&mut self) {
        self.results.clear();
        self.scroll = 0.0;
        self.running = false;
        self.status.clear();
    }

    pub fn apply(&mut self, report: SearchReport) {
        self.running = false;
        self.scroll = 0.0;
        let result_count = report.results.len();
        self.results = report.results;
        self.status = if let Some(error) = report.error {
            error
        } else if report.cancelled {
            "Search cancelled".into()
        } else if result_count == 0 {
            format!("No matches in {} files", report.scanned_files)
        } else {
            let suffix = if report.truncated { " (capped)" } else { "" };
            format!(
                "{result_count} file{} · {} scanned{suffix}",
                if result_count == 1 { "" } else { "s" },
                report.scanned_files
            )
        };
    }
}

const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
const MAX_SEARCH_FILES: usize = 50_000;
const MAX_SEARCH_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_PENDING_DIRS: usize = 4096;
const MAX_SEARCH_SNIPPET_CHARS: usize = 160;

#[derive(Clone, Copy, Debug)]
struct SearchLimits {
    max_file_bytes: u64,
    max_files: usize,
    max_total_bytes: u64,
    max_results: usize,
    max_pending_dirs: usize,
    max_snippet_chars: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: MAX_SEARCH_FILE_BYTES,
            max_files: MAX_SEARCH_FILES,
            max_total_bytes: MAX_SEARCH_TOTAL_BYTES,
            max_results: MAX_SEARCH_RESULTS,
            max_pending_dirs: MAX_SEARCH_PENDING_DIRS,
            max_snippet_chars: MAX_SEARCH_SNIPPET_CHARS,
        }
    }
}

pub fn run_search(
    root: &std::path::Path,
    query: &str,
    generation: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> SearchReport {
    run_search_with_limits(root, query, generation, cancel, SearchLimits::default())
}

fn run_search_with_limits(
    root: &std::path::Path,
    query: &str,
    generation: u64,
    cancel: &std::sync::atomic::AtomicBool,
    limits: SearchLimits,
) -> SearchReport {
    let mut report = SearchReport {
        generation,
        query: query.to_owned(),
        results: Vec::new(),
        scanned_files: 0,
        truncated: false,
        cancelled: false,
        error: None,
    };
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return report;
    }

    let mut dirs = vec![root.to_path_buf()];
    let mut scanned_bytes = 0u64;
    'walk: while let Some(dir) = dirs.pop() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                if dir == root {
                    report.error = Some(format!("cannot search {}: {e}", root.display()));
                } else {
                    log::debug!("search skipped {}: {e}", dir.display());
                }
                continue;
            }
        };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break 'walk;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    log::debug!("search skipped an unreadable entry in {}: {e}", dir.display());
                    continue;
                }
            };
            let file_type = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) => {
                    log::debug!("search skipped {}: {e}", entry.path().display());
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                if dirs.len() >= limits.max_pending_dirs {
                    report.truncated = true;
                    continue;
                }
                dirs.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if report.scanned_files >= limits.max_files {
                report.truncated = true;
                break;
            }
            let path = entry.path();
            let len = match entry.metadata() {
                Ok(meta) => meta.len(),
                Err(e) => {
                    log::debug!("search skipped {}: {e}", path.display());
                    continue;
                }
            };
            if len > limits.max_file_bytes {
                report.truncated = true;
                log::debug!("search skipped large file {} ({len} bytes)", path.display());
                continue;
            }
            if scanned_bytes.saturating_add(len) > limits.max_total_bytes {
                report.truncated = true;
                break;
            }
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    log::debug!("search skipped {}: {e}", path.display());
                    continue;
                }
            };
            if bytes.len() as u64 > limits.max_file_bytes {
                report.truncated = true;
                continue;
            }
            scanned_bytes = scanned_bytes.saturating_add(bytes.len() as u64);
            if bytes.contains(&0) {
                continue;
            }
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => continue,
            };
            report.scanned_files += 1;
            let mut matches = 0usize;
            let mut first_line = 0usize;
            let mut first_snippet = String::new();
            for (index, line) in text.lines().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    report.cancelled = true;
                    break 'walk;
                }
                let count = line.to_lowercase().match_indices(&needle).count();
                if count == 0 {
                    continue;
                }
                matches = matches.saturating_add(count);
                if first_line == 0 {
                    first_line = index + 1;
                    first_snippet = bounded_snippet(line, limits.max_snippet_chars);
                }
            }
            if matches > 0 {
                report.results.push(SearchResult {
                    path,
                    matches,
                    line: first_line,
                    snippet: first_snippet,
                });
                if report.results.len() >= limits.max_results {
                    report.truncated = true;
                    break;
                }
            }
        }
    }
    report.results.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

fn bounded_snippet(line: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(max_chars.min(line.len()));
    for ch in line.trim().chars().take(max_chars) {
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}
