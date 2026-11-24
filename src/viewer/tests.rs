//! Viewer state-model tests (the panel pixel pins live in
//! viewer/panel_tests.rs).

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use std::fs;
use std::io::Write;
use std::thread;
use std::time::Duration;

/// Fixture: a text file with known content, including a CRLF line, an empty
/// line, and NO trailing newline on the last line.
///
/// ```text
/// alpha\r\n
/// beta\r\n      <- trailing \r stripped in display space
/// (empty)\n
/// gamma         <- no trailing newline
/// ```
fn text_fixture(label: &str) -> (TempDir, PathBuf) {
    let d = temp_dir(label);
    let p = d.path().join("sample.txt");
    fs::File::create(&p)
        .unwrap()
        .write_all(b"alpha\r\nbeta\r\n\ngamma")
        .unwrap();
    (d, p)
}

// ---- open + line index ----

#[test]
fn open_indexes_lines_and_normalizes_display() {
    let (_d, p) = text_fixture("index");
    let v = Viewer::open(&p).expect("opens");
    assert_eq!(v.line_count(), 4, "4 lines, no trailing-newline phantom line");
    assert!(!v.truncated());
    assert_eq!(v.line_text(0).as_deref(), Some("alpha"));
    assert_eq!(v.line_text(1).as_deref(), Some("beta"), "trailing \\r stripped");
    assert_eq!(v.line_text(2).as_deref(), Some(""), "empty middle line");
    assert_eq!(v.line_text(3).as_deref(), Some("gamma"), "no trailing newline");
    assert!(v.line_text(4).is_none(), "out of range");
    assert_eq!(v.max_offset_lines(2), 2);
}

#[test]
fn open_detects_extensionless_shebang_language() {
    let d = temp_dir("shebang");
    let p = d.path().join("script");
    fs::write(&p, "#!/usr/bin/env python3\nprint('hi')\n").expect("write");
    let v = Viewer::open(&p).expect("opens");
    assert_eq!(v.language(), Lang::PythonLike);
}

#[test]
fn open_detects_shebang_without_reading_an_unbounded_first_line() {
    let d = temp_dir("shebang-long");
    let p = d.path().join("script");
    let mut source = String::from("#!/usr/bin/env python3 ");
    source.push_str(&"x".repeat(64 * 1024));
    source.push('\n');
    fs::write(&p, source).expect("write");
    let v = Viewer::open(&p).expect("opens");
    assert_eq!(v.language(), Lang::PythonLike);
}

#[test]
fn open_missing_is_an_io_error() {
    let d = temp_dir("missing");
    let p = d.path().join("not-there");
    assert!(matches!(Viewer::open(&p), Err(ViewerError::Io(_))));
}

#[test]
fn binary_files_are_refused() {
    let d = temp_dir("binary");
    let p = d.path().join("blob.bin");
    // NUL within the sniff window → refusal.
    fs::File::create(&p).unwrap().write_all(&[b'a', 0, b'b']).unwrap();
    assert!(
        matches!(Viewer::open(&p), Err(ViewerError::Binary { .. })),
        "NUL in the first sniff window refuses the file"
    );

    // NUL AFTER the sniff window → allowed (rendered lossy).
    let p2 = d.path().join("late-nul.txt");
    let mut f = fs::File::create(&p2).unwrap();
    f.write_all(&vec![b'x'; BINARY_SNIFF_BYTES + 8]).unwrap();
    f.write_all(&[0]).unwrap();
    f.flush().unwrap();
    assert!(Viewer::open(&p2).is_ok(), "NUL past the sniff window is not binary");
}

