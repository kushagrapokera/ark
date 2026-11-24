//! Bounded multiline state for the Tier-1.5 highlighter.
//!
//! The viewer reconstructs state from at most [`LOOKBACK_LINES`] lines before
//! the viewport, then carries one value through the visible lines. There is no
//! per-file state cache and no whole-file tokenization.

use super::Lang;

/// Maximum number of lines inspected to reconstruct viewport state.
pub const LOOKBACK_LINES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TripleQuote {
    Double,
    Single,
}

impl TripleQuote {
    fn delimiter(self) -> &'static str {
        match self {
            Self::Double => "\"\"\"",
            Self::Single => "'''",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkdownFence {
    marker: u8,
    len: usize,
    lang: Lang,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HtmlEmbed {
    Script,
    Style,
}

impl HtmlEmbed {
    fn lang(self) -> Lang {
        match self {
            Self::Script => Lang::CLike,
            Self::Style => Lang::Css,
        }
    }

    fn closing_tag(self) -> &'static str {
        match self {
            Self::Script => "</script",
            Self::Style => "</style",
        }
    }

    fn opening_tag(self) -> &'static str {
        match self {
            Self::Script => "<script",
            Self::Style => "<style",
        }
    }
}

/// State carried between lines while drawing one viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HighlightState {
    block_comment: bool,
    triple_quote: Option<TripleQuote>,
    markdown_fence: Option<MarkdownFence>,
    embedded: Option<HtmlEmbed>,
}

impl HighlightState {
    pub(super) fn has_block_comment(&self) -> bool {
        self.block_comment
    }

    pub(super) fn has_triple_quote(&self) -> bool {
        self.triple_quote.is_some()
    }
}

/// Result of scanning one line without producing tokens.
pub(super) struct LineScan {
    pub outgoing: HighlightState,
    /// An unclosed block comment/triple string opened on this line.
    pub open_block_comment: Option<usize>,
    pub open_triple_quote: Option<usize>,
    /// End offset of an incoming block comment/triple string closed here.
    pub close_block_comment: Option<usize>,
    pub close_triple_quote: Option<usize>,
}

fn supports_block_comments(lang: Lang) -> bool {
    matches!(
        lang,
        Lang::Rust
            | Lang::CLike
            | Lang::Go
            | Lang::Sql
            | Lang::Css
            | Lang::Json
            | Lang::Nix
            | Lang::Hcl
            | Lang::Protobuf
            | Lang::DashLike
    )
}

