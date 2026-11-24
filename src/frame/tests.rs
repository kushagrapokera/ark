//! Frame tests (impl + tests together would pass the ~400-line module
//! guidance): layout pins, status-line formats, scene ink, interaction
//! effects, and the M4 viewer-pane wiring.

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use crate::ui::testutil::count_color_in;
use std::fs;

// ---- layout ----

#[test]
fn app_layout_matches_spec() {
    let l = app_layout(1024.0, 768.0, 280.0);
    assert_eq!(l.activity_bar, Rect { x: 0.0, y: 88.0, w: 48.0, h: 656.0 });
    assert_eq!(l.tree_panel, Rect { x: 56.0, y: 88.0, w: 216.0, h: 656.0 });
    assert_eq!(l.scroll_track, Rect { x: 264.0, y: 88.0, w: 8.0, h: 656.0 });
    assert_eq!(l.editor_tabs, Rect { x: 296.0, y: 88.0, w: 720.0, h: 30.0 });
    assert_eq!(l.breadcrumb, Rect { x: 296.0, y: 118.0, w: 720.0, h: 22.0 });
    assert_eq!(l.viewer_pane, Rect { x: 296.0, y: 140.0, w: 720.0, h: 604.0 });
    assert_eq!(l.term_dock, Rect { x: 296.0, y: 744.0, w: 720.0, h: 0.0 });
    assert_eq!(l.split_area, Rect { x: 0.0, y: 88.0, w: 1024.0, h: 656.0 });
    assert_eq!(l.status, Rect { x: 0.0, y: 744.0, w: 1024.0, h: 24.0 });

    // Degenerate window sizes never produce negative extents.
    let tiny = app_layout(400.0, 300.0, 160.0);
    assert!(tiny.tree_panel.w >= 0.0 && tiny.tree_panel.h >= 0.0);
    assert!(tiny.viewer_pane.w >= 0.0 && tiny.status.y >= 0.0);
    assert_eq!(app_layout(1024.0, 768.0, 8.0).tree_panel.w, 0.0, "split at the margin");
    assert_eq!(l.commit_panel, Rect { x: 56.0, y: 744.0, w: 216.0, h: 0.0 }, "hidden by default");
}

#[test]
fn app_layout_commit_panel_splits_the_sidebar() {
    let l = app_layout_with_commit(1024.0, 768.0, 280.0, true, false);
    // 656px content − 236px panel = a 420px tree; the editor is unchanged.
    assert_eq!(l.tree_panel, Rect { x: 56.0, y: 88.0, w: 216.0, h: 420.0 });
    assert_eq!(l.scroll_track, Rect { x: 264.0, y: 88.0, w: 8.0, h: 420.0 });
    assert_eq!(
        l.commit_panel,
        Rect { x: 56.0, y: 508.0, w: 216.0, h: crate::commit::PANEL_H }
    );
    assert_eq!(l.viewer_pane, Rect { x: 296.0, y: 140.0, w: 720.0, h: 604.0 });

    // Tiny windows: the panel caps at 60% of the content area, never negative.
    let tiny = app_layout_with_commit(400.0, 300.0, 160.0, true, false);
    assert!((tiny.commit_panel.h - 112.8).abs() < 1e-9, "60% of 188px");
    assert!((tiny.tree_panel.h - 75.2).abs() < 1e-9);
    // No repo → full-height tree, zero-height panel.
    let no_git = app_layout_with_commit(1024.0, 768.0, 280.0, false, false);
    assert_eq!(no_git.tree_panel.h, 656.0);
    assert_eq!(no_git.commit_panel.h, 0.0);
}

