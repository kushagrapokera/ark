//! Line-local token collection, shared regexes and static profile compilation.

use std::sync::OnceLock;

use regex::Regex;

use super::profiles;
use super::{Lang, Span, Token};

/// Shared regexes: one compiled copy per distinct pattern, reused by every
/// profile. Nothing here is compiled until a matching file type is rendered.
static COMMENT_SLASH: OnceLock<Regex> = OnceLock::new();
static COMMENT_HASH: OnceLock<Regex> = OnceLock::new();
static COMMENT_HASH_SEMI: OnceLock<Regex> = OnceLock::new();
static COMMENT_SQL: OnceLock<Regex> = OnceLock::new();
static COMMENT_HTML: OnceLock<Regex> = OnceLock::new();
static COMMENT_CSS: OnceLock<Regex> = OnceLock::new();
static COMMENT_NIX: OnceLock<Regex> = OnceLock::new();
static COMMENT_PLAIN: OnceLock<Regex> = OnceLock::new();
static COMMENT_PERCENT: OnceLock<Regex> = OnceLock::new();
static STRING: OnceLock<Regex> = OnceLock::new();
static ESCAPE: OnceLock<Regex> = OnceLock::new();
static NUMBER: OnceLock<Regex> = OnceLock::new();

fn shared(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex pattern"))
}

fn comment_regex(style: profiles::CommentStyle) -> &'static Regex {
    use profiles::CommentStyle::*;
    match style {
        Slash => shared(&COMMENT_SLASH, r"(?://|/\*.*?\*/).*$"),
        Hash => shared(&COMMENT_HASH, r"#.*$"),
        HashSemi => shared(&COMMENT_HASH_SEMI, r"[#;].*$"),
        Sql => shared(&COMMENT_SQL, r"(?:--|/\*.*?\*/).*$"),
        Html => shared(&COMMENT_HTML, r"<!--.*?-->"),
        Css => shared(&COMMENT_CSS, r"/\*.*?\*/"),
        Nix => shared(&COMMENT_NIX, r"(?://|#|/\*.*?\*/).*$"),
        Plain => shared(&COMMENT_PLAIN, r"(?://|#|--).*$"),
        Percent => shared(&COMMENT_PERCENT, r"%.*$"),
    }
}

fn string_regex() -> &'static Regex {
    shared(
        &STRING,
        r#""(?:[^"\\\n]|\\.)*"|'(?:\\.|[^'\\\n])'|`[^`\n]*`"#,
    )
}

fn escape_regex() -> &'static Regex {
    shared(
        &ESCAPE,
        r#"\\(?:[nrt0\\'\"abfnv]|u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2})"#,
    )
}

fn number_regex() -> &'static Regex {
    shared(
        &NUMBER,
        r"\b0[xX][0-9a-fA-F_]+\b|\b\d[\d_]*(?:\.[\d_]+)?(?:[eE][+-]?\d+)?\b",
    )
}

/// Compiled rule set for one profile. Created on first use. The only owned
/// allocation is the small extras list; all regexes point at shared statics.
pub(super) struct CompiledProfile {
    comment: &'static Regex,
    extras: Vec<&'static profiles::RuleDef>,
    def: profiles::LangDef,
}

pub(super) fn compile(lang: Lang) -> CompiledProfile {
    let def = profiles::lang_def(lang);
    CompiledProfile {
        comment: comment_regex(def.comment),
        extras: def.extras.iter().collect(),
        def,
    }
}

/// Whether `word` appears in a static keyword/type table.
fn has_word(words: &[&str], word: &str, case_insensitive: bool) -> bool {
    words.iter().any(|candidate| {
        if case_insensitive {
            candidate.eq_ignore_ascii_case(word)
        } else {
            *candidate == word
        }
    })
}

/// Visit ASCII identifiers as `(byte_start, byte_end, text)` without allocating.
fn each_identifier(line: &str, mut visit: impl FnMut(usize, usize, &str)) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphabetic() && bytes[i] != b'_' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        visit(start, i, &line[start..i]);
    }
}

/// Scan ASCII identifiers and classify them against the profile's static
/// keyword/control/type tables. No alternation regex and no allocation per word.
fn push_identifiers(
    line: &str,
    def: &profiles::LangDef,
    raw: &mut Vec<(usize, usize, Token)>,
) {
    each_identifier(line, |start, end, word| {
        let token = if has_word(def.control_keywords, word, def.case_insensitive) {
            Some(Token::ControlKeyword)
        } else if has_word(def.keywords, word, def.case_insensitive) {
            Some(Token::Keyword)
        } else if has_word(def.types, word, def.case_insensitive) {
            Some(Token::Type)
        } else {
            None
        };
        if let Some(token) = token {
            raw.push((start, end, token));
        }
    });
}

