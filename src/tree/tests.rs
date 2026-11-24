//! FileTree tests (in the module's own subdirectory: impl + tests together
//! would pass the ~400-line module guidance from AGENTS.md).
use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use std::fs;

/// Fixture: root containing `A/` (itself containing `C/` and `a.txt`)
/// and `b.txt`. Root listing sorts to [A, b.txt]; A sorts to [C, a.txt].
fn tree_fixture(label: &str) -> (TempDir, FileTree) {
    let d = temp_dir(label);
    fs::create_dir_all(d.path().join("A/C")).expect("create A/C");
    fs::File::create(d.path().join("A/a.txt")).expect("create A/a.txt");
    fs::File::create(d.path().join("b.txt")).expect("create b.txt");
    let tree = FileTree::new(d.path());
    (d, tree)
}

// ---- 1. toggle transitions ----

#[test]
fn toggle_expands_and_collapses() {
    let (d, mut tree) = tree_fixture("toggle");
    let a = d.path().join("A");
    assert!(tree.toggle(&a, true), "first toggle expands (changed)");
    assert!(tree.is_expanded(&a), "toggle(dir) → is_expanded");
    assert!(tree.toggle(&a, true), "second toggle collapses (changed)");
    assert!(!tree.is_expanded(&a), "toggle again → collapsed");

    // Files never expand, whatever the caller claims.
    let b = d.path().join("b.txt");
    assert!(!tree.toggle(&b, false), "toggle(file) returns false");
    assert!(!tree.is_expanded(&b), "a file never expands");
    assert_eq!(tree.expanded_dir_count(), 0, "no dir was expanded");
}

// ---- 2. MEMORY RULE: collapse frees the cached listing ----

#[test]
fn collapse_frees_cached_listing() {
    let (d, mut tree) = tree_fixture("cache");
    let a = d.path().join("A");
    assert_eq!(tree.visible_count(), 2, "collapsed tree shows root entries only");

    assert!(tree.toggle(&a, true), "expand A (2 children: C, a.txt)");
    assert_eq!(tree.visible_count(), 4, "visible rows include A's children");
    assert_eq!(tree.expanded_dir_count(), 1);
    assert_eq!(tree.cached_listing_count(), 2, "root + A are cached");

    assert!(tree.toggle(&a, true), "collapse A");
    assert_eq!(tree.visible_count(), 2, "collapse drops A's rows");
    assert_eq!(
        tree.expanded_dir_count(),
        0,
        "MEMORY RULE: no expanded dirs remain"
    );
    assert_eq!(
        tree.cached_listing_count(),
        1,
        "MEMORY RULE: A's cached listing was freed, only the root's remains"
    );
}

#[test]
fn collapse_all_frees_every_expanded_listing_without_allocating() {
    let (d, mut tree) = tree_fixture("collapse-all");
    let a = d.path().join("A");
    let c = d.path().join("A/C");
    assert!(tree.toggle(&a, true));
    assert!(tree.toggle(&c, true));
    assert_eq!(tree.cached_listing_count(), 3);
    tree.offset = 100.0;

    assert!(tree.collapse_all());
    assert_eq!(tree.expanded_dir_count(), 0);
    assert_eq!(tree.cached_listing_count(), 1, "only the root listing remains");
    assert_eq!(tree.offset, 0.0);
    assert!(!tree.collapse_all(), "already collapsed is a no-op");
}

// ---- 3. depth-first row projection ----

#[test]
fn visible_rows_depth_first_order() {
    let (d, mut tree) = tree_fixture("rows");
    let a = d.path().join("A");

    assert!(tree.toggle(&a, true), "expand A");
    let rows = tree.visible_rows();
    let summary: Vec<(&str, usize, bool, bool)> = rows
        .iter()
        .map(|r| (r.name, r.depth, r.is_dir, r.expanded))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("A", 0, true, true),      // root child, expanded
            ("C", 1, true, false),     // A's subdir, collapsed
            ("a.txt", 1, false, false), // A's file
            ("b.txt", 0, false, false), // root child
        ],
        "depth-first, dirs-first order, correct depths"
    );
    assert_eq!(rows[0].path, a.as_path(), "rows carry full paths");
    assert_eq!(rows[1].path, d.path().join("A/C").as_path());
    assert_eq!(rows.len(), tree.visible_count());

    tree.toggle(&a, true); // collapse
    let rows = tree.visible_rows();
    let summary: Vec<(&str, usize, bool, bool)> = rows
        .iter()
        .map(|r| (r.name, r.depth, r.is_dir, r.expanded))
        .collect();
    assert_eq!(
        summary,
        vec![("A", 0, true, false), ("b.txt", 0, false, false)],
        "collapsed A hides its subtree"
    );
}

