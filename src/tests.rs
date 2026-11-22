//! Crate-root test module (declared by main.rs): the app-level event
//! routing pins that need main.rs's private helpers.

use super::*;
use crate::fs::testutil::temp_dir;
use std::fs;
use std::thread;
use std::time::Duration;

/// The M5 fs pipeline at the App level (no window needed): one coalesced
/// signal refreshes the OPEN viewer immediately (leading-edge stat check,
/// read position carried, flash armed) and flags the tree re-list, which
/// runs at its debounce due point.
#[test]
fn fs_change_refreshes_viewer_and_flags_tree() {
    let d = temp_dir("fs-change");
    let p = d.path().join("open.txt");
    fs::write(&p, "one\ntwo\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    // Simulate a double-click open (the cache holds the viewer; the frame
    // would render it).
    app.buffers.get_or_open(&p).expect("open");
    app.open_path = Some(p.clone());
    {
        let v = app.buffers.get_or_open(&p).expect("open");
        assert!(v.set_offset_lines(1, 1), "read position into range");
    }
    assert_eq!(app.tree.visible_count(), 1);

    // fs activity: rewrite the open file (mtime+len change) and add a file.
    thread::sleep(Duration::from_millis(30));
    fs::write(&p, "one\ntwo\nthree\n").expect("rewrite");
    thread::sleep(Duration::from_millis(30));
    fs::File::create(d.path().join("new.txt")).expect("create new file");

    // One signal: the viewer refreshes NOW; the tree is flagged for its
    // debounced re-list.
    assert!(app.process_fs_change(), "the open viewer changed on disk");
    {
        let v = app.buffers.get_or_open(&p).expect("refreshed viewer");
        assert_eq!(v.line_count(), 3, "content re-read from disk");
        assert_eq!(v.offset_lines(), 1, "read position carried across the refresh");
        assert!(v.selection().is_none(), "selection dropped with the old view");
    }
    assert!(app.flash_until.is_some(), "change flash armed");
    assert!(app.fs_dirty, "tree re-list flagged");

    // At the debounce due point the tree re-lists and the flag clears.
    assert!(app.refresh_tree_if_due(Instant::now() + TREE_REFRESH_DEBOUNCE));
    assert_eq!(app.tree.visible_count(), 2, "new file appears in the tree");
    assert!(!app.fs_dirty);
    // A second due-point check without new activity does nothing.
    assert!(!app.refresh_tree_if_due(Instant::now() + 2 * TREE_REFRESH_DEBOUNCE));

    // No viewer open → the same signal only flags the tree.
    let d2 = temp_dir("fs-change-noviewer");
    let mut app2 = App::new(d2.path().to_path_buf(), false, None, None);
    fs::File::create(d2.path().join("x.txt")).expect("create");
    assert!(!app2.process_fs_change(), "no open viewer → no viewer change");
    assert!(app2.refresh_tree_if_due(Instant::now()));
    assert_eq!(app2.tree.visible_count(), 1);
}

/// The M4 viewer wheel contract: pixel deltas convert to whole lines, a
/// moving scroll marks ONLY the viewer pane rect, and a clamped scroll marks
/// nothing.
#[test]
fn viewer_wheel_marks_pane_damage_only() {
    let d = temp_dir("wheel-viewer");
    let p = d.path().join("lines.txt");
    fs::write(&p, (0..40).map(|i| format!("line {i}\n")).collect::<String>()).expect("write");
    let mut viewer = Viewer::open(&p).expect("opens");
    let layout = frame::app_layout(1024.0, 768.0, 280.0); // pane 720×632
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // One notch (40px) → round(40/18.9) = 2 lines; damage = the pane rect.
    assert!(route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(viewer.offset_lines(), 2);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(rects, vec![damage_rect(layout.viewer_pane, 1.0, 1024, 768)]);

    // Notch to the clamp (40 lines − 33 viewport = 7), then one more notch
    // moves nothing → no damage at all.
    while route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(viewer.offset_lines(), 7, "max offset = line_count − viewport_lines");
    assert!(!route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert!(damage.take(canvas.width(), canvas.height()).is_none(), "clamped: nothing pending");
}

#[test]
fn wheel_routes_line_delta_only() {
    use winit::dpi::PhysicalPosition;

    // Wheel down (winit y = −1) → offset increases by 40 logical px.
    assert_eq!(wheel_scroll_px(&MouseScrollDelta::LineDelta(0.0, -1.0)), Some(WHEEL_PX));
    // Wheel up → offset decreases.
    assert_eq!(wheel_scroll_px(&MouseScrollDelta::LineDelta(0.0, 2.0)), Some(-2.0 * WHEEL_PX));
    // PixelDelta (touchpads) is not handled in M3.
    assert_eq!(
        wheel_scroll_px(&MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 40.0))),
        None
    );
}

/// The M3 damage contract at the routing level: a tree scroll marks the
/// panel + status rects ONLY (never full-frame), and a clamped scroll
/// marks nothing.
#[test]
fn wheel_scroll_marks_panel_damage_only() {
    let d = temp_dir("wheel-damage");
    for i in 0..40 {
        fs::File::create(d.path().join(format!("f{i:02}.txt"))).expect("create file");
    }
    let mut tree = FileTree::new(d.path());
    let layout = frame::app_layout(1024.0, 768.0, 280.0); // viewport 632px
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // First notch scrolls → exactly two rects, matching the layout.
    assert!(route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(tree.offset, WHEEL_PX);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(
        rects,
        vec![
            damage_rect(layout.tree_panel, 1.0, 1024, 768),
            damage_rect(layout.status, 1.0, 1024, 768),
        ]
    );

    // Notch to the max offset through the router: every moving notch marks
    // the same two rects.
    while route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(tree.offset, 248.0, "max offset = content (40×22) − viewport (632)");

    // One notch past the clamp moves nothing → no damage at all.
    assert!(!route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert!(
        damage.take(canvas.width(), canvas.height()).is_none(),
        "clamped: nothing pending"
    );

    // Scale 2 doubles the rect and clamps to the canvas (632×2 extends past
    // 768), never yielding out-of-bounds damage.
    assert_eq!(
        damage_rect(layout.tree_panel, 2.0, 1024, 768),
        DamageRect { x: 16, y: 176, width: 528, height: 592 }
    );
}

/// The M8 commit-panel wheel: whole rows, clamped to full pages.
#[test]
fn commit_panel_wheel_scrolls_rows_to_full_pages() {
    use crate::git::parse_status;
    let mut panel = commit::CommitPanel::default();
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 280.0, true);
    assert_eq!(commit::layout(layout.commit_panel).rows, 6);

    // One notch = 40px / 20px rows = 2 rows.
    assert!(route_wheel_commit(&mut panel, WHEEL_PX, layout.commit_panel, 20));
    assert_eq!(panel.scroll, 2);
    assert!(route_wheel_commit(&mut panel, 10_000.0, layout.commit_panel, 20));
    assert_eq!(panel.scroll, 14, "last page stays full (20 − 6)");
    assert!(!route_wheel_commit(&mut panel, WHEEL_PX, layout.commit_panel, 20));
    assert!(route_wheel_commit(&mut panel, -10_000.0, layout.commit_panel, 20));
    assert_eq!(panel.scroll, 0);

    // A status smaller than the window never scrolls.
    let status = parse_status("? a.txt\n");
    assert_eq!(status.entries.len(), 1);
    assert!(!route_wheel_commit(&mut panel, WHEEL_PX, layout.commit_panel, status.entries.len()));
}

/// M8: applying a new status snapshot prunes stale unchecks and leaves live
/// ones in place (bounded panel state).
#[test]
fn app_commit_selection_survives_and_prunes_with_status() {
    use crate::git::parse_status;
    let d = temp_dir("app-commit-retain");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    let first = Arc::new(parse_status("? a.txt\n? b.txt\n").rooted(d.path()));
    app.apply_git_status(Some(first));
    app.commit.toggle(Path::new("b.txt"));
    let status = app.git.as_ref().expect("status applied");
    assert_eq!(
        app.commit.commit_command(status),
        Err(commit::CommitError::EmptyMessage),
        "message is still required"
    );
    app.commit.message.insert("test: selection");
    assert_eq!(
        app.commit.commit_command(status).expect("valid"),
        "git add -- 'a.txt' && git commit -m 'test: selection'"
    );

    // A b-only snapshot keeps the live uncheck but drops the vanished path.
    let second = Arc::new(parse_status("? b.txt\n? c.txt\n").rooted(d.path()));
    app.apply_git_status(Some(second));
    let status = app.git.as_ref().expect("status applied");
    assert!(app.commit.is_checked(Path::new("a.txt")), "stale uncheck pruned");
    assert!(!app.commit.is_checked(Path::new("b.txt")), "live uncheck kept");
    assert_eq!(
        app.commit.commit_command(status).expect("valid"),
        "git add -- 'c.txt' && git commit -m 'test: selection'"
    );
}

/// M9: App-level config routing — whitelisted files enter the config pane,
/// code files are refused by the editor boundary, save/reload go through the
/// App, and closing returns to the viewer.
#[test]
fn app_routes_whitelisted_configs_to_the_enforced_editor() {
    let d = temp_dir("app-config-route");
    let env = d.path().join(".env");
    std::fs::write(&env, "A=1\n").expect("write env");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "a: 1\n").expect("write yaml");
    let code = d.path().join("main.rs");
    std::fs::write(&code, "fn main() {}\n").expect("write code");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);

    app.open_config(env.clone());
    assert_eq!(app.pane, PaneMode::Config);
    assert!(app.config_error.is_none());
    assert_eq!(
        app.config_edits.as_ref().map(|e| e.kind()),
        Some(crate::configed::ConfigKind::Env)
    );
    assert_eq!(app.pane_after_terminals(), PaneMode::Config, "terminal close returns to config");

    // Text mode edit + App-level save (atomic boundary).
    app.open_config(yaml.clone());
    assert!(app.config_edits.as_mut().expect("yaml editor").on_ime_commit("b: 2\n"));
    app.save_config();
    assert_eq!(std::fs::read_to_string(&yaml).expect("read"), "a: 1\nb: 2\n");
    assert!(!app.config_edits.as_ref().expect("editor").is_dirty());

    // A code file is refused at open; the pane shows the error, never edits.
    app.open_config(code.clone());
    assert_eq!(app.pane, PaneMode::Config);
    assert!(app.config_edits.is_none());
    assert!(app.config_error.as_deref().unwrap_or("").contains("whitelisted"));
    assert_eq!(std::fs::read_to_string(&code).expect("read"), "fn main() {}\n");

    app.close_config();
    assert_eq!(app.pane, PaneMode::Viewer);
    assert!(app.config_edits.is_none());
    assert!(app.config_error.is_none());
}