#[test]
fn app_layout_terminal_dock_reserves_the_bottom() {
    let l = app_layout_with_commit(1024.0, 768.0, 280.0, true, true);
    assert_eq!(l.term_dock, Rect { x: 296.0, y: 524.0, w: 720.0, h: 220.0 });
    assert_eq!(l.term_tabbar, Rect { x: 296.0, y: 524.0, w: 660.0, h: 30.0 });
    assert_eq!(l.term_grid, Rect { x: 296.0, y: 554.0, w: 720.0, h: 190.0 });
    assert_eq!(l.viewer_pane, Rect { x: 296.0, y: 140.0, w: 720.0, h: 384.0 });
    // Tiny windows cap the dock at 40% of the content area.
    let tiny = app_layout_with_commit(400.0, 300.0, 160.0, false, true);
    assert!((tiny.term_dock.h - 75.2).abs() < 1e-9, "40% of 188px");
    assert_eq!(tiny.viewer_pane.h, 60.8);
}

// ---- status line ----

#[test]
fn frame_status_formats_tree_state() {
    let d = temp_dir("status");
    fs::create_dir_all(d.path().join("A")).expect("create A");
    fs::File::create(d.path().join("b.txt")).expect("create b.txt");
    let mut tree = FileTree::new(d.path());
    assert_eq!(
        frame_status(&tree, None, None, None, None, 66.0),
        "visible 2 · expanded 0 · selected none · scroll 0% · no git"
    );

    // Expanding an EMPTY dir changes the expansion count, not the rows;
    // content (44px) still fits the 66px viewport → 0%.
    tree.toggle(&tree.root.join("A"), true);
    assert_eq!(
        frame_status(&tree, None, None, None, None, 66.0),
        "visible 2 · expanded 1 · selected none · scroll 0% · no git"
    );

    // Overflow: 10 files (220px) in a 3-row viewport (66px) → max offset
    // 154; scrolling to the clamp reads 100%. A tree selection shows when
    // no viewer is open.
    let d2 = temp_dir("status-scroll");
    for i in 0..10 {
        fs::File::create(d2.path().join(format!("f{i}.txt"))).expect("create");
    }
    let mut big = FileTree::new(d2.path());
    assert!(big.scroll_by(1000.0, 66.0), "10 rows in a 3-row viewport overflows");
    assert_eq!(big.offset, 154.0, "clamped to the max offset");
    assert!(frame_status(&big, None, None, None, None, 66.0).contains("scroll 100% · no git"), "max offset → 100%");
    big.select(&big.root.join("f3.txt"));
    assert!(
        frame_status(&big, None, None, None, None, 66.0).contains("selected f3.txt"),
        "{}",
        frame_status(&big, None, None, None, None, 66.0)
    );

    // A listing failure replaces the status line (error surface).
    let missing = temp_dir("status-missing");
    let gone = missing.path().join("gone");
    let err_tree = FileTree::new(&gone);
    assert!(frame_status(&err_tree, None, None, None, None, 66.0).contains("cannot list"), "error surfaces");
}

#[test]
fn frame_status_formats_viewer_state() {
    let d = temp_dir("status-viewer");
    let p = d.path().join("sample.txt");
    fs::write(&p, "alpha\nbeta\ngamma\n").expect("write");
    let tree = FileTree::new(d.path());
    let viewer = Viewer::open(&p).expect("opens");

    // Viewer segment: name, line count, no selection → none.
    assert_eq!(
        frame_status(&tree, Some(&viewer), None, None, None, 632.0),
        "visible 1 · expanded 0 · scroll 0% · sample.txt 3 lines · sel none · no git"
    );

    // A selection reports its char count.
    let mut v = viewer;
    v.select_at(0, 0);
    v.extend_select(0, 5);
    assert!(
        frame_status(&tree, Some(&v), None, None, None, 632.0).contains("sel 5 chars"),
        "{}",
        frame_status(&tree, Some(&v), None, None, None, 632.0)
    );

    // A truncated view carries the flag.
    let big = d.path().join("big.txt");
    fs::write(&big, "x\n".repeat(100)).expect("write");
    let trunc = Viewer::open_with_cap(&big, 50).expect("opens");
    assert!(trunc.truncated());
    assert!(
        frame_status(&tree, Some(&trunc), None, None, None, 632.0).contains("big.txt 25 lines · truncated"),
        "{}",
        frame_status(&tree, Some(&trunc), None, None, None, 632.0)
    );
}

