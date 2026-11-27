//! Frame tests (impl + tests together would pass the ~400-line module
//! guidance): layout pins, scene ink, interaction effects, and the M4
//! viewer-pane wiring.

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
use crate::new_item::{NewItemKind, NewItemPrompt};
use crate::search;
use crate::ui::testutil::count_color_in;
use std::fs;

// ---- layout ----

#[test]
fn app_layout_matches_card_design() {
    let l = app_layout(1024.0, 768.0, 304.0);
    assert_eq!(
        l.title_bar,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1024.0,
            h: 36.0
        }
    );
    assert_eq!(
        l.workbench,
        Rect {
            x: 8.0,
            y: 44.0,
            w: 1008.0,
            h: 716.0
        }
    );
    assert_eq!(
        l.activity_bar,
        Rect {
            x: 8.0,
            y: 44.0,
            w: 48.0,
            h: 716.0
        }
    );
    assert_eq!(
        l.explorer_card,
        Rect {
            x: 64.0,
            y: 44.0,
            w: 240.0,
            h: 716.0
        }
    );
    assert_eq!(
        l.tree_panel,
        Rect {
            x: 72.0,
            y: 108.0,
            w: 224.0,
            h: 644.0
        }
    );
    assert_eq!(
        l.scroll_track,
        Rect {
            x: 288.0,
            y: 108.0,
            w: 8.0,
            h: 644.0
        }
    );
    assert_eq!(
        l.editor_card,
        Rect {
            x: 312.0,
            y: 44.0,
            w: 704.0,
            h: 716.0
        }
    );
    assert_eq!(
        l.editor_tabs,
        Rect {
            x: 312.0,
            y: 44.0,
            w: 704.0,
            h: 36.0
        }
    );
    assert_eq!(
        l.breadcrumb,
        Rect {
            x: 312.0,
            y: 80.0,
            w: 704.0,
            h: 24.0
        }
    );
    assert_eq!(
        l.viewer_pane,
        Rect {
            x: 320.0,
            y: 104.0,
            w: 688.0,
            h: 648.0
        }
    );
    assert_eq!(
        l.term_dock,
        Rect {
            x: 312.0,
            y: 760.0,
            w: 704.0,
            h: 0.0
        }
    );
    assert_eq!(
        l.split_area,
        Rect {
            x: 304.0,
            y: 44.0,
            w: 8.0,
            h: 716.0
        }
    );
    // Degenerate window sizes never produce negative extents.
    let tiny = app_layout(400.0, 300.0, 160.0);
    assert!(tiny.tree_panel.w >= 0.0 && tiny.tree_panel.h >= 0.0);
    assert!(tiny.viewer_pane.w >= 0.0 && tiny.workbench.h >= 0.0);
    assert_eq!(
        app_layout(1024.0, 768.0, 8.0).explorer_card.w,
        160.0,
        "minimum explorer width"
    );
}

#[test]
fn workbench_reclaims_the_former_status_bar_space() {
    let layout = app_layout(1024.0, 768.0, 304.0);
    assert_eq!(layout.workbench.y + layout.workbench.h, 760.0);
    assert_eq!(layout.activity_bar.h, 716.0);
    assert_eq!(layout.explorer_card.h, 716.0);
    assert_eq!(layout.editor_card.h, 716.0);
}

#[test]
fn bottom_edge_has_no_status_bar() {
    for (w, h) in [(1024.0, 768.0), (480.0, 640.0), (400.0, 300.0)] {
        let (_d, mut tree) = scene_fixture("bottom-no-status");
        let mut theme = frame_theme();
        theme.surface_chrome = (1, 1, 1);
        let layout = app_layout_with_commit(w, h, 304.0, false, false);
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(w as u32, h as u32);
        draw_static_frame(
            &mut canvas,
            &mut text,
            &theme,
            &mut tree,
            &layout,
            Pointer::default(),
            &[],
            None,
        );
        let bottom = (layout.workbench.y + layout.workbench.h).ceil() as u32;
        assert_eq!(
            crate::ui::testutil::count_not_color_in(
                &canvas,
                theme.surface_chrome,
                0,
                bottom,
                w as u32,
                h as u32,
            ),
            0,
            "the former status-bar strip is empty at {w}×{h}; ink={:?}",
            crate::ui::testutil::ink_bbox_in(
                &canvas,
                theme.surface_chrome,
                0,
                bottom,
                w as u32,
                h as u32,
            )
        );
    }
}

#[test]
fn responsive_layout_prioritizes_editor_on_small_windows() {
    let small = app_layout_with_commit(480.0, 640.0, 304.0, true, true);
    assert_eq!(small.explorer_card.w, 0.0, "compact width hides Explorer");
    assert_eq!(small.tree_panel.w, 0.0);
    assert_eq!(
        small.split_area.w, 0.0,
        "hidden Explorer has no resize handle"
    );
    assert!(sidebar_handle(&small, 304.0).is_none());
    assert_eq!(small.activity_bar.w, 40.0, "compact activity card");
    assert!(
        small.editor_card.w > small.workbench.w * 0.75,
        "editor receives the space freed by Explorer"
    );
    assert_eq!(
        small.term_dock.h, 140.0,
        "compact width shrinks the terminal"
    );
    assert!(small.term_grid.w > 0.0);

    let short = app_layout_with_commit(600.0, 480.0, 304.0, true, true);
    assert_eq!(short.explorer_card.w, 0.0);
    assert_eq!(short.terminal_toolbar.h, 30.0);
    assert!(short.term_grid.h > 0.0 && short.viewer_pane.h > 0.0);

    let full = app_layout_with_commit(1440.0, 900.0, 304.0, true, true);
    assert_eq!(full.explorer_card.w, 240.0);
    assert!(
        full.editor_card.w > 1_000.0,
        "large window expands the editor"
    );
    assert_eq!(full.term_dock.h, 224.0);

    // Every region remains non-negative at the window's minimum size.
    let minimum = app_layout_with_commit(400.0, 300.0, 304.0, true, true);
    for rect in [
        minimum.activity_bar,
        minimum.explorer_card,
        minimum.editor_card,
        minimum.viewer_pane,
        minimum.term_dock,
        minimum.term_grid,
    ] {
        assert!(rect.w >= 0.0 && rect.h >= 0.0, "degenerate rect: {rect:?}");
    }
}

#[test]
fn editor_actions_omit_agent_launcher_icons() {
    let (_d, mut tree) = scene_fixture("editor-agent-icons");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.bg_elevated, 932, 48, 970, 76),
        0,
        "Claude/Codex editor-action placeholders are gone"
    );
}

#[test]
fn compact_editor_tabs_stay_clear_of_actions() {
    let layout = app_layout_with_commit(480.0, 640.0, 304.0, false, true);
    let strip = chrome::tab_strip(&layout);
    assert!(
        strip.w >= layout.editor_card.w - 60.0,
        "narrow strips reserve only the actions that are drawn"
    );
    let rects = file_tab_rects(strip, 9);
    assert!(!rects.is_empty());
    assert!(
        rects.last().expect("tab").x + rects.last().expect("tab").w <= strip.x + strip.w,
        "bounded tab cache never paints under editor actions"
    );

    let full = app_layout(1024.0, 768.0, 304.0);
    assert_eq!(
        chrome::tab_strip(&full).w,
        666.0,
        "full layout reserves only the Split action"
    );
}

#[test]
fn hidden_explorer_does_not_draw_a_resize_handle() {
    let (_d, mut tree) = scene_fixture("hidden-handle");
    let theme = frame_theme();
    let layout = app_layout_with_commit(480.0, 640.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(480, 640);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        count_color_in(&canvas, theme.fg_dim, 296, 44, 320, 80),
        0,
        "the divider handle disappears with the Explorer card"
    );
}

#[test]
fn compact_terminal_plus_button_opens_the_first_shell() {
    let (_d, mut tree) = scene_fixture("compact-terminal");
    let theme = frame_theme();
    let layout = app_layout_with_commit(480.0, 640.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(480, 640);
    let plus = chrome::terminal_actions(&layout).plus;
    let plus_x = plus.x + plus.w / 2.0;
    let plus_y = plus.y + plus.h / 2.0;
    let effects = draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: plus_x,
            y: plus_y,
            down: false,
            click: true,
        },
        &[],
        None,
    );
    assert!(
        effects.open_terminal,
        "compact toolbar keeps + aligned and clickable"
    );
}