// ---- 4. MEMORY RULE: rows borrow, never clone ----

#[test]
fn row_refs_borrow_from_state() {
    let (d, mut tree) = tree_fixture("borrow");
    let a = d.path().join("A");
    tree.toggle(&a, true);

    // Two independent calls must reference the SAME cached storage —
    // cloned listings would live in two distinct live allocations.
    let r1 = tree.visible_rows();
    let r2 = tree.visible_rows();
    assert!(
        std::ptr::eq(r1[0].path, r2[0].path) && std::ptr::eq(r1[0].name, r2[0].name),
        "RowRef path/name borrow the tree's cached listings (no per-call clone)"
    );
    // The per-frame Vec is bounded by visible nodes, not content size.
    assert_eq!(r1.len(), tree.visible_count());
    assert_eq!(r2.len(), tree.visible_count());
}

// ---- 5. scroll clamping ----

#[test]
fn scroll_clamps_to_visible_content() {
    let d = temp_dir("scroll");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
        fs::File::create(d.path().join(name)).expect("create file");
    }
    let mut tree = FileTree::new(d.path());
    assert_eq!(tree.visible_count(), 5);
    // Viewport 100 ≈ 4.5 rows: max offset = 5×ROW_H − 100 = 10.
    let viewport = 100.0;
    let max_offset = 5.0 * ROW_H - viewport;
    assert_eq!(max_offset, 10.0);

    assert!(tree.scroll_by(100.0, viewport), "overshoot counts as a change");
    assert_eq!(tree.offset, max_offset, "offset clamps to count×ROW_H − viewport");
    assert!(!tree.scroll_by(1.0, viewport), "at max: no movement, no change");
    assert!(!tree.set_offset(max_offset, viewport), "setting the current value: no change");
    // Move within range, then set beyond max: set_offset clamps and
    // reports the move (3.0 → clamped max).
    assert!(tree.set_offset(3.0, viewport), "set within range moves");
    assert_eq!(tree.offset, 3.0);
    assert!(tree.set_offset(500.0, viewport), "set_offset clamps too (moved within range)");
    assert_eq!(tree.offset, max_offset);
    assert!(tree.scroll_by(-100.0, viewport), "scroll up clamps at 0");
    assert_eq!(tree.offset, 0.0);
    assert!(!tree.scroll_by(-1.0, viewport), "at min: no movement, no change");
    // Negative set from a non-zero offset clamps to 0 and reports the move.
    assert!(tree.set_offset(5.0, viewport), "move within range first");
    assert!(tree.set_offset(-50.0, viewport), "negative set clamps to 0 (moved)");
    assert_eq!(tree.offset, 0.0);

    // Empty tree: offset is forced 0, nothing ever moves.
    let e = temp_dir("scroll-empty");
    let mut empty = FileTree::new(e.path());
    assert_eq!(empty.visible_count(), 0);
    assert!(!empty.scroll_by(40.0, viewport), "empty tree cannot scroll");
    assert_eq!(empty.offset, 0.0, "empty tree forces offset 0");
    assert!(!empty.set_offset(500.0, viewport), "set on an empty tree forces 0");
    assert_eq!(empty.offset, 0.0);
}

// ---- 5b. non-finite scroll inputs (mirrors ui::Scrollbar discipline) ----

