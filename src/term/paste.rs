//! Bounded clipboard-to-PTY encoding.

use alacritty_terminal::term::TermMode;

pub const MAX_PASTE_BYTES: usize = 64 * 1024;

/// Encode clipboard text for a PTY, using bracketed-paste markers when the
/// terminal has enabled them. Oversized payloads are rejected, never truncated.
pub fn encode_paste(mode: &TermMode, text: &str) -> Result<Vec<u8>, std::io::Error> {
    if text.len() > MAX_PASTE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("terminal paste exceeds {MAX_PASTE_BYTES} bytes"),
        ));
    }
    if !mode.contains(TermMode::BRACKETED_PASTE) {
        return Ok(text.as_bytes().to_vec());
    }
    let mut out = Vec::with_capacity(text.len() + 12);
    out.extend_from_slice(b"\x1b[200~");
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\x1b[201~");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_encoding_supports_bracketed_mode_and_enforces_the_cap() {
        let plain = TermMode::empty();
        assert_eq!(encode_paste(&plain, "a\nb").expect("plain paste"), b"a\nb");

        let bracketed = TermMode::BRACKETED_PASTE;
        assert_eq!(
            encode_paste(&bracketed, "a\nb").expect("bracketed paste"),
            b"\x1b[200~a\nb\x1b[201~"
        );
        assert!(encode_paste(&plain, &"x".repeat(64 * 1024)).is_ok());
        assert!(encode_paste(&plain, &"x".repeat(64 * 1024 + 1)).is_err());
    }
}