// ---- scene composition + interactions ----

fn scene_fixture(label: &str) -> (TempDir, FileTree) {
    let d = temp_dir(label);
    fs::create_dir_all(d.path().join("A/C")).expect("create A/C");
    fs::File::create(d.path().join("A/C/x.txt")).expect("create A/C/x.txt");
    for i in 0..40 {
        fs::File::create(d.path().join(format!("f{i:02}.txt"))).expect("create file");
    }
    let tree = FileTree::new(d.path());
    (d, tree)
}

/// Chrome-invisible theme: only the tokens under pin are distinct.
fn frame_theme() -> Theme {
    let bg = (0, 0, 0);
    Theme {
        bg,
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
        syntax_type: (137, 180, 250),
        syntax_function: (249, 226, 175),
        syntax_constant: (137, 180, 250),
        syntax_property: (137, 180, 250),
        syntax_control: (0, 0, 0),
        syntax_variable: (0, 0, 0),
        syntax_regex: (0, 0, 0),
        syntax_escape: (0, 0, 0),
        syntax_decorator: (0, 0, 0),
        syntax_selector: (0, 0, 0),
        editor_current_line: (0, 0, 0),
        editor_indent_guide: (0, 0, 0),
        editor_indent_guide_active: (0, 0, 0),
        editor_bracket_1: (0, 0, 0),
        editor_bracket_2: (0, 0, 0),
        editor_bracket_3: (0, 0, 0),
        on_accent: (30, 30, 46),
        git_modified: (250, 179, 135),
        git_added: (166, 227, 161),
        git_deleted: (243, 139, 168),
        git_untracked: (148, 226, 213),
    }
}

#[test]
fn frame_scene_renders_spec_surfaces() {
    let (_d, mut tree) = scene_fixture("scene");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(!effects.tree_changed && !effects.scrolled, "static frame changes nothing");
    assert_eq!(effects.opened, None);

    // Header (accent) + info line (dim) ink.
    assert!(count_color_in(&canvas, theme.accent, 0, 20, 400, 50) > 0, "header ink");
    assert_eq!(
        count_color_in(&canvas, theme.fg_dim, 0, 50, 260, 70),
        0,
        "debug info line is removed"
    );
    assert_eq!(
        count_color_in(&canvas, theme.bg, 700, 8, 1016, 30),
        316 * 22,
        "agent launcher buttons are removed"
    );
    // Tree rows: fg ink inside the panel's top rows.
    assert!(count_color_in(&canvas, theme.fg, 56, 88, 272, 200) > 0, "tree row ink");
    // Scrollbar track + thumb ink at the panel's right edge.
    assert!(count_color_in(&canvas, theme.bg_elevated, 264, 88, 272, 744) > 100, "track");
    assert!(count_color_in(&canvas, theme.fg_dim, 264, 88, 272, 744) > 100, "thumb");
    // Placeholder pane (no viewer open): border ink + centered dim text.
    assert!(count_color_in(&canvas, theme.border, 296, 140, 1016, 744) > 2000, "border");
    assert!(count_color_in(&canvas, theme.fg_dim, 400, 300, 912, 500) > 0, "placeholder text");
    // Status bar: accent fill + on-accent text.
    assert!(count_color_in(&canvas, theme.accent, 0, 744, 1024, 768) > 10_000, "status accent bar");
    assert!(count_color_in(&canvas, theme.on_accent, 0, 744, 900, 768) > 0, "status ink");
}

