//! Text editor model tests: UTF-8 editing, selection, undo/redo, IME.

use super::*;

fn ed(text: &str) -> TextEditor {
    TextEditor::new(text.to_string(), 4096)
}

#[test]
fn insert_backspace_delete_are_utf8_safe() {
    let mut e = ed("aé\nb");
    assert_eq!(e.cursor(), (1, 1), "starts at the end");
    assert!(e.insert("λ"));
    assert_eq!(e.text(), "aé\nbλ");
    assert!(e.backspace());
    assert_eq!(e.text(), "aé\nb", "the 2-byte lambda is removed whole");
    assert!(e.backspace());
    assert_eq!(e.text(), "aé\n");
    assert!(!e.delete(), "caret at the end of the document");

    let mut e = ed("ab");
    e.set_cursor(0, 0, false);
    assert!(e.delete());
    assert_eq!(e.text(), "b");
    assert!(!e.backspace());
    assert!(e.insert("x"));
    assert_eq!(e.text(), "xb");
}

#[test]
fn selection_replaces_and_extracts() {
    let mut e = ed("hello world");
    e.set_cursor(0, 6, false);
    assert!(e.select_all());
    assert_eq!(e.selected_text(), Some("hello world"));
    assert_eq!(e.selected_range(), Some(0..11));
    assert!(e.insert("bye"));
    assert_eq!(e.text(), "bye");
    assert_eq!(e.selected_text(), None);

    // Shift+arrow extends; plain arrow collapses.
    let mut e = ed("abcd");
    e.set_cursor(0, 1, false);
    assert!(e.move_cursor(Motion::Right, true));
    assert!(e.move_cursor(Motion::Right, true));
    assert_eq!(e.selected_text(), Some("bc"));
    assert!(e.move_cursor(Motion::Left, false));
    assert_eq!(e.selected_text(), None);
    assert_eq!(e.cursor(), (0, 2));
}

#[test]
fn vertical_navigation_keeps_the_preferred_column() {
    let mut e = ed("abcdef\nab\nabcdef");
    e.set_cursor(0, 5, false);
    assert!(e.move_cursor(Motion::Down, false));
    assert_eq!(e.cursor(), (1, 2), "clamped to the short line");
    assert!(e.move_cursor(Motion::Down, false));
    assert_eq!(e.cursor(), (2, 5), "preferred column restored");
    assert!(e.move_cursor(Motion::Up, false));
    assert_eq!(e.cursor(), (1, 2));
    assert!(e.move_cursor(Motion::Home, false));
    assert_eq!(e.cursor(), (1, 0));
    assert!(e.move_cursor(Motion::End, false));
    assert_eq!(e.cursor(), (1, 2));
    assert!(e.move_cursor(Motion::PageDown, false));
    assert_eq!(e.cursor(), (2, 2));
}

#[test]
fn undo_redo_walks_edits_and_redo_clears_on_new_input() {
    let mut e = ed("");
    e.insert("a");
    e.insert("b");
    e.insert("c");
    assert_eq!(e.text(), "abc");
    assert!(e.undo());
    assert_eq!(e.text(), "ab");
    assert!(e.undo());
    assert_eq!(e.text(), "a");
    assert!(e.redo());
    assert_eq!(e.text(), "ab");
    assert!(e.insert("X"));
    assert_eq!(e.text(), "abX");
    assert!(!e.redo(), "new edit clears redo");
    assert!(e.undo());
    assert!(e.undo());
    assert!(e.undo());
    assert_eq!(e.text(), "", "undo reaches the original text");
    assert!(!e.undo());
}

#[test]
fn undo_is_bounded_by_ops_and_bytes() {
    let mut e = ed("");
    for _ in 0..(MAX_UNDO_OPS + 50) {
        e.insert("x");
    }
    let mut undos = 0;
    while e.undo() {
        undos += 1;
    }
    assert_eq!(undos, MAX_UNDO_OPS, "oldest edits fall off the cap");
    assert_eq!(e.text(), "x".repeat(50));

    let mut e = TextEditor::new(String::new(), MAX_UNDO_BYTES);
    let chunk = "y".repeat(100 * 1024);
    e.insert(&chunk);
    e.insert(&chunk);
    e.insert(&chunk);
    e.insert(&chunk);
    e.insert(&chunk);
    e.insert(&chunk);
    let mut undos = 0;
    while e.undo() {
        undos += 1;
    }
    assert!(undos <= 5, "delta bytes cap the undo memory: {undos}");
}

#[test]
fn growth_cap_and_line_accounting() {
    let mut e = TextEditor::new("abc".into(), 5);
    assert!(e.insert("de"));
    assert!(!e.insert("f"), "5-byte cap refuses growth");
    assert_eq!(e.text(), "abcde");
    assert_eq!(e.line_count(), 1);

    let mut e = TextEditor::new("abcde".into(), 32);
    assert!(e.insert("\nnext"));
    assert_eq!(e.line_count(), 2);
    assert_eq!(e.line(0), "abcde");
    assert_eq!(e.line(1), "next");
    assert_eq!(e.cursor(), (1, 4));
    assert_eq!(e.line_start(1), 6);
    // A trailing newline opens a final empty line for the caret.
    assert!(e.insert("\n"));
    assert_eq!(e.line_count(), 3);
    assert_eq!(e.line(2), "");
    assert_eq!(e.cursor(), (2, 0));
}

#[test]
fn scroll_and_hscroll_keep_the_caret_visible() {
    let mut e = ed(&(0..50).map(|i| format!("line {i}\n")).collect::<String>());
    e.set_cursor(40, 3, false);
    e.ensure_cursor_visible(10, 20);
    assert_eq!(e.scroll_line(), 31, "line 40 at the bottom of 31..41");
    assert_eq!(e.hscroll_col(), 0);
    assert!(e.scroll_by(5, 10));
    assert_eq!(e.scroll_line(), 36);
    assert!(e.scroll_by(10_000, 10));
    assert_eq!(e.scroll_line(), e.line_count() - 10, "clamped to a full page");

    // Horizontal scrolling uses the actual caret column.
    let mut e = ed(&format!("{}\n", "x".repeat(100)));
    e.set_cursor(0, 60, false);
    e.ensure_cursor_visible(10, 20);
    assert_eq!(e.scroll_line(), 0);
    assert_eq!(e.hscroll_col(), 41, "caret at the right edge");
}

#[test]
fn preedit_is_separate_until_commit() {
    let mut e = ed("ab");
    assert!(e.set_preedit("か"));
    assert_eq!(e.preedit(), Some("か"));
    assert_eq!(e.text(), "ab", "preedit never mutates the document");
    assert!(e.insert("か"));
    assert!(e.set_preedit(""));
    assert_eq!(e.preedit(), None);
    assert_eq!(e.text(), "abか");
}

#[test]
fn dirty_tracks_edits_and_resets_on_save() {
    let mut e = ed("a");
    assert!(!e.is_dirty());
    e.insert("b");
    assert!(e.is_dirty());
    e.mark_clean();
    assert!(!e.is_dirty());
    e.set_cursor(0, 0, false);
    assert!(!e.is_dirty(), "moving never dirties");
    e.insert("\n");
    assert!(e.is_dirty());
}
