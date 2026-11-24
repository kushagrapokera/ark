//! Bounded editor decorations for the read-only viewer: current-line band,
//! indent guides and three-color bracket pairs. No per-file cache.

use crate::highlight::{Span, Token};
use crate::render::{Canvas, ColorRgb, Theme};

use super::LINE_H;

const INDENT_WIDTH: usize = 4;
const MAX_INDENT_LEVELS: usize = 64;
const MAX_BRACKET_DEPTH: usize = 256;

#[derive(Default)]
pub(super) struct BracketState {
    stack: Vec<(u8, usize)>,
}

pub(super) fn draw_current_line(
    canvas: &mut Canvas,
    theme: &Theme,
    scale: f64,
    x: f64,
    y: f64,
    width: f64,
) {
    if width > 0.0 {
        canvas.fill_rect(x, y, width, LINE_H, theme.editor_current_line, scale);
    }
}

#[allow(clippy::too_many_arguments)] // explicit raster inputs match the viewer draw style
pub(super) fn draw_indent_guides(
    canvas: &mut Canvas,
    theme: &Theme,
    scale: f64,
    text_x: f64,
    row_y: f64,
    line: &str,
    advance: f64,
    active: bool,
    width: f64,
) {
    let indent = line
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .map(|c| if c == '\t' { INDENT_WIDTH } else { 1 })
        .sum::<usize>();
    if indent < INDENT_WIDTH {
        return;
    }
    let levels = (indent / INDENT_WIDTH).min(MAX_INDENT_LEVELS);
    for level in 1..=levels {
        let color = if active && level == levels {
            theme.editor_indent_guide_active
        } else {
            theme.editor_indent_guide
        };
        let x = text_x + (level * INDENT_WIDTH) as f64 * advance;
        if x < text_x || x >= text_x + width {
            continue;
        }
        canvas.fill_rect(x, row_y + 2.0, 1.0, LINE_H - 4.0, color, scale);
    }
}

fn bracket_color(theme: &Theme, index: usize) -> ColorRgb {
    match index % 3 {
        0 => theme.editor_bracket_1,
        1 => theme.editor_bracket_2,
        _ => theme.editor_bracket_3,
    }
}

fn is_open(ch: u8) -> bool {
    matches!(ch, b'(' | b'[' | b'{')
}

fn matches_close(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']') | (b'{', b'}'))
}

/// Bracket glyph spans in CHAR indices. Strings/comments/regex literals are
/// excluded; the stack is bounded so a pathological line cannot grow memory.
pub(super) fn bracket_spans(
    line: &str,
    syntax: &[Span],
    state: &mut BracketState,
    theme: &Theme,
) -> Vec<(usize, usize, ColorRgb)> {
    let mut out = Vec::new();
    for (char_index, (_, ch)) in line.char_indices().enumerate() {
        let protected = syntax.iter().any(|span| {
            matches!(span.token, Token::String | Token::Comment | Token::Regex)
                && char_index >= span.start
                && char_index < span.end
        });
        if protected {
            continue;
        }
        if !ch.is_ascii() {
            continue;
        }
        let byte = ch as u8;
        if is_open(byte) {
            let color_index = state.stack.len() % 3;
            if state.stack.len() < MAX_BRACKET_DEPTH {
                state.stack.push((byte, color_index));
            }
            out.push((
                char_index,
                char_index + 1,
                bracket_color(theme, color_index),
            ));
        } else if matches!(byte, b')' | b']' | b'}') {
            let color_index = if state
                .stack
                .last()
                .is_some_and(|(open, _)| matches_close(*open, byte))
            {
                state.stack.pop().map(|(_, index)| index).unwrap_or(0)
            } else {
                state.stack.len() % 3
            };
            out.push((
                char_index,
                char_index + 1,
                bracket_color(theme, color_index),
            ));
        }
    }
    out
}
