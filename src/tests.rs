//! Crate-root test module (declared by main.rs): the app-level event
//! routing pins that need main.rs's private helpers.

use super::*;
use crate::fs::testutil::temp_dir;
use crate::new_item::NewItemKind;
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
    // Simulate a single-click open (the cache holds the viewer; the frame
    // would render it).
    app.open_viewer(p.clone(), 0);
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
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, true, true); // editor body 688×416
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // One notch (40px) → round(40/21.6) = 2 lines; damage = the pane rect.
    assert!(route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(viewer.offset_lines(), 2);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(rects, vec![damage_rect(layout.viewer_pane, 1.0, 1024, 768)]);

    // Notch to the clamp (40 lines − 19 viewport = 21), then one more notch
    // moves nothing → no damage at all.
    while route_wheel_viewer(&mut viewer, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(viewer.offset_lines(), 21, "max offset = line_count − viewport_lines");
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
    let layout = frame::app_layout(1024.0, 768.0, 304.0); // tree viewport 644px
    let canvas = Canvas::new(1024, 768);
    let mut damage = Damage::new(64);
    damage.take(canvas.width(), canvas.height()); // consume the initial full wave

    // First notch scrolls → exactly one panel rect, matching the layout.
    assert!(route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert_eq!(tree.offset, WHEEL_PX);
    let rects = damage.take(canvas.width(), canvas.height()).expect("scroll damage");
    assert_eq!(rects, vec![damage_rect(layout.tree_panel, 1.0, 1024, 768)]);

    // Notch to the max offset through the router: every moving notch marks
    // the same panel rect.
    while route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage) {
        let _ = damage.take(canvas.width(), canvas.height());
    }
    assert_eq!(
        tree.offset,
        40.0 * 22.0 - layout.tree_panel.h,
        "max offset = content − viewport"
    );

    // One notch past the clamp moves nothing → no damage at all.
    assert!(!route_wheel_scroll(&mut tree, WHEEL_PX, &layout, 1.0, &canvas, &mut damage));
    assert!(
        damage.take(canvas.width(), canvas.height()).is_none(),
        "clamped: nothing pending"
    );

    // Scale 2 doubles the rect and clamps to the canvas (644×2 extends past
    // 768), never yielding out-of-bounds damage.
    assert_eq!(
        damage_rect(layout.tree_panel, 2.0, 1024, 768),
        DamageRect { x: 144, y: 216, width: 448, height: 552 }
    );
}

/// The M8 commit-panel wheel: whole rows, clamped to full pages.
#[test]
fn commit_panel_wheel_scrolls_rows_to_full_pages() {
    use crate::git::parse_status;
    let mut panel = commit::CommitPanel::default();
    let panel_rect = Rect { x: 64.0, y: 400.0, w: 240.0, h: 236.0 };
    assert_eq!(commit::layout(panel_rect).rows, 6);

    // One notch = 40px / 20px rows = 2 rows.
    assert!(route_wheel_commit(&mut panel, WHEEL_PX, panel_rect, 20));
    assert_eq!(panel.scroll, 2);
    assert!(route_wheel_commit(&mut panel, 10_000.0, panel_rect, 20));
    assert_eq!(panel.scroll, 14, "last page stays full (20 − 6)");
    assert!(!route_wheel_commit(&mut panel, WHEEL_PX, panel_rect, 20));
    assert!(route_wheel_commit(&mut panel, -10_000.0, panel_rect, 20));
    assert_eq!(panel.scroll, 0);

    // A status smaller than the window never scrolls.
    let status = parse_status("? a.txt\n");
    assert_eq!(status.entries.len(), 1);
    assert!(!route_wheel_commit(&mut panel, WHEEL_PX, panel_rect, status.entries.len()));
}

/// M8: applying a new status snapshot prunes stale unchecks and leaves live
/// ones in place (bounded panel state).
#[test]
fn app_keyword_search_runs_inline_without_a_proxy() {
    let d = temp_dir("app-search-inline");
    fs::write(d.path().join("code.rs"), "fn Target() {}\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.search.input.text = "target".into();
    app.search.input.cursor = app.search.input.text.len();

    app.start_search("target".into());

    assert!(!app.search.running);
    assert_eq!(app.search.results.len(), 1);
    assert_eq!(app.search.results[0].path, d.path().join("code.rs"));
    assert_eq!(app.search.results[0].line, 1);
    assert!(app.search.status.contains("1 file"));
}

#[test]
fn app_keyword_search_rejects_stale_reports_and_runs_the_queued_query() {
    let d = temp_dir("app-search-stale");
    fs::write(d.path().join("old.txt"), "old\n").expect("write old");
    fs::write(d.path().join("new.txt"), "beta\n").expect("write new");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.search.input.text = "beta".into();
    app.search.input.cursor = app.search.input.text.len();
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    app.search_inflight = true;
    app.search_cancel = Some(cancel.clone());

    app.start_search("beta".into());
    assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
    assert!(app.search_queued.is_some());

    app.apply_search_report(search::SearchReport {
        generation: 0,
        query: "old".into(),
        results: vec![search::SearchResult {
            path: d.path().join("old.txt"),
            matches: 1,
            line: 1,
            snippet: "old".into(),
        }],
        scanned_files: 1,
        truncated: false,
        cancelled: false,
        error: None,
    });

    assert!(app.search_queued.is_none());
    assert!(!app.search_inflight);
    assert_eq!(app.search.results.len(), 1);
    assert_eq!(app.search.results[0].path, d.path().join("new.txt"));
}

#[test]
fn app_search_activity_focuses_the_keyword_field() {
    let d = temp_dir("app-search-focus");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.set_activity(frame::ActivityPanel::Search);
    assert_eq!(app.activity, frame::ActivityPanel::Search);
    assert!(app.search.input.focused);
    assert!(!app.term_focused);
}

#[test]
fn app_search_result_opens_at_the_matching_line() {
    let d = temp_dir("app-search-open");
    let path = d.path().join("lines.txt");
    fs::write(
        &path,
        (0..40)
            .map(|i| if i == 29 { "finalVerdict\n".to_owned() } else { format!("line {i}\n") })
            .collect::<String>(),
    )
    .expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.set_activity(frame::ActivityPanel::Search);
    app.search.input.text = "finalverdict".into();
    app.search.input.cursor = app.search.input.text.len();

    app.open_search_result(path.clone(), 30, 0);

    assert_eq!(app.active_path().as_deref(), Some(path.as_path()));
    assert_eq!(app.activity, frame::ActivityPanel::Search);
    assert!(!app.search.input.focused);
    let viewer = app.buffers.get_or_open(&path).expect("viewer");
    assert_eq!(viewer.offset_lines(), 29);
    let matched = viewer.search_match().expect("search result highlight");
    assert_eq!((matched.line, matched.start_col, matched.end_col), (29, 0, 12));
}

#[test]
fn app_normal_open_clears_the_search_highlight() {
    let d = temp_dir("app-search-highlight-clear");
    let path = d.path().join("code.rs");
    fs::write(&path, "needle\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(path.clone(), 0);
    assert!(app.buffers.get_or_open(&path).expect("viewer").set_search_match(1, "needle"));

    app.open_viewer(path.clone(), 0);

    assert!(app.buffers.get_or_open(&path).expect("viewer").search_match().is_none());
}

#[test]
fn app_keyword_search_ignores_obsolete_reports_from_an_older_worker() {
    let d = temp_dir("app-search-obsolete");
    fs::write(d.path().join("new.txt"), "new needle\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.search.input.text = "needle".into();
    app.search_generation = 2;
    app.search_inflight = true;
    let current_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    app.search_cancel = Some(current_cancel.clone());

    app.apply_search_report(search::SearchReport {
        generation: 1,
        query: "old".into(),
        results: Vec::new(),
        scanned_files: 0,
        truncated: false,
        cancelled: true,
        error: None,
    });

    assert!(app.search_inflight, "an unrelated stale report must not clear the current worker");
    assert!(app.search_cancel.is_some());
    assert!(!current_cancel.load(std::sync::atomic::Ordering::Relaxed));

    app.apply_search_report(search::SearchReport {
        generation: 2,
        query: "needle".into(),
        results: vec![search::SearchResult {
            path: d.path().join("new.txt"),
            matches: 1,
            line: 1,
            snippet: "new needle".into(),
        }],
        scanned_files: 1,
        truncated: false,
        cancelled: false,
        error: None,
    });
    assert!(!app.search_inflight);
    assert_eq!(app.search.results.len(), 1);
}

#[test]
fn app_search_result_uses_the_enforced_editor_for_config_files() {
    let d = temp_dir("app-search-config");
    let path = d.path().join("config.yaml");
    fs::write(&path, "keyword: true\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.set_activity(frame::ActivityPanel::Search);

    app.open_search_result(path.clone(), 1, 0);

    assert!(app.config_edits.is_some());
    assert_eq!(app.pane, PaneMode::Config);
    assert_eq!(app.activity, frame::ActivityPanel::Search);
}

#[test]
fn app_config_autosave_waits_for_debounce_then_saves() {
    let d = temp_dir("app-config-autosave");
    let path = d.path().join("config.yaml");
    fs::write(&path, "a: 1\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_config(path.clone());
    assert!(app.config_edits.as_mut().expect("editor").on_ime_commit("b: 2\n"));
    app.schedule_config_autosave();
    let due = app.config_autosave_at.expect("autosave scheduled");

    assert!(!app.autosave_config_if_due(due - Duration::from_nanos(1)));
    assert_eq!(fs::read_to_string(&path).expect("read"), "a: 1\n");
    assert!(app.autosave_config_if_due(due));
    assert_eq!(fs::read_to_string(&path).expect("read"), "a: 1\nb: 2\n");
    assert!(!app.config_edits.as_ref().expect("editor").is_dirty());
    assert!(app.config_autosave_at.is_none());
}

#[test]
fn app_config_autosave_does_not_overwrite_external_changes() {
    let d = temp_dir("app-config-autosave-conflict");
    let path = d.path().join("config.yaml");
    fs::write(&path, "a: 1\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_config(path.clone());
    assert!(app.config_edits.as_mut().expect("editor").on_ime_commit("mine\n"));
    app.schedule_config_autosave();
    let due = app.config_autosave_at.expect("autosave scheduled");
    thread::sleep(Duration::from_millis(20));
    fs::write(&path, "a: 2\n").expect("external edit");

    assert!(app.autosave_config_if_due(due));
    assert_eq!(fs::read_to_string(&path).expect("read"), "a: 2\n");
    let editor = app.config_edits.as_ref().expect("editor");
    assert!(editor.is_dirty());
    assert!(editor.external_change());
    assert!(app.config_autosave_at.is_none());
    assert!(app.config_edits.as_mut().expect("editor").on_ime_commit("more\n"));
    app.schedule_config_autosave();
    assert!(app.config_autosave_at.is_none(), "conflicts require explicit reload or save");
}

#[test]
fn app_closing_config_flushes_pending_autosave() {
    let d = temp_dir("app-config-autosave-close");
    let path = d.path().join("config.yaml");
    fs::write(&path, "a: 1\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_config(path.clone());
    assert!(app.config_edits.as_mut().expect("editor").on_ime_commit("b: 2\n"));
    app.schedule_config_autosave();

    app.close_config();

    assert_eq!(fs::read_to_string(&path).expect("read"), "a: 1\nb: 2\n");
    assert!(app.config_edits.is_none());
    assert!(app.config_autosave_at.is_none());
}

#[test]
fn app_markdown_preview_toggles_and_resets_on_tab_change() {
    let d = temp_dir("app-readme-preview");
    let readme = d.path().join("README.md");
    let code = d.path().join("main.rs");
    fs::write(&readme, "# Title\n\n**bold**\n").expect("write readme");
    fs::write(&code, "fn main() {}\n").expect("write code");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(readme.clone(), 0);

    app.toggle_markdown_preview();
    assert!(app.markdown.preview);
    app.markdown.scroll = 40.0;
    app.toggle_markdown_preview();
    assert!(!app.markdown.preview);
    assert_eq!(app.markdown.scroll, 0.0);

    app.toggle_markdown_preview();
    app.open_viewer(code.clone(), 0);
    assert!(!app.markdown.preview, "non-Markdown tabs reset formatted mode");
    assert_eq!(app.markdown.scroll, 0.0);
}

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

/// M12: LRU-backed tab selection and close routing.
#[test]
fn app_terminal_close_returns_focus_after_the_last_shell() {
    let d = temp_dir("app-terminal-close");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    let size = term::GridSize { cols: 80, rows: 24 };
    for label in ["first", "second"] {
        app.terms.tabs.push(
            term::Terminal::spawn_program_with(
                d.path(),
                std::path::Path::new("/bin/cat"),
                label,
                size,
                || {},
            )
            .expect("spawn test terminal"),
        );
    }
    app.terms.active = 1;
    app.activity = frame::ActivityPanel::Terminal;
    app.term_focused = true;

    app.close_terminal_tab();
    assert_eq!(app.terms.tabs.len(), 1);
    assert_eq!(app.terms.active, 0);
    assert!(app.term_focused, "remaining shell keeps keyboard focus");

    app.close_terminal_tab();
    assert!(!app.terms.is_open());
    assert_eq!(app.activity, frame::ActivityPanel::Explorer);
    assert!(!app.term_focused);
    assert_eq!(app.pane, PaneMode::Viewer);
}

#[test]
fn app_opening_a_visible_file_focuses_its_existing_group() {
    let d = temp_dir("app-split-focus");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    fs::write(&a, "a\n").expect("write a");
    fs::write(&b, "b\n").expect("write b");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(a.clone(), 0);
    app.toggle_editor_split();
    app.open_viewer(b.clone(), 1);
    assert_eq!(app.active_group_index(), 1);

    app.open_viewer(a.clone(), 1);
    assert_eq!(app.active_group_index(), 0);
    assert_eq!(app.active_path().as_deref(), Some(a.as_path()));
    assert_eq!(app.editor_groups.groups[1].tabs, vec![b]);
}

#[test]
fn app_closing_a_group_evicts_its_tabs_and_collapses() {
    let d = temp_dir("app-split-close");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    fs::write(&a, "a\n").expect("write a");
    fs::write(&b, "b\n").expect("write b");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(a.clone(), 0);
    app.toggle_editor_split();
    app.open_viewer(b.clone(), 1);

    app.close_editor_group(1);
    assert!(!app.editor_groups.is_split());
    assert_eq!(app.active_path().as_deref(), Some(a.as_path()));
    assert!(app.buffers.contains(&a));
    assert!(!app.buffers.contains(&b));
}

#[test]
fn app_config_is_a_tab_in_the_active_group() {
    let d = temp_dir("app-split-config");
    let code = d.path().join("main.rs");
    let config = d.path().join("config.yaml");
    fs::write(&code, "fn main() {}\n").expect("write code");
    fs::write(&config, "a: 1\n").expect("write config");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(code.clone(), 0);
    app.toggle_editor_split();
    app.open_config(config.clone());

    assert_eq!(app.active_group_index(), 1);
    assert_eq!(app.editor_groups.groups[1].tabs, vec![config.clone()]);
    assert_eq!(app.pane, PaneMode::Config);
    app.focus_editor_group(0);
    assert_eq!(app.pane, PaneMode::Viewer);
    app.select_tab(1, config);
    assert_eq!(app.pane, PaneMode::Config);
}

#[test]
fn app_config_error_tab_can_be_refocused_from_another_group() {
    let d = temp_dir("app-split-config-error");
    let code = d.path().join("main.rs");
    let viewer = d.path().join("a.txt");
    fs::write(&code, "fn main() {}\n").expect("write code");
    fs::write(&viewer, "a\n").expect("write viewer");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(viewer, 0);
    app.toggle_editor_split();
    app.open_config(code);
    app.focus_editor_group(0);
    app.set_activity(frame::ActivityPanel::Config);
    assert_eq!(app.active_group_index(), 1);
    assert_eq!(app.pane, PaneMode::Config);
}

#[test]
fn app_closing_group_with_config_closes_the_editor() {
    let d = temp_dir("app-split-close-config");
    let code = d.path().join("main.rs");
    let config = d.path().join("config.yaml");
    fs::write(&code, "fn main() {}\n").expect("write code");
    fs::write(&config, "a: 1\n").expect("write config");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(code, 0);
    app.toggle_editor_split();
    app.open_config(config);
    assert!(app.config_edits.is_some());

    app.close_editor_group(1);
    assert!(app.config_edits.is_none());
    assert!(!app.editor_groups.is_split());
    assert_eq!(app.pane, PaneMode::Viewer);
}

#[test]
fn app_fs_change_refreshes_both_visible_viewers() {
    let d = temp_dir("app-split-refresh");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    fs::write(&a, "a\n").expect("write a");
    fs::write(&b, "b\n").expect("write b");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(a.clone(), 0);
    app.toggle_editor_split();
    app.open_viewer(b.clone(), 1);
    thread::sleep(Duration::from_millis(30));
    fs::write(&a, "a\nmore\n").expect("edit a");
    fs::write(&b, "b\nmore\n").expect("edit b");

    assert!(app.process_fs_change());
    assert_eq!(app.buffers.get_or_open(&a).expect("a").line_count(), 2);
    assert_eq!(app.buffers.get_or_open(&b).expect("b").line_count(), 2);
}

#[test]
fn app_wheel_routes_to_the_requested_editor_group() {
    let d = temp_dir("app-split-wheel");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    let lines = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
    fs::write(&a, &lines).expect("write a");
    fs::write(&b, &lines).expect("write b");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(a.clone(), 0);
    app.toggle_editor_split();
    app.open_viewer(b.clone(), 1);
    let rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 400.0 };

    assert!(app.route_wheel_viewer_group(0, rect, WHEEL_PX, false));
    assert_eq!(app.buffers.get_or_open(&a).expect("a").offset_lines(), 2);
    assert_eq!(app.buffers.get_or_open(&b).expect("b").offset_lines(), 0);
    assert!(app.route_wheel_viewer_group(1, rect, WHEEL_PX, false));
    assert_eq!(app.buffers.get_or_open(&b).expect("b").offset_lines(), 2);
}

#[test]
fn app_shift_wheel_scrolls_the_viewer_horizontally() {
    let d = temp_dir("app-horizontal-wheel");
    let p = d.path().join("long.txt");
    fs::write(&p, format!("{}\n", "a".repeat(100))).expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(p.clone(), 0);
    let rect = Rect { x: 0.0, y: 0.0, w: 240.0, h: 200.0 };

    assert!(app.route_wheel_viewer_group(0, rect, WHEEL_PX, true));
    let viewer = app.buffers.get_or_open(&p).expect("viewer");
    assert!(viewer.horizontal_offset() > 0);
    assert_eq!(viewer.offset_lines(), 0);
}

#[test]
fn sidebar_handle_activity_pulse_decays_to_idle() {
    let now = Instant::now();
    assert_eq!(handle_activity_pulse(now, now), 1.0);
    assert!(
        (handle_activity_pulse(now, now - Duration::from_millis(400)) - 0.5).abs() < 0.001,
        "pulse fades linearly after input"
    );
    assert_eq!(handle_activity_pulse(now, now - HANDLE_PULSE), 0.0);
}

#[test]
fn app_new_item_uses_the_selected_folder_as_its_target() {
    let d = temp_dir("app-explorer-selected-parent");
    let parent = d.path().join("src");
    fs::create_dir(&parent).expect("create src");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.tree.select(&parent);

    app.request_new_item(NewItemKind::File);
    assert_eq!(
        app.new_item.as_ref().expect("prompt").parent,
        parent
    );
    assert!(app.tree.is_expanded(&parent), "inline prompt expands its target folder");
    let prompt = app.new_item.as_mut().expect("prompt");
    prompt.input.text = "notes.txt".into();
    prompt.input.cursor = prompt.input.text.len();
    app.submit_new_item();

    assert!(parent.join("notes.txt").is_file());
    assert!(!d.path().join("notes.txt").exists());
    assert!(app.tree.is_expanded(&parent), "target folder expands to reveal the new file");
}

#[test]
fn app_root_prompt_returns_the_tree_to_the_top() {
    let d = temp_dir("app-explorer-root-inline");
    for i in 0..40 {
        fs::write(d.path().join(format!("f{i:02}.txt")), "x\n").expect("write");
    }
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    assert!(app.tree.set_offset(100.0, 100.0));

    app.request_new_item(NewItemKind::File);
    assert_eq!(app.tree.offset, 0.0, "root inline field is visible at the top");
    assert_eq!(app.new_item.as_ref().expect("prompt").parent, d.path());
}

#[test]
fn app_new_item_uses_the_selected_files_parent() {
    let d = temp_dir("app-explorer-selected-file-parent");
    let parent = d.path().join("src");
    fs::create_dir(&parent).expect("create src");
    let file = parent.join("main.rs");
    fs::write(&file, "fn main() {}\n").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.tree.select(&file);

    app.request_new_item(NewItemKind::Folder);
    assert_eq!(
        app.new_item.as_ref().expect("prompt").parent,
        parent
    );
}

#[test]
fn app_new_item_does_not_treat_a_selected_symlink_as_a_folder() {
    use std::os::unix::fs::symlink;

    let d = temp_dir("app-explorer-selected-symlink");
    let target = d.path().join("src");
    fs::create_dir(&target).expect("create src");
    let link = d.path().join("link");
    symlink(&target, &link).expect("symlink");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.tree.select(&link);

    app.request_new_item(NewItemKind::File);
    assert_eq!(
        app.new_item.as_ref().expect("prompt").parent,
        d.path()
    );
    let prompt = app.new_item.as_mut().expect("prompt");
    prompt.input.text = "notes.txt".into();
    prompt.input.cursor = prompt.input.text.len();
    app.submit_new_item();

    assert!(d.path().join("notes.txt").is_file());
    assert!(!target.join("notes.txt").exists());
}

#[test]
fn app_prompt_keyboard_edits_and_submits_in_the_selected_folder() {
    use winit::keyboard::{Key, NamedKey};

    let d = temp_dir("app-explorer-prompt-keys");
    let parent = d.path().join("src");
    fs::create_dir(&parent).expect("create src");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.tree.select(&parent);
    app.request_new_item(NewItemKind::File);

    assert!(app.handle_new_item_key(
        &Key::Character("notes.txt".into()),
        Some("notes.txt")
    ));
    assert!(app.handle_new_item_key(&Key::Named(NamedKey::Home), None));
    assert_eq!(app.new_item.as_ref().expect("prompt").input.cursor, 0);
    assert!(!app.handle_new_item_key(&Key::Named(NamedKey::Backspace), None));
    assert_eq!(
        app.new_item.as_ref().expect("prompt").input.text,
        "notes.txt"
    );
    assert!(app.handle_new_item_key(&Key::Named(NamedKey::End), None));
    assert!(app.handle_new_item_key(&Key::Named(NamedKey::Backspace), None));
    assert!(app.handle_new_item_key(
        &Key::Character("t".into()),
        Some("t")
    ));
    assert!(app.handle_new_item_key(&Key::Named(NamedKey::Enter), None));

    assert!(parent.join("notes.txt").is_file());
    assert!(!d.path().join("notes.txt").exists());
    assert!(app.new_item.is_none());
}

#[test]
fn app_explorer_create_actions_create_root_contained_entries() {
    let d = temp_dir("app-explorer-create");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);

    app.request_new_item(NewItemKind::File);
    app.new_item.as_mut().expect("prompt").input.text = "notes.txt".into();
    app.new_item.as_mut().expect("prompt").input.cursor = 10;
    app.submit_new_item();
    assert!(d.path().join("notes.txt").is_file());
    assert!(app.new_item.is_none());

    app.request_new_item(NewItemKind::Folder);
    app.new_item.as_mut().expect("prompt").input.text = "new-folder".into();
    app.new_item.as_mut().expect("prompt").input.cursor = 10;
    app.submit_new_item();
    assert!(d.path().join("new-folder").is_dir());
    assert!(app.new_item.is_none());
}

#[test]
fn app_new_whitelisted_config_opens_the_enforced_editor() {
    let d = temp_dir("app-explorer-new-config");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.request_new_item(NewItemKind::File);
    let prompt = app.new_item.as_mut().expect("prompt");
    prompt.input.text = "config.yaml".into();
    prompt.input.cursor = prompt.input.text.len();
    app.submit_new_item();
    assert!(d.path().join("config.yaml").is_file());
    assert_eq!(app.pane, PaneMode::Config);
    assert!(app.config_edits.is_some());
}

#[test]
fn app_new_item_refuses_overwrite_and_keeps_the_prompt_error() {
    let d = temp_dir("app-explorer-overwrite");
    fs::write(d.path().join("exists.txt"), "keep").expect("write");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.request_new_item(NewItemKind::File);
    let prompt = app.new_item.as_mut().expect("prompt");
    prompt.input.text = "exists.txt".into();
    prompt.input.cursor = prompt.input.text.len();
    app.submit_new_item();
    assert!(app.new_item.is_some());
    assert!(app.new_item.as_ref().expect("prompt").error.is_some());
    assert_eq!(fs::read_to_string(d.path().join("exists.txt")).expect("read"), "keep");
}

#[test]
fn app_explorer_refresh_and_collapse_actions_update_the_tree() {
    let d = temp_dir("app-explorer-tree-actions");
    fs::create_dir_all(d.path().join("A/C")).expect("create A/C");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    let a = d.path().join("A");
    let c = d.path().join("A/C");
    assert!(app.tree.toggle(&a, true));
    assert!(app.tree.toggle(&c, true));

    app.collapse_explorer();
    assert_eq!(app.tree.expanded_dir_count(), 0);
    assert_eq!(app.tree.visible_count(), 1, "collapse keeps only root rows");

    fs::write(d.path().join("fresh.txt"), "fresh\n").expect("write fresh");
    app.refresh_explorer();
    assert!(app.tree.visible_rows().iter().any(|row| row.name == "fresh.txt"));
}

#[test]
fn terminal_output_selects_a_terminal_only_render_wave() {
    let d = temp_dir("app-terminal-render-wave");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    app.layout_visibility.terminal = true;
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            term::GridSize::new(80, 24),
            || {},
        )
        .expect("spawn test terminal"),
    );
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    assert!(matches!(app.take_render_wave(&layout), RenderWave::Scene(_)));
    app.damage.take(app.canvas.width(), app.canvas.height());

    app.terminal_dirty = true;
    assert_eq!(
        app.take_render_wave(&layout),
        RenderWave::Terminal(damage_rect(layout.term_grid, 1.0, 1024, 768))
    );
    assert!(!app.terminal_dirty, "the terminal wave consumes the dirty flag");
}

#[test]
fn scene_damage_takes_precedence_over_a_terminal_only_repaint() {
    let d = temp_dir("app-terminal-scene-wave");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    app.layout_visibility.terminal = true;
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            term::GridSize::new(80, 24),
            || {},
        )
        .expect("spawn test terminal"),
    );
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    app.damage.take(app.canvas.width(), app.canvas.height());
    let tree_damage = damage_rect(layout.tree_panel, 1.0, 1024, 768);
    app.damage.mark(tree_damage);
    app.terminal_dirty = true;

    assert_eq!(app.take_render_wave(&layout), RenderWave::Scene(vec![tree_damage]));
    assert!(!app.terminal_dirty, "a scene wave already paints the terminal");
}

#[test]
fn terminal_output_while_hovering_chrome_keeps_the_tooltip_layer() {
    let d = temp_dir("app-terminal-tooltip-wave");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    app.layout_visibility.terminal = true;
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            term::GridSize::new(80, 24),
            || {},
        )
        .expect("spawn test terminal"),
    );
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    app.damage.take(app.canvas.width(), app.canvas.height());
    app.pointer.x = layout.terminal_toolbar.x + 40.0;
    app.pointer.y = layout.terminal_toolbar.y + layout.terminal_toolbar.h / 2.0;
    app.terminal_dirty = true;

    assert_eq!(
        app.take_render_wave(&layout),
        RenderWave::Scene(vec![DamageRect {
            x: 0,
            y: 0,
            width: 1024,
            height: 768,
        }]),
        "PTY output must repaint the scene while a terminal-chrome tooltip is visible"
    );
}