#[test]
fn frame_viewer_pane_renders_over_placeholder() {
    let (_d, mut tree) = scene_fixture("viewer-pane");
    // The viewed file sits OUTSIDE the tree fixture dir (a sibling temp dir).
    let vd = temp_dir("viewer-pane-file");
    let file = vd.path().join("viewed.rs");
    fs::write(&file, "fn main() {\n    let s = \"hi\";\n}\n").expect("write");
    let mut viewer = Viewer::open(&file).expect("opens");

    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: Some(&mut viewer),
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );

    // Gutter mark strip (bg_elevated) at the pane's left edge; line-number
    // dim ink and code fg ink inside the pane; NO placeholder text below it
    // would remain (the viewer fills the pane).
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 296, 140, 300, 744) > 300,
        "viewer gutter mark strip"
    );
    assert!(count_color_in(&canvas, theme.fg_dim, 296, 140, 1016, 250) > 0, "line numbers");
    assert!(count_color_in(&canvas, theme.fg, 300, 140, 1008, 250) > 0, "code text ink");
    assert!(
        count_color_in(&canvas, theme.syntax_keyword, 300, 140, 1008, 250) > 0,
        "`fn` keyword ink in the first lines"
    );
    // No border rect anymore (the viewer pane has no outline): border ink in
    // the pane's bottom strip is gone compared to the placeholder.
    // The x-range excludes the scrollbar track (whose outline is
    // border-colored).
    assert_eq!(
        count_color_in(&canvas, theme.border, 296, 724, 1000, 742),
        0,
        "viewer pane paints no placeholder border"
    );
}

#[test]
fn frame_interactions_report_effects() {
    let (_d, mut tree) = scene_fixture("effects");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    // One Frame re-pointed per interaction (draw_frame takes &mut Frame).
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: None,
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
        git: None,
        file_diff: None,
        commit: &mut CommitPanel::default(),
        config: None,
        config_error: None,
        terms: None,
        term_focused: false,
        tabbar: &mut TabBar::default(),
    };
    let mut draw_at = |frame: &mut Frame,
                       pointer: Pointer,
                       dbl: bool,
                       tree: &mut FileTree,
                       scrollbar: &mut Scrollbar,
                       viewer_scrollbar: &mut Scrollbar| {
        frame.pointer = pointer;
        frame.dbl_click = dbl;
        draw_frame(frame, &layout, tree, &mut split, &mut split_drag, scrollbar, viewer_scrollbar)
    };

    // Click the chevron of row 0 (dir "A") → toggles, full-damage flag.
    let click_chevron = Pointer { x: 63.0, y: 99.0, down: false, click: true };
    let effects = draw_at(&mut frame, click_chevron, false, &mut tree, &mut scrollbar, &mut viewer_scrollbar);
    assert!(effects.tree_changed && !effects.scrolled, "chevron click toggles");
    assert!(tree.is_expanded(&tree.root.join("A")));

    // Scrollbar thumb drag: press the thumb at the track top (offset 0),
    // drag past the free-span end → offset clamps to max, scrolled flag.
    let press = Pointer { x: 268.0, y: 89.0, down: true, click: false };
    let effects = draw_at(&mut frame, press, false, &mut tree, &mut scrollbar, &mut viewer_scrollbar);
    assert!(!effects.scrolled, "grabbing the thumb doesn't move it yet");
    let drag = Pointer { x: 268.0, y: 600.0, down: true, click: false };
    let effects = draw_at(&mut frame, drag, false, &mut tree, &mut scrollbar, &mut viewer_scrollbar);
    assert!(effects.scrolled && !effects.tree_changed, "thumb drag scrolls");
    let content = tree.visible_count() as f64 * ROW_H;
    assert_eq!(tree.offset, content - 656.0, "drag past the end clamps to max offset");

    // Collapse-while-scrolled self-heal: the row set shrinks underneath a
    // large offset (API-level mutation standing in for any out-of-frame
    // change) → the next frame re-clamps the offset and reports it as a
    // scroll (panel damage suffices — the shift is content-internal).
    assert!(tree.toggle(&tree.root.join("A"), true), "collapse A");
    let stale_content = tree.visible_count() as f64 * ROW_H;
    assert!(
        tree.offset > stale_content - 656.0,
        "offset is stale-large after the collapse"
    );
    let effects = draw_at(&mut frame, Pointer::default(), false, &mut tree, &mut scrollbar, &mut viewer_scrollbar);
    assert!(effects.scrolled && !effects.tree_changed, "clamp reports the heal as a scroll");
    assert_eq!(tree.offset, stale_content - 656.0, "offset re-clamped to the new max");
}

