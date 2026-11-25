//! Native clipboard I/O and bounded text normalization.

use std::io::{self, Read};

pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

pub fn copy_text(text: &str) -> io::Result<()> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("clipboard copy exceeds {MAX_CLIPBOARD_BYTES} bytes"),
        ));
    }
    use wl_clipboard_rs::copy::{copy, MimeType, Options, Source};
    copy(
        Options::new(),
        Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
        MimeType::Text,
    )
    .map_err(|e| io::Error::other(format!("clipboard copy failed: {e}")))
}

pub fn copy_viewer_selection(viewer: &crate::viewer::Viewer) -> io::Result<usize> {
    let chars = viewer.selection_len();
    if chars == 0 {
        return Ok(0);
    }
    if chars > MAX_CLIPBOARD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("clipboard copy exceeds {MAX_CLIPBOARD_BYTES} bytes"),
        ));
    }
    let text = viewer.selection_text().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "viewer selection disappeared before copy",
        )
    })?;
    copy_text(&text)?;
    Ok(text.chars().count())
}

pub fn paste_text(max_bytes: usize) -> io::Result<String> {
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};
    let cap = max_bytes.min(MAX_CLIPBOARD_BYTES);
    let (pipe, _guard) = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text)
        .map_err(|e| io::Error::other(format!("clipboard paste failed: {e}")))?;
    let mut bytes = Vec::new();
    pipe.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io::Error::other(format!("clipboard read failed: {e}")))?;
    validate_text(&bytes, cap)
}

pub fn paste_single_line(max_chars: usize) -> io::Result<String> {
    paste_text(MAX_CLIPBOARD_BYTES).map(|text| single_line(&text, max_chars))
}

pub(crate) fn single_line(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(max_chars.min(text.len()));
    let mut count = 0usize;
    let mut previous_cr = false;
    for ch in text.chars() {
        if count >= max_chars {
            break;
        }
        if ch == '\n' && previous_cr {
            previous_cr = false;
            continue;
        }
        previous_cr = ch == '\r';
        out.push(if ch.is_control() { ' ' } else { ch });
        count += 1;
    }
    out
}

pub(crate) fn validate_text(bytes: &[u8], max_bytes: usize) -> io::Result<String> {
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("clipboard text exceeds {max_bytes} bytes"),
        ));
    }
    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "clipboard text contains NUL",
        ));
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "clipboard text is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_paste_normalizes_controls_and_caps_chars() {
        assert_eq!(
            single_line("alpha\r\nbeta\ngamma\tdelta", 32),
            "alpha beta gamma delta"
        );
        assert_eq!(single_line("éclair", 3), "écl");
        assert_eq!(single_line("abcdef", 0), "");
    }

    #[test]
    fn clipboard_text_is_utf8_nul_free_and_size_bounded() {
        assert_eq!(validate_text(b"hello", 5).expect("valid"), "hello");
        assert!(validate_text(b"toolarge", 4).is_err());
        assert!(validate_text(&[0xff], 4).is_err());
        assert!(validate_text(b"a\0b", 4).is_err());
    }

    #[test]
    fn oversized_copy_is_rejected_before_native_clipboard_io() {
        let error = copy_text(&"x".repeat(MAX_CLIPBOARD_BYTES + 1)).expect_err("oversized copy");
        assert!(error.to_string().contains("exceeds"), "{error}");
    }

    /// End-to-end copy round-trip through the native Wayland clipboard.
    #[ignore = "needs a live Wayland compositor"]
    #[test]
    fn clipboard_round_trip() {
        copy_text("ark clipboard round trip").expect("copy succeeds");
        let pasted = paste_text(1024).expect("paste succeeds");
        assert_eq!(pasted.trim_end_matches('\n'), "ark clipboard round trip");
    }
}