#[test]
fn hidden_terminal_output_does_not_schedule_a_repaint() {
    let d = temp_dir("app-terminal-hidden-wave");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            term::GridSize::new(80, 24),
            || {},
        )
        .expect("spawn test terminal"),
    );
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, false, false);
    app.damage.take(app.canvas.width(), app.canvas.height());
    app.terminal_dirty = true;

    assert_eq!(app.take_render_wave(&layout), RenderWave::None);
    assert!(!app.terminal_dirty);
}

#[test]
fn app_terminal_divider_drag_resizes_and_clamps_the_dock() {
    let d = temp_dir("app-terminal-resize");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, true, true);
    app.pointer.x = layout.term_dock.x + 40.0;
    app.pointer.y = layout.term_dock.y;

    assert!(app.begin_terminal_resize(&layout));
    app.resize_terminal_to(layout.term_dock.y - 50.0, &layout);
    assert_eq!(app.term_height, Some(274.0));
    app.resize_terminal_to(10_000.0, &layout);
    assert_eq!(app.term_height, Some(120.0));
    app.resize_terminal_to(-1_000.0, &layout);
    assert_eq!(app.term_height, Some(548.0));
}

#[test]
fn app_terminal_divider_drag_coalesces_damage_and_redraws() {
    let d = temp_dir("app-terminal-resize-coalesce");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    let layout = frame::app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    app.damage.take(app.canvas.width(), app.canvas.height());
    app.pointer.x = layout.term_dock.x + 40.0;
    app.pointer.y = layout.term_dock.y;
    assert!(app.begin_terminal_resize(&layout));

    for y in [
        layout.term_dock.y - 20.0,
        layout.term_dock.y - 60.0,
        layout.term_dock.y - 100.0,
    ] {
        app.pointer.y = y;
        assert!(app.resize_terminal_to(y, &layout));
        app.mark_terminal_resize_damage(&layout);
    }
    assert!(app.term_drag.dirty, "movement queues one frame");
    assert!(app.take_terminal_resize_redraw());
    assert!(!app.take_terminal_resize_redraw(), "later wakes do not duplicate the redraw");

    let region = Rect {
        x: layout.editor_card.x,
        y: layout.workbench.y,
        w: layout.editor_card.w,
        h: layout.workbench.h,
    };
    let expected = vec![damage_rect(region, 1.0, 1024, 768)];
    assert_eq!(
        app.take_render_wave(&layout),
        RenderWave::Scene(expected.clone()),
        "the whole drag shares one bounded damage region per frame"
    );

    // Consuming one frame must re-arm the same region for later movement.
    app.pointer.y = layout.term_dock.y - 140.0;
    assert!(app.resize_terminal_to(app.pointer.y, &layout));
    app.mark_terminal_resize_damage(&layout);
    assert_eq!(
        app.take_render_wave(&layout),
        RenderWave::Scene(expected),
        "the next drag frame repaints instead of freezing after the first"
    );
}