fn line_comment_len(bytes: &[u8], pos: usize, lang: Lang) -> Option<usize> {
    let rest = &bytes[pos..];
    match lang {
        Lang::Rust | Lang::CLike | Lang::Go | Lang::Json | Lang::Protobuf => {
            rest.starts_with(b"//").then_some(2)
        }
        Lang::Sql | Lang::DashLike => rest.starts_with(b"--").then_some(2),
        Lang::Nix | Lang::Hcl => {
            if rest.starts_with(b"//") {
                Some(2)
            } else if rest.starts_with(b"#") {
                Some(1)
            } else {
                None
            }
        }
        Lang::PythonLike | Lang::Shell | Lang::PowerShell | Lang::Cmake | Lang::Graphql => {
            rest.starts_with(b"#").then_some(1)
        }
        Lang::Plain => {
            if rest.starts_with(b"//") || rest.starts_with(b"--") {
                Some(2)
            } else if rest.starts_with(b"#") {
                Some(1)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start > haystack.len() || needle.len() > haystack.len() - start {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|rel| start + rel)
}

fn skip_char_literal(bytes: &[u8], pos: usize) -> Option<usize> {
    let limit = (pos + 8).min(bytes.len());
    let mut i = pos + 1;
    while i < limit {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn skip_quoted(line: &str, pos: usize, quote: u8) -> usize {
    let bytes = line.as_bytes();
    let mut i = pos + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn scan_code_line(line: &str, lang: Lang, state: &mut HighlightState) -> LineScan {
    let mut scan = LineScan {
        outgoing: *state,
        open_block_comment: None,
        open_triple_quote: None,
        close_block_comment: None,
        close_triple_quote: None,
    };
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if state.block_comment {
            if let Some(rel) = find_bytes(bytes, b"*/", i) {
                let end = rel + 2;
                scan.close_block_comment = Some(end);
                state.block_comment = false;
                i = end;
                continue;
            }
            return scan;
        }
        if let Some(delimiter) = state.triple_quote {
            let delim = delimiter.delimiter().as_bytes();
            if let Some(rel) = find_bytes(bytes, delim, i) {
                let end = rel + delim.len();
                scan.close_triple_quote = Some(end);
                state.triple_quote = None;
                i = end;
                continue;
            }
            return scan;
        }
        if lang == Lang::PythonLike
            && (bytes[i..].starts_with(b"\"\"\"") || bytes[i..].starts_with(b"'''"))
        {
            let delimiter = if bytes[i..].starts_with(b"\"\"\"") {
                TripleQuote::Double
            } else {
                TripleQuote::Single
            };
            let delim = delimiter.delimiter().as_bytes();
            if let Some(rel) = find_bytes(bytes, delim, i + 3) {
                i = rel + 3;
                continue;
            }
            state.triple_quote = Some(delimiter);
            scan.open_triple_quote = Some(i);
            return scan;
        }
        if supports_block_comments(lang) && bytes[i..].starts_with(b"/*") {
            if let Some(rel) = find_bytes(bytes, b"*/", i + 2) {
                i = rel + 2;
                continue;
            }
            state.block_comment = true;
            scan.open_block_comment = Some(i);
            return scan;
        }
        if matches!(bytes[i], b'"' | b'`') {
            i = skip_quoted(line, i, bytes[i]);
            continue;
        }
        if bytes[i] == b'\'' {
            // Treat a quote as a char literal only when a close quote is near;
            // this avoids swallowing Rust lifetimes such as `'a` and keeps
            // apostrophe-heavy prose linear to scan.
            if let Some(end) = skip_char_literal(bytes, i) {
                i = end;
                continue;
            }
        }
        if line_comment_len(bytes, i, lang).is_some() {
            return scan;
        }
        i += 1;
    }

    scan.outgoing = *state;
    scan
}

fn opening_fence(line: &str) -> Option<MarkdownFence> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let marker = *trimmed.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|b| *b == marker).count();
    if len < 3 {
        return None;
    }
    let info = trimmed[len..].trim();
    Some(MarkdownFence { marker, len, lang: lang_for_fence(info) })
}

fn is_closing_fence(line: &str, fence: MarkdownFence) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let len = trimmed.bytes().take_while(|b| *b == fence.marker).count();
    len >= fence.len && trimmed[len..].trim().is_empty()
}

fn lang_for_fence(info: &str) -> Lang {
    let name = info
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('{')
        .to_ascii_lowercase();
    match name.as_str() {
        "rust" | "rs" => Lang::Rust,
        "c" => Lang::CLike,
        "cpp" | "c++" | "cc" | "cxx" => Lang::CLike,
        "csharp" | "cs" => Lang::CLike,
        "java" => Lang::CLike,
        "kotlin" | "kt" => Lang::CLike,
        "swift" => Lang::CLike,
        "javascript" | "js" | "jsx" => Lang::CLike,
        "typescript" | "ts" | "tsx" => Lang::CLike,
        "php" => Lang::CLike,
        "go" | "golang" => Lang::Go,
        "python" | "py" => Lang::PythonLike,
        "ruby" | "rb" => Lang::PythonLike,
        "perl" | "pl" => Lang::PythonLike,
        "r" => Lang::PythonLike,
        "julia" | "jl" => Lang::PythonLike,
        "dart" => Lang::PythonLike,
        "elixir" | "ex" => Lang::PythonLike,
        "bash" | "sh" | "shell" | "zsh" | "fish" => Lang::Shell,
        "powershell" | "ps1" => Lang::PowerShell,
        "sql" => Lang::Sql,
        "html" | "xml" | "svg" | "vue" | "svelte" | "astro" => Lang::Markup,
        "css" | "scss" | "sass" | "less" => Lang::Css,
        "json" | "jsonc" | "json5" => Lang::Json,
        "yaml" | "yml" => Lang::Yaml,
        "toml" => Lang::Toml,
        "ini" | "cfg" => Lang::Ini,
        "dotenv" | "env" => Lang::Dotenv,
        "docker" | "dockerfile" => Lang::Dockerfile,
        "make" | "makefile" => Lang::Makefile,
        "nix" => Lang::Nix,
        "hcl" | "terraform" | "tf" => Lang::Hcl,
        "proto" | "protobuf" => Lang::Protobuf,
        "lua" | "haskell" | "hs" | "elm" => Lang::DashLike,
        "cmake" => Lang::Cmake,
        "graphql" | "gql" => Lang::Graphql,
        "latex" | "tex" => Lang::Latex,
        _ => Lang::Plain,
    }
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|start| {
        h[*start..*start + n.len()]
            .iter()
            .zip(n)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn find_tag_ci(haystack: &str, needle: &str) -> Option<usize> {
    let start = find_ascii_ci(haystack, needle)?;
    let bytes = haystack.as_bytes();
    let next = bytes.get(start + needle.len()).copied();
    if next.is_none_or(|b| matches!(b, b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')) {
        Some(start)
    } else {
        None
    }
}

fn opening_embed(line: &str) -> Option<HtmlEmbed> {
    if find_tag_ci(line, "<script").is_some() {
        Some(HtmlEmbed::Script)
    } else if find_tag_ci(line, "<style").is_some() {
        Some(HtmlEmbed::Style)
    } else {
        None
    }
}

/// Effective tokenization language for the current line.
pub(super) fn effective_lang(line: &str, base: Lang, state: HighlightState) -> Lang {
    if base == Lang::Markdown {
        if let Some(fence) = state.markdown_fence {
            if is_closing_fence(line, fence) {
                return Lang::Markdown;
            }
            return fence.lang;
        }
    }
    if base == Lang::Markup {
        if let Some(embed) = state.embedded {
            if find_tag_ci(line, embed.closing_tag()).is_some() {
                return Lang::Markup;
            }
            return embed.lang();
        }
    }
    base
}

/// Scan one line and return its outgoing state plus delimiter positions used
/// by the token collector.
pub(super) fn scan_line(line: &str, base: Lang, incoming: HighlightState) -> LineScan {
    let effective = effective_lang(line, base, incoming);
    let mut outgoing = incoming;
    let mut scan = if matches!(effective, Lang::Markdown | Lang::Markup) {
        LineScan {
            outgoing,
            open_block_comment: None,
            open_triple_quote: None,
            close_block_comment: None,
            close_triple_quote: None,
        }
    } else {
        scan_code_line(line, effective, &mut outgoing)
    };

    if base == Lang::Markdown {
        if let Some(fence) = incoming.markdown_fence {
            if is_closing_fence(line, fence) {
                outgoing.markdown_fence = None;
                outgoing.block_comment = false;
                outgoing.triple_quote = None;
            }
        } else if let Some(fence) = opening_fence(line) {
            outgoing.markdown_fence = Some(fence);
            outgoing.block_comment = false;
            outgoing.triple_quote = None;
        }
    } else if base == Lang::Markup {
        if let Some(embed) = incoming.embedded {
            if find_tag_ci(line, embed.closing_tag()).is_some() {
                outgoing.embedded = None;
                outgoing.block_comment = false;
                outgoing.triple_quote = None;
            }
        } else if let Some(embed) = opening_embed(line) {
            let opens = find_tag_ci(line, embed.opening_tag()).unwrap_or(0);
            let closes_same_line = find_tag_ci(line, embed.closing_tag())
                .is_some_and(|close| close > opens);
            if !closes_same_line {
                outgoing.embedded = Some(embed);
            }
            outgoing.block_comment = false;
            outgoing.triple_quote = None;
        }
    }

    scan.outgoing = outgoing;
    scan
}