#[test]
fn new_item_prompt_is_an_inline_child_row() {
    let (_d, mut tree) = scene_fixture("new-item-inline");
    let layout = app_layout(1024.0, 768.0, 304.0);
    let parent = tree.root.join("A");
    tree.select(&parent);
    let anchored = chrome::new_item_layout(&layout, &tree, &parent).expect("prompt layout");
    assert_eq!(
        anchored.input,
        Rect {
            x: 108.0,
            y: 131.0,
            w: 176.0,
            h: 20.0,
        },
        "field occupies the first child row below the selected folder"
    );
    assert_eq!(anchored.handle.x, 92.0, "drag handle precedes the field");
    assert_eq!(anchored.handle.y, anchored.input.y);

    let nested = parent.join("C");
    assert!(tree.toggle(&parent, true));
    tree.select(&nested);
    let nested_layout = chrome::new_item_layout(&layout, &tree, &nested).expect("nested layout");
    assert_eq!(nested_layout.input.y, 153.0);
    assert_eq!(nested_layout.input.x, 124.0, "nested field follows the row indent");

    let root_layout = chrome::new_item_layout(&layout, &tree, &tree.root).expect("root layout");
    assert_eq!(root_layout.input.y, 109.0, "root field is the first tree row");
}

#[test]
fn new_item_prompt_draws_an_inline_field_without_buttons() {
    let (_d, mut tree) = scene_fixture("new-item-inline-draw");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut prompt = NewItemPrompt::new(NewItemKind::File, tree.root.clone());
    let mut canvas = Canvas::new(1024, 768);
    let mut text = TextSystem::new();
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        ui_scale: 1.0,
        activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
        animation_time: 0.0,
        title: "ark",
        root: Path::new("/repo"),
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
        commit: None,
        config: None,
        config_error: None,
        config_error_path: None,
        terms: None,
        term_focused: false,
        groups: None,
        active_group: 0,
        group_viewers: [None, None],
        group_flash: [false, false],
        group_scrollbars: None,
        group_split: None,
        group_split_drag: None,
        new_item: Some(&mut prompt),
        icon_tooltip: None,
        search: None,
        markdown: None,
        update: None,
    };
    let _ = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    let field = chrome::new_item_layout(&layout, &tree, &tree.root).expect("field").input;
    assert!(
        count_color_in(
            &canvas,
            theme.accent,
            field.x as u32,
            field.y as u32,
            (field.x + field.w) as u32,
            (field.y + field.h) as u32,
        ) > 0,
        "inline focused field paints its accent border"
    );
}

#[test]
fn explorer_root_actions_fit_without_overlap_or_title_collision() {
    let layout = app_layout_with_commit(1440.0, 900.0, 304.0, false, false);
    let actions = chrome::explorer_action_rects(&layout);
    assert!(actions.windows(2).all(|pair| pair[0].x + pair[0].w <= pair[1].x));
    assert!(actions[3].x + actions[3].w <= layout.explorer_root.x + layout.explorer_root.w);
    assert!(layout.explorer_root.x + 28.0 < actions[0].x - 8.0);
}

#[test]
fn editor_strip_has_no_more_action() {
    assert_eq!(
        chrome::editor_action_icons(700.0, true, false, false),
        &[crate::icons::Icon::Split]
    );
    assert_eq!(
        chrome::editor_action_icons(700.0, true, false, true),
        &[crate::icons::Icon::Close, crate::icons::Icon::Split]
    );
    assert_eq!(
        chrome::editor_action_icons(700.0, false, false, false),
        &[crate::icons::Icon::Split]
    );
    assert_eq!(
        chrome::editor_action_icons(700.0, false, true, false),
        &[crate::icons::Icon::Markdown, crate::icons::Icon::Split]
    );
    assert_eq!(
        chrome::editor_action_icons(700.0, true, true, true),
        &[
            crate::icons::Icon::Close,
            crate::icons::Icon::Markdown,
            crate::icons::Icon::Split,
        ]
    );
}

#[test]
fn primary_split_group_does_not_draw_a_close_group_action() {
    assert_eq!(
        chrome::editor_action_icons(700.0, true, true, false),
        &[crate::icons::Icon::Markdown, crate::icons::Icon::Split],
        "only the secondary split group needs a close-group action"
    );
}

#[test]
fn split_markdown_actions_are_spaced_and_clear_of_tabs() {
    let tabs = Rect { x: 312.0, y: 44.0, w: 540.0, h: 36.0 };
    for close_group in [false, true] {
        let actions = chrome::editor_action_icons(tabs.w, true, true, close_group);
        let reserve = chrome::editor_action_reserve(actions.len());
        let pane = Rect { w: tabs.w - reserve, ..tabs };
        let tab_rects = file_tab_rects(pane, 9);
        let action_rects = chrome::editor_action_rects(tabs, actions.len());

        assert!(tab_rects.last().expect("tab").x + tab_rects.last().expect("tab").w <= pane.x + pane.w);
        assert!(
            pane.x + pane.w + 4.0 <= action_rects.first().expect("action").x,
            "tabs must stop before the action hit areas"
        );
        assert!(
            action_rects.windows(2).all(|pair| pair[0].x + pair[0].w + 4.0 <= pair[1].x),
            "Markdown and Split actions need a visible gap"
        );
        assert!(
            action_rects.last().expect("action").x + action_rects.last().expect("action").w
                <= tabs.x + tabs.w
        );
    }
}

#[test]
fn markdown_action_is_clickable_only_for_markdown_tabs() {
    let d = temp_dir("markdown-action");
    let readme = d.path().join("README.md");
    let guide = d.path().join("guide.md");
    fs::write(&readme, "# Title\n").expect("write readme");
    fs::write(&guide, "# Guide\n").expect("write guide");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let (_d, mut tree) = scene_fixture("readme-action-tree");

    for (path, expected) in [
        (readme.as_path(), true),
        (guide.as_path(), true),
        (Path::new("/repo/main.rs"), false),
    ] {
        let files = vec![path.to_path_buf()];
        let effects = draw_static_frame_with(
            &mut canvas,
            &mut text,
            &theme,
            &mut tree,
            &layout,
            Pointer {
                x: layout.editor_tabs.x + layout.editor_tabs.w - 36.0,
                y: layout.editor_tabs.y + layout.editor_tabs.h / 2.0,
                down: false,
                click: true,
            },
            &files,
            Some(path),
            ActivityPanel::Explorer,
            None,
            None,
        );
        assert_eq!(effects.toggle_markdown, expected, "active path {path:?}");
    }
}

#[test]
fn explorer_root_actions_report_clicks() {
    let (_d, mut tree) = scene_fixture("explorer-actions");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut canvas = Canvas::new(1024, 768);
    let mut text = TextSystem::new();
    let action_rects = chrome::explorer_action_rects(&layout);
    for (rect, expected) in action_rects.into_iter().zip(["file", "folder", "refresh", "collapse"]) {
        let effects = draw_static_frame(
            &mut canvas,
            &mut text,
            &theme,
            &mut tree,
            &layout,
            Pointer {
                x: rect.x + rect.w / 2.0,
                y: rect.y + rect.h / 2.0,
                down: false,
                click: true,
            },
            &[],
            None,
        );
        match expected {
            "file" => assert!(effects.new_file),
            "folder" => assert!(effects.new_folder),
            "refresh" => assert!(effects.refresh_tree),
            "collapse" => assert!(effects.collapse_tree),
            _ => unreachable!(),
        }
    }
}

#[test]
fn short_activity_bar_keeps_search_action_reachable() {
    let (_d, mut tree) = scene_fixture("compact-activity");
    let theme = frame_theme();
    let layout = app_layout_with_commit(400.0, 300.0, 304.0, false, true);
    let search_icon = chrome::activity_item_rect(&layout, 1).expect("search icon");
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(400, 300);
    let effects = draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: search_icon.x + search_icon.w / 2.0,
            y: search_icon.y + search_icon.h / 2.0,
            down: false,
            click: true,
        },
        &[],
        None,
    );
    assert_eq!(effects.set_activity, Some(ActivityPanel::Search));
}

#[test]
fn compact_breadcrumb_stays_inside_the_editor_card() {
    let (_d, mut tree) = scene_fixture("compact-breadcrumb");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    let layout = app_layout_with_commit(480.0, 640.0, 304.0, false, true);
    let path = PathBuf::from(
        "/repo/a/very/long/project/path/with/many/components/that/overflows/the/editor/breadcrumb.rs",
    );
    let files = vec![path.clone()];
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(480, 640);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &files,
        Some(&path),
    );
    let pad_x = (layout.breadcrumb.x + layout.breadcrumb.w) as u32;
    assert_eq!(
        crate::ui::testutil::count_not_color_in(
            &canvas,
            theme.surface_chrome,
            pad_x,
            layout.breadcrumb.y as u32,
            480,
            (layout.breadcrumb.y + layout.breadcrumb.h) as u32,
        ),
        0,
        "breadcrumb ink is clipped to the editor card"
    );
}

