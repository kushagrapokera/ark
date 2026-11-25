//! Bounded per-viewer search navigation highlight.

use super::Viewer;

/// One search hit in DISPLAY-space char columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub query: String,
}

const MAX_SEARCH_QUERY_CHARS: usize = 128;

impl Viewer {
    pub fn search_match(&self) -> Option<&SearchMatch> {
        self.search_match.as_ref()
    }

    /// Highlight the first case-insensitive literal match on a 1-based line.
    /// Returns whether the retained highlight changed.
    pub fn set_search_match(&mut self, line_1_based: usize, query: &str) -> bool {
        let next = if query.is_empty() || query.chars().count() > MAX_SEARCH_QUERY_CHARS {
            None
        } else {
            let line = line_1_based.saturating_sub(1);
            self.line_text(line).and_then(|text| {
                let pattern = regex::RegexBuilder::new(&regex::escape(query))
                    .case_insensitive(true)
                    .build()
                    .ok()?;
                let found = pattern.find(&text)?;
                Some(SearchMatch {
                    line,
                    start_col: text[..found.start()].chars().count(),
                    end_col: text[..found.end()].chars().count(),
                    query: query.to_owned(),
                })
            })
        };
        if next == self.search_match {
            return false;
        }
        self.search_match = next;
        true
    }

    pub fn clear_search_match(&mut self) -> bool {
        self.search_match.take().is_some()
    }
}
