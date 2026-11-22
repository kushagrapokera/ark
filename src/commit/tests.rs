//! Commit panel tests: pure model (default-checked selection, pruning,
//! scroll, validation, key routing), pinned layout, raster/hit-test pins.

use super::*;
use crate::fs::testutil::temp_dir;
use crate::git::parse_status;
use crate::ui::testutil::count_color_in;
use std::path::Path;

fn status_fixture() -> GitStatus {
    parse_status(
        "? two words.txt\n\
         1 .M N... 000000 100644 100644 111 222 src/main.rs\n\
         1 .D N... 000000 100644 100644 111 222 gone.txt\n",
    )
    .rooted(Path::new("/repo"))
}

fn panel_theme() -> Theme {
    Theme {
        bg: (0, 0, 0),
        bg_elevated: (10, 20, 30),
        fg: (255, 255, 255),
        fg_dim: (90, 90, 90),
        accent: (200, 220, 255),
        border: (60, 60, 70),
        selection: (40, 50, 60),
        syntax_keyword: (203, 166, 247),
        syntax_string: (166, 227, 161),
        syntax_number: (250, 179, 135),
        syntax_comment: (108, 112, 134),
        git_modified: (250, 179, 135),
        git_added: (166, 227, 161),
        git_deleted: (243, 139, 168),
        git_untracked: (148, 226, 213),
    }
}

// ---- pure model ----

#[test]
fn every_change_is_checked_by_default_and_toggles_by_path() {
    let status = status_fixture();
    let mut panel = CommitPanel::default();
    assert_eq!(panel.selected_paths(&status).len(), 3, "default: all checked");
    assert!(panel.is_checked(Path::new("src/main.rs")));

    panel.toggle(Path::new("src/main.rs"));
    assert!(!panel.is_checked(Path::new("src/main.rs")));
    assert_eq!(
        panel.selected_paths(&status),
        vec![PathBuf::from("two words.txt"), PathBuf::from("gone.txt")],
        "unchecked path excluded, status order kept"
    );
    assert!(panel.error.is_none());

    panel.toggle(Path::new("src/main.rs"));
    assert_eq!(panel.selected_paths(&status).len(), 3, "toggle restores");
}

#[test]
fn retain_prunes_unchecked_paths_and_clamps_scroll() {
    let status = status_fixture();
    let mut panel = CommitPanel::default();
    panel.toggle(Path::new("two words.txt"));
    panel.toggle(Path::new("vanished.txt")); // stale (not in the snapshot)
    panel.scroll = 99;
    panel.retain(&status);
    assert_eq!(panel.scroll, 2, "clamped to the last row");
    assert!(!panel.is_checked(Path::new("two words.txt")), "live uncheck kept");
    assert!(panel.is_checked(Path::new("vanished.txt")), "stale uncheck pruned");

    panel.retain(&parse_status(""));
    assert_eq!(panel.scroll, 0, "empty snapshot resets the window");
}

#[test]
fn scroll_clamps_to_full_pages() {
    let mut panel = CommitPanel::default();
    // 10 rows, 4 visible → max first row = 6.
    assert!(!panel.scroll_by(-1, 4, 10), "already at the top");
    assert!(panel.scroll_by(2, 4, 10));
    assert_eq!(panel.scroll, 2);
    assert!(panel.scroll_by(100, 4, 10));
    assert_eq!(panel.scroll, 6, "last page stays full");
    assert!(!panel.scroll_by(1, 4, 10), "clamped: no movement");
    assert!(panel.scroll_by(-100, 4, 10));
    assert_eq!(panel.scroll, 0);
    // Fewer rows than the window → nothing scrolls.
    assert!(!panel.scroll_by(1, 8, 3));
}

#[test]
fn commit_command_validates_then_quotes_selected_paths() {
    let status = status_fixture();
    let mut panel = CommitPanel::default();
    panel.toggle(Path::new("src/main.rs"));

    assert_eq!(panel.commit_command(&status), Err(CommitError::EmptyMessage));
    panel.message.insert("fix: it's selected");
    assert_eq!(
        panel.commit_command(&status).expect("valid"),
        "git add -- 'two words.txt' 'gone.txt' && git commit -m 'fix: it'\\''s selected'"
    );

    for entry in &status.entries {
        if panel.is_checked(&entry.path) {
            panel.toggle(&entry.path); // all off
        }
    }
    assert_eq!(panel.commit_command(&status), Err(CommitError::NoSelection));
    assert_eq!(CommitError::NoSelection.to_string(), "select a file to commit");
    assert_eq!(CommitError::EmptyMessage.to_string(), "commit message required");
}