#[test]
fn title_bar_without_update_controller_has_no_search_field() {
    let (_d, mut tree) = scene_fixture("title-clean");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.surface = (10, 20, 30);
    theme.accent = (200, 220, 255);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        count_color_in(&canvas, theme.surface, 300, 6, 700, 30),
        0,
        "the top-center search/command field is gone"
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, 700, 6, 900, 30),
        0,
        "the legacy frame without an update controller paints no update badge"
    );
    let actions = chrome::title_bar_actions(&layout);
    assert!(actions.explorer.contains(968.0, 18.0));
    assert!(actions.terminal.contains(992.0, 18.0));
}

#[test]
fn title_bar_removes_menus_and_navigation() {
    let (_d, mut tree) = scene_fixture("title-no-menus");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );

    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.surface_chrome, 40, 0, 900, 36),
        0,
        "File/Edit/Selection/View/Go, More, Back, and Forward are gone"
    );
}

#[test]
fn title_bar_logo_is_larger() {
    let (_d, mut tree) = scene_fixture("title-logo-size");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.accent = (200, 220, 255);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );

    let (x0, y0, x1, y1) = crate::ui::testutil::ink_bbox_in(
        &canvas,
        theme.surface_chrome,
        0,
        0,
        40,
        36,
    )
    .expect("logo ink");
    assert!(x1 - x0 >= 22 && y1 - y0 >= 20, "larger logo bbox: {:?}", (x0, y0, x1, y1));
}

#[test]
fn explorer_header_has_no_more_icon() {
    let d = temp_dir("explorer-header-clean");
    let mut tree = FileTree::new(d.path());
    let mut theme = frame_theme();
    theme.bg_elevated = (10, 20, 30);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );

    assert_eq!(
        crate::ui::testutil::count_not_color_in(
            &canvas,
            theme.bg_elevated,
            layout.explorer_header.y as u32 + 2,
            200,
            (layout.explorer_header.x + layout.explorer_header.w - 3.0) as u32,
            (layout.explorer_header.y + layout.explorer_header.h - 2.0) as u32,
        ),
        0,
        "the Explorer header ellipsis is gone"
    );
}

#[test]
fn hovering_an_icon_shows_its_name() {
    let d = temp_dir("icon-tooltip");
    let mut tree = FileTree::new(d.path());
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.bg_elevated = (10, 20, 30);
    theme.fg = (255, 255, 255);
    theme.fg_dim = theme.surface_chrome;
    let layout = app_layout(1024.0, 768.0, 304.0);
    let action = chrome::explorer_action_rects(&layout)[0];
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: action.x + action.w / 2.0,
            y: action.y + action.h / 2.0,
            down: false,
            click: false,
        },
        &[],
        None,
    );

    let center_x = action.x + action.w / 2.0;
    let tooltip_ink = crate::ui::testutil::count_not_color_in(
        &canvas,
        theme.bg_elevated,
        (center_x - 32.0) as u32,
        (action.y + action.h + 6.0) as u32,
        (center_x + 32.0) as u32,
        (action.y + action.h + 24.0) as u32,
    );
    assert!(
        tooltip_ink > 0,
        "hovering New File paints its tooltip name below the icon; ink={tooltip_ink}"
    );
}

#[test]
fn title_bar_layout_buttons_report_their_actions() {
    let (_d, mut tree) = scene_fixture("title-actions");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let actions = chrome::title_bar_actions(&layout);
    for (rect, expected) in [
        (actions.explorer, LayoutAction::ToggleExplorer),
        (actions.terminal, LayoutAction::ToggleTerminal),
    ] {
        let effects = draw_static_frame(
            &mut canvas,
            &mut text,
            &theme,
            &mut tree,
            &layout,
            Pointer {
                x: rect.x + rect.w / 2.0,
                y: rect.y + rect.h / 2.0,
                down: false,
                click: true,
            },
            &[],
            None,
        );
        assert_eq!(effects.layout_action, Some(expected));
    }
}

#[test]
fn frame_update_actions_reach_effects() {
    let (_d, mut tree) = scene_fixture("update-actions");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut canvas = Canvas::new(1024, 768);
    let mut text = TextSystem::new();
    let install_layout = ark::update::InstallLayout::new(
        PathBuf::from("/data"),
        PathBuf::from("/bin"),
        PathBuf::from("/state"),
        PathBuf::from("/cache"),
        PathBuf::from("/config"),
    );
    let mut controller = crate::update_process::UpdateController::new(
        Some(install_layout),
        Some(PathBuf::from("/helper")),
    );
    let title = chrome::title_bar_actions(&layout).update;
    let effects = draw_static_frame_full(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: title.x + title.w / 2.0,
            y: title.y + title.h / 2.0,
            down: false,
            click: true,
        },
        &[],
        None,
        ActivityPanel::Explorer,
        None,
        None,
        Some(&controller),
    );
    assert_eq!(effects.update_action, Some(crate::update_process::UpdateAction::Check));

    controller.ui.open = true;
    controller.ui.state = ark::update::UpdateState::Available;
    controller.ui.version = Some(ark::update::Version::parse("0.1.1").unwrap());
    let panel = update::panel_layout(&layout, false);
    let effects = draw_static_frame_full(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: panel.primary.x + panel.primary.w / 2.0,
            y: panel.primary.y + panel.primary.h / 2.0,
            down: false,
            click: true,
        },
        &[],
        None,
        ActivityPanel::Explorer,
        None,
        None,
        Some(&controller),
    );
    assert_eq!(effects.update_action, Some(crate::update_process::UpdateAction::Install));
}

#[test]
fn layout_visibility_options_hide_cards() {
    let hidden = LayoutVisibility {
        explorer: false,
        terminal: false,
    };
    let layout = app_layout_with_visibility(
        1024.0,
        768.0,
        304.0,
        0.5,
        1,
        hidden,
        None,
    );
    assert_eq!(layout.explorer_card.w, 0.0);
    assert_eq!(layout.term_dock.h, 0.0);
    assert_eq!(layout.viewer_pane.w, layout.editor_card.w - 16.0);
    assert_eq!(layout.editor_card.x, layout.activity_bar.x + layout.activity_bar.w + 8.0);
    assert_eq!(layout.editor_card.h, layout.workbench.h);
}

#[test]
fn title_bar_omits_agent_launcher_icons() {
    let (_d, mut tree) = scene_fixture("title-agent-icons");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.surface_chrome, 748, 8, 806, 28),
        0,
        "Claude/Codex launcher placeholders are gone"
    );
}

#[test]
fn activity_bar_has_only_explorer_search_and_source_control() {
    assert_eq!(
        chrome::activity_bar_icons(),
        &[
            crate::icons::Icon::File,
            crate::icons::Icon::Search,
            crate::icons::Icon::SourceControl,
        ]
    );
}

#[test]
fn search_activity_icon_activates_the_search_panel() {
    let (_d, mut tree) = scene_fixture("search-activity");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let search_icon = chrome::activity_item_rect(&layout, 1).expect("search icon");
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let effects = draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: search_icon.x + search_icon.w / 2.0,
            y: search_icon.y + search_icon.h / 2.0,
            down: false,
            click: true,
        },
        &[],
        None,
    );
    assert_eq!(effects.set_activity, Some(ActivityPanel::Search));
}

#[test]
fn search_panel_focuses_input_and_opens_a_result() {
    let (_d, mut tree) = scene_fixture("search-panel");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let panel: search::SearchLayout = search::panel_layout(layout.explorer_card);
    let mut state = crate::search::SearchState::default();
    state.results.push(crate::search::SearchResult {
        path: PathBuf::from("/repo/src/main.rs"),
        matches: 3,
        line: 42,
        snippet: "fn target() {}".into(),
    });
    state.status = "1 file · 12 scanned".into();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);

    let _ = draw_static_frame_with(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: panel.input.x + 10.0,
            y: panel.input.y + panel.input.h / 2.0,
            down: false,
            click: true,
        },
        &[],
        None,
        ActivityPanel::Search,
        Some(&mut state),
        None,
    );
    assert!(state.input.focused);

    let effects = draw_static_frame_with(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: panel.results.x + 10.0,
            y: panel.results.y + 10.0,
            down: false,
            click: true,
        },
        &[],
        None,
        ActivityPanel::Search,
        Some(&mut state),
        None,
    );
    assert_eq!(
        effects.search_open,
        Some((PathBuf::from("/repo/src/main.rs"), 42))
    );
    assert!(!state.input.focused);
    assert!(
        count_color_in(
            &canvas,
            theme.fg,
            panel.results.x as u32,
            panel.results.y as u32,
            (panel.results.x + panel.results.w) as u32,
            (panel.results.y + 46.0) as u32,
        ) > 0,
        "search result text paints above the scrollbar track"
    );
}

