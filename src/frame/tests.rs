//! Frame tests (impl + tests together would pass the ~400-line module
//! guidance): layout pins, status-line formats, scene ink, interaction
//! effects, and the M4 viewer-pane wiring.

use super::*;
use crate::fs::testutil::{temp_dir, TempDir};
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
            h: 692.0
        }
    );
    assert_eq!(
        l.activity_bar,
        Rect {
            x: 8.0,
            y: 44.0,
            w: 48.0,
            h: 692.0
        }
    );
    assert_eq!(
        l.explorer_card,
        Rect {
            x: 64.0,
            y: 44.0,
            w: 240.0,
            h: 692.0
        }
    );
    assert_eq!(
        l.tree_panel,
        Rect {
            x: 72.0,
            y: 108.0,
            w: 224.0,
            h: 564.0
        }
    );
    assert_eq!(
        l.scroll_track,
        Rect {
            x: 288.0,
            y: 108.0,
            w: 8.0,
            h: 564.0
        }
    );
    assert_eq!(
        l.editor_card,
        Rect {
            x: 312.0,
            y: 44.0,
            w: 704.0,
            h: 692.0
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
            w: 624.0,
            h: 624.0
        }
    );
    assert_eq!(
        l.minimap,
        Rect {
            x: 952.0,
            y: 104.0,
            w: 56.0,
            h: 624.0
        }
    );
    assert_eq!(
        l.term_dock,
        Rect {
            x: 312.0,
            y: 736.0,
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
            h: 692.0
        }
    );
    assert_eq!(
        l.status,
        Rect {
            x: 0.0,
            y: 744.0,
            w: 1024.0,
            h: 24.0
        }
    );

    // Degenerate window sizes never produce negative extents.
    let tiny = app_layout(400.0, 300.0, 160.0);
    assert!(tiny.tree_panel.w >= 0.0 && tiny.tree_panel.h >= 0.0);
    assert!(tiny.viewer_pane.w >= 0.0 && tiny.status.y >= 0.0);
    assert_eq!(
        app_layout(1024.0, 768.0, 8.0).explorer_card.w,
        160.0,
        "minimum explorer width"
    );
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
    assert_eq!(small.minimap.w, 0.0, "narrow editor hides minimap");
    assert_eq!(small.activity_bar.w, 40.0, "compact activity card");
    assert!(
        small.editor_card.w > small.workbench.w * 0.75,
        "editor receives the space freed by Explorer"
    );
    assert_eq!(
        small.term_dock.h, 140.0,
        "compact width shrinks the terminal"
    );
    assert_eq!(small.terminal_sessions.w, 0.0);

    let short = app_layout_with_commit(600.0, 480.0, 304.0, true, true);
    assert_eq!(short.explorer_card.w, 0.0);
    assert_eq!(short.minimap.w, 0.0);
    assert_eq!(short.terminal_toolbar.h, 30.0);
    assert!(short.term_grid.h > 0.0 && short.viewer_pane.h > 0.0);

    let full = app_layout_with_commit(1440.0, 900.0, 304.0, true, true);
    assert_eq!(full.explorer_card.w, 240.0);
    assert_eq!(full.minimap.w, 56.0);
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
        minimum.status,
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
        646.0,
        "full layout reserves only the two remaining actions"
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
    let plus_x = layout.term_dock.x + layout.term_dock.w - 58.0;
    let plus_y = layout.term_dock.y + layout.terminal_toolbar.h / 2.0;
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
fn activity_bar_omits_unused_placeholders_below_config() {
    let (_d, mut tree) = scene_fixture("activity-placeholders");
    let mut theme = frame_theme();
    theme.surface = (2, 2, 2);
    theme.text_muted = (77, 77, 77);
    theme.git_modified = (88, 99, 111);
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
        crate::ui::testutil::count_not_color_in(&canvas, theme.surface, 14, 252, 50, 408),
        0,
        "unused icons below Config are gone"
    );
    assert_eq!(
        crate::ui::testutil::count_not_color_in(&canvas, theme.surface, 14, 660, 50, 732),
        0,
        "unused account/settings placeholders are gone"
    );
}