/// Fill remaining code identifiers with VS Code-style variable/type categories.
/// Called after the more specific extras so calls, members, constants and
/// declarations win equal-span ties.
fn push_variables(
    line: &str,
    def: &profiles::LangDef,
    raw: &mut Vec<(usize, usize, Token)>,
) {
    if !def.variables && !def.camel_case_types {
        return;
    }
    each_identifier(line, |start, end, word| {
        let camel_case = def.camel_case_types
            && word.as_bytes()[0].is_ascii_uppercase()
            && word.bytes().any(|b| b.is_ascii_lowercase());
        if camel_case {
            raw.push((start, end, Token::Type));
        } else if def.variables {
            raw.push((start, end, Token::Variable));
        }
    });
}

/// Push string spans, splitting out escape sequences so they can use Dark+'s
/// amber escape color instead of being swallowed by the surrounding string.
fn push_strings(line: &str, raw: &mut Vec<(usize, usize, Token)>) {
    let escapes = escape_regex();
    for m in string_regex().find_iter(line) {
        let text = m.as_str();
        let mut cursor = 0;
        for escape in escapes.find_iter(text) {
            if escape.start() > cursor {
                raw.push((m.start() + cursor, m.start() + escape.start(), Token::String));
            }
            raw.push((m.start() + escape.start(), m.start() + escape.end(), Token::Escape));
            cursor = escape.end();
        }
        if cursor < text.len() {
            raw.push((m.start() + cursor, m.end(), Token::String));
        }
    }
}

/// Collect matches for one rule into `raw`.
fn push_matches(
    re: &Regex,
    token: Token,
    group: usize,
    line: &str,
    lang: Lang,
    raw: &mut Vec<(usize, usize, Token)>,
) {
    if group > 0 {
        for caps in re.captures_iter(line) {
            let Some(matched) = caps.get(group) else { continue };
            if matched.start() < matched.end() {
                raw.push((matched.start(), matched.end(), token));
            }
        }
    } else {
        for m in re.find_iter(line) {
            if token == Token::Comment && lang == Lang::Rust && is_rust_attribute(m.as_str()) {
                continue;
            }
            if m.start() < m.end() {
                raw.push((m.start(), m.end(), token));
            }
        }
    }
}

/// Resolve overlapping byte spans and convert them to renderer char indices.
pub(super) fn finalize_spans(line: &str, mut raw: Vec<(usize, usize, Token)>) -> Vec<Span> {
    // Compete by start position (earlier wins; ties go to the longer span),
    // then drop any span overlapping an accepted one.
    raw.sort_by_key(|&(s, e, _)| (s, std::cmp::Reverse(e)));
    let mut spans = Vec::new();
    let mut last_end = 0usize;
    for (s, e, t) in raw {
        if s < last_end {
            continue;
        }
        last_end = e;
        let start = line[..s].chars().count();
        let end = start + line[s..e].chars().count();
        spans.push(Span { start, end, token: t });
    }
    spans
}

/// True when a comment match actually starts at a Rust attribute (`#[` or
/// `#![`), which must stay plain.
fn is_rust_attribute(matched: &str) -> bool {
    matched.starts_with("#[") || matched.starts_with("#![")
}


/// Collect the line-local static-profile spans. `offset` is added so a
/// continuation prefix and the remaining segment share one byte space.
pub(super) fn collect_static_spans(
    p: &CompiledProfile,
    line: &str,
    lang: Lang,
    offset: usize,
    raw: &mut Vec<(usize, usize, Token)>,
) {
    let first = raw.len();

    push_matches(p.comment, Token::Comment, 0, line, lang, raw);
    push_strings(line, raw);
    push_matches(number_regex(), Token::Number, 0, line, lang, raw);
    push_identifiers(line, &p.def, raw);
    for rule in &p.extras {
        push_matches(shared(rule.cell, rule.pattern), rule.token, rule.group, line, lang, raw);
    }
    push_variables(line, &p.def, raw);

    for (start, end, _) in &mut raw[first..] {
        *start += offset;
        *end += offset;
    }
}
