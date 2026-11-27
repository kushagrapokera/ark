//! Tree-icon regressions: language colors/tooltips, folder glyphs, and the
//! logical-coordinate contract under HiDPI.

use super::panel_tests::{panel_fixture, panel_theme};
use super::*;
use crate::fs::testutil::temp_dir;
use crate::text::TextSystem;
use crate::ui::testutil::{count_color_in, ink_bbox_in};
use std::fs;

#[test]
fn tree_file_icons_use_language_color_and_tooltip() {
    let d = temp_dir("tree-language-icon");
    fs::File::create(d.path().join("main.py")).expect("create python file");
    let tree = FileTree::new(d.path());
    let mut theme = panel_theme();
    theme.syntax_constant = (12, 34, 56);
    let panel = Rect {
        x: 10.0,
        y: 10.0,
        w: 200.0,
        h: 30.0,
    };
    let pointer = Pointer {
        x: panel.x + 2.0 + ICON_SIZE / 2.0,
        y: panel.y + ROW_H / 2.0,
        down: false,
        click: false,
    };
    let mut canvas = Canvas::new(220, 60);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let tooltip = draw_tree_panel_with_insertion(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        panel,
        &tree,
        pointer,
        None,
        None,
    )
    .expect("language icon tooltip");
    assert_eq!(tooltip.label, "Python File");
    assert!(
        count_color_in(&canvas, theme.syntax_constant, 12, 10, 28, 32) > 0,
        "Python icon must use its language color"
    );
}

#[test]
fn folder_rows_use_folder_glyph_color() {
    let (_d, tree) = panel_fixture("tree-folder-icon");
    let mut theme = panel_theme();
    theme.accent = (11, 22, 33);
    let mut canvas = Canvas::new(220, 60);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    draw_tree_panel(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 200.0,
            h: 30.0,
        },
        &tree,
        Pointer::default(),
        None,
    );
    assert!(
        count_color_in(&canvas, theme.accent, 10, 10, 26, 32) > 0,
        "folder row must draw a folder icon"
    );
}

#[test]
fn tree_icons_are_large_without_touching_labels() {
    const {
        assert!(ICON_SIZE >= 18.0, "tree icons need at least 18 logical px");
        assert!(
            ICON_SIZE + 2.0 <= ROW_H,
            "icons must retain vertical breathing room"
        );
    }

    // The filled badge is the widest file glyph; its right edge must stay
    // clear of the existing label origin at indent+20.
    let widest_symbol_right = 2.0 + ICON_SIZE / 2.0 + 0.42 * ICON_SIZE;
    assert!(
        widest_symbol_right <= 20.0,
        "larger icons would collide with row labels: {widest_symbol_right}"
    );
}

#[test]
fn tree_icon_positions_scale_exactly_once() {
    let (_d, tree) = panel_fixture("tree-icon-hidpi");
    let mut theme = panel_theme();
    theme.accent = theme.fg;
    let panel = Rect {
        x: 10.0,
        y: 10.0,
        w: 200.0,
        h: 30.0,
    };
    let mut text = TextSystem::new();
    let mut one = Canvas::new(220, 90);
    one.fill(theme.bg);
    draw_tree_panel(
        &mut one,
        &mut text,
        &theme,
        1.0,
        panel,
        &tree,
        Pointer::default(),
        None,
    );
    let mut two = Canvas::new(440, 180);
    two.fill(theme.bg);
    draw_tree_panel(
        &mut two,
        &mut text,
        &theme,
        2.0,
        panel,
        &tree,
        Pointer::default(),
        None,
    );

    // These bands contain only the folder glyph, not its label.
    let b1 = ink_bbox_in(&one, theme.bg, 0, 0, 28, 45).expect("1x folder icon");
    let b2 = ink_bbox_in(&two, theme.bg, 0, 0, 56, 90).expect("2x folder icon");
    for (axis, a, b) in [
        (0, b1.0, b2.0),
        (1, b1.1, b2.1),
        (2, b1.2, b2.2),
        (3, b1.3, b2.3),
    ] {
        assert!(
            (f64::from(b) - 2.0 * f64::from(a)).abs() <= 2.0,
            "bbox edge {axis}: 2x {b} must be twice 1x {a} (±2px AA slop)"
        );
    }
}