#[test]
fn scroll_nan_inf_inputs_are_noops() {
    let d = temp_dir("scroll-nan");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
        fs::File::create(d.path().join(name)).expect("create file");
    }
    let mut tree = FileTree::new(d.path());
    assert_eq!(tree.visible_count(), 5);
    let viewport = 100.0; // content 5×ROW_H = 110 → max offset 10
    assert!(tree.set_offset(5.0, viewport));
    assert_eq!(tree.offset, 5.0);

    // Non-finite deltas never move or corrupt the offset.
    assert!(!tree.scroll_by(f64::NAN, viewport), "NaN dy is a no-op");
    assert_eq!(tree.offset, 5.0);
    assert!(!tree.scroll_by(f64::INFINITY, viewport), "+Inf dy is a no-op");
    assert_eq!(tree.offset, 5.0);
    assert!(!tree.scroll_by(f64::NEG_INFINITY, viewport), "−Inf dy is a no-op");
    assert_eq!(tree.offset, 5.0);

    // Non-finite set values are ignored too.
    assert!(!tree.set_offset(f64::NAN, viewport), "NaN set is a no-op");
    assert_eq!(tree.offset, 5.0);
    assert!(!tree.set_offset(f64::INFINITY, viewport), "+Inf set is a no-op");
    assert_eq!(tree.offset, 5.0);

    // The pub offset field can be corrupted directly; the next scroll call
    // repairs it (non-finite → 0, like Scrollbar::clamp).
    tree.offset = f64::NAN;
    assert!(tree.scroll_by(1.0, viewport), "repairing NaN → 0 counts as a change");
    assert_eq!(tree.offset, 0.0, "non-finite pub offset snaps to 0");
    assert!(tree.scroll_by(3.0, viewport), "scrolling works again after the repair");
    assert_eq!(tree.offset, 3.0);
    tree.offset = f64::INFINITY;
    assert!(tree.set_offset(2.0, viewport), "set repairs +Inf (moved)");
    assert_eq!(tree.offset, 2.0);
}

// ---- 6. selection semantics ----

#[test]
fn offset_clamp_accounts_for_a_virtual_row() {
    let (_d, mut tree) = tree_fixture("virtual-row-offset");
    let viewport = 44.0;
    let normal_max = tree.visible_count() as f64 * ROW_H - viewport;
    assert!(tree.set_offset_with_extra(normal_max + ROW_H, viewport, 1));
    assert_eq!(tree.offset, normal_max + ROW_H);
    assert!(tree.set_offset(tree.offset, viewport));
    assert_eq!(tree.offset, normal_max, "removing the virtual row re-clamps");
}

#[test]
fn expand_to_reveals_a_nested_directory_and_its_ancestors() {
    let (d, mut tree) = tree_fixture("expand-to");
    let a = d.path().join("A");
    let c = a.join("C");
    assert!(tree.toggle(&a, true));
    assert!(tree.toggle(&c, true));
    assert!(tree.toggle(&a, true), "collapse the ancestor");

    assert!(tree.expand_to(&c));
    assert!(tree.is_expanded(&a));
    assert!(tree.is_expanded(&c));
    assert!(tree.visible_rows().iter().any(|row| row.path == c));
}

#[test]
fn selection_semantics() {
    let (d, mut tree) = tree_fixture("select");
    let a = d.path().join("A");
    let a_txt = d.path().join("A/a.txt");

    tree.select(&a_txt);
    assert_eq!(tree.selected(), Some(a_txt.as_path()), "select sets the selection");

    // Selecting a directory never toggles its expansion.
    tree.clear_selection();
    tree.select(&a);
    assert!(!tree.is_expanded(&a), "select(dir) does NOT expand");
    assert_eq!(tree.expanded_dir_count(), 0);

    // clear_selection empties.
    tree.clear_selection();
    assert_eq!(tree.selected(), None, "clear empties the selection");

    // Selection survives collapse: the selected node stays selected
    // while hidden (the highlight simply vanishes until re-expanded).
    tree.toggle(&a, true); // expand A
    tree.select(&a_txt);
    tree.toggle(&a, true); // collapse A — a.txt is now hidden
    assert_eq!(
        tree.selected(),
        Some(a_txt.as_path()),
        "selection of a collapsed-then-hidden node REMAINS"
    );
}

// ---- 6b. selection getter round-trip (Fix A, Task 2) ----

