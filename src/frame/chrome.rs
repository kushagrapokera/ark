//! Card-workbench chrome from the supplied design. All placeholder controls
//! are drawn from static labels/fixed arrays; no chrome state or cache is
//! retained between frames.

use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::icons::{self, Icon};
use crate::render::ColorRgb;
use crate::text::TextStyle;
use crate::tree::{FileTree, INDENT_PX, ROW_H};
use crate::ui::{draw_button, Press, Rect};

use super::{ActivityPanel, AppLayout, EditorGroup, EditorGroupLayout, Frame, CARD_RADIUS};

fn text(f: &mut Frame, label: &str, x: f64, y: f64, size: f64, color: ColorRgb, bold: bool) {
    let font_size = (size * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, font_size * 1.35);
    let baseline = (y + size / 2.0) * f.scale + f64::from(m.ascent - m.descent) / 2.0;
    f.text.draw_line(
        f.canvas,
        label,
        (x * f.scale) as f32,
        baseline as f32,
        TextStyle {
            color,
            font_size,
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        },
    );
}

fn icon(f: &mut Frame, icon: Icon, cx: f64, cy: f64, size: f64, color: ColorRgb) {
    icons::draw(
        f.canvas.pixmap_mut(),
        icon,
        (cx * f.scale) as f32,
        (cy * f.scale) as f32,
        (size * f.scale) as f32,
        color,
        f.scale,
    );
}

fn card(f: &mut Frame, rect: Rect) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    f.canvas.fill_round_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        CARD_RADIUS,
        f.theme.border_subtle,
        f.scale,
    );
    f.canvas.fill_round_rect(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        (CARD_RADIUS - 1.0).max(0.0),
        f.theme.surface,
        f.scale,
    );
}