#[test]
fn app_terminal_divider_release_flushes_only_the_final_pty_size() {
    let d = temp_dir("app-terminal-resize-flush");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.canvas.resize(1024, 768);
    app.layout_visibility.terminal = true;
    let layout_for = |app: &App| {
        frame::app_layout_with_visibility(
            1024.0,
            768.0,
            app.split.position,
            app.editor_groups.divider,
            app.editor_groups.groups.len(),
            app.layout_visibility,
            app.term_height,
        )
    };
    let initial_layout = layout_for(&app);
    let font_size = (UI_FONT_SIZE as f64 * app.scale * initial_layout.ui_scale) as f32;
    let m = app.text.mono_metrics(font_size, font_size * 1.35);
    let advance = f64::from(m.advance) / app.scale;
    let initial = term::GridSize::fit(
        initial_layout.term_grid.w,
        initial_layout.term_grid.h,
        advance,
        viewer::LINE_H,
    );
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            initial,
            || {},
        )
        .expect("spawn test terminal"),
    );
    app.pointer.x = initial_layout.term_dock.x + 40.0;
    app.pointer.y = initial_layout.term_dock.y - 4.0;
    assert!(app.begin_terminal_resize(&initial_layout));

    // One painted drag frame leaves the visual model at nine rows.
    assert!(app.resize_terminal_to(initial_layout.term_dock.y - 32.0, &initial_layout));
    let intermediate_layout = layout_for(&app);
    frame::draw_terminal_grid(
        &mut app.canvas,
        &mut app.text,
        &app.theme,
        app.scale,
        intermediate_layout.ui_scale,
        intermediate_layout.term_grid,
        &mut app.terms,
        false,
    );
    let intermediate = app.terms.active().expect("terminal").size();
    assert_eq!(intermediate.rows, 9);
    assert_eq!(app.terms.active().expect("terminal").pty_size(), initial);

    // Release arrives before the final visual frame. It must skip the stale
    // nine-row PTY size and send only the final eight-row size.
    assert!(app.resize_terminal_to(initial_layout.term_dock.y - 12.0, &initial_layout));
    let final_layout = layout_for(&app);
    assert!(app.finish_terminal_resize(Some(&final_layout)));
    assert!(!app.term_drag.dragging);
    assert_eq!(app.terms.active().expect("terminal").size().rows, 8);
    assert_eq!(
        app.terms.active().expect("terminal").pty_size(),
        app.terms.active().expect("terminal").size(),
        "release synchronizes the final grid without an intermediate SIGWINCH"
    );
    assert!(!app.finish_terminal_resize(None), "release without a drag is a no-op");
}

