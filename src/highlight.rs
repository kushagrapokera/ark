//! Tier-1.5 token highlighter: static language/file-type profiles plus
//! bounded multiline state, applied per VISIBLE line only.
//!
//! No AST, grammar bundle or whole-file token cache. Profiles compile lazily,
//! identical regexes are shared, keyword/type tables are scanned directly, and
//! the viewer reconstructs multiline state from at most 256 preceding lines.

use std::sync::OnceLock;

mod detect;
mod profiles;
mod state;
mod theme;
mod tokenize;

pub use detect::lang_for_source;
pub use state::HighlightState;
pub use theme::token_color;

use tokenize::{collect_static_spans, compile, finalize_spans, CompiledProfile};

/// Token kinds mapped to theme colors at render time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Keyword,
    ControlKeyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Constant,
    Property,
    Variable,
    Regex,
    Escape,
    Decorator,
    Selector,
}

/// One highlighted span of a line, in CHAR indices (render maps col → x).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: Token,
}

/// Language/file families recognized by the static highlighter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    CLike,
    Go,
    PythonLike,
    Shell,
    PowerShell,
    Sql,
    Markup,
    Css,
    Json,
    Yaml,
    Toml,
    Ini,
    Dotenv,
    Dockerfile,
    Makefile,
    Markdown,
    Nix,
    Hcl,
    Protobuf,
    DashLike,
    Cmake,
    Graphql,
    Latex,
    Plain,
}

impl Lang {
    pub const ALL: [Lang; 25] = [
        Lang::Rust,
        Lang::CLike,
        Lang::Go,
        Lang::PythonLike,
        Lang::Shell,
        Lang::PowerShell,
        Lang::Sql,
        Lang::Markup,
        Lang::Css,
        Lang::Json,
        Lang::Yaml,
        Lang::Toml,
        Lang::Ini,
        Lang::Dotenv,
        Lang::Dockerfile,
        Lang::Makefile,
        Lang::Markdown,
        Lang::Nix,
        Lang::Hcl,
        Lang::Protobuf,
        Lang::DashLike,
        Lang::Cmake,
        Lang::Graphql,
        Lang::Latex,
        Lang::Plain,
    ];

    fn index(self) -> usize {
        self as usize
    }
}

/// Lazily compiled profiles. One `OnceLock` per family; unused languages cost
/// only an empty cell.
pub struct Highlighter {
    profiles: Vec<OnceLock<CompiledProfile>>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Highlighter {
            profiles: Lang::ALL.iter().map(|_| OnceLock::new()).collect(),
        }
    }

    fn profile(&self, lang: Lang) -> &CompiledProfile {
        self.profiles[lang.index()].get_or_init(|| compile(lang))
    }

    /// Reconstruct multiline state from at most [`state::LOOKBACK_LINES`]
    /// lines before `start`. The callback returns borrowed display lines.
    pub fn state_before<S: AsRef<str>>(
        &self,
        lang: Lang,
        start: usize,
        mut line_at: impl FnMut(usize) -> Option<S>,
    ) -> HighlightState {
        let mut state = HighlightState::default();
        let first = start.saturating_sub(state::LOOKBACK_LINES);
        for index in first..start {
            if let Some(line) = line_at(index) {
                state = state::scan_line(line.as_ref(), lang, state).outgoing;
            }
        }
        state
    }

    /// Highlight one line for a specific language family without multiline
    /// carry. Kept for tests and callers that only have one line.
    pub fn line_spans_for(&self, line: &str, lang: Lang) -> Vec<Span> {
        self.line_spans_with_state(line, lang, &mut HighlightState::default())
    }

    /// Highlight one line and advance the caller-owned bounded state.
    pub fn line_spans_with_state(
        &self,
        line: &str,
        lang: Lang,
        state: &mut HighlightState,
    ) -> Vec<Span> {
        let incoming = *state;
        let scan = state::scan_line(line, lang, incoming);
        let effective = state::effective_lang(line, lang, incoming);
        let mut raw: Vec<(usize, usize, Token)> = Vec::new();
        let mut cursor = 0;

        if incoming.has_block_comment() {
            if let Some(end) = scan.close_block_comment {
                raw.push((0, end, Token::Comment));
                cursor = end;
            } else {
                raw.push((0, line.len(), Token::Comment));
                *state = scan.outgoing;
                return finalize_spans(line, raw);
            }
        } else if incoming.has_triple_quote() {
            if let Some(end) = scan.close_triple_quote {
                raw.push((0, end, Token::String));
                cursor = end;
            } else {
                raw.push((0, line.len(), Token::String));
                *state = scan.outgoing;
                return finalize_spans(line, raw);
            }
        }

        if cursor < line.len() {
            collect_static_spans(self.profile(effective), &line[cursor..], effective, cursor, &mut raw);
        }
        if let Some(start) = scan.open_block_comment {
            raw.push((start, line.len(), Token::Comment));
        }
        if let Some(start) = scan.open_triple_quote {
            raw.push((start, line.len(), Token::String));
        }

        *state = scan.outgoing;
        finalize_spans(line, raw)
    }

    /// Highlight one line with the Rust family. Kept for the existing M4
    /// tests and any Rust-specific call sites; production renders pass the
    /// detected language explicitly.
    #[allow(dead_code)] // test/frozen M4 surface; production passes a detected Lang
    pub fn line_spans(&self, line: &str) -> Vec<Span> {
        self.line_spans_for(line, Lang::Rust)
    }
}

#[cfg(test)]
mod tests;