#[test]
fn search_bottom_page_renders_and_opens_the_visible_result() {
    let (_d, mut tree) = scene_fixture("search-scroll-bottom");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let panel: search::SearchLayout = search::panel_layout(layout.explorer_card);
    let mut state = crate::search::SearchState::default();
    for i in 0..150 {
        state.results.push(crate::search::SearchResult {
            path: PathBuf::from(format!("/repo/src/f{i:03}.rs")),
            matches: 1,
            line: 1,
            snippet: format!("fn target{i}() {{}}"),
        });
    }
    state.scroll = state.results.len() as f64 * 46.0 - panel.results.h;
    let expected_index = ((state.scroll + 10.0) / 46.0).floor() as usize;
    let expected = state.results[expected_index].clone();
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);

    let effects = draw_static_frame_with(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: panel.results.x + 10.0,
            y: panel.results.y + 10.0,
            down: false,
            click: true,
        },
        &[],
        None,
        ActivityPanel::Search,
        Some(&mut state),
        None,
    );

    assert_eq!(
        effects.search_open,
        Some((expected.path, expected.line)),
        "the bottom page must hit-test the row actually visible at its top"
    );
    assert!(
        count_color_in(
            &canvas,
            theme.fg,
            panel.results.x as u32,
            panel.results.y as u32,
            (panel.results.x + panel.results.w) as u32,
            (panel.results.y + panel.results.h) as u32,
        ) > 0,
        "the final result page must paint instead of going blank"
    );
}

#[test]
fn source_control_icon_opens_the_per_file_commit_card() {
    let status = crate::git::parse_status(
        "? two words.txt\n\
         1 .M N... 000000 100644 100644 111 222 src/main.rs\n",
    )
    .rooted(Path::new("/repo"));
    let mut theme = frame_theme();
    theme.surface = (2, 2, 2);
    let (_d, mut tree) = scene_fixture("source-control-card");
    let mut commit = CommitPanel::default();

    for (w, h) in [(1024.0, 768.0), (480.0, 640.0)] {
        let layout = app_layout_with_commit(w, h, 304.0, false, true);
        let body = if w >= 760.0 {
            source_control_panel(&layout).expect("sidebar source control panel")
        } else {
            layout.viewer_pane
        };
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(w as u32, h as u32);
        let mut split = Split { position: 304.0 };
        let mut split_drag = SplitDrag::default();
        let mut scrollbar = Scrollbar::default();
        let mut viewer_scrollbar = Scrollbar::default();
        let pointer = if w >= 760.0 {
            Pointer {
                x: 32.0,
                y: 150.0,
                down: false,
                click: true,
            }
        } else {
            Pointer::default()
        };
        let effects = draw_frame(
            &mut Frame {
                canvas: &mut canvas,
                text: &mut text,
                theme: &theme,
                scale: 1.0,
            ui_scale: 1.0,
                activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
                animation_time: 0.0,
                title: "ark",
                root: Path::new("/repo"),
                open_files: &[],
                active_path: None,
                activity: ActivityPanel::Explorer,
                pointer,
                dbl_click: false,
                viewer: None,
                viewer_error: None,
                viewer_flash: false,
                highlighter: &Highlighter::new(),
                git: Some(&status),
                file_diff: None,
                commit: Some(&mut commit),
                config: None,
                config_error: None,
                config_error_path: None,
                terms: None,
                term_focused: false,
                new_item: None,
                icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
                groups: None,
                active_group: 0,
                group_viewers: [None, None],
                group_flash: [false, false],
                group_scrollbars: None,
                group_split: None,
                group_split_drag: None,
            },
            &layout,
            &mut tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        );
        if w >= 760.0 {
            assert_eq!(effects.set_activity, Some(ActivityPanel::SourceControl));
        }

        let mut frame = Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
            title: "ark",
            root: Path::new("/repo"),
            open_files: &[],
            active_path: None,
            activity: ActivityPanel::SourceControl,
            pointer: Pointer::default(),
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: Some(&status),
            file_diff: None,
            commit: Some(&mut commit),
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
        };
        let _ = draw_frame(
            &mut frame,
            &layout,
            &mut tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        );
        assert!(
            count_color_in(
                frame.canvas,
                theme.accent,
                body.x as u32,
                body.y as u32,
                (body.x + body.w) as u32,
                (body.y + body.h) as u32,
            ) > 0,
            "per-file checkboxes render in the active Source Control card at {w}x{h}"
        );
        let buttons = commit::layout(body);
        frame.pointer = Pointer {
            x: buttons.commit_button.x + buttons.commit_button.w / 2.0,
            y: buttons.commit_button.y + buttons.commit_button.h / 2.0,
            down: false,
            click: true,
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
        assert!(
            effects.commit_clicked,
            "Source Control Commit button is wired"
        );
    }
}

#[test]
fn unsplit_group_layout_keeps_existing_editor_geometry() {
    let layout = app_layout_with_groups(1024.0, 768.0, 304.0, false, false, 0.5, 1);
    assert_eq!(layout.editor_group_count, 1);
    assert_eq!(layout.editor_groups[0].tabs, layout.editor_tabs);
    assert_eq!(layout.editor_groups[0].breadcrumb, layout.breadcrumb);
    assert_eq!(layout.editor_groups[0].body, layout.viewer_pane);
    assert_eq!(layout.editor_group_split.w, 0.0);
}

#[test]
fn split_group_layout_divides_the_editor_card_with_a_gap() {
    let layout = app_layout_with_groups(1024.0, 768.0, 304.0, false, false, 0.5, 2);
    assert_eq!(layout.editor_group_count, 2);
    assert_eq!(
        layout.editor_groups[0],
        EditorGroupLayout {
            tabs: Rect {
                x: 312.0,
                y: 44.0,
                w: 348.0,
                h: 36.0
            },
            breadcrumb: Rect {
                x: 312.0,
                y: 80.0,
                w: 348.0,
                h: 24.0
            },
            body: Rect {
                x: 320.0,
                y: 104.0,
                w: 332.0,
                h: 648.0
            },
        }
    );
    assert_eq!(
        layout.editor_groups[1].tabs,
        Rect {
            x: 668.0,
            y: 44.0,
            w: 348.0,
            h: 36.0
        }
    );
    assert_eq!(
        layout.editor_groups[1].body,
        Rect {
            x: 676.0,
            y: 104.0,
            w: 332.0,
            h: 648.0
        }
    );
    assert_eq!(
        layout.editor_group_split,
        Rect {
            x: 660.0,
            y: 44.0,
            w: 8.0,
            h: 716.0
        }
    );
}

#[test]
fn split_group_layout_uses_the_full_group_width() {
    let layout = app_layout_with_groups(1440.0, 900.0, 304.0, false, false, 0.5, 2);
    assert_eq!(layout.editor_groups[0].body.w, 540.0);
    assert_eq!(layout.editor_groups[1].body.w, 540.0);
}

#[test]
fn split_group_layout_clamps_both_groups_to_minimum_widths() {
    let layout = app_layout_with_groups(1024.0, 768.0, 304.0, false, false, 0.0, 2);
    assert_eq!(layout.editor_groups[0].tabs.w, 180.0);
    assert_eq!(layout.editor_groups[1].tabs.w, 516.0);
    assert!(layout.editor_groups[0].body.w > 0.0);
    assert!(layout.editor_groups[1].body.w > 0.0);
}

