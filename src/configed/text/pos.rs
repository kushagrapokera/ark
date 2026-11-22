//! Byte/char position helpers for the textarea model (split from `text.rs`
//! to keep each module focused).

/// Line-start byte offsets; a trailing newline opens a final empty line.
pub(super) fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

pub(super) fn truncate_to_boundary(mut s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s
}

pub(super) fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub(super) fn prev_boundary(s: &str, i: usize) -> usize {
    let i = floor_boundary(s, i);
    if i == 0 {
        return 0;
    }
    let mut p = i - 1;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

pub(super) fn next_boundary(s: &str, i: usize) -> usize {
    let i = floor_boundary(s, i);
    if i >= s.len() {
        return s.len();
    }
    let mut n = i + 1;
    while n < s.len() && !s.is_char_boundary(n) {
        n += 1;
    }
    n
}
