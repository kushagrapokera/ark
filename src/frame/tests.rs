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
    assert_eq!(l.tree_panel, Rect { x: 8.0, y: 88.0, w: 264.0, h: 632.0 });
    assert_eq!(l.scroll_track, Rect { x: 264.0, y: 88.0, w: 8.0, h: 632.0 });
    assert_eq!(l.viewer_pane, Rect { x: 296.0, y: 88.0, w: 720.0, h: 632.0 });
    assert_eq!(l.split_area, Rect { x: 0.0, y: 88.0, w: 1024.0, h: 632.0 });
    assert_eq!(l.status, Rect { x: 0.0, y: 726.0, w: 1024.0, h: 20.0 });

    // Degenerate window sizes never produce negative extents.
    let tiny = app_layout(400.0, 300.0, 160.0);
    assert!(tiny.tree_panel.w >= 0.0 && tiny.tree_panel.h >= 0.0);
    assert!(tiny.viewer_pane.w >= 0.0 && tiny.status.y >= 0.0);
    assert_eq!(app_layout(1024.0, 768.0, 8.0).tree_panel.w, 0.0, "split at the margin");
}

// ---- status line ----

#[test]
fn frame_status_formats_tree_state() {
    let d = temp_dir("status");
    fs::create_dir_all(d.path().join("A")).expect("create A");
    fs::File::create(d.path().join("b.txt")).expect("create b.txt");
    let mut tree = FileTree::new(d.path());
    assert_eq!(
        frame_status(&tree, None, None, 66.0),
        "visible 2 · expanded 0 · selected none · scroll 0%"
    );

    // Expanding an EMPTY dir changes the expansion count, not the rows;
    // content (44px) still fits the 66px viewport → 0%.
    tree.toggle(&tree.root.join("A"), true);
    assert_eq!(
        frame_status(&tree, None, None, 66.0),
        "visible 2 · expanded 1 · selected none · scroll 0%"
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
    assert!(frame_status(&big, None, None, 66.0).ends_with("scroll 100%"), "max offset → 100%");
    big.select(&big.root.join("f3.txt"));
    assert!(
        frame_status(&big, None, None, 66.0).contains("selected f3.txt"),
        "{}",
        frame_status(&big, None, None, 66.0)
    );

    // A listing failure replaces the status line (error surface).
    let missing = temp_dir("status-missing");
    let gone = missing.path().join("gone");
    let err_tree = FileTree::new(&gone);
    assert!(frame_status(&err_tree, None, None, 66.0).contains("cannot list"), "error surfaces");
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
        frame_status(&tree, Some(&viewer), None, 632.0),
        "visible 1 · expanded 0 · scroll 0% · sample.txt 3 lines · sel none"
    );

    // A selection reports its char count.
    let mut v = viewer;
    v.select_at(0, 0);
    v.extend_select(0, 5);
    assert!(
        frame_status(&tree, Some(&v), None, 632.0).ends_with("sel 5 chars"),
        "{}",
        frame_status(&tree, Some(&v), None, 632.0)
    );

    // A truncated view carries the flag.
    let big = d.path().join("big.txt");
    fs::write(&big, "x\n".repeat(100)).expect("write");
    let trunc = Viewer::open_with_cap(&big, 50).expect("opens");
    assert!(trunc.truncated());
    assert!(
        frame_status(&tree, Some(&trunc), None, 632.0).contains("big.txt 25 lines · truncated"),
        "{}",
        frame_status(&tree, Some(&trunc), None, 632.0)
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
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
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
    assert!(count_color_in(&canvas, theme.fg_dim, 0, 50, 700, 70) > 0, "info line ink");
    // Tree rows: fg ink inside the panel's top rows.
    assert!(count_color_in(&canvas, theme.fg, 8, 88, 264, 200) > 0, "tree row ink");
    // Scrollbar track + thumb ink at the panel's right edge.
    assert!(count_color_in(&canvas, theme.bg_elevated, 264, 88, 272, 720) > 100, "track");
    assert!(count_color_in(&canvas, theme.fg_dim, 264, 88, 272, 720) > 100, "thumb");
    // Placeholder pane (no viewer open): border ink + centered dim text.
    assert!(count_color_in(&canvas, theme.border, 296, 88, 1016, 720) > 2000, "border");
    assert!(count_color_in(&canvas, theme.fg_dim, 400, 300, 912, 500) > 0, "placeholder text");
    // Status line: dim ink near baseline h−28 = 740.
    assert!(count_color_in(&canvas, theme.fg_dim, 0, 724, 900, 748) > 0, "status ink");
}

#[test]
fn frame_viewer_pane_renders_over_placeholder() {
    let (_d, mut tree) = scene_fixture("viewer-pane");
    // The viewed file sits OUTSIDE the tree fixture dir (a sibling temp dir).
    let vd = temp_dir("viewer-pane-file");
    let file = vd.path().join("viewed.txt");
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
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: Some(&mut viewer),
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
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
        count_color_in(&canvas, theme.bg_elevated, 296, 88, 300, 718) > 300,
        "viewer gutter mark strip"
    );
    assert!(count_color_in(&canvas, theme.fg_dim, 296, 88, 1016, 200) > 0, "line numbers");
    assert!(count_color_in(&canvas, theme.fg, 300, 88, 1008, 200) > 0, "code text ink");
    assert!(
        count_color_in(&canvas, theme.syntax_keyword, 300, 88, 1008, 200) > 0,
        "`fn` keyword ink in the first lines"
    );
    // No border rect anymore (the viewer pane has no outline): border ink in
    // the pane's bottom strip is gone compared to the placeholder.
    // The x-range excludes the scrollbar track (whose outline is
    // border-colored).
    assert_eq!(
        count_color_in(&canvas, theme.border, 296, 700, 1000, 718),
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
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: None,
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
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
    let click_chevron = Pointer { x: 15.0, y: 99.0, down: false, click: true };
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
    assert_eq!(tree.offset, content - 632.0, "drag past the end clamps to max offset");

    // Collapse-while-scrolled self-heal: the row set shrinks underneath a
    // large offset (API-level mutation standing in for any out-of-frame
    // change) → the next frame re-clamps the offset and reports it as a
    // scroll (panel damage suffices — the shift is content-internal).
    assert!(tree.toggle(&tree.root.join("A"), true), "collapse A");
    let stale_content = tree.visible_count() as f64 * ROW_H;
    assert!(
        tree.offset > stale_content - 632.0,
        "offset is stale-large after the collapse"
    );
    let effects = draw_at(&mut frame, Pointer::default(), false, &mut tree, &mut scrollbar, &mut viewer_scrollbar);
    assert!(effects.scrolled && !effects.tree_changed, "clamp reports the heal as a scroll");
    assert_eq!(tree.offset, stale_content - 632.0, "offset re-clamped to the new max");
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
        pointer: Pointer::default(),
        dbl_click: false,
        viewer: Some(&mut viewer),
        viewer_error: None,
        viewer_flash: false,
        highlighter: &Highlighter::new(),
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
        Pointer { x: 50.0, y: 121.0, down: false, click: true },
        false,
        &mut tree,
        &mut viewer_scrollbar,
    );
    let effects = draw_at(
        &mut frame,
        Pointer { x: 50.0, y: 121.0, down: false, click: true },
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
    let press = Pointer { x: 1012.0, y: 89.0, down: true, click: false };
    let effects = draw_at(&mut frame, press, false, &mut tree, &mut viewer_scrollbar);
    assert!(!effects.viewer_scrolled, "grabbing the viewer thumb doesn't move it");
    let drag = Pointer { x: 1012.0, y: 700.0, down: true, click: false };
    let effects = draw_at(&mut frame, drag, false, &mut tree, &mut viewer_scrollbar);
    // The same drag extends the text selection (tree_changed rides the
    // event-level full damage); the scrollbar reports viewer_scrolled.
    assert!(effects.viewer_scrolled, "viewer thumb drag scrolls");
    let viewport_lines = (632.0 / LINE_H).floor() as usize;
    assert_eq!(
        viewer.offset_lines(),
        viewer.line_count() - viewport_lines,
        "drag past the end clamps to the max line offset"
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
        draw_frame(
            &mut Frame {
                canvas,
                text,
                theme,
                scale: 1.0,
                title: "ark",
                pointer: Pointer::default(),
                dbl_click: false,
                viewer: Some(viewer),
                viewer_error: None,
                viewer_flash: flash,
                highlighter: &Highlighter::new(),
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
        count_color_in(&canvas, theme.accent, 296, 88, 1016, 91) >= 720 * 3,
        "flash paints the accent strip across the pane's top edge"
    );
    draw(false, &mut viewer, &mut canvas, &mut text, &theme, &mut tree, &mut scrollbar, &mut viewer_scrollbar, &mut tabbar);
    assert_eq!(
        count_color_in(&canvas, theme.accent, 296, 88, 1016, 91),
        0,
        "no flash → no strip"
    );
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
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: Some(&mut viewer),
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
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