#[test]
fn message_keys_edit_only_while_focused_and_clear_the_error() {
    use winit::keyboard::{Key, NamedKey};
    let mut panel = CommitPanel::default();
    assert!(!panel.handle_key(&Key::Character("a".into()), Some("a")), "unfocused ignores keys");

    panel.message.focused = true;
    assert!(panel.handle_key(&Key::Character("a".into()), Some("a")));
    assert!(panel.handle_key(&Key::Character("b".into()), Some("b")));
    // The space bar is a NAMED key in winit; its text must still insert.
    assert!(panel.handle_key(&Key::Named(NamedKey::Space), Some(" ")));
    assert_eq!(panel.message.text, "ab ");
    assert!(panel.handle_key(&Key::Named(NamedKey::Backspace), None));
    assert_eq!(panel.message.text, "ab");

    panel.error = Some("boom".into());
    assert!(panel.handle_key(&Key::Character("c".into()), Some("c")));
    assert_eq!(panel.message.text, "abc");
    assert!(panel.error.is_none(), "editing clears the validation message");

    // Control characters never insert; unknown named keys are ignored.
    assert!(!panel.handle_key(&Key::Character("x".into()), Some("\n")));
    assert!(!panel.handle_key(&Key::Named(NamedKey::ArrowUp), None));

    // Escape blurs.
    assert!(panel.handle_key(&Key::Named(NamedKey::Escape), None));
    assert!(!panel.message.focused);
    assert!(!panel.handle_key(&Key::Character("z".into()), Some("z")));
    assert_eq!(panel.message.text, "abc");
}

#[test]
fn elide_path_keeps_the_file_name_visible() {
    assert_eq!(elide_path("short.txt", 20), "short.txt");
    assert_eq!(elide_path("a/very/long/path/file.rs", 12), "...h/file.rs");
    assert_eq!(elide_path("abcdef", 3), "abc", "no room for the marker");
    assert_eq!(elide_path("abcdef", 0), "");
}

// ---- layout + raster ----

#[test]
fn layout_pins_panel_geometry() {
    let panel = Rect { x: 8.0, y: 484.0, w: 264.0, h: PANEL_H };
    let l = layout(panel);
    assert_eq!(l.title, Rect { x: 16.0, y: 492.0, w: 248.0, h: 22.0 });
    assert_eq!(l.list, Rect { x: 8.0, y: 514.0, w: 264.0, h: 134.0 });
    assert_eq!(l.rows, 6);
    assert_eq!(l.field, Rect { x: 16.0, y: 656.0, w: 248.0, h: 24.0 });
    assert_eq!(l.commit_button, Rect { x: 16.0, y: 688.0, w: 120.0, h: 24.0 });
    assert_eq!(l.push_button, Rect { x: 144.0, y: 688.0, w: 120.0, h: 24.0 });

    // Tiny panels never produce negative list heights or rows.
    let tiny = layout(Rect { x: 0.0, y: 0.0, w: 80.0, h: 60.0 });
    assert_eq!(tiny.rows, 0);
    assert!(tiny.list.h >= 0.0 && tiny.field.w >= 0.0);
}

#[test]
fn draw_paints_checks_badges_text_and_the_message_placeholder() {
    let status = status_fixture();
    let theme = panel_theme();
    let mut panel = CommitPanel::default();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(264, 236);
    let panel_rect = Rect { x: 0.0, y: 0.0, w: 264.0, h: 236.0 };
    canvas.fill(theme.bg);

    let effects = panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel_rect,
        &status,
        Pointer::default(),
    );
    assert_eq!(effects, PanelEffects::default(), "no click: nothing to dispatch");

    // Title (dim) + file-name ink (fg) + the message placeholder (dim).
    assert!(count_color_in(&canvas, theme.fg_dim, 8, 0, 256, 22) > 0, "title ink");
    assert!(count_color_in(&canvas, theme.fg, 0, 30, 264, 90) > 0, "file-name ink");
    assert!(
        count_color_in(&canvas, theme.fg_dim, 8, 172, 256, 196) > 0,
        "empty-message placeholder"
    );
    // Badge letters use the M7 git tokens.
    assert!(count_color_in(&canvas, theme.git_untracked, 0, 30, 264, 50) > 0, "U badge");
    assert!(count_color_in(&canvas, theme.git_modified, 0, 50, 264, 70) > 0, "M badge");
    assert!(count_color_in(&canvas, theme.git_deleted, 0, 70, 264, 90) > 0, "D badge");
    // All three boxes check-filled with accent (border + check ink cover
    // part of each 14×14 fill; measured 112 exact-accent px per box).
    let checked_accent = count_color_in(&canvas, theme.accent, 0, 30, 264, 90);
    assert!(checked_accent >= 3 * 100, "checked boxes: {checked_accent}");

    // Uncheck row 0 by clicking it: the row toggle is reported and the
    // checkbox stops painting an accent fill.
    let click_row = Pointer { x: 60.0, y: 40.0, down: false, click: true };
    let effects = panel.draw(&mut canvas, &mut text, &theme, 1.0, panel_rect, &status, click_row);
    assert!(effects.toggled && !effects.commit && !effects.push);
    assert!(!panel.is_checked(Path::new("two words.txt")));
    canvas.fill(theme.bg);
    panel.message.focused = false; // the row click blurred (no field hit)
    let _ = panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel_rect,
        &status,
        Pointer::default(),
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, 8, 33, 22, 47),
        0,
        "unchecked row 0 box has no accent fill"
    );
}

