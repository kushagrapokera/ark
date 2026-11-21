//! Regex token highlighter (M4): one compiled rule set, applied per VISIBLE
//! line only (viewport discipline — no whole-file passes, no per-frame
//! compilation).
//!
//! The rule set is deliberately GENERIC (one profile for every language):
//! comments (`//`, `#` — with a code-level exception for Rust attributes),
//! string/char literals, numbers and a multi-language keyword list. Span
//! resolution: matches compete by start position, earlier-start wins; ties
//! go to the longer span. That makes `"http://x"` a string (the string
//! starts before the `//`) and `// "q"` a comment (the comment starts
//! before the quote).

use regex::Regex;

/// Token kinds mapped to theme colors at render time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Keyword,
    String,
    Number,
    Comment,
}

/// One highlighted span of a line, in CHAR indices (render maps col → x).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: Token,
}

/// Compiled rule set. Static patterns — the `expect`s fire only on a bug in
/// these constants, never on user input.
pub struct Highlighter {
    comment: Regex,
    string: Regex,
    number: Regex,
    keyword: Regex,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Highlighter {
            // `//` or `#` to end of line. Rust attribute lines (`#[…]`,
            // `#![…]`) are filtered out in code below (the regex crate has
            // no lookaround).
            comment: Regex::new(r"(?://|#).*$").expect("static comment pattern"),
            // Double-quoted with escapes; single-quote CHAR literals only
            // (so lifetimes like `'static` and prose apostrophes never match).
            string: Regex::new(r#""(?:[^"\\\n]|\\.)*"|'(?:\\.|[^'\\\n])'"#)
                .expect("static string pattern"),
            number: Regex::new(r"\b0[xX][0-9a-fA-F_]+\b|\b\d[\d_]*(?:\.[\d_]+)?(?:[eE][+-]?\d+)?\b")
                .expect("static number pattern"),
            keyword: Regex::new(
                r"\b(?:as|async|await|bool|break|case|catch|char|class|const|continue|def|default|do|double|elif|else|enum|export|extends|false|fn|for|from|func|if|impl|import|in|let|loop|match|mod|move|mut|new|None|null|package|pass|pub|raise|ref|return|self|static|struct|super|switch|this|throw|trait|true|try|type|unsafe|use|val|var|void|where|while|with)\b",
            )
            .expect("static keyword pattern"),
        }
    }

    /// Highlight one line: non-overlapping spans sorted by start.
    pub fn line_spans(&self, line: &str) -> Vec<Span> {
        let mut raw: Vec<(usize, usize, Token)> = Vec::new();
        for (re, token) in [
            (&self.comment, Token::Comment),
            (&self.string, Token::String),
            (&self.number, Token::Number),
            (&self.keyword, Token::Keyword),
        ] {
            for m in re.find_iter(line) {
                if token == Token::Comment && is_rust_attribute(m.as_str()) {
                    continue; // `#[…]` / `#![…]` are not comments
                }
                raw.push((m.start(), m.end(), token));
            }
        }
        // Compete by start position (earlier wins; ties go to the longer
        // span), then drop any span overlapping an accepted one.
        raw.sort_by_key(|&(s, e, _)| (s, std::cmp::Reverse(e)));
        let mut spans = Vec::new();
        let mut last_end = 0usize;
        for (s, e, t) in raw {
            if s < last_end {
                continue;
            }
            last_end = e;
            // byte → char index conversion (spans feed col → x rendering)
            let start = line[..s].chars().count();
            let end = start + line[s..e].chars().count();
            spans.push(Span { start, end, token: t });
        }
        spans
    }
}

/// True when a comment match actually starts at a Rust attribute (`#[` or
/// `#![`), which must stay plain.
fn is_rust_attribute(matched: &str) -> bool {
    matched.starts_with("#[") || matched.starts_with("#![")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(usize, usize, Token)> {
        Highlighter::new()
            .line_spans(line)
            .into_iter()
            .map(|s| (s.start, s.end, s.token))
            .collect()
    }

    #[test]
    fn keywords_and_numbers_span_correctly() {
        assert_eq!(
            kinds("let x = 0x1F + 1_000 + 1.5e3;"),
            vec![
                (0, 3, Token::Keyword),  // let
                (8, 12, Token::Number),  // 0x1F
                (15, 20, Token::Number), // 1_000
                (23, 28, Token::Number), // 1.5e3
            ]
        );
    }

    #[test]
    fn strings_win_over_comments_inside_them() {
        // The `//` sits INSIDE the string literal: the string starts earlier
        // and swallows the would-be comment.
        assert_eq!(
            kinds(r#"let url = "http://x";"#),
            vec![(0, 3, Token::Keyword), (10, 20, Token::String)],
        );
    }

    #[test]
    fn comments_win_over_strings_inside_them() {
        // The comment starts at col 0; its quotes are plain comment text.
        assert_eq!(kinds("// it's \"quoted\" // x"), vec![(0, 21, Token::Comment)]);
    }

    #[test]
    fn rust_attributes_are_not_comments() {
        assert_eq!(kinds("#[derive(Debug)]"), Vec::new());
        assert_eq!(kinds("#![allow(dead_code)]"), Vec::new());
        // Shell/python style comments still work.
        assert_eq!(kinds("# TODO fix"), vec![(0, 10, Token::Comment)]);
        assert_eq!(kinds("#!/bin/sh"), vec![(0, 9, Token::Comment)]);
    }

    #[test]
    fn char_literals_match_but_prose_apostrophes_do_not() {
        assert_eq!(
            kinds("let c = '\\n';"),
            vec![(0, 3, Token::Keyword), (8, 12, Token::String)]
        );
        assert_eq!(kinds("it's fine"), Vec::new(), "no closing quote → no string");
    }

    #[test]
    fn spans_use_char_indices_not_bytes() {
        // é is one char / two bytes: string span must land on char columns.
        assert_eq!(
            kinds("let café = \"ü\";"),
            vec![(0, 3, Token::Keyword), (11, 14, Token::String)]
        );
    }

    #[test]
    fn empty_and_plain_lines_have_no_spans() {
        assert_eq!(kinds(""), Vec::new());
        assert_eq!(kinds("plain_identifier + call()"), Vec::new());
    }
}