#[test]
fn split_frame_draws_two_groups_and_routes_group_tab_clicks() {
    let d = temp_dir("split-frame");
    let a = d.path().join("a.txt");
    let b = d.path().join("b.txt");
    fs::write(&a, "alpha\nbeta\n").expect("write a");
    fs::write(&b, "gamma\ndelta\n").expect("write b");
    let mut va = Viewer::open(&a).expect("open a");
    let mut vb = Viewer::open(&b).expect("open b");
    let mut groups = crate::editor_groups::EditorGroups::new();
    groups.open(a.clone());
    groups.toggle_split();
    groups.open(b.clone());
    let mut divider = groups.divider;
    let mut group_drag = SplitDrag::default();
    let mut scrollbars = [Scrollbar::default(), Scrollbar::default()];
    let theme = frame_theme();
    let layout = app_layout_with_groups(1024.0, 768.0, 304.0, false, false, divider, 2);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let (_tree_dir, mut tree) = scene_fixture("split-frame-tree");
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut tree_scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        ui_scale: 1.0,
        activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
        animation_time: 0.0,
        title: "ark",
        root: Path::new("/repo"),
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
        commit: None,
        config: None,
        config_error: None,
        config_error_path: None,
        terms: None,
        term_focused: false,
        new_item: None,
        icon_tooltip: None,
        search: None,
        markdown: None,
        update: None,
        groups: Some(&groups.groups),
        active_group: 1,
        group_viewers: [Some(&mut va), Some(&mut vb)],
        group_flash: [false, false],
        group_scrollbars: Some(&mut scrollbars),
        group_split: Some(&mut divider),
        group_split_drag: Some(&mut group_drag),
    };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(effects.select_tab.is_none());
    assert!(
        count_color_in(frame.canvas, theme.fg, 320, 104, 652, 728) > 0,
        "left group viewer renders"
    );
    assert!(
        count_color_in(frame.canvas, theme.fg, 676, 104, 1008, 728) > 0,
        "right group viewer renders"
    );

    frame.pointer = Pointer {
        x: 700.0,
        y: 62.0,
        down: false,
        click: true,
    };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(effects.select_tab, Some((1, b.clone())));
    assert_eq!(effects.focus_group, Some(1));

    frame.pointer = Pointer {
        x: 998.0,
        y: 62.0,
        down: false,
        click: true,
    };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        effects.toggle_split,
        "Split action is wired in the group toolbar"
    );

    frame.pointer = Pointer {
        x: 664.0,
        y: 300.0,
        down: true,
        click: false,
    };
    let _ = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    frame.pointer.x = 720.0;
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(effects.group_split_moved);
    frame.pointer = Pointer {
        x: 980.0,
        y: 62.0,
        down: false,
        click: true,
    };
    let effects = draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut tree_scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(effects.close_group, Some(1));
    assert!(
        **frame.group_split.as_ref().expect("divider state") > 0.5,
        "divider follows the pointer"
    );
}

#[test]
fn source_control_panel_uses_the_sidebar_or_compact_editor_body() {
    let wide = app_layout_with_commit(1024.0, 768.0, 304.0, true, false);
    assert_eq!(
        source_control_panel(&wide),
        Some(Rect {
            x: 65.0,
            y: 80.0,
            w: 238.0,
            h: 679.0
        })
    );
    assert_eq!(
        wide.tree_panel,
        Rect {
            x: 72.0,
            y: 108.0,
            w: 224.0,
            h: 644.0
        }
    );
    assert_eq!(
        wide.viewer_pane,
        Rect {
            x: 320.0,
            y: 104.0,
            w: 688.0,
            h: 648.0
        }
    );

    let compact = app_layout_with_commit(480.0, 640.0, 304.0, false, false);
    assert_eq!(source_control_panel(&compact), Some(compact.viewer_pane));
}

#[test]
fn app_layout_terminal_card_reserves_the_bottom() {
    let l = app_layout_with_commit(1024.0, 768.0, 304.0, true, true);
    assert_eq!(
        l.term_dock,
        Rect {
            x: 312.0,
            y: 536.0,
            w: 704.0,
            h: 224.0
        }
    );
    assert_eq!(
        l.terminal_toolbar,
        Rect {
            x: 312.0,
            y: 536.0,
            w: 704.0,
            h: 36.0
        }
    );
    assert_eq!(
        l.term_grid,
        Rect {
            x: 320.0,
            y: 572.0,
            w: 688.0,
            h: 180.0
        }
    );
    assert_eq!(
        l.viewer_pane,
        Rect {
            x: 320.0,
            y: 104.0,
            w: 688.0,
            h: 416.0
        }
    );
    // Tiny windows use the compact terminal fraction and smaller headers.
    let tiny = app_layout_with_commit(400.0, 300.0, 160.0, false, true);
    assert!((tiny.term_dock.h - 89.6).abs() < 1e-9, "35% of 256px");
    assert!((tiny.viewer_pane.h - 106.4).abs() < 1e-9);
    assert!((tiny.term_grid.h - 55.6).abs() < 1e-9);
}

#[test]
fn terminal_resize_handle_uses_three_horizontal_dots() {
    let (_d, mut tree) = scene_fixture("terminal-resize-dots");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer::default(),
        &[],
        None,
    );

    let grip = terminal_resize_handle(&layout).expect("terminal grip");
    let y = ((grip.y + grip.h / 2.0) * 1.0) as u32;
    let x0 = (grip.x + 8.0) as u32;
    let x1 = (grip.x + grip.w - 8.0) as u32;
    let bg = tiny_skia::PremultipliedColorU8::from_rgba(
        theme.surface_chrome.0,
        theme.surface_chrome.1,
        theme.surface_chrome.2,
        255,
    )
    .expect("opaque tooltip background");
    let mut runs = 0usize;
    let mut in_run = false;
    for x in x0..x1 {
        let ink = canvas.pixmap().pixel(x, y) != Some(bg);
        if ink && !in_run {
            runs += 1;
        }
        in_run = ink;
    }
    assert!(runs >= 3, "three separate horizontal dots at the grip center, got {runs} runs");
}

#[test]
fn terminal_height_is_adjustable_and_clamped() {
    let resized = app_layout_with_groups_and_terminal(
        1024.0,
        768.0,
        304.0,
        true,
        true,
        0.5,
        1,
        Some(320.0),
    );
    assert_eq!(resized.term_dock.h, 320.0);
    assert_eq!(resized.term_dock.y, 440.0);
    assert_eq!(resized.viewer_pane.h, 320.0);
    assert_eq!(
        resized.terminal_resize,
        Rect {
            x: 312.0,
            y: 432.0,
            w: 704.0,
            h: 8.0,
        }
    );
    let handle = terminal_resize_handle(&resized).expect("terminal handle");
    assert_eq!(handle.x + handle.w / 2.0, resized.term_dock.x + resized.term_dock.w / 2.0);
    assert_eq!(handle.y + handle.h / 2.0, resized.terminal_resize.y + 4.0);

    assert_eq!(clamp_terminal_height(10.0, 692.0, 8.0), 120.0);
    assert_eq!(clamp_terminal_height(10_000.0, 692.0, 8.0), 524.0);
    assert_eq!(app_layout_with_commit(1024.0, 768.0, 304.0, true, true).term_dock.h, 224.0);
}

#[test]
fn explorer_footer_is_removed_and_tree_uses_the_space() {
    let layout = app_layout(1024.0, 768.0, 304.0);
    assert_eq!(
        layout.tree_panel.y + layout.tree_panel.h + 8.0,
        layout.workbench.y + layout.workbench.h
    );
}

#[test]
fn terminal_toolbar_uses_tabs_profiles_and_no_split_or_more() {
    let labels: Vec<_> = term::terminal_menu_options()
        .iter()
        .map(|option| option.label)
        .collect();
    assert_eq!(
        labels,
        vec![
            "New Terminal",
            "New Terminal Window",
            "bash",
            "fish",
            "JavaScript Debug Terminal",
        ]
    );
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, true, true);
    let (menu, rows) = chrome::terminal_menu_layout(&layout).expect("terminal menu");
    assert_eq!(rows.len(), 5);
    assert!(menu.y > layout.terminal_toolbar.y + layout.terminal_toolbar.h);
    let tabs = chrome::terminal_tab_rects(&layout, 3);
    assert_eq!(tabs.len(), 3);
    assert!(tabs
        .windows(2)
        .all(|pair| pair[0].x + pair[0].w < pair[1].x));
    let actions = chrome::terminal_actions(&layout);
    assert!(tabs.last().expect("tab").x + tabs.last().expect("tab").w < actions.plus.x);
    assert_eq!(
        chrome::terminal_action_icons(),
        &[
            crate::icons::Icon::Plus,
            crate::icons::Icon::ChevronDown,
            crate::icons::Icon::Trash,
        ]
    );
}