#[test]
fn app_layout_actions_toggle_explorer_and_terminal() {
    let d = temp_dir("app-layout-actions");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    assert!(app.layout_visibility.explorer);
    assert!(!app.layout_visibility.terminal, "terminal dock starts hidden");

    app.apply_layout_action(frame::LayoutAction::ToggleExplorer);
    app.apply_layout_action(frame::LayoutAction::ToggleTerminal);
    assert!(!app.layout_visibility.explorer);
    assert!(app.layout_visibility.terminal);
}

#[test]
fn app_terminal_button_reveals_the_dock_when_no_shell_exists() {
    let d = temp_dir("app-terminal-empty-toggle");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    assert!(!app.layout_visibility.terminal);

    app.apply_layout_action(frame::LayoutAction::ToggleTerminal);
    assert!(app.layout_visibility.terminal, "the first click opens the terminal surface");
}

#[test]
fn app_terminal_button_hides_and_reveals_existing_shells() {
    let d = temp_dir("app-terminal-existing-toggle");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.terms.tabs.push(
        term::Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            term::GridSize { cols: 40, rows: 8 },
            || {},
        )
        .expect("spawn test terminal"),
    );

    app.apply_layout_action(frame::LayoutAction::ToggleTerminal);
    assert!(app.layout_visibility.terminal);
    assert!(app.term_focused);

    app.apply_layout_action(frame::LayoutAction::ToggleTerminal);
    assert!(!app.layout_visibility.terminal, "an open dock toggles closed");
    assert!(!app.term_focused);

    app.apply_layout_action(frame::LayoutAction::ToggleTerminal);
    assert!(app.layout_visibility.terminal, "a hidden dock reveals its shells");
    assert!(app.term_focused);
}

