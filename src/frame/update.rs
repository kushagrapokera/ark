use ark::update::UpdateState;

use crate::render::ColorRgb;
use crate::ui::{draw_button, Pointer, Press, Rect};
use crate::update_process::{UpdateAction, UpdateController};

use super::{AppLayout, Frame};

#[derive(Clone, Copy, Debug)]
pub struct UpdatePanelLayout {
    pub panel: Rect,
    pub primary: Rect,
    pub secondary: Rect,
}

pub fn title_rect(layout: &AppLayout) -> Rect {
    if layout.title_bar.w < 720.0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    }
    Rect {
        x: layout.title_bar.w - 90.0,
        y: 8.0,
        w: 20.0,
        h: 20.0,
    }
}

pub fn title_action(
    layout: &AppLayout,
    pointer: Pointer,
    controller: &UpdateController,
) -> Option<UpdateAction> {
    if pointer.click && title_rect(layout).contains(pointer.x, pointer.y) {
        controller.title_action()
    } else {
        None
    }
}

pub fn panel_action(
    layout: &AppLayout,
    pointer: Pointer,
    controller: &UpdateController,
    warning: bool,
) -> Option<UpdateAction> {
    if !pointer.click {
        return None;
    }
    let rects = panel_layout(layout, warning);
    if rects.primary.contains(pointer.x, pointer.y) {
        controller.primary_action()
    } else if rects.secondary.contains(pointer.x, pointer.y) {
        Some(UpdateAction::Later)
    } else {
        None
    }
}

pub fn panel_layout(layout: &AppLayout, warning: bool) -> UpdatePanelLayout {
    let width = (layout.title_bar.w - 16.0).clamp(0.0, 400.0);
    let y = layout.title_bar.y + layout.title_bar.h + 4.0;
    let bottom = layout.workbench.y + layout.workbench.h;
    let height = if warning { 224.0_f64 } else { 196.0_f64 }.min((bottom - y).max(0.0));
    let panel = Rect {
        x: layout.title_bar.w - width - 8.0,
        y,
        w: width,
        h: height,
    };
    let button_h = 26.0_f64.min((height - 20.0).max(0.0));
    let primary_w = (width - 36.0).clamp(0.0, 150.0);
    let secondary_w = (width - 36.0).clamp(0.0, 112.0);
    let primary = Rect {
        x: panel.x + panel.w - 12.0 - primary_w,
        y: panel.y + panel.h - 12.0 - button_h,
        w: primary_w,
        h: button_h,
    };
    let secondary = Rect {
        x: primary.x - secondary_w - 8.0,
        y: primary.y,
        w: secondary_w,
        h: button_h,
    };
    UpdatePanelLayout {
        panel,
        primary,
        secondary,
    }
}

pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    }
}