#[test]
fn short_activity_bar_keeps_config_action_reachable() {
    let (_d, mut tree) = scene_fixture("compact-activity");
    let theme = frame_theme();
    let layout = app_layout_with_commit(400.0, 300.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(400, 300);
    let effects = draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &layout,
        Pointer {
            x: 28.0,
            y: 190.0,
            down: false,
            click: true,
        },
        &[],
        None,
    );
    assert_eq!(
        effects.set_activity,
        Some(ActivityPanel::Config),
        "functional actions stay reachable when optional activity icons are hidden"
    );
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
fn compact_title_bar_keeps_controls_inside_the_window() {
    let (_d, mut tree) = scene_fixture("compact-title");
    let mut theme = frame_theme();
    theme.text_muted = (77, 77, 77);

    let narrow = app_layout_with_commit(400.0, 300.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(400, 300);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &narrow,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        count_color_in(&canvas, theme.accent, 399, 7, 400, 29),
        0,
        "Update never extends past the title bar"
    );

    let medium = app_layout_with_commit(900.0, 700.0, 304.0, false, true);
    let mut canvas = Canvas::new(900, 700);
    draw_static_frame(
        &mut canvas,
        &mut text,
        &theme,
        &mut tree,
        &medium,
        Pointer::default(),
        &[],
        None,
    );
    assert_eq!(
        count_color_in(&canvas, theme.text_muted, 720, 9, 774, 27),
        0,
        "optional title icons do not overlap Update"
    );
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
fn compact_status_bar_drops_optional_items_before_the_left_cluster() {
    let (_d, mut tree) = scene_fixture("compact-status");
    let mut theme = frame_theme();
    theme.surface_chrome = (1, 1, 1);
    theme.text_muted = (77, 77, 77);
    let layout = app_layout_with_commit(400.0, 300.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(400, 300);
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
            theme.surface_chrome,
            180,
            layout.status.y as u32 + 1,
            240,
            layout.status.y as u32 + 23,
        ),
        0,
        "right-aligned status items leave a gap after the branch controls"
    );
    assert!(
        crate::ui::testutil::count_not_color_in(
            &canvas,
            theme.surface_chrome,
            300,
            layout.status.y as u32 + 1,
            400,
            layout.status.y as u32 + 23,
        ) > 0,
        "essential status items remain visible"
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
            Rect {
                x: 65.0,
                y: 81.0,
                w: 238.0,
                h: 654.0,
            }
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
                terms: None,
                term_focused: false,
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
            terms: None,
            term_focused: false,
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
fn source_control_panel_uses_the_sidebar_or_compact_editor_body() {
    let wide = app_layout_with_commit(1024.0, 768.0, 304.0, true, false);
    assert_eq!(
        source_control_panel(&wide),
        Some(Rect {
            x: 65.0,
            y: 80.0,
            w: 238.0,
            h: 655.0
        })
    );
    assert_eq!(
        wide.tree_panel,
        Rect {
            x: 72.0,
            y: 108.0,
            w: 224.0,
            h: 564.0
        }
    );
    assert_eq!(
        wide.viewer_pane,
        Rect {
            x: 320.0,
            y: 104.0,
            w: 624.0,
            h: 624.0
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
            y: 512.0,
            w: 704.0,
            h: 224.0
        }
    );
    assert_eq!(
        l.terminal_toolbar,
        Rect {
            x: 312.0,
            y: 512.0,
            w: 704.0,
            h: 36.0
        }
    );
    assert_eq!(
        l.term_grid,
        Rect {
            x: 320.0,
            y: 548.0,
            w: 652.0,
            h: 180.0
        }
    );
    assert_eq!(
        l.terminal_sessions,
        Rect {
            x: 980.0,
            y: 548.0,
            w: 36.0,
            h: 188.0
        }
    );
    assert_eq!(
        l.viewer_pane,
        Rect {
            x: 320.0,
            y: 104.0,
            w: 624.0,
            h: 392.0
        }
    );
    // Tiny windows use the compact terminal fraction and smaller headers.
    let tiny = app_layout_with_commit(400.0, 300.0, 160.0, false, true);
    assert!((tiny.term_dock.h - 81.2).abs() < 1e-9, "35% of 232px");
    assert!((tiny.viewer_pane.h - 90.8).abs() < 1e-9);
    assert!((tiny.term_grid.h - 47.2).abs() < 1e-9);
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
            terms: None,
            term_focused: false,
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
    assert!(!effects.open_terminal && !effects.close_terminal);
    let (plus, _) = chrome::terminal_actions(&layout);
    let plus = plus.expect("terminal toolbar action");
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
    assert!(count_color_in(&canvas, theme.border_subtle, 312, 512, 1016, 736) > 1000);
    assert!(count_color_in(&canvas, theme.text_dim, 320, 548, 900, 620) > 0);

    let effects = {
        let mut frame = Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
            root: Path::new("/"),
            open_files: &[],
            active_path: None,
            activity: ActivityPanel::Explorer,
            pointer: Pointer {
                x: 898.0,
                y: 522.0,
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
            terms: None,
            term_focused: false,
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
fn vertical_terminal_sessions_switch_the_active_pty() {
    let d = temp_dir("terminal-sessions");
    let size = term::GridSize { cols: 80, rows: 24 };
    let mut terms = TerminalTabs::default();
    for label in ["first", "second"] {
        terms.tabs.push(
            term::Terminal::spawn_program_with(d.path(), Path::new("/bin/cat"), label, size, || {})
                .expect("spawn test terminal"),
        );
    }
    terms.active = 0;
    assert_eq!(terms.tabs.len(), 2);
    let (_tree_dir, mut tree) = scene_fixture("terminal-sessions-frame");
    let theme = frame_theme();
    let layout = app_layout_with_commit(1024.0, 768.0, 304.0, false, true);
    let mut text = TextSystem::new();
    let mut canvas = Canvas::new(1024, 768);
    let mut split = Split { position: 304.0 };
    let mut split_drag = SplitDrag::default();
    let mut scrollbar = Scrollbar::default();
    let mut viewer_scrollbar = Scrollbar::default();
    let sessions = layout.terminal_sessions;
    let click = Pointer {
        x: sessions.x + 18.0,
        y: sessions.y + 8.0 + 32.0 + 12.0,
        down: false,
        click: true,
    };
    assert!(Rect {
        x: sessions.x + 6.0,
        y: sessions.y + 40.0,
        w: 24.0,
        h: 24.0
    }
    .contains(click.x, click.y));
    draw_frame(
        &mut Frame {
            canvas: &mut canvas,
            text: &mut text,
            theme: &theme,
            scale: 1.0,
            title: "ark",
            root: Path::new("/"),
            open_files: &[],
            active_path: None,
            activity: ActivityPanel::Explorer,
            pointer: click,
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
            terms: Some(&mut terms),
            term_focused: false,
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert_eq!(
        terms.active, 1,
        "second session icon selects the second PTY"
    );
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
    assert!(
        big.scroll_by(1000.0, 66.0),
        "10 rows in a 3-row viewport overflows"
    );
    assert_eq!(big.offset, 154.0, "clamped to the max offset");
    assert!(
        frame_status(&big, None, None, None, None, 66.0).contains("scroll 100% · no git"),
        "max offset → 100%"
    );
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
    assert!(
        frame_status(&err_tree, None, None, None, None, 66.0).contains("cannot list"),
        "error surfaces"
    );
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
        frame_status(&tree, Some(&trunc), None, None, None, 632.0)
            .contains("big.txt 25 lines · truncated"),
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
            title: "ark",
            root: Path::new("/repo"),
            open_files,
            active_path,
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
            terms: None,
            term_focused: false,
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
            terms: None,
            term_focused: false,
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

    // Title bar, card chrome, tree, minimap and status surfaces.
    assert!(
        count_color_in(&canvas, theme.accent, 0, 0, 40, 36) > 0,
        "title icon"
    );
    assert!(
        count_color_in(&canvas, theme.border_subtle, 64, 44, 304, 736) > 1000,
        "explorer card border"
    );
    assert!(
        count_color_in(&canvas, theme.border_subtle, 312, 44, 1016, 736) > 1000,
        "editor card border"
    );
    assert!(
        count_color_in(&canvas, theme.fg, 72, 108, 296, 300) > 0,
        "tree row ink"
    );
    assert!(
        count_color_in(&canvas, theme.bg_elevated, 288, 108, 296, 672) > 100,
        "track"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 288, 108, 296, 672) > 100,
        "thumb"
    );
    assert!(
        count_color_in(&canvas, theme.text_dim, 952, 104, 1008, 300) > 0,
        "minimap placeholder"
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 320, 104, 944, 728) > 0,
        "placeholder text"
    );
    assert!(
        count_color_in(&canvas, theme.accent, 0, 744, 32, 768) > 500,
        "status remote block"
    );
    assert!(
        count_color_in(&canvas, theme.text_muted, 32, 744, 1024, 768) > 0,
        "status ink"
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
            terms: None,
            term_focused: false,
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
        terms: None,
        term_focused: false,
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
fn frame_double_click_opens_and_viewer_scrolls() {
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
        terms: None,
        term_focused: false,
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

    // Double-click on file row 1 ("f00.txt", band [130, 152)) → Opened.
    // (The first click already selected it in a prior frame.)
    let _ = draw_at(
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
    let effects = draw_at(
        &mut frame,
        Pointer {
            x: 100.0,
            y: 141.0,
            down: false,
            click: true,
        },
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
    let press = Pointer {
        x: 940.0,
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
        x: 940.0,
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
                terms: None,
                term_focused: false,
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
            terms: None,
            term_focused: false,
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
        count_color_in(&canvas, theme.fg, 320, 104, 944, 300) > 0,
        "config ink"
    );
    assert!(
        frame_status(&tree, None, Some(&editor), None, None, layout.viewer_pane.h)
            .contains("config config.yaml"),
        "{}",
        frame_status(&tree, None, Some(&editor), None, None, layout.viewer_pane.h)
    );

    // Header Save button (panel right edge) reports the effect.
    let save_click = Pointer {
        x: 900.0,
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
            terms: None,
            term_focused: false,
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
        terms: None,
        term_focused: false,
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
    assert_eq!(effects.select_tab.as_deref(), Some(files[1].as_path()));
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
    assert_eq!(effects.close_tab.as_deref(), Some(files[0].as_path()));
    // Activity bar: click the Config icon (fourth slot).
    frame.pointer = Pointer {
        x: 24.0,
        y: 230.0,
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

    let handle = sidebar_handle(&layout, 280.0).expect("visible Explorer divider");
    assert_eq!(
        handle,
        Rect {
            x: 276.0,
            y: 50.0,
            w: 16.0,
            h: 24.0
        }
    );

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
            commit: None,
            config: None,
            config_error: None,
            terms: None,
            term_focused: false,
        },
        &layout,
        &mut tree,
        &mut split,
        &mut split_drag,
        &mut scrollbar,
        &mut viewer_scrollbar,
    );
    assert!(
        count_color_in(&canvas, theme.fg_dim, 276, 50, 292, 74) >= 3,
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
                commit: None,
                config: None,
                config_error: None,
                terms: None,
                term_focused: false,
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
            y: 62.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 340.0,
            y: 62.0,
            down: true,
            click: false,
        },
        false,
    );
    draw(
        &mut split,
        Pointer {
            x: 340.0,
            y: 62.0,
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
            y: 62.0,
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
            y: 62.0,
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
            terms: None,
            term_focused: false,
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
