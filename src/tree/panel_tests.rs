//! Tree-panel tests: row hit-testing (`row_at`), pointer interaction
//! (`handle_pointer`) and viewport-only rendering pins (`draw_tree_panel`).
//! Pixel pins use the M2 chrome-invisible theme technique (all tokens but
//! the one under test set to bg so exact-color counts are deterministic).

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use crate::render::{Canvas, Theme};
use crate::text::TextSystem;
use crate::ui::testutil::{count_color_in, ink_bbox_in};
use crate::ui::{Pointer, Rect};
use std::fs;

/// Fixture: root containing `A/` (containing `C/` and `a.txt`) and `b.txt`,
/// expanded state controlled by the caller. Rows: [A, C, a.txt, b.txt].
fn panel_fixture(label: &str) -> (TempDir, FileTree) {
    let d = temp_dir(label);
    fs::create_dir_all(d.path().join("A/C")).expect("create A/C");
    fs::File::create(d.path().join("A/a.txt")).expect("create A/a.txt");
    fs::File::create(d.path().join("b.txt")).expect("create b.txt");
    let tree = FileTree::new(d.path());
    (d, tree)
}

/// 4-row panel exactly covering rows [A, C, a.txt, b.txt] at offset 0.
const PANEL: Rect = Rect { x: 10.0, y: 10.0, w: 200.0, h: 88.0 };

/// Theme with only the row-ink tokens distinct: fg paints icons/names,
/// bg_elevated the hover bg, selection the selected bg.
fn panel_theme() -> Theme {
    let bg = (0, 0, 0);
    Theme {
        bg,
        surface_chrome: bg,
        surface: bg,
        bg_elevated: (10, 20, 30),
        surface_hover: bg,
        fg: (255, 255, 255),
        fg_dim: bg,
        text_muted: bg,
        text_dim: bg,
        accent: bg,
        border: bg,
        border_subtle: bg,
        selection: (40, 50, 60),
        syntax_keyword: bg,
        syntax_string: bg,
        syntax_number: bg,
        syntax_comment: bg,
        syntax_type: bg,
        syntax_function: bg,
        syntax_constant: bg,
        syntax_property: bg,
        syntax_control: bg,
        syntax_variable: bg,
        syntax_regex: bg,
        syntax_escape: bg,
        syntax_decorator: bg,
        syntax_selector: bg,
        editor_current_line: bg,
        editor_indent_guide: bg,
        editor_indent_guide_active: bg,
        editor_bracket_1: bg,
        editor_bracket_2: bg,
        editor_bracket_3: bg,
        on_accent: bg,
        git_modified: bg,
        git_added: bg,
        git_deleted: bg,
        git_untracked: bg,
    }
}

// ---- row_at: offset-aware hit testing ----

#[test]
fn row_at_maps_offset_to_rows() {
    let (_d, mut tree) = panel_fixture("row-at");
    tree.toggle(&tree.root.join("A"), true); // rows: A, C, a.txt, b.txt

    // Row 0 (A, dir, depth 0): pointer anywhere in [10, 32).
    let hit = row_at(PANEL, 50.0, 21.0, &tree).expect("row 0 hit");
    assert_eq!(hit.row.name, "A");
    assert!(hit.row.is_dir && hit.row.depth == 0);
    assert_eq!(hit.y, 10.0, "row 0 top edge");

    // The whole directory row is the same hit target, including the chevron.
    assert_eq!(
        row_at(PANEL, 15.0, 21.0, &tree).expect("A hit").row.name,
        "A"
    );
    assert_eq!(
        row_at(PANEL, 50.0, 21.0, &tree).expect("A hit").row.name,
        "A"
    );

    // Deeper row: C at depth 1.
    let c = row_at(PANEL, 30.0, 43.0, &tree).expect("row 1 hit");
    assert_eq!(c.row.name, "C");
    assert_eq!(c.row.depth, 1);
    assert_eq!(
        row_at(PANEL, 20.0, 43.0, &tree).expect("C hit").row.name,
        "C"
    );

    // File rows use the same whole-row hit target.
    let a_txt = row_at(PANEL, 15.0, 65.0, &tree).expect("row 2 hit");
    assert_eq!(a_txt.row.name, "a.txt");

    // Scroll shifts the mapping: a 3-row viewport on the 4-row content has
    // max offset 22 (overshoot clamps); at offset 22 pointer row-0 band
    // hits row 1, whose panel-space y is the panel's top band.
    let (_d2, mut tree2) = panel_fixture("row-at-scroll");
    tree2.toggle(&tree2.root.join("A"), true);
    let scroll_panel = Rect { x: 10.0, y: 10.0, w: 200.0, h: 66.0 };
    assert!(tree2.scroll_by(5.0 * ROW_H, scroll_panel.h), "overflow exists");
    assert_eq!(tree2.offset, 22.0, "clamped to count×ROW_H − viewport");
    let hit = row_at(scroll_panel, 50.0, 21.0, &tree2).expect("hit through offset");
    assert_eq!(hit.row.name, "C", "offset 22 maps pointer y=21 to row 1");
    assert_eq!(hit.y, 10.0, "row 1 now occupies the panel's top band");

    // Misses: outside the panel, inside the scrollbar track zone, or past
    // the last row (stale offset reaching beyond the visible rows).
    assert!(row_at(PANEL, 5.0, 21.0, &tree).is_none(), "left of panel");
    assert!(row_at(PANEL, 50.0, 9.0, &tree).is_none(), "above panel");
    assert!(row_at(PANEL, 50.0, 99.0, &tree).is_none(), "below panel");
    assert!(
        row_at(PANEL, PANEL.x + PANEL.w - 4.0, 21.0, &tree).is_none(),
        "the 8px scrollbar track zone never hits a row"
    );
    let (_d3, mut stale) = panel_fixture("row-at-stale");
    stale.offset = 500.0; // pub field corrupted directly (e.g. future bug)
    assert!(
        row_at(PANEL, 50.0, 21.0, &stale).is_none(),
        "offset past the content → past the last row → None"
    );
}