#[test]
fn frame_double_click_opens_and_viewer_scrolls() {
    let (_d, mut tree) = scene_fixture("dbl-open");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    // A viewed file OUTSIDE the tree fixture dir (100 lines for overflow).
    let vd = temp_dir("dbl-open-file");
    let file = vd.path().join("viewed.txt");
    fs::write(&file, (0..100).map(|i| format!("line {i}\n")).collect::<String>()).expect("write");
    let mut viewer = Viewer::open(&file).expect("opens");

    // The viewer stays open in the pane for every frame (the App keeps the
    // open file borrowed from the LRU cache; here the same borrow persists).
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: Some(&mut viewer),
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
        git: None,
        file_diff: None,
        commit: &mut CommitPanel::default(),
        config: None,
        config_error: None,
        terms: None,
        term_focused: false,
        tabbar: &mut TabBar::default(),
    };
    let mut draw_at = |frame: &mut Frame,
                       pointer: Pointer,
                       dbl: bool,
                       tree: &mut FileTree,
                       viewer_scrollbar: &mut Scrollbar| {
        frame.pointer = pointer;
        frame.dbl_click = dbl;
        draw_frame(frame, &layout, tree, &mut split, &mut split_drag, &mut scrollbar, viewer_scrollbar)
    };

    // Double-click on file row 1 ("f00.txt", band [110, 132)) → Opened.
    // (The first click already selected it in a prior frame.)
    let _ = draw_at(
        &mut frame,
        Pointer { x: 100.0, y: 121.0, down: false, click: true },
        false,
        &mut tree,
        &mut viewer_scrollbar,
    );
    let effects = draw_at(
        &mut frame,
        Pointer { x: 100.0, y: 121.0, down: false, click: true },
        true,
        &mut tree,
        &mut viewer_scrollbar,
    );
    assert_eq!(
        effects.opened,
        Some(tree.root.join("f00.txt")),
        "double-click reports the tree row's file to open"
    );

    // Viewer scrollbar thumb drag past the end → offset clamps to the max
    // line offset and reports viewer_scrolled (viewer rect damage only).
    let press = Pointer { x: 1012.0, y: 141.0, down: true, click: false };
    let effects = draw_at(&mut frame, press, false, &mut tree, &mut viewer_scrollbar);
    assert!(!effects.viewer_scrolled, "grabbing the viewer thumb doesn't move it");
    let drag = Pointer { x: 1012.0, y: 700.0, down: true, click: false };
    let effects = draw_at(&mut frame, drag, false, &mut tree, &mut viewer_scrollbar);
    // The same drag extends the text selection (tree_changed rides the
    // event-level full damage); the scrollbar reports viewer_scrolled.
    assert!(effects.viewer_scrolled, "viewer thumb drag scrolls");
    assert_eq!(
        viewer.offset_lines(),
        68,
        "drag past the end clamps to the scrollbar's last line"
    );
}

// ---- M5: change flash + viewer offset self-heal ----