#[test]
fn terminal_card_placeholder_and_new_button_render_without_a_pty() {
    let (_d, mut tree) = scene_fixture("terminal-placeholder");
    let mut theme = frame_theme();
    theme.text_muted = (77, 77, 77);
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, true, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let effects = {
        let mut frame = Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
            title: "ark",
            root: Path::new("/"),
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
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
        };
        draw_frame(
            &mut frame,
            &layout,
            &mut tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        )
    };
    assert!(!effects.open_terminal && effects.close_terminal.is_none());
    let plus = chrome::terminal_actions(&layout).plus;
    assert!(
        count_color_in(
            &canvas,
            theme.text_muted,
            plus.x as u32,
            plus.y as u32,
            (plus.x + plus.w) as u32,
            (plus.y + plus.h) as u32
        ) > 0,
        "terminal + glyph paints above its hit-area background"
    );
    assert!(count_color_in(&canvas, theme.border_subtle, 312, 536, 1016, 760) > 1000);
    assert!(count_color_in(&canvas, theme.text_dim, 320, 572, 900, 644) > 0);

    let effects = {
        let mut frame = Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
            title: "ark",
            root: Path::new("/"),
            open_files: &[],
            active_path: None,
            activity: ActivityPanel::Explorer,
            pointer: Pointer {
                x: plus.x + plus.w / 2.0,
                y: plus.y + plus.h / 2.0,
                down: false,
                click: true,
            },
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
        };
        draw_frame(
            &mut frame,
            &layout,
            &mut tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        )
    };
    assert!(
        effects.open_terminal,
        "placeholder terminal + opens the first PTY tab"
    );
}

#[test]
fn terminal_grid_repaint_leaves_the_rest_of_the_canvas_untouched() {
    let d = temp_dir("terminal-grid-repaint");
    let size = term::GridSize::new(80, 24);
    let mut terms = TerminalTabs::default();
    terms.tabs.push(
        term::Terminal::spawn_program_with(d.path(), Path::new("/bin/cat"), "test", size, || {})
            .expect("spawn test terminal"),
    );
    let theme = Theme::DARK_PLUS;
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let sentinel = theme.accent;
    canvas.fill_rect(0.0, 0.0, 1024.0, 768.0, sentinel, 1.0);

    draw_terminal_grid(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        layout.ui_scale,
        layout.term_grid,
        &mut terms,
        false,
    );

    assert_eq!(
        canvas.pixmap().pixel(20, 20),
        tiny_skia::PremultipliedColorU8::from_rgba(sentinel.0, sentinel.1, sentinel.2, 255),
        "terminal output must not repaint the title bar or Explorer"
    );
    assert_eq!(
        canvas.pixmap().pixel(
            (layout.term_grid.x + layout.term_grid.w / 2.0) as u32,
            (layout.term_grid.y + layout.term_grid.h / 2.0) as u32,
        ),
        tiny_skia::PremultipliedColorU8::from_rgba(theme.bg.0, theme.bg.1, theme.bg.2, 255),
        "the terminal grid itself is painted"
    );
}

#[test]
fn terminal_action_tooltips_name_the_hovered_action() {
    let (_d, mut tree) = scene_fixture("terminal-action-tooltips");
    let theme = frame_theme();
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    let actions = chrome::terminal_actions(&layout);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    for (rect, expected) in [
        (actions.plus, "New Terminal"),
        (actions.menu, "Terminal Profiles"),
        (actions.close, "Close Terminal"),
    ] {
        let effects = draw_static_frame(
            &mut canvas,
            &mut text,
            &theme,
            &mut tree,
            &layout,
            Pointer {
                x: rect.x + rect.w / 2.0,
                y: rect.y + rect.h / 2.0,
                down: false,
                click: false,
            },
            &[],
            None,
        );
        assert_eq!(effects.hovered_icon, Some(expected));
    }
}

#[test]
fn terminal_tabs_select_and_close_by_index() {
    let d = temp_dir("terminal-tabs-frame");
    let size = term::GridSize { cols: 80, rows: 24 };
    let mut terms = TerminalTabs::default();
    for label in ["first", "second"] {
        terms.tabs.push(
            term::Terminal::spawn_program_with(d.path(), Path::new("/bin/cat"), label, size, || {})
                .expect("spawn test terminal"),
        );
    }
    terms.active = 0;
    let (_tree_dir, mut tree) = scene_fixture("terminal-tabs-frame-tree");
    let theme = frame_theme();
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let tabs = chrome::terminal_tab_rects(&layout, 2);
    let mut draw = |pointer: Pointer, terms: &mut TerminalTabs, canvas: &mut Canvas, text: &mut TextSystem| {
        draw_frame(
            &mut Frame {
                canvas,
                text,
                theme: &theme,
                scale: 1.0,
            ui_scale: 1.0,
                activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
                animation_time: 0.0,
                title: "ark",
                root: Path::new("/"),
                open_files: &[],
                active_path: None,
                activity: ActivityPanel::Explorer,
                pointer,
                dbl_click: false,
                viewer: None,
                viewer_error: None,
                viewer_flash: false,
                highlighter: &Highlighter::new(),
                git: None,
                file_diff: None,
                commit: None,
                config: None,
                config_error: None,
                config_error_path: None,
                terms: Some(terms),
                term_focused: false,
                new_item: None,
                icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
                groups: None,
                active_group: 0,
                group_viewers: [None, None],
                group_flash: [false, false],
                group_scrollbars: None,
                group_split: None,
                group_split_drag: None,
            },
            &layout,
            &mut tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            &mut viewer_scrollbar,
        )
    };
    let second = tabs[1];
    let effects = draw(
        Pointer {
            x: second.x + 20.0,
            y: second.y + second.h / 2.0,
            down: false,
            click: true,
        },
        &mut terms,
        &mut canvas,
        &mut text,
    );
    assert_eq!(effects.terminal_select, Some(1));

    terms.menu_open = true;
    let _ = draw(
        Pointer::default(),
        &mut terms,
        &mut canvas,
        &mut text,
    );
    let (_, menu_rows) = chrome::terminal_menu_layout(&layout).expect("menu layout");
    let first_row = menu_rows[0];
    assert!(
        count_color_in(
            &canvas,
            theme.fg,
            first_row.x as u32,
            first_row.y as u32,
            (first_row.x + first_row.w) as u32,
            (first_row.y + first_row.h) as u32,
        ) > 0,
        "profile menu paints above the active terminal grid"
    );

    let bash_row = menu_rows[2];
    let effects = draw(
        Pointer {
            x: bash_row.x + 20.0,
            y: bash_row.y + bash_row.h / 2.0,
            down: false,
            click: true,
        },
        &mut terms,
        &mut canvas,
        &mut text,
    );
    assert_eq!(
        effects.terminal_menu_action,
        Some(term::TerminalMenuAction::OpenProfile(
            term::TerminalProfile::Bash
        ))
    );

    let first = tabs[0];
    let effects = draw(
        Pointer {
            x: first.x + first.w - 10.0,
            y: first.y + first.h / 2.0,
            down: false,
            click: true,
        },
        &mut terms,
        &mut canvas,
        &mut text,
    );
    assert_eq!(effects.close_terminal, Some(0));
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

#[allow(clippy::too_many_arguments)] // one explicit test-frame context
fn draw_static_frame(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    tree: &mut FileTree,
    layout: &AppLayout,
    pointer: Pointer,
    open_files: &[PathBuf],
    active_path: Option<&Path>,
) -> FrameEffects {
    draw_static_frame_with(
        canvas,
        text,
        theme,
        tree,
        layout,
        pointer,
        open_files,
        active_path,
        ActivityPanel::Explorer,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)] // one explicit test-frame context
fn draw_static_frame_with(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    tree: &mut FileTree,
    layout: &AppLayout,
    pointer: Pointer,
    open_files: &[PathBuf],
    active_path: Option<&Path>,
    activity: ActivityPanel,
    search: Option<&mut crate::search::SearchState>,
    markdown: Option<&mut crate::markdown::MarkdownState>,
) -> FrameEffects {
    draw_static_frame_full(
        canvas,
        text,
        theme,
        tree,
        layout,
        pointer,
        open_files,
        active_path,
        activity,
        search,
        markdown,
        None,
    )
}

#[allow(clippy::too_many_arguments)] // one explicit test-frame context
fn draw_static_frame_full(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    tree: &mut FileTree,
    layout: &AppLayout,
    pointer: Pointer,
    open_files: &[PathBuf],
    active_path: Option<&Path>,
    activity: ActivityPanel,
    search: Option<&mut crate::search::SearchState>,
    markdown: Option<&mut crate::markdown::MarkdownState>,
    update: Option<&crate::update_process::UpdateController>,
) -> FrameEffects {
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    draw_frame(
        &mut Frame {
            canvas,
            text,
            theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
            title: "ark",
            root: Path::new("/repo"),
            open_files,
            active_path,
            activity,
            pointer,
            dbl_click: false,
            viewer: None,
            viewer_error: None,
            viewer_flash: false,
            highlighter: &Highlighter::new(),
            git: None,
            file_diff: None,
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search,
            markdown,
            update,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
        },
        layout,
        tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    )
}

/// Chrome-invisible theme: only the tokens under pin are distinct.
fn frame_theme() -> Theme {
    let bg = (0, 0, 0);
    Theme {
        bg,
        surface_chrome: bg,
        surface: bg,
        bg_elevated: (10, 20, 30),
        surface_hover: bg,
        fg: (255, 255, 255),
        fg_dim: (90, 90, 90),
        text_muted: bg,
        text_dim: bg,
        accent: (200, 220, 255),
        border: (60, 60, 70),
        border_subtle: bg,
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
        editor_search_match: (0, 0, 0),
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
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.surface = (2, 2, 2);
    theme.border_subtle = (3, 3, 3);
    theme.text_muted = (90, 90, 90);
    let layout = app_layout(1024.0, 768.0, 304.0);
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
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        !effects.tree_changed && !effects.scrolled,
        "static frame changes nothing"
    );
    assert_eq!(effects.opened, None);

    // Title bar, card chrome, and tree surfaces.
    assert!(
        count_color_in(&canvas, theme.accent, 0, 0, 40, 36) > 0,
        "title icon"
    );
    assert!(
        count_color_in(&canvas, theme.border_subtle, 64, 44, 304, 760) > 1000,
        "explorer card border"
    );
    assert!(
        count_color_in(&canvas, theme.border_subtle, 312, 44, 1016, 760) > 1000,
        "editor card border"
    );
    assert!(
        count_color_in(&canvas, theme.fg, 72, 108, 296, 300) > 0,
        "tree row ink"
    );
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 288, 108, 296, 752) > 100,
        "track"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 288, 108, 296, 752) > 100,
        "thumb"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 320, 104, 1008, 752) > 0,
        "placeholder text"
    );
    assert_eq!(
        count_color_in(&canvas, theme.border, 320, 104, 1008, 752),
        0,
        "placeholder has no inner grey box"
    );
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
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
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
        count_color_in(&canvas, theme.bg_elevated, 320, 104, 324, 300) > 300,
        "viewer gutter mark strip"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 320, 104, 944, 250) > 0,
        "line numbers"
    );
    assert!(
        count_color_in(&canvas, theme.fg, 320, 104, 944, 250) > 0,
        "code text ink"
    );
    assert!(
        count_color_in(&canvas, theme.syntax_keyword, 320, 104, 944, 250) > 0,
        "`fn` keyword ink in the first lines"
    );
}