// ---- handle_pointer: click routing ----

#[test]
fn handle_pointer_routes_clicks() {
    let (_d, mut tree) = panel_fixture("clicks");
    let a = tree.root.join("A");
    let b = tree.root.join("b.txt");

    // Chevron click expands and selects the folder.
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 15.0, y: 21.0, down: false, click: true }),
        TreeAction::Toggled
    );
    assert!(tree.is_expanded(&a));
    assert_eq!(tree.selected(), Some(a.as_path()));

    // Clicking anywhere else on the folder row collapses it.
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 50.0, y: 21.0, down: false, click: true }),
        TreeAction::Toggled
    );
    assert!(!tree.is_expanded(&a));
    assert_eq!(tree.selected(), Some(a.as_path()));

    // A is collapsed, so b.txt is row 1 and opens on the first click.
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 50.0, y: 43.0, down: false, click: true }),
        TreeAction::Opened(b.clone())
    );
    assert_eq!(tree.selected(), Some(b.as_path()));
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 50.0, y: 43.0, down: false, click: true }),
        TreeAction::Opened(b.clone()),
        "re-clicking a file still opens it"
    );

    // Clicks outside the panel or without the click flag never act.
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 500.0, y: 21.0, down: false, click: true }),
        TreeAction::None
    );
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 15.0, y: 21.0, down: false, click: false }),
        TreeAction::None
    );
    assert_eq!(tree.selected(), Some(b.as_path()), "state untouched");
}

// ---- handle_pointer: single-click opens files ----

#[test]
fn handle_pointer_opens_files_on_single_click() {
    let (_d, mut tree) = panel_fixture("open");
    let b = tree.root.join("b.txt");

    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 50.0, y: 43.0, down: false, click: true }),
        TreeAction::Opened(b.clone())
    );

    // Folder row bodies toggle on a single click, never open.
    let a = tree.root.join("A");
    assert_eq!(
        handle_pointer(&mut tree, PANEL, Pointer { x: 50.0, y: 21.0, down: false, click: true }),
        TreeAction::Toggled
    );
    assert!(tree.is_expanded(&a));
}

// ---- draw_tree_panel: viewport-only ink ----

/// 100 files named with strictly growing length ("a", "aa", …) so a row
/// band's ink width identifies which row is rendered there.
fn wide_fixture(label: &str) -> (TempDir, FileTree) {
    let d = temp_dir(label);
    for i in 0..100 {
        let name = "a".repeat(i + 1);
        fs::File::create(d.path().join(&name)).expect("create file");
    }
    let tree = FileTree::new(d.path());
    (d, tree)
}

#[test]
fn long_tree_names_stay_inside_the_panel() {
    let d = temp_dir("long-tree-name");
    let name = "a".repeat(120);
    fs::File::create(d.path().join(&name)).expect("create long name");
    let tree = FileTree::new(d.path());
    let theme = panel_theme();
    let panel = Rect { x: 10.0, y: 10.0, w: 200.0, h: 30.0 };
    let mut canvas = Canvas::new(400, 40);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    draw_tree_panel(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel,
        &tree,
        Pointer::default(),
        None,
    );
    let bbox = ink_bbox_in(&canvas, theme.bg, 0, 0, 400, 40).expect("tree ink");
    assert!(
        bbox.2 < (panel.x + panel.w - TRACK_W) as u32,
        "name ink must stop before the scrollbar track, got {bbox:?}"
    );
}