#[test]
fn viewer_flash_paints_accent_strip() {
    let (_d, mut tree) = scene_fixture("flash");
    let vd = temp_dir("flash-file");
    let file = vd.path().join("viewed.txt");
    fs::write(&file, "one\n").expect("write");
    let mut viewer = Viewer::open(&file).expect("opens");

    let theme = frame_theme();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut tabbar = TabBar::default();

    // draw_frame fills the background itself; a free fn (not a closure) so
    // canvas borrows end before each pixel assertion.
    #[allow(clippy::too_many_arguments)] // borrows must outlive no closure: explicit beats bundled
    fn draw(flash: bool, viewer: &mut Viewer, canvas: &mut Canvas, text: &mut TextSystem,
            theme: &Theme, tree: &mut FileTree, scrollbar: &mut Scrollbar,
            viewer_scrollbar: &mut Scrollbar, tabbar: &mut TabBar) {
        let layout = app_layout(1024.0, 768.0, 280.0);
        let mut split = Split { position: 280.0 };
        let mut split_drag = SplitDrag::default();
        let mut commit = CommitPanel::default();
        draw_frame(
            &mut Frame {
                canvas,
                text,
                theme,
                scale: 1.0,
                title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
                pointer: Pointer::default(),
                dbl_click: false,
                viewer: Some(viewer),
                viewer_error: None,
                viewer_flash: flash,
                highlighter: &Highlighter::new(),
                git: None,
                file_diff: None,
                commit: &mut commit,
                config: None,
                config_error: None,
                terms: None,
                term_focused: false,
                tabbar,
            },
            &layout,
            tree,
            &mut split,
            &mut split_drag,
            scrollbar,
            viewer_scrollbar,
        );
    }

    draw(true, &mut viewer, &mut canvas, &mut text, &theme, &mut tree, &mut scrollbar, &mut viewer_scrollbar, &mut tabbar);
    assert!(
        count_color_in(&canvas, theme.accent, 296, 140, 1016, 143) >= 720 * 3,
        "flash paints the accent strip across the pane's top edge"
    );
    draw(false, &mut viewer, &mut canvas, &mut text, &theme, &mut tree, &mut scrollbar, &mut viewer_scrollbar, &mut tabbar);
    assert_eq!(
        count_color_in(&canvas, theme.accent, 296, 140, 1016, 143),
        0,
        "no flash → no strip"
    );
}

#[test]
fn frame_draws_commit_panel_and_reports_clicks() {
    let (_d, mut tree) = scene_fixture("commit-frame");
    let status = crate::git::parse_status(
        "? new.txt\n1 .M N... 000000 100644 100644 111 222 src/main.rs\n",
    )
    .rooted(tree.root.as_path());
    let theme = frame_theme();
    let layout = app_layout_with_commit(1024.0, 768.0, 280.0, true, false);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    // One full frame with the panel: title, checked boxes and buttons paint.
    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: Some(&status),
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(!effects.commit_toggled && !effects.commit_clicked && !effects.push_clicked);
    // Panel title ink (56..272 × 508..538), checked boxes + badges in the
    // list (rows start at 538), two button fills at 712..736.
    assert!(count_color_in(&canvas, theme.fg_dim, 56, 508, 272, 538) > 0, "panel title");
    assert!(
        count_color_in(&canvas, theme.accent, 56, 538, 272, 672) >= 2 * 100,
        "both changed files start checked"
    );
    assert!(count_color_in(&canvas, theme.git_untracked, 56, 538, 272, 558) > 0, "U badge row");
    assert!(count_color_in(&canvas, theme.git_modified, 56, 558, 272, 578) > 0, "M badge row");
    assert!(count_color_in(&canvas, theme.bg_elevated, 64, 712, 264, 736) > 1000, "buttons");

    // Interactions: row toggle, Commit, Push, then message-field focus.
    let mut commit = CommitPanel::default();
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: None,
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
        git: Some(&status),
        file_diff: None,
        commit: &mut commit,
        config: None,
        config_error: None,
        terms: None,
        term_focused: false,
        tabbar: &mut TabBar::default(),
    };
    {
        let mut draw = |frame: &mut Frame, pointer: Pointer| {
            frame.pointer = pointer;
            draw_frame(
                frame,
                &layout,
                &mut tree,
                &mut split,
                &mut split_drag,
                &mut scrollbar,
                &mut viewer_scrollbar,
            )
        };
        let row_click = Pointer { x: 120.0, y: 548.0, down: false, click: true };
        assert!(draw(&mut frame, row_click).commit_toggled, "file row toggles");
        let commit_click = Pointer { x: 112.0, y: 724.0, down: false, click: true };
        let effects = draw(&mut frame, commit_click);
        assert!(effects.commit_clicked && !effects.push_clicked, "Commit button");
        let push_click = Pointer { x: 216.0, y: 724.0, down: false, click: true };
        let effects = draw(&mut frame, push_click);
        assert!(effects.push_clicked && !effects.commit_clicked, "Push button");
        let field_click = Pointer { x: 120.0, y: 692.0, down: false, click: true };
        let _ = draw(&mut frame, field_click);
    }
    assert!(commit.message.focused, "message field takes keyboard focus");
    assert!(!commit.is_checked(&status.entries[0].path), "row click unchecked it");
}