#[test]
fn size_cap_truncates_with_flag() {
    let d = temp_dir("cap");
    let p = d.path().join("big.txt");
    fs::File::create(&p)
        .unwrap()
        .write_all(b"line one\nline two\nline three\n")
        .unwrap();

    // Cap inside line two: the view ends after "line" (13 bytes), flag set.
    let v = Viewer::open_with_cap(&p, 13).expect("opens under cap");
    assert!(v.truncated(), "view region smaller than the file → truncated");
    assert_eq!(v.line_count(), 2, "lines indexed only within the view region");
    assert_eq!(v.line_text(0).as_deref(), Some("line one"));
    assert_eq!(v.line_text(1).as_deref(), Some("line"), "view cut mid-line");
    assert!(v.line_text(2).is_none());

    // Cap at the exact file size: nothing truncated.
    let v = Viewer::open_with_cap(&p, 30).expect("opens");
    assert!(!v.truncated());
    assert_eq!(v.line_count(), 3);

    // Empty file: zero lines, never panics.
    let e = d.path().join("empty.txt");
    fs::File::create(&e).unwrap();
    let v = Viewer::open(&e).expect("opens empty");
    assert_eq!(v.line_count(), 0);
    assert_eq!(v.max_offset_lines(10), 0);
}

// ---- scroll ----

#[test]
fn scroll_clamps_to_line_count() {
    let (_d, p) = text_fixture("scroll");
    let mut v = Viewer::open(&p).expect("opens");
    assert!(v.set_offset_lines(100, 2), "overshoot clamps to max and moves");
    assert_eq!(v.offset_lines(), 2, "max = 4 − 2");
    assert!(!v.scroll_by_lines(1, 2), "at max: no movement");
    assert!(v.scroll_by_lines(-1, 2), "back up moves");
    assert_eq!(v.offset_lines(), 1);
    assert!(v.scroll_by_lines(-100, 2), "undershoot clamps at 0");
    assert_eq!(v.offset_lines(), 0);
    assert!(!v.set_offset_lines(0, 2), "already at 0");

    // Empty viewport (collapsed panel): nothing can move.
    let mut v2 = Viewer::open(&text_fixture("scroll2").1).unwrap();
    assert!(!v2.scroll_by_lines(1, 0));
    assert_eq!(v2.offset_lines(), 0);
}

// ---- selection ----

#[test]
fn horizontal_scroll_clamps_to_the_longest_line() {
    let d = temp_dir("horizontal-scroll");
    let p = d.path().join("long.txt");
    fs::write(&p, "abcdefghij\nxy\n").expect("write");
    let mut v = Viewer::open(&p).expect("opens");
    assert_eq!(v.max_line_chars(), 10);
    assert_eq!(v.max_horizontal_offset(4), 6);
    assert!(v.set_horizontal_offset(100, 4));
    assert_eq!(v.horizontal_offset(), 6);
    assert!(
        v.set_horizontal_offset(6, 8),
        "narrowing the pane re-clamps"
    );
    assert_eq!(v.horizontal_offset(), 2);
    assert!(v.set_horizontal_offset(2, 20), "no overflow resets to zero");
    assert_eq!(v.horizontal_offset(), 0);
}

#[test]
fn selection_clamps_and_orders() {
    let (_d, p) = text_fixture("select");
    let mut v = Viewer::open(&p).expect("opens");

    // Out-of-range positions clamp to the content; a clamped single-point
    // selection is zero-length and reports None.
    v.select_at(99, 999);
    assert_eq!(v.selection(), None, "zero-length selection is None");
    assert_eq!(v.selection_len(), 0);

    // Extend past the anchor: ordering normalizes.
    v.select_at(0, 1);
    v.extend_select(1, 2);
    assert_eq!(v.selection(), Some(((0, 1), (1, 2))), "ordered start ≤ end");

    // Extend backwards past the anchor: still ordered.
    v.select_at(1, 2);
    v.extend_select(0, 1);
    assert_eq!(v.selection(), Some(((0, 1), (1, 2))), "reverse drag orders the same span");
    assert_eq!(v.selection_len(), 7, "\"lpha\" + \\n + \"be\" = 4 + 1 + 2 chars");
}

#[test]
fn selection_text_extracts_lines_and_spans() {
    let (_d, p) = text_fixture("seltext");
    let mut v = Viewer::open(&p).expect("opens");

    // Full-line span (0,0 → 2,0): "alpha\nbeta\n" — ends at the START of line 2.
    v.select_at(0, 0);
    v.extend_select(2, 0);
    assert_eq!(v.selection_text().as_deref(), Some("alpha\nbeta\n"), "CR normalized away");

    // Partial spans on both ends.
    v.select_at(0, 2);
    v.extend_select(1, 2);
    assert_eq!(v.selection_text().as_deref(), Some("pha\nbe"), "char columns map per line");

    // Single partial line.
    v.select_at(3, 1);
    v.extend_select(3, 4);
    assert_eq!(v.selection_text().as_deref(), Some("amm"), "no trailing newline needed");

    // Collapse.
    v.collapse_selection();
    assert_eq!(v.selection_text(), None);
    assert_eq!(v.selection_len(), 0);
}