#[test]
fn app_close_active_tab_prefers_the_editor_over_terminal_close() {
    let d = temp_dir("app-close-active-tab");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    fs::write(&a, "a\n").expect("write a");
    fs::write(&b, "b\n").expect("write b");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    let a_active = a.clone();
    app.open_viewer(a, 0);
    app.open_viewer(b.clone(), 0);

    assert!(app.close_active_editor_tab());
    assert_eq!(app.active_path().as_deref(), Some(a_active.as_path()));
    assert!(!app.buffers.contains(&b));
}

#[test]
fn app_tabs_select_viewer_and_close_config_or_file() {
    let d = temp_dir("app-tabs");
    let code = d.path().join("main.rs");
    std::fs::write(&code, "fn main() {}\n").expect("write code");
    let config = d.path().join("config.yaml");
    std::fs::write(&config, "a: 1\n").expect("write config");
    let mut app = App::new(d.path().to_path_buf(), false, None, None);
    app.open_viewer(code.clone(), 0);
    app.open_config(config.clone());
    assert_eq!(app.pane, PaneMode::Config);

    // Selecting the viewer tab returns to the read-only viewer.
    app.select_tab(0, code.clone());
    assert_eq!(app.pane, PaneMode::Viewer);
    assert_eq!(app.active_path().as_deref(), Some(code.as_path()));

    // Selecting the config tab returns to the editor.
    app.select_tab(0, config.clone());
    assert_eq!(app.pane, PaneMode::Config);

    // Closing the config tab drops the editor; closing the file tab evicts it.
    app.close_tab(0, config.clone());
    assert!(app.config_edits.is_none());
    app.close_tab(0, code.clone());
    assert!(app.active_path().is_none());
    assert!(!app.buffers.contains(&code));
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
    app.open_viewer(p.clone(), 0);
    app.request_diff_refresh(); // no proxy → runs inline

    let (path, diff) = app.file_diff.as_ref().expect("diff applied");
    assert_eq!(path, &p);
    assert_eq!(
        diff.marks,
        vec![(2, crate::git::GutterMark::Modified)],
        "the edited line's gutter mark must survive the integration"
    );
}