#[test]
fn term_tabbar_syncs_a_newly_opened_tab() {
    let mut bar = TabBar { tabs: vec!["fish".into()], active: 0 };
    // Opening the M8 push tab: the model's active index must win over the
    // bar's retained index.
    sync_term_tabbar(&mut bar, vec!["fish".into(), "git push".into()], 1);
    assert_eq!(bar.tabs.len(), 2);
    assert_eq!(bar.active, 1, "newly opened tab becomes active");

    // An unchanged list keeps the user's bar-click selection.
    bar.active = 0;
    sync_term_tabbar(&mut bar, vec!["fish".into(), "git push".into()], 1);
    assert_eq!(bar.active, 0, "unchanged labels never reset the active tab");

    // Closing a tab carries the model's clamped index.
    sync_term_tabbar(&mut bar, vec!["fish".into()], 0);
    assert_eq!(bar.active, 0);
}

#[test]
fn frame_config_pane_renders_and_reports_save_click() {
    let (_d, mut tree) = scene_fixture("config-frame");
    let cd = temp_dir("config-frame-file");
    let yaml = cd.path().join("config.yaml");
    fs::write(&yaml, "alpha: 1\nbeta: 2\n").expect("write");
    let mut editor =
        ConfigEditor::open(cd.path(), &yaml, &crate::config::Config::default().editable)
            .expect("open config");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: Some(&mut editor),
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(!effects.config_save && !effects.config_reload);
    // Header title + textarea ink inside the editor body.
    assert!(count_color_in(&canvas, theme.fg, 296, 140, 1016, 300) > 0, "config ink");
    assert!(
        frame_status(&tree, None, Some(&editor), None, None, 604.0)
            .contains("config config.yaml"),
        "{}",
        frame_status(&tree, None, Some(&editor), None, None, 604.0)
    );

    // Header Save button (panel right edge) reports the effect.
    let save_click = Pointer { x: 977.0, y: 159.0, down: false, click: true };
    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: save_click,
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: Some(&mut editor),
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(effects.config_save && !effects.config_reload);
}

#[test]
fn file_tabs_breadcrumbs_and_activity_bar_report_clicks() {
    let (_d, mut tree) = scene_fixture("workbench-tabs");
    let files = vec![
        PathBuf::from("/repo/src/main.rs"),
        PathBuf::from("/repo/config.yaml"),
    ];
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        title: "ark",
        root: Path::new("/repo"),
        open_files: &files,
        active_path: Some(files[0].as_path()),
        activity: ActivityPanel::Explorer,
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: None,
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
        git: None,
        file_diff: None,
        commit: &mut CommitPanel::default(),
        config: None,
        config_error: None,
        terms: None,
        term_focused: false,
        tabbar: &mut TabBar::default(),
    };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(effects.select_tab.is_none() && effects.close_tab.is_none());
    // Tab strip and breadcrumb paint inside the editor column.
    assert!(
        count_color_in(frame.canvas, theme.bg_elevated, 296, 88, 1016, 118) > 10_000,
        "tab strip background"
    );
    assert!(
        crate::ui::testutil::count_not_color_in(frame.canvas, theme.bg, 296, 118, 1016, 140) > 0,
        "breadcrumb ink"
    );
    assert!(
        crate::ui::testutil::count_not_color_in(frame.canvas, theme.bg, 900, 88, 1016, 118) > 0,
        "READ-ONLY badge"
    );
    // Activity bar icons/badges paint left of the tree.
    assert!(count_color_in(frame.canvas, theme.bg_elevated, 0, 88, 48, 744) > 10_000, "activity track");

    // Second file tab selects; first tab's × closes.
    frame.pointer = Pointer { x: 500.0, y: 103.0, down: false, click: true };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(effects.select_tab.as_deref(), Some(files[1].as_path()));
    frame.pointer = Pointer { x: 442.0, y: 103.0, down: false, click: true };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(effects.close_tab.as_deref(), Some(files[0].as_path()));
    // Activity bar: click the Config icon (fourth slot).
    frame.pointer = Pointer { x: 24.0, y: 270.0, down: false, click: true };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(effects.set_activity, Some(ActivityPanel::Config));
}

