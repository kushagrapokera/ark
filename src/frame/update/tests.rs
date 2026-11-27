use super::*;
use crate::frame::app_layout;
use ark::update::InstallLayout;

#[test]
fn panel_stays_inside_window_and_below_title_bar() {
    let layout = app_layout(1024.0, 768.0, 304.0);
    let normal = panel_layout(&layout, false);
    assert!(normal.panel.x >= 0.0);
    assert!(normal.panel.y >= layout.title_bar.y + layout.title_bar.h);
    assert!(normal.panel.x + normal.panel.w <= layout.title_bar.w);
    assert!(normal.panel.y + normal.panel.h <= layout.workbench.y + layout.workbench.h);
    let warning = panel_layout(&layout, true);
    assert!(warning.panel.h > normal.panel.h);
}

#[test]
fn compact_panel_clamps_to_window_width() {
    let layout = app_layout(320.0, 240.0, 304.0);
    let rects = panel_layout(&layout, true);
    assert!(rects.panel.x >= 0.0);
    assert!(rects.panel.x + rects.panel.w <= 320.0);
    assert!(rects.primary.w > 0.0);
    assert!(rects.secondary.w > 0.0);
}

#[test]
fn sizes_use_bounded_human_units() {
    assert_eq!(format_size(999), "999 B");
    assert_eq!(format_size(1024), "1.0 KiB");
    assert_eq!(format_size(4_212_334), "4.0 MiB");
}

#[test]
fn title_button_reports_check_or_open_action() {
    let layout = app_layout(1024.0, 768.0, 304.0);
    let rect = title_rect(&layout);
    assert!(rect.w > 0.0);
    let pointer = Pointer {
        x: rect.x + rect.w / 2.0,
        y: rect.y + rect.h / 2.0,
        down: false,
        click: true,
    };
    let mut controller = UpdateController::new(
        Some(InstallLayout::new(
            std::path::PathBuf::from("/data"),
            std::path::PathBuf::from("/bin"),
            std::path::PathBuf::from("/state"),
            std::path::PathBuf::from("/cache"),
            std::path::PathBuf::from("/config"),
        )),
        Some(std::path::PathBuf::from("/helper")),
    );
    assert_eq!(
        title_action(&layout, pointer, &controller),
        Some(UpdateAction::Check)
    );
    controller.ui.state = ark::update::UpdateState::Available;
    assert_eq!(
        title_action(&layout, pointer, &controller),
        Some(UpdateAction::TogglePanel)
    );
}

#[test]
fn panel_buttons_map_to_install_later_and_restart() {
    let layout = app_layout(1024.0, 768.0, 304.0);
    let rects = panel_layout(&layout, false);
    let controller = UpdateController::new(
        Some(InstallLayout::new(
            std::path::PathBuf::from("/data"),
            std::path::PathBuf::from("/bin"),
            std::path::PathBuf::from("/state"),
            std::path::PathBuf::from("/cache"),
            std::path::PathBuf::from("/config"),
        )),
        Some(std::path::PathBuf::from("/helper")),
    );
    let primary = Pointer {
        x: rects.primary.x + rects.primary.w / 2.0,
        y: rects.primary.y + rects.primary.h / 2.0,
        down: false,
        click: true,
    };
    let secondary = Pointer {
        x: rects.secondary.x + rects.secondary.w / 2.0,
        y: rects.secondary.y + rects.secondary.h / 2.0,
        down: false,
        click: true,
    };
    assert_eq!(
        panel_action(&layout, secondary, &controller, false),
        Some(UpdateAction::Later)
    );

    let mut controller = controller;
    controller.ui.state = ark::update::UpdateState::Available;
    assert_eq!(
        panel_action(&layout, primary, &controller, false),
        Some(UpdateAction::Install)
    );
    controller.ui.state = ark::update::UpdateState::Installed;
    assert_eq!(
        panel_action(&layout, primary, &controller, false),
        Some(UpdateAction::Restart)
    );
}