#[test]
fn inserted_tree_row_shifts_existing_rows_down() {
    let (_d, mut tree) = panel_fixture("inserted-row");
    let a = tree.root.join("A");
    let c = a.join("C");
    assert!(tree.toggle(&a, true));
    tree.select(&c);
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(220, 108);
    canvas.fill(theme.bg);

    draw_tree_panel_with_insertion(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        PANEL,
        &tree,
        Pointer::default(),
        None,
        Some(1),
    );

    assert_eq!(
        count_color_in(&canvas, theme.selection, 10, 32, 210, 54),
        0,
        "the first child no longer occupies the insertion row"
    );
    assert!(
        count_color_in(&canvas, theme.selection, 10, 54, 210, 76) > 0,
        "the first child shifts below the inserted row"
    );
}

#[test]
fn viewport_only_rendering() {
    let (_d, mut tree) = wide_fixture("viewport");
    assert_eq!(tree.visible_count(), 100);
    let theme = panel_theme();
    let panel = Rect { x: 10.0, y: 10.0, w: 200.0, h: 66.0 }; // 3 rows high

    let draw = |canvas: &mut Canvas, tree: &FileTree| {
        canvas.fill(theme.bg);
        let mut text = TextSystem::new();
        draw_tree_panel(canvas, &mut text, &theme, 1.0, panel, tree, Pointer::default(), None);
    };

    let mut canvas = Canvas::new(220, 90);
    draw(&mut canvas, &tree);
    // All ink confined to the 3-row band [10, 76) — nothing below it.
    let bbox = ink_bbox_in(&canvas, theme.bg, 0, 0, 220, 90).expect("tree ink");
    assert!(bbox.1 >= 10 && bbox.3 < 76, "ink rows confined to the visible band, got {bbox:?}");
    // Band 3 [54, 76) shows row 2 = "aaa" (3 chars + icon).
    let (_, short) = band_width(&canvas, theme.bg, 54, 76);

    // Scroll by two rows: band 3 now shows row 4 = "aaaaa" (5 chars).
    assert!(tree.scroll_by(2.0 * ROW_H, panel.h));
    draw(&mut canvas, &tree);
    let bbox = ink_bbox_in(&canvas, theme.bg, 0, 0, 220, 90).expect("scrolled tree ink");
    assert!(bbox.1 >= 10 && bbox.3 < 76, "scrolled ink still viewport-only");
    let (_, long) = band_width(&canvas, theme.bg, 54, 76);
    assert!(
        long > short + 10,
        "band 3 must show the LONGER row 4 after scrolling 2 rows ({short} → {long})"
    );
}

/// Ink extent (min_x, max_x) within a horizontal pixel band.
fn band_width(canvas: &Canvas, bg: crate::render::ColorRgb, y0: u32, y1: u32) -> (u32, u32) {
    let bbox = ink_bbox_in(canvas, bg, 0, y0, canvas.width(), y1)
        .expect("band must contain ink");
    (bbox.0, bbox.2)
}

// ---- draw_tree_panel: row visuals ----

#[test]
fn row_visuals_pin_colors() {
    let (_d, mut tree) = panel_fixture("visuals");
    let theme = panel_theme();
    let a = tree.root.join("A");
    let b = tree.root.join("b.txt");
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(220, 90);

    let draw = |canvas: &mut Canvas, text: &mut TextSystem, tree: &FileTree, pointer: Pointer| {
        canvas.fill(theme.bg);
        draw_tree_panel(canvas, text, &theme, 1.0, PANEL, tree, pointer, None);
    };

    // Collapsed dir: chevron ink in the depth-0 chevron cell.
    draw(&mut canvas, &mut text, &tree, Pointer::default());
    let w_right = ink_bbox_in(&canvas, theme.bg, 10, 10, 27, 32).expect("collapsed chevron ink");

    // Expanded dir: ChevronDown is a wider glyph than ChevronRight.
    tree.toggle(&a, true);
    draw(&mut canvas, &mut text, &tree, Pointer::default());
    let w_down = ink_bbox_in(&canvas, theme.bg, 10, 10, 27, 32).expect("expanded chevron ink");
    // Robust glyph discriminator (AA edges round tightly): ChevronDown is
    // wider than tall, ChevronRight taller than wide.
    assert!(
        w_down.2 - w_down.0 > w_down.3 - w_down.1,
        "ChevronDown ({w_down:?}) must be wider than tall"
    );
    assert!(
        w_right.3 - w_right.1 > w_right.2 - w_right.0,
        "ChevronRight ({w_right:?}) must be taller than wide"
    );

    // File row: collapse A again → rows [A, b.txt], the file at row 1
    // (y [32, 54)) with its File icon ink at indent+2.
    tree.toggle(&a, true);
    draw(&mut canvas, &mut text, &tree, Pointer::default());
    assert!(
        ink_bbox_in(&canvas, theme.bg, 12, 32, 27, 54).is_some(),
        "file icon ink in its depth-0 cell"
    );

    // Hover: row 0 under the pointer paints bg_elevated across the row.
    let hover_pointer = Pointer { x: 50.0, y: 21.0, down: false, click: false };
    draw(&mut canvas, &mut text, &tree, hover_pointer);
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 10, 10, 202, 32) > 1000,
        "hovered row paints bg_elevated"
    );
    assert_eq!(
        count_color_in(&canvas, theme.bg_elevated, 10, 32, 202, 54),
        0,
        "non-hovered rows stay clean"
    );

    // Selection: row bg selection, winning over hover when both apply.
    tree.select(&b);
    draw(&mut canvas, &mut text, &tree, Pointer::default());
    assert!(
        count_color_in(&canvas, theme.selection, 10, 32, 202, 54) > 1000,
        "selected row paints the selection bg"
    );
    draw(&mut canvas, &mut text, &tree, Pointer { x: 50.0, y: 43.0, down: false, click: false });
    assert!(
        count_color_in(&canvas, theme.selection, 10, 32, 202, 54) > 1000
            && count_color_in(&canvas, theme.bg_elevated, 10, 32, 202, 54) == 0,
        "selection wins when a selected row is also hovered"
    );
}