pub(super) fn draw_panel(f: &mut Frame, layout: &AppLayout, click: bool) -> Option<UpdateAction> {
    let controller = f.update?;
    let ui = &controller.ui;
    if !ui.open || !controller.supported() {
        return None;
    }
    let warning = f.terms.is_some()
        && matches!(
            ui.state,
            UpdateState::Available | UpdateState::Staged | UpdateState::Installed
        );
    let rects = panel_layout(layout, warning);
    if rects.panel.w <= 0.0 || rects.panel.h <= 0.0 {
        return None;
    }
    f.canvas.fill_round_rect(
        rects.panel.x,
        rects.panel.y,
        rects.panel.w,
        rects.panel.h,
        8.0,
        f.theme.border,
        f.scale,
    );
    f.canvas.fill_round_rect(
        rects.panel.x + 1.0,
        rects.panel.y + 1.0,
        rects.panel.w - 2.0,
        rects.panel.h - 2.0,
        7.0,
        f.theme.bg_elevated,
        f.scale,
    );
    super::chrome::text(
        f,
        "ark update",
        rects.panel.x + 12.0,
        rects.panel.y + 10.0,
        11.0,
        f.theme.fg,
        true,
    );

    let version = ui
        .version
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "ark".to_owned());
    let mut lines: Vec<(String, ColorRgb)> = Vec::with_capacity(5);
    match ui.state {
        UpdateState::Idle => lines.push((
            ui.status
                .clone()
                .unwrap_or_else(|| "ark is up to date".to_owned()),
            f.theme.fg,
        )),
        UpdateState::Checking => lines.push(("Checking for updates…".into(), f.theme.fg)),
        UpdateState::Available => {
            lines.push((format!("Version {version} is available"), f.theme.fg));
            if let Some(size) = ui.size {
                lines.push((
                    format!("Download size: {}", format_size(size)),
                    f.theme.text_muted,
                ));
            }
            if let Some(notes) = &ui.notes_url {
                lines.push((
                    format!("Notes: {}", super::truncate_chars(notes, 52)),
                    f.theme.text_muted,
                ));
            }
        }
        UpdateState::Downloading => {
            lines.push((format!("Downloading {version}…"), f.theme.fg));
            lines.push((format!("{}% complete", ui.percent), f.theme.text_muted));
        }
        UpdateState::Verifying => lines.push(("Verifying download…".into(), f.theme.fg)),
        UpdateState::Staged => lines.push((
            ui.status
                .clone()
                .unwrap_or_else(|| format!("Version {version} is ready to install")),
            f.theme.fg,
        )),
        UpdateState::Installed => {
            lines.push((format!("Version {version} installed"), f.theme.fg));
            lines.push(("Restart to finish".into(), f.theme.text_muted));
        }
        UpdateState::Failed => {
            lines.push((
                format!(
                    "Update failed [{}]",
                    ui.error_code.as_deref().unwrap_or("unknown")
                ),
                f.theme.git_deleted,
            ));
            if let Some(message) = &ui.error_message {
                lines.push((super::truncate_chars(message, 52), f.theme.text_muted));
            }
        }
        UpdateState::Unsupported => return None,
    }
    let mut y = rects.panel.y + 38.0;
    for (line, color) in lines {
        super::chrome::text(f, &line, rects.panel.x + 12.0, y, 9.0, color, false);
        y += 17.0;
    }

    if matches!(ui.state, UpdateState::Downloading | UpdateState::Verifying) {
        let track = Rect {
            x: rects.panel.x + 12.0,
            y: rects.panel.y + 112.0,
            w: rects.panel.w - 24.0,
            h: 6.0,
        };
        f.canvas.fill_round_rect(
            track.x,
            track.y,
            track.w,
            track.h,
            3.0,
            f.theme.border,
            f.scale,
        );
        let fill = track.w * f64::from(ui.percent) / 100.0;
        if fill > 0.0 {
            f.canvas.fill_round_rect(
                track.x,
                track.y,
                fill,
                track.h,
                3.0,
                f.theme.accent,
                f.scale,
            );
        }
    }
    if warning {
        super::chrome::text(
            f,
            "Terminals are running; restart will close them.",
            rects.panel.x + 12.0,
            rects.panel.y + 126.0,
            9.0,
            f.theme.git_modified,
            false,
        );
    }

    let mut pointer = f.pointer;
    pointer.click = click;
    if let Some(action) = controller.primary_action() {
        let label = match action {
            UpdateAction::Check => "Check again",
            UpdateAction::Install => "Install Update",
            UpdateAction::Restart => "Restart to finish",
            _ => "",
        };
        if !label.is_empty()
            && draw_button(
                f.canvas,
                label,
                rects.primary,
                f.theme,
                f.text,
                pointer,
                f.scale,
            ) == Press::Clicked
        {
            return panel_action(layout, pointer, controller, warning);
        }
    }
    if draw_button(
        f.canvas,
        "Update Later",
        rects.secondary,
        f.theme,
        f.text,
        pointer,
        f.scale,
    ) == Press::Clicked
    {
        return Some(UpdateAction::Later);
    }
    panel_action(layout, pointer, controller, warning)
}

#[cfg(test)]
mod tests;