pub(super) fn draw_title_bar(f: &mut Frame, layout: &AppLayout) {
    let bar = layout.title_bar;
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.surface_chrome, f.scale);
    icon(f, Icon::Code, 20.0, 18.0, 16.0, f.theme.accent);

    let full = bar.w >= 1024.0;
    let medium = bar.w >= 720.0;
    if full {
        let menus = ["File", "Edit", "Selection", "View", "Go"];
        let mut x = 44.0;
        for label in menus {
            text(f, label, x, 9.0, 11.0, f.theme.text_muted, false);
            x += match label {
                "Selection" => 66.0,
                "File" | "Edit" | "View" => 42.0,
                _ => 30.0,
            };
        }
        icon(f, Icon::More, x + 4.0, 18.0, 14.0, f.theme.text_muted);
        icon(
            f,
            Icon::ChevronLeft,
            x + 42.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::ChevronRight,
            x + 64.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
    }

    let (reserve, left, show_update) = if full {
        (190.0, 340.0, true)
    } else if medium {
        (190.0, 40.0, true)
    } else if bar.w >= 520.0 {
        (100.0, 40.0, true)
    } else if bar.w >= 300.0 {
        (72.0, 40.0, true)
    } else {
        (12.0, 40.0, false)
    };
    let command_w = (bar.w - left - reserve).clamp(0.0, 320.0);
    if command_w > 0.0 {
        let command_x = left + (bar.w - left - reserve - command_w) / 2.0;
        let command = Rect {
            x: command_x,
            y: 6.0,
            w: command_w,
            h: 24.0,
        };
        f.canvas.fill_round_rect(
            command.x,
            command.y,
            command.w,
            command.h,
            4.0,
            f.theme.surface,
            f.scale,
        );
        f.canvas.stroke_rect_strips(
            command.x,
            command.y,
            command.w,
            command.h,
            f.theme.border,
            f.scale,
        );
        icon(
            f,
            Icon::Search,
            command.x + 14.0,
            18.0,
            13.0,
            f.theme.text_muted,
        );
        let max_chars = ((command.w - 38.0) / 6.6).floor().max(1.0) as usize;
        let title = super::truncate_chars(f.title, max_chars);
        text(f, &title, command.x + 28.0, 9.0, 11.0, f.theme.fg, false);
    }

    if show_update {
        let update = Rect {
            x: bar.w - reserve + 8.0,
            y: 7.0,
            w: 58.0,
            h: 22.0,
        };
        f.canvas.fill_round_rect(
            update.x,
            update.y,
            update.w,
            update.h,
            3.0,
            f.theme.accent,
            f.scale,
        );
        text(
            f,
            "Update",
            update.x + 10.0,
            11.0,
            10.0,
            f.theme.on_accent,
            true,
        );
    }

    if full {
        icon(
            f,
            Icon::PanelLeft,
            bar.w - 108.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::PanelBottom,
            bar.w - 84.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::PanelRight,
            bar.w - 60.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(f, Icon::More, bar.w - 32.0, 18.0, 14.0, f.theme.text_muted);
    } else if bar.w >= 520.0 {
        icon(f, Icon::More, bar.w - 20.0, 18.0, 14.0, f.theme.text_muted);
    }
}

pub(super) fn draw_cards(f: &mut Frame, layout: &AppLayout) {
    f.canvas.fill(f.theme.surface_chrome);
    card(f, layout.activity_bar);
    card(f, layout.explorer_card);
    card(f, layout.editor_card);
    if layout.term_dock.h > 0.0 {
        card(f, layout.term_dock);
    }
}

pub(super) fn draw_activity_bar(f: &mut Frame, layout: &AppLayout) -> Option<ActivityPanel> {
    let bar = layout.activity_bar;
    if bar.w <= 0.0 || bar.h <= 0.0 {
        return None;
    }
    let items = [
        Icon::File,
        Icon::Search,
        Icon::SourceControl,
        Icon::Bug,
        Icon::Extension,
    ];
    let mut clicked = None;
    let step = if bar.h >= 360.0 {
        40.0
    } else if bar.h >= 220.0 {
        32.0
    } else {
        28.0
    };
    let visible = ((bar.h - 8.0) / step).floor().max(0.0) as usize;
    for (index, glyph) in items.into_iter().take(visible).enumerate() {
        let rect = Rect {
            x: bar.x + 6.0,
            y: bar.y + 8.0 + index as f64 * step,
            w: bar.w - 12.0,
            h: step - 4.0,
        };
        let active = matches!(
            (index, f.activity),
            (0, ActivityPanel::Explorer)
                | (2, ActivityPanel::SourceControl)
                | (3, ActivityPanel::Terminal)
                | (4, ActivityPanel::Config)
        );
        if active {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                3.0,
                f.theme.surface_hover,
                f.scale,
            );
            f.canvas
                .fill_rect(rect.x, rect.y, 2.0, rect.h, f.theme.accent, f.scale);
        } else if rect.contains(f.pointer.x, f.pointer.y) {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                3.0,
                f.theme.surface_hover,
                f.scale,
            );
        }
        let color = if active {
            f.theme.fg
        } else {
            f.theme.text_muted
        };
        icon(
            f,
            glyph,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
            18.0,
            color,
        );
        if index == 2 {
            let count = f.git.map(|git| git.changed_count()).unwrap_or(0);
            if count > 0 {
                let badge = Rect {
                    x: rect.x + rect.w - 16.0,
                    y: rect.y + rect.h - 12.0,
                    w: 14.0,
                    h: 12.0,
                };
                f.canvas.fill_round_rect(
                    badge.x,
                    badge.y,
                    badge.w,
                    badge.h,
                    6.0,
                    f.theme.accent,
                    f.scale,
                );
                text(
                    f,
                    &count.to_string(),
                    badge.x + 4.0,
                    badge.y + 1.0,
                    8.0,
                    f.theme.on_accent,
                    true,
                );
            }
        }
        if f.new_item.is_none() && f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
            clicked = match index {
                0 => Some(ActivityPanel::Explorer),
                2 => Some(ActivityPanel::SourceControl),
                3 => Some(ActivityPanel::Terminal),
                4 => Some(ActivityPanel::Config),
                _ => None,
            };
        }
    }

    clicked
}