// ---- draw_tree_panel: degenerate frames ----

#[test]
fn panel_draw_degenerates_safely() {
    let (_d, tree) = panel_fixture("degenerate");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(220, 90);
    canvas.fill(theme.bg);

    for panel in [
        Rect { x: 10.0, y: 10.0, w: 0.0, h: 66.0 },
        Rect { x: 10.0, y: 10.0, w: 200.0, h: 0.0 },
        Rect { x: 10.0, y: 10.0, w: -5.0, h: 66.0 },
    ] {
        draw_tree_panel(&mut canvas, &mut text, &theme, 1.0, panel, &tree, Pointer::default(), None);
    }
    draw_tree_panel(&mut canvas, &mut text, &theme, 0.0, PANEL, &tree, Pointer::default(), None);
    draw_tree_panel(&mut canvas, &mut text, &theme, f64::NAN, PANEL, &tree, Pointer::default(), None);
    assert_eq!(
        count_color_in(&canvas, theme.bg, 0, 0, 220, 90),
        220 * 90,
        "degenerate frames rasterize nothing"
    );

    // NaN offset (pub field corrupted) must not panic either.
    let (_d4, mut nan_tree) = panel_fixture("degenerate-nan");
    nan_tree.offset = f64::NAN;
    draw_tree_panel(&mut canvas, &mut text, &theme, 1.0, PANEL, &nan_tree, Pointer::default(), None);
    assert!(row_at(PANEL, 50.0, 21.0, &nan_tree).is_none(), "NaN offset hits nothing");
}

// ---- M7: git badges ----

#[test]
fn git_badges_render_at_the_row_edge() {
    let (d, tree) = panel_fixture("git-badge");
    // Distinct badge tokens (the shared theme keeps them chrome-invisible).
    let mut theme = panel_theme();
    theme.git_modified = (250, 179, 135);
    theme.git_untracked = (148, 226, 213);
    theme.git_added = (166, 227, 161);
    // The fixture's b.txt is modified; A/ untracked.
    let status = crate::git::parse_status(
        "1 .M N... 000000 100644 100644 111 222 b.txt\n? A\n",
    )
    .rooted(d.path());
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(220, 90);
    canvas.fill(theme.bg);
    draw_tree_panel(&mut canvas, &mut text, &theme, 1.0, PANEL, &tree, Pointer::default(), Some(&status));

    // The badge zone sits left of the scrollbar track: [panel.x+w-8-12, panel.x+w-8).
    let zone_x0 = (PANEL.x + PANEL.w - crate::tree::TRACK_W - crate::tree::BADGE_W) as u32;
    let zone_x1 = (PANEL.x + PANEL.w - crate::tree::TRACK_W) as u32;
    assert!(
        count_color_in(&canvas, theme.git_modified, zone_x0, 32, zone_x1, 54) > 0,
        "M badge ink next to b.txt's row"
    );
    assert!(
        count_color_in(&canvas, theme.git_untracked, zone_x0, 10, zone_x1, 32) > 0,
        "U badge ink next to A's row (dirs badge too — untracked dir)"
    );
    assert!(
        count_color_in(&canvas, theme.git_modified, 30, 32, 190, 54) > 0,
        "b.txt's name uses the same color as its M badge"
    );
    assert!(
        count_color_in(&canvas, theme.git_untracked, 30, 10, 190, 32) > 0,
        "A's name uses the same color as its U badge"
    );
    // No badge on rows without a status.
    assert_eq!(
        count_color_in(&canvas, theme.git_added, zone_x0, 10, zone_x1, 54),
        0,
        "no A ink"
    );
}