#[test]
fn selected_getter_reflects_state() {
    let (d, mut tree) = tree_fixture("getter");
    assert_eq!(tree.selected(), None, "fresh tree has no selection");
    let b = d.path().join("b.txt");
    tree.select(&b);
    assert_eq!(tree.selected(), Some(b.as_path()), "getter reflects select");
    tree.clear_selection();
    assert_eq!(tree.selected(), None, "getter reflects clear");
}

// ---- 6c. last_error is a "most recent listing outcome" surface ----

#[test]
fn successful_listing_clears_last_error() {
    let d = temp_dir("err-clear");
    fs::create_dir_all(d.path().join("A")).expect("create A");
    let missing = d.path().join("vanished");
    let mut tree = FileTree::new(d.path());
    assert!(tree.last_error.is_none(), "healthy root starts clean");

    assert!(!tree.toggle(&missing, true), "failed listing keeps the dir collapsed");
    assert!(tree.last_error.is_some(), "failure surfaces");
    assert!(
        tree.toggle(&d.path().join("A"), true),
        "a subsequent successful listing …"
    );
    assert!(
        tree.last_error.is_none(),
        "… clears the error: last_error is None while every listing succeeded"
    );
}

// ---- 7. root listing failure surfaces, never panics ----

#[test]
fn root_error_surfaces() {
    let d = temp_dir("root-err");
    let missing = d.path().join("not-there");
    let tree = FileTree::new(&missing);
    assert!(tree.last_error.is_some(), "root listing failure surfaces in last_error");
    assert_eq!(tree.visible_count(), 0, "failed root → empty tree");
    assert!(tree.visible_rows().is_empty());
}

// ---- 8. (extra, error path) failed subdirectory listing surfaces ----

#[test]
fn toggle_failure_surfaces_last_error() {
    let (d, mut tree) = tree_fixture("toggle-err");
    let missing = d.path().join("vanished");
    assert!(!tree.toggle(&missing, true), "a failed listing changes nothing");
    assert!(!tree.is_expanded(&missing), "failed dir stays collapsed");
    assert!(
        tree.last_error.is_some(),
        "no silent failures: the status-line error surface is set"
    );
}

// ---- 9. live refresh (M5) ----

#[test]
fn refresh_picks_up_new_and_removed_rows() {
    let (d, mut tree) = tree_fixture("refresh");
    let a = d.path().join("A");
    tree.toggle(&a, true); // expand A → [A, C, a.txt, b.txt]

    fs::File::create(d.path().join("new.txt")).expect("create new root file");
    fs::File::create(d.path().join("A/new-a.txt")).expect("create file inside expanded A");
    fs::remove_dir_all(d.path().join("A/C")).expect("remove expanded A's subdir");
    tree.refresh();

    let summary: Vec<(&str, usize)> =
        tree.visible_rows().iter().map(|r| (r.name, r.depth)).collect();
    assert_eq!(
        summary,
        vec![("A", 0), ("a.txt", 1), ("new-a.txt", 1), ("b.txt", 0), ("new.txt", 0)],
        "re-listed: new root row, new child row, vanished subdir gone"
    );
}

#[test]
fn refresh_collapses_vanished_dirs() {
    let (d, mut tree) = tree_fixture("refresh-vanish");
    tree.toggle(&d.path().join("A"), true);
    assert_eq!(tree.expanded_dir_count(), 1);
    fs::remove_dir_all(d.path().join("A")).expect("remove expanded dir");
    tree.refresh();
    assert_eq!(tree.expanded_dir_count(), 0, "vanished dir collapsed");
    assert_eq!(
        tree.visible_rows().iter().map(|r| r.name).collect::<Vec<_>>(),
        vec!["b.txt"],
        "only surviving rows remain"
    );
}

#[test]
fn refresh_surfaces_root_failure_and_empties() {
    let d = temp_dir("refresh-root");
    fs::File::create(d.path().join("x.txt")).expect("create");
    let mut tree = FileTree::new(d.path());
    assert!(tree.last_error.is_none());
    drop(d); // the root vanishes with the temp dir
    tree.refresh();
    assert!(tree.last_error.is_some(), "root listing failure surfaces");
    assert_eq!(tree.visible_count(), 0);
}