#[derive(Default)]
pub(super) struct ExplorerEffects {
    pub new_file: bool,
    pub new_folder: bool,
    pub refresh: bool,
    pub collapse: bool,
}

pub(super) fn draw_explorer_chrome(f: &mut Frame, layout: &AppLayout) -> ExplorerEffects {
    let mut effects = ExplorerEffects::default();
    if layout.explorer_card.w <= 0.0 {
        return effects;
    }
    let header = layout.explorer_header;
    f.canvas.fill_rect(
        header.x + 1.0,
        header.y + 1.0,
        header.w - 2.0,
        header.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    text(
        f,
        if f.activity == ActivityPanel::SourceControl {
            "SOURCE CONTROL"
        } else {
            "EXPLORER"
        },
        header.x + 12.0,
        header.y + 10.0,
        10.0,
        f.theme.fg,
        true,
    );
    icon(
        f,
        Icon::More,
        header.x + header.w - 16.0,
        header.y + header.h / 2.0,
        14.0,
        f.theme.text_muted,
    );
    if f.activity == ActivityPanel::SourceControl {
        return effects;
    }

    let root = layout.explorer_root;
    icon(
        f,
        Icon::ChevronDown,
        root.x + 14.0,
        root.y + root.h / 2.0,
        12.0,
        f.theme.fg,
    );
    text(
        f,
        f.title,
        root.x + 28.0,
        root.y + 7.0,
        11.0,
        f.theme.fg,
        true,
    );
    let actions = [
        Icon::NewFile,
        Icon::NewFolder,
        Icon::Refresh,
        Icon::Collapse,
    ];
    for (index, glyph) in actions.into_iter().enumerate() {
        let cx = root.x + root.w - 58.0 + index as f64 * 16.0;
        let rect = Rect {
            x: cx - 8.0,
            y: root.y + 4.0,
            w: 16.0,
            h: root.h - 8.0,
        };
        let hovered = rect.contains(f.pointer.x, f.pointer.y);
        if hovered {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                3.0,
                f.theme.surface_hover,
                f.scale,
            );
        }
        icon(
            f,
            glyph,
            cx,
            root.y + root.h / 2.0,
            12.0,
            if hovered {
                f.theme.fg
            } else {
                f.theme.text_muted
            },
        );
        if f.new_item.is_none() && f.pointer.click && hovered {
            match index {
                0 => effects.new_file = true,
                1 => effects.new_folder = true,
                2 => effects.refresh = true,
                3 => effects.collapse = true,
                _ => {}
            }
        }
    }

    let footer = layout.explorer_footer;
    f.canvas.fill_rect(
        footer.x + 1.0,
        footer.y,
        footer.w - 2.0,
        footer.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    f.canvas.fill_rect(
        footer.x + 1.0,
        footer.y,
        footer.w - 2.0,
        1.0,
        f.theme.border_subtle,
        f.scale,
    );
    for (index, label) in ["OUTLINE", "TIMELINE"].into_iter().enumerate() {
        let y = footer.y + 6.0 + index as f64 * 28.0;
        icon(
            f,
            Icon::ChevronRight,
            footer.x + 14.0,
            y + 8.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            label,
            footer.x + 28.0,
            y + 2.0,
            10.0,
            f.theme.text_muted,
            true,
        );
    }
    effects
}

pub(super) struct NewItemLayout {
    pub(super) panel: Rect,
    pub(super) input: Rect,
    pub(super) create: Rect,
    pub(super) cancel: Rect,
}

pub(super) fn new_item_layout(
    layout: &AppLayout,
    tree: &FileTree,
    parent: &Path,
) -> Option<NewItemLayout> {
    let explorer = layout.explorer_card;
    if explorer.w <= 0.0 || explorer.h < 132.0 {
        return None;
    }
    let (anchor_y, depth) = if parent == tree.root {
        (layout.explorer_root.y + layout.explorer_root.h + 8.0, 0)
    } else if let Some((index, row)) = tree
        .visible_rows()
        .iter()
        .enumerate()
        .find(|(_, row)| row.path == parent)
    {
        (
            layout.tree_panel.y + index as f64 * ROW_H - tree.offset + ROW_H,
            row.depth,
        )
    } else {
        (layout.explorer_root.y + layout.explorer_root.h + 8.0, 0)
    };
    let panel_h = 116.0;
    let min_y = explorer.y + 8.0;
    let max_y = explorer.y + explorer.h - 8.0 - panel_h;
    let panel_x = explorer.x + 8.0 + (depth as f64 * INDENT_PX).min(48.0);
    let panel = Rect {
        x: panel_x,
        y: anchor_y.clamp(min_y, max_y),
        w: (explorer.x + explorer.w - 8.0 - panel_x).max(0.0),
        h: panel_h,
    };
    let inner_w = (panel.w - 16.0).max(0.0);
    let button_w = ((inner_w - 8.0) / 2.0).max(0.0);
    Some(NewItemLayout {
        panel,
        input: Rect {
            x: panel.x + 8.0,
            y: panel.y + 32.0,
            w: inner_w,
            h: 24.0,
        },
        create: Rect {
            x: panel.x + 8.0,
            y: panel.y + 82.0,
            w: button_w,
            h: 24.0,
        },
        cancel: Rect {
            x: panel.x + 16.0 + button_w,
            y: panel.y + 82.0,
            w: button_w,
            h: 24.0,
        },
    })
}

pub(super) fn draw_new_item_prompt(
    f: &mut Frame,
    layout: &AppLayout,
    tree: &FileTree,
) -> (bool, bool) {
    let Some(prompt) = f.new_item.take() else {
        return (false, false);
    };
    let Some(l) = new_item_layout(layout, tree, &prompt.parent) else {
        f.new_item = Some(prompt);
        return (false, false);
    };
    card(f, l.panel);
    text(
        f,
        prompt.kind.title(),
        l.panel.x + 10.0,
        l.panel.y + 10.0,
        11.0,
        f.theme.fg,
        true,
    );
    prompt
        .input
        .draw(f.canvas, l.input, f.theme, f.text, f.scale);
    if prompt.input.text.is_empty() {
        text(
            f,
            prompt.kind.placeholder(),
            l.input.x + 10.0,
            l.input.y + (l.input.h - 11.0) / 2.0,
            11.0,
            f.theme.fg_dim,
            false,
        );
    }
    if let Some(error) = &prompt.error {
        let max_chars = ((l.panel.w - 20.0) / 6.0).floor().max(1.0) as usize;
        text(
            f,
            &super::truncate_chars(error, max_chars),
            l.panel.x + 10.0,
            l.panel.y + 62.0,
            9.0,
            f.theme.git_deleted,
            false,
        );
    }
    let create = draw_button(
        f.canvas, "Create", l.create, f.theme, f.text, f.pointer, f.scale,
    ) == Press::Clicked;
    let cancel = draw_button(
        f.canvas, "Cancel", l.cancel, f.theme, f.text, f.pointer, f.scale,
    ) == Press::Clicked;
    f.new_item = Some(prompt);
    (create, cancel)
}

pub(super) fn draw_source_control_breadcrumb(f: &mut Frame, pane: Rect) {
    if pane.w <= 0.0 || pane.h <= 0.0 {
        return;
    }
    f.canvas
        .fill_rect(pane.x, pane.y, pane.w, pane.h, f.theme.bg, f.scale);
    text(
        f,
        "SOURCE CONTROL",
        pane.x + 12.0,
        pane.y + (pane.h - 11.0) / 2.0,
        11.0,
        f.theme.fg,
        true,
    );
    if pane.w >= 260.0 {
        text(
            f,
            f.title,
            pane.x + 132.0,
            pane.y + (pane.h - 11.0) / 2.0,
            11.0,
            f.theme.text_muted,
            false,
        );
    }
}

#[derive(Default)]
pub(super) struct EditorGroupEffects {
    pub select: Option<PathBuf>,
    pub close: Option<PathBuf>,
    pub close_group: bool,
    pub toggle_split: bool,
    pub focus: bool,
}

pub(super) fn draw_editor_group_chrome(
    f: &mut Frame,
    layout: EditorGroupLayout,
    group: &EditorGroup,
    index: usize,
    active_group: usize,
    is_split: bool,
    source_control_body: bool,
) -> EditorGroupEffects {
    let mut effects = EditorGroupEffects::default();
    let tabs = layout.tabs;
    if tabs.w <= 0.0 || tabs.h <= 0.0 {
        return effects;
    }
    f.canvas.fill_rect(
        tabs.x + 1.0,
        tabs.y + 1.0,
        tabs.w - 2.0,
        tabs.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    if index == active_group {
        f.canvas.fill_rect(
            tabs.x + 1.0,
            tabs.y + 1.0,
            tabs.w - 2.0,
            2.0,
            f.theme.accent,
            f.scale,
        );
    }

    let actions: &[Icon] = if is_split {
        if tabs.w >= 360.0 {
            &[Icon::Close, Icon::Split, Icon::More]
        } else if tabs.w >= 240.0 {
            &[Icon::Close, Icon::Split]
        } else {
            &[Icon::Close]
        }
    } else if tabs.w >= 640.0 {
        &[Icon::Split, Icon::More]
    } else {
        &[]
    };
    let reserve = match actions.len() {
        3 => 76.0,
        2 => 58.0,
        1 => 38.0,
        _ => 0.0,
    };
    let tab_pane = Rect {
        x: tabs.x,
        y: tabs.y,
        w: (tabs.w - reserve).max(0.0),
        h: tabs.h,
    };
    let font_size = (crate::text::UI_FONT_SIZE as f64 * f.scale) as f32;
    let m = f
        .text
        .mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance) / f.scale;
    let rects = super::file_tab_rects(tab_pane, group.tabs.len());
    for (rect, path) in rects.iter().zip(group.tabs.iter()) {
        let active = group.active.as_deref() == Some(path.as_path());
        if active {
            f.canvas
                .fill_rect(rect.x, rect.y, rect.w, rect.h, f.theme.bg, f.scale);
            f.canvas.fill_rect(
                rect.x,
                rect.y + rect.h - 2.0,
                rect.w,
                2.0,
                f.theme.accent,
                f.scale,
            );
        } else if rect.contains(f.pointer.x, f.pointer.y) {
            f.canvas
                .fill_rect(rect.x, rect.y, rect.w, rect.h, f.theme.selection, f.scale);
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let max_chars = (((rect.w - 44.0) / advance).floor()).max(1.0) as usize;
        let label = crate::frame::truncate_chars(name.as_ref(), max_chars);
        let baseline = (rect.y + rect.h / 2.0) * f.scale + f64::from(m.ascent - m.descent) / 2.0;
        f.text.draw_line(
            f.canvas,
            &label,
            ((rect.x + 10.0) * f.scale) as f32,
            baseline as f32,
            TextStyle {
                color: f.theme.fg,
                font_size,
                weight: Weight::NORMAL,
            },
        );
        let close = Rect {
            x: rect.x + rect.w - 22.0,
            y: rect.y + (rect.h - 16.0) / 2.0,
            w: 16.0,
            h: 16.0,
        };
        f.text.draw_line(
            f.canvas,
            "\u{d7}",
            ((close.x + 4.0) * f.scale) as f32,
            baseline as f32,
            TextStyle {
                color: f.theme.fg_dim,
                font_size,
                weight: Weight::NORMAL,
            },
        );
        if f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
            if close.contains(f.pointer.x, f.pointer.y) {
                effects.close = Some(path.clone());
            } else {
                effects.select = Some(path.clone());
                effects.focus = true;
            }
        }
    }

    if !actions.is_empty() {
        let first = tabs.x + tabs.w - 18.0 - (actions.len() - 1) as f64 * 18.0;
        for (action_index, glyph) in actions.iter().enumerate() {
            let cx = first + action_index as f64 * 18.0;
            icon(
                f,
                *glyph,
                cx,
                tabs.y + tabs.h / 2.0,
                14.0,
                f.theme.text_muted,
            );
            let rect = Rect {
                x: cx - 9.0,
                y: tabs.y + (tabs.h - 18.0) / 2.0,
                w: 18.0,
                h: 18.0,
            };
            if f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
                match glyph {
                    Icon::Close if is_split && index == 1 => effects.close_group = true,
                    Icon::Split => effects.toggle_split = true,
                    _ => {}
                }
            }
        }
    }

    if source_control_body && index == active_group {
        draw_source_control_breadcrumb(f, layout.breadcrumb);
    } else if let Some(path) = group.active.as_deref() {
        super::draw_path_breadcrumb(f, layout.breadcrumb, path);
    } else {
        f.canvas.fill_rect(
            layout.breadcrumb.x,
            layout.breadcrumb.y,
            layout.breadcrumb.w,
            layout.breadcrumb.h,
            f.theme.bg,
            f.scale,
        );
    }
    draw_minimap_rect(f, layout.minimap);
    effects
}