#[test]
fn frame_interactions_report_effects() {
    let (_d, mut tree) = scene_fixture("effects");
    let theme = frame_theme();
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    // One Frame re-pointed per interaction (draw_frame takes &mut Frame).
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        ui_scale: 1.0,
        activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
        animation_time: 0.0,
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
        commit: None,
        config: None,
        config_error: None,
        config_error_path: None,
        terms: None,
        term_focused: false,
        new_item: None,
        icon_tooltip: None,
        search: None,
        markdown: None,
        update: None,
        groups: None,
        active_group: 0,
        group_viewers: [None, None],
        group_flash: [false, false],
        group_scrollbars: None,
        group_split: None,
        group_split_drag: None,
    };
    let mut draw_at = |frame: &mut Frame,
                       pointer: Pointer,
                       dbl: bool,
                       tree: &mut FileTree,
                       scrollbar: &mut Scrollbar,
                       viewer_scrollbar: &mut Scrollbar| {
        frame.pointer = pointer;
        frame.dbl_click = dbl;
        draw_frame(
            frame,
            &layout,
            tree,
            &mut split,
            &mut split_drag,
            scrollbar,
            viewer_scrollbar,
        )
    };

    // Click the chevron of row 0 (dir "A") → toggles, full-damage flag.
    let click_chevron = Pointer {
        x: 80.0,
        y: 119.0,
        down: false,
        click: true,
    };
    let effects = draw_at(
        &mut frame,
        click_chevron,
        false,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        effects.tree_changed && !effects.scrolled,
        "chevron click toggles"
    );
    assert!(tree.is_expanded(&tree.root.join("A")));

    // Scrollbar thumb drag: press the thumb at the track top (offset 0),
    // drag past the free-span end → offset clamps to max, scrolled flag.
    let press = Pointer {
        x: 292.0,
        y: 110.0,
        down: true,
        click: false,
    };
    let effects = draw_at(
        &mut frame,
        press,
        false,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(!effects.scrolled, "grabbing the thumb doesn't move it yet");
    let drag = Pointer {
        x: 292.0,
        y: 600.0,
        down: true,
        click: false,
    };
    let effects = draw_at(
        &mut frame,
        drag,
        false,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        effects.scrolled && !effects.tree_changed,
        "thumb drag scrolls"
    );
    let content = tree.visible_count() as f64 * ROW_H;
    assert_eq!(
        tree.offset,
        content - layout.tree_panel.h,
        "drag past the end clamps to max offset"
    );

    // Collapse-while-scrolled self-heal: the row set shrinks underneath a
    // large offset (API-level mutation standing in for any out-of-frame
    // change) → the next frame re-clamps the offset and reports it as a
    // scroll (panel damage suffices — the shift is content-internal).
    assert!(tree.toggle(&tree.root.join("A"), true), "collapse A");
    let stale_content = tree.visible_count() as f64 * ROW_H;
    assert!(
        tree.offset > stale_content - layout.tree_panel.h,
        "offset is stale-large after the collapse"
    );
    let effects = draw_at(
        &mut frame,
        Pointer::default(),
        false,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        effects.scrolled && !effects.tree_changed,
        "clamp reports the heal as a scroll"
    );
    assert_eq!(
        tree.offset,
        stale_content - layout.tree_panel.h,
        "offset re-clamped to the new max"
    );
}