/// M9: saving `ark.toml` itself applies the new whitelist immediately (and
/// the enforced editor then refuses files it no longer allows).
#[test]
fn app_reloads_whitelist_after_saving_ark_toml() {
    let d = temp_dir("app-ark-toml");
    let ark = d.path().join("ark.toml");
    std::fs::write(&ark, "editable = [\".env*\", \"*.yaml\", \"*.toml\"]\n").expect("write");
    let env = d.path().join(".env");
    std::fs::write(&env, "A=1\n").expect("write env");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    assert_eq!(app.settings.editable, vec![".env*", "*.yaml", "*.toml"]);

    app.open_config(ark.clone());
    let mut mods = winit::keyboard::ModifiersState::empty();
    mods |= winit::keyboard::ModifiersState::CONTROL;
    let editor = app.config_edits.as_mut().expect("ark.toml editor");
    assert!(editor.handle_key(
        &winit::keyboard::Key::Character("a".into()),
        Some("a"),
        &mods
    ));
    assert!(editor.on_ime_commit("editable = [\"*.yaml\"]\n"));
    app.save_config();
    assert_eq!(app.settings.editable, vec!["*.yaml"], "whitelist reloaded on save");

    // `.env` is no longer human-writable: the editor refuses, code viewer path
    // is what the UI uses next.
    app.open_config(env);
    assert!(app.config_edits.is_none());
    assert!(app.config_error.is_some());
}

/// M7 regression (found during M8): the per-file diff subprocess must run in
/// the project root, not with the FILE as its working directory. The old
/// `diff_file(&path, &path)` failed at spawn (current_dir=file), was
/// swallowed by `unwrap_or_default`, and silently dropped all gutter marks.
#[test]
fn app_diff_refresh_runs_in_the_project_root() {
    let d = temp_dir("app-diff-root");
    let p = d.path().join("f.txt");
    std::fs::write(&p, "one\ntwo\nthree\n").expect("write");
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
    std::fs::write(&p, "one\nTWO\nthree\n").expect("edit");

    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.git = Some(Arc::new(crate::git::status(d.path()).expect("status")));
    app.open_path = Some(p.clone());
    app.request_diff_refresh(); // no proxy → runs inline

    let (path, diff) = app.file_diff.as_ref().expect("diff applied");
    assert_eq!(path, &p);
    assert_eq!(
        diff.marks,
        vec![(2, crate::git::GutterMark::Modified)],
        "the edited line's gutter mark must survive the integration"
    );
}