#[test]
fn sidebar_three_dot_handle_drags_and_double_click_resets() {
    let (_d, mut tree) = scene_fixture("sidebar-handle");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut commit = CommitPanel::default();
    let mut tabbar = TabBar::default();

    let handle = sidebar_handle(280.0);
    assert_eq!(handle, Rect { x: 272.0, y: 60.0, w: 16.0, h: 24.0 });

    draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
            root: std::path::Path::new("/"),
            open_files: &[],
            active_path: None,
            activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut commit,
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut tabbar,
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 272, 60, 288, 84) >= 3,
        "three-dot handle paints in the header gap"
    );

    let mut draw = |split: &mut Split, pointer: Pointer, dbl: bool| {
        draw_frame(
            &mut Frame {
                canvas: &mut canvas,
                text: &mut text,
                theme: &theme,
                scale: 1.0,
                title: "ark",
                root: std::path::Path::new("/"),
                open_files: &[],
                active_path: None,
                activity: ActivityPanel::Explorer,
                pointer,
                dbl_click: dbl,
                viewer: None,
                viewer_error: None,
                viewer_flash: false,
                highlighter: &Highlighter::new(),
                git: None,
                file_diff: None,
                commit: &mut commit,
                config: None,
                config_error: None,
                terms: None,
                term_focused: false,
                tabbar: &mut tabbar,
            },
            &layout,
            &mut tree,
            split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        );
    };

    // Drag the handle itself: 280 → 340.
    draw(&mut split, Pointer { x: 280.0, y: 72.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 340.0, y: 72.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 340.0, y: 72.0, down: false, click: false }, false);
    assert_eq!(split.position, 340.0);

    // Double-click the handle at its new position resets to the default.
    draw(&mut split, Pointer { x: 340.0, y: 72.0, down: false, click: true }, true);
    assert_eq!(split.position, SIDEBAR_DEFAULT_PX);

    // The normal divider drag band still works and clamps.
    draw(&mut split, Pointer { x: 280.0, y: 300.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 500.0, y: 300.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 500.0, y: 300.0, down: false, click: false }, false);
    assert_eq!(split.position, 500.0);
    draw(&mut split, Pointer { x: 500.0, y: 300.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 0.0, y: 300.0, down: true, click: false }, false);
    draw(&mut split, Pointer { x: 0.0, y: 300.0, down: false, click: false }, false);
    assert_eq!(split.position, SPLIT_MIN_PX);
}

#[test]
fn viewer_offset_self_heals_before_hit_testing() {
    let (_d, mut tree) = scene_fixture("self-heal");
    let vd = temp_dir("self-heal-file");
    let file = vd.path().join("viewed.txt");
    fs::write(&file, "one\n").expect("write"); // 1 line only
    let mut viewer = Viewer::open(&file).expect("opens");
    viewer.restore_offset(50); // stale (e.g. the file shrank on refresh)

    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 280.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 280.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
        root: std::path::Path::new("/"),
        open_files: &[],
        active_path: None,
        activity: ActivityPanel::Explorer,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: Some(&mut viewer),
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: &mut CommitPanel::default(),
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
            tabbar: &mut TabBar::default(),
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(effects.viewer_scrolled, "the stale offset reports a scroll");
    assert_eq!(
        viewer.offset_lines(),
        0,
        "offset clamped to max(1 line − 33-row viewport) = 0"
    );
}
