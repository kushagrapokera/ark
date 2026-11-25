//! Bounded Markdown block and inline parsing. One temporary block at a time;
//! no document-wide AST is retained.

use crate::viewer::Viewer;

const MAX_INLINE_CHARS: usize = 2_000;
const MAX_CODE_LINES: usize = 512;
const MAX_TABLE_ROWS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InlineKind {
    Normal,
    Bold,
    Code,
    Link,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InlineSpan {
    pub(super) text: String,
    pub(super) kind: InlineKind,
}

#[derive(Debug)]
pub(super) enum BlockKind {
    Heading { level: usize, spans: Vec<InlineSpan> },
    Paragraph(Vec<InlineSpan>),
    List { ordered: Option<usize>, spans: Vec<InlineSpan> },
    Quote(Vec<InlineSpan>),
    Code(Vec<String>),
    Rule,
    Table { headers: Vec<Vec<InlineSpan>>, rows: Vec<Vec<Vec<InlineSpan>>> },
}

pub(super) fn inline_spans(line: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut normal_start = 0usize;
    let mut i = 0usize;
    let mut chars = 0usize;
    while i < line.len() && chars < MAX_INLINE_CHARS {
        let rest = &line[i..];
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                push_normal(&mut spans, &line[normal_start..i]);
                let inner_end = i + 2 + end;
                spans.push(InlineSpan {
                    text: line[i + 2..inner_end].chars().take(MAX_INLINE_CHARS).collect(),
                    kind: InlineKind::Bold,
                });
                i = inner_end + 2;
                normal_start = i;
                chars += 2;
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                push_normal(&mut spans, &line[normal_start..i]);
                let inner_end = i + 1 + end;
                spans.push(InlineSpan {
                    text: line[i + 1..inner_end].chars().take(MAX_INLINE_CHARS).collect(),
                    kind: InlineKind::Code,
                });
                i = inner_end + 1;
                normal_start = i;
                chars += 2;
                continue;
            }
        }
        let image = rest.starts_with("![");
        let link_start = if image { i + 1 } else { i };
        if rest.starts_with('[') || image {
            if let Some(close) = line[link_start + 1..].find(']') {
                let label_end = link_start + 1 + close;
                if line.as_bytes().get(label_end + 1) == Some(&b'(') {
                    if let Some(url_end) = line[label_end + 2..].find(')') {
                        push_normal(&mut spans, &line[normal_start..i]);
                        let url_end = label_end + 2 + url_end;
                        spans.push(InlineSpan {
                            text: line[link_start + 1..label_end]
                                .chars()
                                .take(MAX_INLINE_CHARS)
                                .collect(),
                            kind: InlineKind::Link,
                        });
                        i = url_end + 1;
                        normal_start = i;
                        chars += 2;
                        continue;
                    }
                }
            }
        }
        let ch = rest.chars().next().expect("nonempty rest");
        i += ch.len_utf8();
        chars += 1;
    }
    push_normal(&mut spans, &line[normal_start..i]);
    spans
}

fn push_normal(spans: &mut Vec<InlineSpan>, text: &str) {
    if !text.is_empty() {
        spans.push(InlineSpan { text: text.to_owned(), kind: InlineKind::Normal });
    }
}

pub(super) fn next_block(viewer: &Viewer, index: usize) -> (BlockKind, usize) {
    let line = viewer.line_text(index).unwrap_or_default();
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        let fence = &trimmed[..3];
        let mut lines = Vec::new();
        let mut next = index + 1;
        while next < viewer.line_count() {
            let code = viewer.line_text(next).unwrap_or_default();
            if code.trim_start().starts_with(fence) {
                return (BlockKind::Code(lines), next + 1);
            }
            if lines.len() < MAX_CODE_LINES {
                lines.push(code.chars().take(MAX_INLINE_CHARS).collect());
            }
            next += 1;
        }
        return (BlockKind::Code(lines), next);
    }
    let heading = trimmed.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&heading) && trimmed.as_bytes().get(heading) == Some(&b' ') {
        return (
            BlockKind::Heading {
                level: heading,
                spans: inline_spans(trimmed[heading..].trim_start()),
            },
            index + 1,
        );
    }
    if is_rule(trimmed) {
        return (BlockKind::Rule, index + 1);
    }
    if let Some((ordered, text)) = list_marker(trimmed) {
        return (
            BlockKind::List { ordered, spans: inline_spans(text) },
            index + 1,
        );
    }
    if let Some(text) = trimmed.strip_prefix('>') {
        return (
            BlockKind::Quote(inline_spans(text.trim_start())),
            index + 1,
        );
    }
    if trimmed.contains('|') && is_table_separator(&viewer.line_text(index + 1).unwrap_or_default()) {
        let headers = table_cells(&line)
            .into_iter()
            .map(inline_spans)
            .collect();
        let mut rows = Vec::new();
        let mut next = index + 2;
        while next < viewer.line_count() {
            let row = viewer.line_text(next).unwrap_or_default();
            if !row.contains('|') || row.trim().is_empty() {
                break;
            }
            if rows.len() < MAX_TABLE_ROWS {
                rows.push(table_cells(&row).into_iter().map(inline_spans).collect());
            }
            next += 1;
        }
        return (BlockKind::Table { headers, rows }, next);
    }
    (BlockKind::Paragraph(inline_spans(&line)), index + 1)
}

fn is_rule(line: &str) -> bool {
    let mut count = 0usize;
    for ch in line.chars() {
        match ch {
            '-' | '*' | '_' => count += 1,
            ' ' | '\t' => {}
            _ => return false,
        }
    }
    count >= 3
}

fn list_marker(line: &str) -> Option<(Option<usize>, &str)> {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(text) = line.strip_prefix(prefix) {
            return Some((None, text));
        }
    }
    let (number, text) = line.split_once(". ")?;
    if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        return Some((Some(number.parse().ok()?), text));
    }
    None
}

fn is_table_separator(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim();
            cell.len() >= 3 && cell.bytes().all(|b| matches!(b, b'-' | b':' | b' ' | b'\t'))
        })
}

fn table_cells(line: &str) -> Vec<&str> {
    line.trim().trim_matches('|').split('|').map(str::trim).collect()
}

pub(super) fn wrap_spans(spans: Vec<InlineSpan>, width: f64, advance: f64) -> Vec<Vec<InlineSpan>> {
    let max_chars = (width / advance.max(1.0)).floor().max(1.0) as usize;
    let mut lines = Vec::new();
    let mut current: Vec<InlineSpan> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let mut text = span.text.as_str();
        while !text.is_empty() {
            let remaining = max_chars.saturating_sub(used).max(1);
            let take = text.chars().count().min(remaining);
            let split = char_split(text, take);
            let (head, tail) = text.split_at(split);
            if !head.is_empty() {
                let mut piece = head.to_owned();
                if used == 0 && piece.starts_with(' ') {
                    piece.remove(0);
                }
                current.push(InlineSpan { text: piece, kind: span.kind });
                used += head.chars().count();
            }
            text = tail;
            if used >= max_chars {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn char_split(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map(|(i, _)| i).unwrap_or(text.len())
}