#[test]
fn pos_at_clamps_to_line_and_col() {
    let (_d, p) = text_fixture("pos");
    let v = Viewer::open(&p).expect("opens");
    assert_eq!(v.pos_at(0, 3), (0, 3));
    assert_eq!(v.pos_at(0, 99), (0, 5), "col clamps to the line's char count");
    assert_eq!(v.pos_at(99, 0), (3, 0), "line clamps to the last line");
    assert_eq!(v.pos_at(99, 99), (3, 5));
}

// ---- multi-line char selection with unicode ----

#[test]
fn selection_counts_chars_not_bytes() {
    let d = temp_dir("unicode");
    let p = d.path().join("u.txt");
    // café = 5 chars / 6 bytes; naïve = 6 chars / 7 bytes.
    fs::File::create(&p).unwrap().write_all("café\nnaïve".as_bytes()).unwrap();
    let mut v = Viewer::open(&p).expect("opens");
    v.select_at(0, 3);
    v.extend_select(1, 2);
    assert_eq!(v.selection_len(), 4, "\"é\\nna\" = 2 + 1 + 2 chars");
    assert_eq!(v.selection_text().as_deref(), Some("é\nna"));
}

// ---- clipboard (needs a live Wayland compositor — run manually) ----

/// End-to-end copy round-trip: copy via the data-control protocol, read back
/// via the same protocol. Requires `WAYLAND_DISPLAY` (run on a real session:
/// `cargo test clipboard_round_trip -- --ignored`).
#[test]
#[ignore = "needs a live Wayland compositor"]
fn clipboard_round_trip() {
    copy_to_clipboard("ark clipboard round trip").expect("copy succeeds");
    let (mut pipe, _) = wl_clipboard_rs::paste::get_contents(
        wl_clipboard_rs::paste::ClipboardType::Regular,
        wl_clipboard_rs::paste::Seat::Unspecified,
        wl_clipboard_rs::paste::MimeType::Text,
    )
    .expect("paste succeeds");
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut pipe, &mut buf).expect("read pipe");
    assert_eq!(buf.trim_end_matches('\n'), "ark clipboard round trip");
}

// ---- disk-change detection + read-position carry (M5) ----

#[test]
fn changed_on_disk_tracks_len_mtime_and_vanishing() {
    let d = temp_dir("changed");
    let p = d.path().join("watched.txt");
    fs::write(&p, "one\n").expect("write");
    let v = Viewer::open(&p).expect("opens");
    assert!(!v.changed_on_disk(), "a fresh view matches disk");

    // Same length, different content → mtime differs.
    thread::sleep(Duration::from_millis(30));
    fs::write(&p, "two\n").expect("rewrite");
    assert!(v.changed_on_disk(), "mtime change detected");

    // Different length → detected even with a same-tick write.
    thread::sleep(Duration::from_millis(30));
    fs::write(&p, "one\ntwo\nthree\n").expect("grow");
    assert!(v.changed_on_disk(), "length change detected");

    // Vanished file: stat fails → changed (the refresh attempt follows).
    fs::remove_file(&p).expect("remove");
    assert!(v.changed_on_disk(), "vanished file reports changed");
}

#[test]
fn restore_offset_carries_the_raw_read_position() {
    let (_d, p) = text_fixture("restore");
    let mut v = Viewer::open(&p).expect("opens");
    v.restore_offset(99);
    assert_eq!(v.offset_lines(), 99, "raw value stored; the next draw's self-heal clamps it");
    // A normal clamped setter still works afterwards.
    assert!(v.set_offset_lines(99, 2));
    assert_eq!(v.offset_lines(), 2, "max = 4 − 2");
}