#[test]
fn frame_single_click_opens_and_viewer_scrolls() {
    let (_d, mut tree) = scene_fixture("dbl-open");
    let theme = frame_theme();
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, true, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    // A viewed file OUTSIDE the tree fixture dir (100 lines for overflow).
    let vd = temp_dir("dbl-open-file");
    let file = vd.path().join("viewed.txt");
    fs::write(
        &file,
        (0..100).map(|i| format!("line {i}\n")).collect::<String>(),
    )
    .expect("write");
    let mut viewer = Viewer::open(&file).expect("opens");

    // The viewer stays open in the pane for every frame (the App keeps the
    // open file borrowed from the LRU cache; here the same borrow persists).
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        ui_scale: 1.0,
        activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
        animation_time: 0.0,
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
        commit: None,
        config: None,
        config_error: None,
        config_error_path: None,
        terms: None,
        term_focused: false,
        new_item: None,
        icon_tooltip: None,
        search: None,
        markdown: None,
        update: None,
        groups: None,
        active_group: 0,
        group_viewers: [None, None],
        group_flash: [false, false],
        group_scrollbars: None,
        group_split: None,
        group_split_drag: None,
    };
    let mut draw_at = |frame: &mut Frame,
                       pointer: Pointer,
                       dbl: bool,
                       tree: &mut FileTree,
                       viewer_scrollbar: &mut Scrollbar| {
        frame.pointer = pointer;
        frame.dbl_click = dbl;
        draw_frame(
            frame,
            &layout,
            tree,
            &mut split,
            &mut split_drag,
            &mut scrollbar,
            viewer_scrollbar,
        )
    };

    // Single-click on file row 1 ("f00.txt", band [130, 152)) → Opened.
    let effects = draw_at(
        &mut frame,
        Pointer {
            x: 100.0,
            y: 141.0,
            down: false,
            click: true,
        },
        false,
        &mut tree,
        &mut viewer_scrollbar,
    );
    assert_eq!(
        effects.opened,
        Some(tree.root.join("f00.txt")),
        "single-click reports the tree row's file to open"
    );

    // Viewer scrollbar thumb drag past the end → offset clamps to the max
    // line offset and reports viewer_scrolled (viewer rect damage only).
    let press = Pointer {
        x: 1004.0,
        y: 110.0,
        down: true,
        click: false,
    };
    let effects = draw_at(&mut frame, press, false, &mut tree, &mut viewer_scrollbar);
    assert!(
        !effects.viewer_scrolled,
        "grabbing the viewer thumb doesn't move it"
    );
    let drag = Pointer {
        x: 1004.0,
        y: 700.0,
        down: true,
        click: false,
    };
    let effects = draw_at(&mut frame, drag, false, &mut tree, &mut viewer_scrollbar);
    // The same drag extends the text selection (tree_changed rides the
    // event-level full damage); the scrollbar reports viewer_scrolled.
    assert!(effects.viewer_scrolled, "viewer thumb drag scrolls");
    let viewport_lines = (layout.viewer_pane.h / LINE_H).floor() as usize;
    assert_eq!(
        viewer.offset_lines(),
        viewer.line_count().saturating_sub(viewport_lines),
        "drag past the end clamps to the last full viewport"
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

    // draw_frame fills the background itself; a free fn (not a closure) so
    // canvas borrows end before each pixel assertion.
    #[allow(clippy::too_many_arguments)] // borrows must outlive no closure: explicit beats bundled
    fn draw(
        flash: bool,
        viewer: &mut Viewer,
        canvas: &mut Canvas,
        text: &mut TextSystem,
        theme: &Theme,
        tree: &mut FileTree,
        scrollbar: &mut Scrollbar,
        viewer_scrollbar: &mut Scrollbar,
    ) -> FrameEffects {
        let layout = app_layout(1024.0, 768.0, 304.0);
        let mut split = Split { position: 304.0 };
        let mut split_drag = SplitDrag::default();
        draw_frame(
            &mut Frame {
                canvas,
                text,
                theme,
                scale: 1.0,
            ui_scale: 1.0,
                activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
                animation_time: 0.0,
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
                commit: None,
                config: None,
                config_error: None,
                config_error_path: None,
                terms: None,
                term_focused: false,
                new_item: None,
                icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
                groups: None,
                active_group: 0,
                group_viewers: [None, None],
                group_flash: [false, false],
                group_scrollbars: None,
                group_split: None,
                group_split_drag: None,
            },
            &layout,
            tree,
            &mut split,
            &mut split_drag,
            scrollbar,
            viewer_scrollbar,
        )
    }

    draw(
        true,
        &mut viewer,
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        count_color_in(&canvas, theme.accent, 320, 104, 944, 107) >= 624 * 3,
        "flash paints the accent strip across the pane's top edge"
    );
    draw(
        false,
        &mut viewer,
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, 320, 104, 944, 107),
        0,
        "no flash → no strip"
    );
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
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: Some(&mut editor),
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
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
    assert!(
        count_color_in(&canvas, theme.fg, 320, 104, 1008, 300) > 0,
        "config ink"
    );

    // Header Save button (panel right edge) reports the effect.
    let save_click = Pointer {
        x: layout.viewer_pane.x + layout.viewer_pane.w - 44.0,
        y: 123.0,
        down: false,
        click: true,
    };
    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: Some(&mut editor),
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
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
    let mut theme = frame_theme();
    theme.surface = (2, 2, 2);
    theme.border_subtle = (3, 3, 3);
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let mut frame = Frame {
        canvas: &mut canvas,
        text: &mut text,
        theme: &theme,
        scale: 1.0,
        ui_scale: 1.0,
        activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
        animation_time: 0.0,
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
        commit: None,
        config: None,
        config_error: None,
        config_error_path: None,
        terms: None,
        term_focused: false,
        new_item: None,
        icon_tooltip: None,
        search: None,
        markdown: None,
        update: None,
        groups: None,
        active_group: 0,
        group_viewers: [None, None],
        group_flash: [false, false],
        group_scrollbars: None,
        group_split: None,
        group_split_drag: None,
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
        count_color_in(frame.canvas, theme.bg_elevated, 312, 44, 900, 80) > 10_000,
        "tab strip background"
    );
    assert!(
        crate::ui::testutil::count_not_color_in(frame.canvas, theme.surface, 312, 80, 1016, 104)
            > 0,
        "breadcrumb ink"
    );
    assert!(
        crate::ui::testutil::count_not_color_in(frame.canvas, theme.surface, 850, 44, 900, 80) > 0,
        "READ-ONLY badge"
    );
    // Activity card paints left of the tree.
    assert!(
        count_color_in(frame.canvas, theme.surface, 8, 44, 56, 736) > 10_000,
        "activity card"
    );

    // Second file tab selects; first tab's × closes.
    frame.pointer = Pointer {
        x: 550.0,
        y: 62.0,
        down: false,
        click: true,
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
    assert_eq!(effects.select_tab, Some((0, files[1].clone())));
    frame.pointer = Pointer {
        x: 458.0,
        y: 62.0,
        down: false,
        click: true,
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
    assert_eq!(effects.close_tab, Some((0, files[0].clone())));
    // Activity bar: click the Search icon (second slot).
    frame.pointer = Pointer {
        x: 38.0,
        y: 110.0,
        down: false,
        click: true,
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
    assert_eq!(effects.set_activity, Some(ActivityPanel::Search));
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

    let handle = sidebar_handle(&layout, 280.0).expect("visible Explorer divider");
    assert_eq!(
        handle,
        Rect {
            x: 274.0,
            y: 385.0,
            w: 20.0,
            h: 34.0
        }
    );

    let mut frame = Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
    };
    draw_frame(
        &mut frame,
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    let dots = crate::ui::testutil::ink_bbox_in(frame.canvas, theme.surface, 274, 385, 294, 419)
        .expect("three-dot handle ink");
    assert!(
        dots.2 - dots.0 >= 3 && dots.3 - dots.1 >= 15,
        "three bold dots paint at the center of the sidebar divider: {dots:?}"
    );

    frame.canvas.fill(theme.surface);
    frame.activity_pulse = 1.0;
    frame.animation_time = std::f64::consts::FRAC_PI_2 / 10.0;
    draw_sidebar_handle(&mut frame, handle, false);
    assert!(
        count_color_in(frame.canvas, theme.accent, 274, 373, 294, 407) > 0,
        "recent activity pulses the handle toward the accent color"
    );

    let mut draw = |split: &mut Split, pointer: Pointer, dbl: bool| {
        draw_frame(
            &mut Frame {
                canvas: &mut canvas,
                text: &mut text,
                theme: &theme,
                scale: 1.0,
            ui_scale: 1.0,
                activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
                animation_time: 0.0,
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
                commit: None,
                config: None,
                config_error: None,
                config_error_path: None,
                terms: None,
                term_focused: false,
                new_item: None,
                icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
                groups: None,
                active_group: 0,
                group_viewers: [None, None],
                group_flash: [false, false],
                group_scrollbars: None,
                group_split: None,
                group_split_drag: None,
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
    draw(
        &mut split,
        Pointer {
            x: 284.0,
            y: 390.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 340.0,
            y: 390.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 340.0,
            y: 390.0,
            down: false,
            click: false,
        },
        false,
    );
    assert_eq!(split.position, 336.0);

    // Double-click the handle at its new position resets to the default.
    draw(
        &mut split,
        Pointer {
            x: 340.0,
            y: 390.0,
            down: false,
            click: true,
        },
        true,
    );
    assert_eq!(split.position, SIDEBAR_DEFAULT_PX);

    // The normal divider drag band still works and clamps.
    draw(
        &mut split,
        Pointer {
            x: 284.0,
            y: 300.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 480.0,
            y: 300.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 480.0,
            y: 300.0,
            down: false,
            click: false,
        },
        false,
    );
    assert_eq!(split.position, 500.0);
    draw(
        &mut split,
        Pointer {
            x: 504.0,
            y: 390.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 0.0,
            y: 300.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 0.0,
            y: 300.0,
            down: false,
            click: false,
        },
        false,
    );
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
    let layout = app_layout(1024.0, 768.0, 304.0);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();

    let effects = draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            ui_scale: 1.0,
            activity_pulse: 0.0,
            layout_visibility: LayoutVisibility::default(),
            animation_time: 0.0,
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
            commit: None,
            config: None,
            config_error: None,
            config_error_path: None,
            terms: None,
            term_focused: false,
            new_item: None,
            icon_tooltip: None,
            search: None,
            markdown: None,
            update: None,
            groups: None,
            active_group: 0,
            group_viewers: [None, None],
            group_flash: [false, false],
            group_scrollbars: None,
            group_split: None,
            group_split_drag: None,
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