pub(super) fn draw_minimap_rect(f: &mut Frame, map: Rect) {
    if map.w <= 0.0 || map.h <= 0.0 {
        return;
    }
    f.canvas
        .fill_rect(map.x, map.y, 1.0, map.h, f.theme.border_subtle, f.scale);
    let colors = [
        f.theme.syntax_comment,
        f.theme.syntax_keyword,
        f.theme.syntax_type,
        f.theme.syntax_function,
        f.theme.syntax_variable,
        f.theme.syntax_string,
        f.theme.syntax_number,
        f.theme.syntax_control,
    ];
    let widths = [42.0, 26.0, 38.0, 18.0, 44.0, 34.0, 30.0, 40.0];
    for index in 0..24 {
        let y = map.y + 8.0 + index as f64 * 9.0;
        if y >= map.y + map.h - 4.0 {
            break;
        }
        let width = widths[index % widths.len()];
        f.canvas.fill_rect(
            map.x + map.w - 4.0 - width,
            y,
            width,
            2.0,
            colors[index % colors.len()],
            f.scale,
        );
    }
}

pub(super) fn draw_terminal_chrome(f: &mut Frame, layout: &AppLayout) -> (bool, bool) {
    let pane = layout.term_dock;
    if pane.h <= 0.0 {
        return (false, false);
    }
    let toolbar = layout.terminal_toolbar;
    f.canvas.fill_rect(
        toolbar.x + 1.0,
        toolbar.y + 1.0,
        toolbar.w - 2.0,
        toolbar.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    let tabs: &[&str] = if pane.w >= 560.0 {
        &["PROBLEMS", "OUTPUT", "TERMINAL", "PORTS", "DEBUG CONSOLE"]
    } else if pane.w >= 360.0 {
        &["PROBLEMS", "OUTPUT", "TERMINAL"]
    } else {
        &["TERMINAL"]
    };
    let mut x = toolbar.x + 12.0;
    for label in tabs {
        let w = match *label {
            "DEBUG CONSOLE" => 116.0,
            "PROBLEMS" | "OUTPUT" => 70.0,
            _ => 62.0,
        };
        text(
            f,
            label,
            x,
            toolbar.y + (toolbar.h - 9.0) / 2.0,
            9.0,
            if *label == "TERMINAL" {
                f.theme.fg
            } else {
                f.theme.text_muted
            },
            true,
        );
        if *label == "TERMINAL" {
            f.canvas.fill_rect(
                x,
                toolbar.y + toolbar.h - 2.0,
                w - 12.0,
                2.0,
                f.theme.accent,
                f.scale,
            );
        }
        x += w;
    }

    if pane.w >= 700.0 {
        text(
            f,
            "bash",
            toolbar.x + toolbar.w - 210.0,
            toolbar.y + (toolbar.h - 10.0) / 2.0,
            10.0,
            f.theme.fg,
            false,
        );
    }
    let (plus, close) = terminal_actions(layout);
    let open_clicked = plus.is_some_and(|rect| {
        draw_button(f.canvas, "", rect, f.theme, f.text, f.pointer, f.scale) == Press::Clicked
    });
    let close_clicked = close.is_some_and(|rect| {
        draw_button(f.canvas, "", rect, f.theme, f.text, f.pointer, f.scale) == Press::Clicked
    });
    let actions: &[Icon] = match terminal_action_count(pane.w) {
        5 => &[
            Icon::Plus,
            Icon::Split,
            Icon::Close,
            Icon::More,
            Icon::ChevronUp,
        ],
        2 => &[Icon::Plus, Icon::Close],
        _ => &[Icon::Plus],
    };
    let first = pane.x + pane.w - 38.0 - (actions.len() - 1) as f64 * 20.0;
    for (index, glyph) in actions.iter().enumerate() {
        icon(
            f,
            *glyph,
            first + index as f64 * 20.0,
            toolbar.y + toolbar.h / 2.0,
            13.0,
            f.theme.text_muted,
        );
    }

    let sessions = layout.terminal_sessions;
    if sessions.w > 0.0 && sessions.h > 0.0 {
        f.canvas.fill_rect(
            sessions.x,
            sessions.y,
            1.0,
            sessions.h,
            f.theme.border_subtle,
            f.scale,
        );
        let (count, active) = f
            .terms
            .as_ref()
            .map(|terms| (terms.tabs.len().min(3), terms.active))
            .unwrap_or((3, 0));
        let visible = count.min(((sessions.h - 8.0) / 32.0).floor().max(1.0) as usize);
        for index in 0..visible {
            let y = sessions.y + 8.0 + index as f64 * 32.0;
            if index == active {
                f.canvas.fill_round_rect(
                    sessions.x + 6.0,
                    y,
                    24.0,
                    24.0,
                    3.0,
                    f.theme.surface_hover,
                    f.scale,
                );
            }
            icon(
                f,
                Icon::Terminal,
                sessions.x + 18.0,
                y + 12.0,
                14.0,
                if index == active {
                    f.theme.fg
                } else {
                    f.theme.text_muted
                },
            );
        }
    }
    (open_clicked, close_clicked)
}

fn terminal_action_count(width: f64) -> usize {
    if width >= 560.0 {
        5
    } else if width >= 360.0 {
        2
    } else {
        1
    }
}

/// Hit rects for the two functional terminal actions. Chrome and interaction
/// share this pure geometry so resizing cannot leave a click target misplaced.
pub(super) fn terminal_actions(layout: &AppLayout) -> (Option<Rect>, Option<Rect>) {
    let pane = layout.term_dock;
    let toolbar = layout.terminal_toolbar;
    if pane.w <= 0.0 || pane.h <= 0.0 || toolbar.h < 20.0 {
        return (None, None);
    }
    let count = terminal_action_count(pane.w);
    let first = pane.x + pane.w - 38.0 - (count - 1) as f64 * 20.0;
    let y = pane.y + (toolbar.h - 20.0) / 2.0;
    let plus = Some(Rect {
        x: first - 10.0,
        y,
        w: 20.0,
        h: 20.0,
    });
    let close = (count >= 2).then(|| {
        let index = if count == 5 { 2 } else { 1 };
        Rect {
            x: first + index as f64 * 20.0 - 10.0,
            y,
            w: 20.0,
            h: 20.0,
        }
    });
    (plus, close)
}

pub(super) fn draw_terminal_placeholder(f: &mut Frame, layout: &AppLayout) {
    let grid = layout.term_grid;
    if grid.w <= 0.0 || grid.h <= 0.0 {
        return;
    }
    text(
        f,
        "ark@ark",
        grid.x,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_control,
        false,
    );
    text(
        f,
        "in",
        grid.x + 58.0,
        grid.y + 8.0,
        11.0,
        f.theme.text_muted,
        false,
    );
    text(
        f,
        "repo:",
        grid.x + 76.0,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_type,
        false,
    );
    text(
        f,
        f.title,
        grid.x + 108.0,
        grid.y + 8.0,
        11.0,
        f.theme.fg,
        false,
    );
    text(
        f,
        "on",
        grid.x + 150.0,
        grid.y + 8.0,
        11.0,
        f.theme.text_muted,
        false,
    );
    text(
        f,
        "main",
        grid.x + 168.0,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_function,
        false,
    );
    text(
        f,
        ">",
        grid.x,
        grid.y + 34.0,
        11.0,
        f.theme.git_untracked,
        true,
    );
    text(
        f,
        "Type a command to begin",
        grid.x + 18.0,
        grid.y + 34.0,
        11.0,
        f.theme.text_dim,
        false,
    );
}

pub(super) fn draw_status_bar(f: &mut Frame, layout: &AppLayout) {
    let bar = layout.status;
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.surface_chrome, f.scale);
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, 1.0, f.theme.border_subtle, f.scale);
    f.canvas
        .fill_rect(bar.x, bar.y, 32.0, bar.h, f.theme.accent, f.scale);
    icon(
        f,
        Icon::Remote,
        bar.x + 16.0,
        bar.y + bar.h / 2.0,
        13.0,
        f.theme.on_accent,
    );

    let mut left_end = bar.x + 32.0;
    if bar.w >= 180.0 {
        text(
            f,
            "main*",
            bar.x + 42.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        left_end = bar.x + 82.0;
    }
    if bar.w >= 300.0 {
        icon(
            f,
            Icon::Sync,
            bar.x + 88.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        left_end = bar.x + 104.0;
    }
    if bar.w >= 360.0 {
        icon(
            f,
            Icon::Close,
            bar.x + 112.0,
            bar.y + bar.h / 2.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            "0",
            bar.x + 122.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        icon(
            f,
            Icon::Warning,
            bar.x + 144.0,
            bar.y + bar.h / 2.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            "0",
            bar.x + 154.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        left_end = bar.x + 170.0;
    }

    // Pick the richest right-hand group that fits after the left cluster.
    let groups: [&[&str]; 5] = [
        &[],
        &["Ln 1, Col 1"],
        &["Ln 1, Col 1", "UTF-8"],
        &["Ln 1, Col 1", "Spaces: 4", "UTF-8", "LF", "{} Rust"],
        &[
            "Ln 1, Col 1",
            "Spaces: 4",
            "UTF-8",
            "LF",
            "{} Rust",
            "Prettier",
        ],
    ];
    let available = (bar.x + bar.w - left_end - 12.0).max(0.0);
    let mut labels: &[&str] = &[];
    for group in groups.iter().rev() {
        let width = group
            .iter()
            .map(|label| label.len() as f64 * 6.2 + 16.0)
            .sum::<f64>();
        if width <= available {
            labels = group;
            break;
        }
    }
    let mut x = bar.x + bar.w - 12.0;
    for label in labels.iter().rev() {
        let w = label.len() as f64 * 6.2 + 16.0;
        x -= w;
        text(
            f,
            label,
            x + 8.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
    }
    if x - 106.0 >= left_end {
        icon(
            f,
            Icon::Zoom,
            x - 18.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Asterisk,
            x - 42.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Check,
            x - 66.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Bell,
            x - 94.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
    }
}

/// Width reserved for the editor actions that are visible at this width.
#[cfg(test)]
pub(super) fn tab_strip(layout: &AppLayout) -> Rect {
    let reserve = match editor_action_count(layout.editor_tabs.w) {
        2 => 58.0,
        _ => 38.0,
    };
    Rect {
        x: layout.editor_tabs.x,
        y: layout.editor_tabs.y,
        w: (layout.editor_tabs.w - reserve).max(0.0),
        h: layout.editor_tabs.h,
    }
}

#[cfg(test)]
fn editor_action_count(width: f64) -> usize {
    if width >= 320.0 {
        2
    } else {
        1
    }
}