#[test]
fn draw_focuses_the_field_and_reports_button_clicks() {
    let status = status_fixture();
    let theme = panel_theme();
    let mut panel = CommitPanel::default();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(264, 236);
    let panel_rect = Rect { x: 0.0, y: 0.0, w: 264.0, h: 236.0 };

    // Click the message field → it gains focus and paints the accent ring.
    let field_click = Pointer { x: 100.0, y: 184.0, down: false, click: true };
    let effects = panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel_rect,
        &status,
        field_click,
    );
    assert!(panel.message.focused);
    assert_eq!(effects, PanelEffects::default());
    assert!(
        count_color_in(&canvas, theme.accent, 16, 172, 264, 196) > 0,
        "focused field ring"
    );

    // Buttons fire independently (Commit at 16..136, Push at 144..264).
    let commit_click = Pointer { x: 70.0, y: 216.0, down: false, click: true };
    let effects = panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel_rect,
        &status,
        commit_click,
    );
    assert!(effects.commit && !effects.push && !effects.toggled);
    let push_click = Pointer { x: 200.0, y: 216.0, down: false, click: true };
    let effects = panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel_rect,
        &status,
        push_click,
    );
    assert!(effects.push && !effects.commit && !effects.toggled);
}

#[test]
fn draw_shows_validation_error_in_the_title() {
    let status = status_fixture();
    let theme = panel_theme();
    let mut panel = CommitPanel {
        error: Some("commit message required".into()),
        ..CommitPanel::default()
    };
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(264, 236);
    panel.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        Rect { x: 0.0, y: 0.0, w: 264.0, h: 236.0 },
        &status,
        Pointer::default(),
    );
    assert!(
        count_color_in(&canvas, theme.git_deleted, 8, 0, 256, 22) > 0,
        "error title painted in the deleted/red token"
    );
}

#[test]
fn live_panel_command_commits_selected_files() {
    // The panel's validated command is the exact string the PTY tab runs.
    let d = temp_dir("panel-live-commit");
    std::fs::write(d.path().join("a.txt"), "a\n").expect("write");
    std::fs::write(d.path().join("b.txt"), "b\n").expect("write");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(d.path())
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@ark"]);
    git(&["config", "user.name", "test"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "init"]);
    std::fs::write(d.path().join("a.txt"), "a changed\n").expect("edit a");
    std::fs::write(d.path().join("b.txt"), "b changed\n").expect("edit b");

    let status = crate::git::status(d.path()).expect("status");
    let mut panel = CommitPanel::default();
    panel.toggle(Path::new("b.txt")); // commit only a.txt
    panel.message.insert("feat: panel commit");
    let command = panel.commit_command(&status).expect("valid command");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(d.path())
        .output()
        .expect("sh runs");
    assert!(out.status.success(), "panel command failed: {command}\n{}", String::from_utf8_lossy(&out.stderr));

    assert_eq!(git(&["log", "-1", "--format=%s"]).trim(), "feat: panel commit");
    assert_eq!(git(&["show", "HEAD:a.txt"]), "a changed\n", "selected file committed");
    let after = crate::git::status(d.path()).expect("status after");
    assert_eq!(after.entries.len(), 1, "unselected b.txt remains modified");
    assert_eq!(after.entries[0].path, PathBuf::from("b.txt"));
}
