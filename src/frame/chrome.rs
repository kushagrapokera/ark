//! Card-workbench chrome from the supplied design. All placeholder controls
//! are drawn from static labels/fixed arrays; no chrome state or cache is
//! retained between frames.

use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::icons::{self, Icon};
use crate::render::{Canvas, ColorRgb, Theme};
use crate::term::{terminal_menu_options, TerminalMenuAction};
use crate::text::{TextStyle, TextSystem};
use crate::tree::{FileTree, CHEVRON_ZONE, INDENT_PX, ROW_H, TRACK_W};
use crate::ui::Rect;

use super::{ActivityPanel, AppLayout, EditorGroup, EditorGroupLayout, Frame, LayoutAction, CARD_RADIUS};

fn text(f: &mut Frame, label: &str, x: f64, y: f64, size: f64, color: ColorRgb, bold: bool) {
    let size = super::ui_size(size, f.ui_scale);
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
        (super::ui_size(size, f.ui_scale) * f.scale) as f32,
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

#[derive(Clone, Copy, Debug)]
pub(super) struct TitleBarActions {
    pub explorer: Rect,
    pub terminal: Rect,
}

impl Default for TitleBarActions {
    fn default() -> Self {
        let empty = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        Self {
            explorer: empty,
            terminal: empty,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct TitleBarEffects {
    pub action: Option<LayoutAction>,
}

pub(super) fn title_bar_actions(layout: &AppLayout) -> TitleBarActions {
    let bar = layout.title_bar;
    if bar.w < 720.0 {
        return TitleBarActions::default();
    }
    let y = 8.0;
    TitleBarActions {
        explorer: Rect { x: bar.w - 66.0, y, w: 20.0, h: 20.0 },
        terminal: Rect { x: bar.w - 42.0, y, w: 20.0, h: 20.0 },
    }
}

pub(super) fn draw_title_bar(f: &mut Frame, layout: &AppLayout) -> TitleBarEffects {
    let mut effects = TitleBarEffects::default();
    let bar = layout.title_bar;
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.surface_chrome, f.scale);
    icon(f, Icon::Code, 20.0, 18.0, 16.0, f.theme.accent);

    if bar.w >= 1024.0 {
        let menus = ["File", "Edit", "Selection", "View", "Go"];
        let mut x = 44.0;
        let size_scale = super::ui_size(1.0, f.ui_scale);
        for label in menus {
            text(f, label, x, 9.0, 11.0, f.theme.text_muted, false);
            x += match label {
                "Selection" => 66.0,
                "File" | "Edit" | "View" => 42.0,
                _ => 30.0,
            } * size_scale;
        }
        icon(f, Icon::More, x + 4.0 * size_scale, 18.0, 14.0, f.theme.text_muted);
        icon(f, Icon::ChevronLeft, x + 42.0 * size_scale, 18.0, 14.0, f.theme.text_muted);
        icon(f, Icon::ChevronRight, x + 64.0 * size_scale, 18.0, 14.0, f.theme.text_muted);
    }

    let actions = title_bar_actions(layout);
    for (glyph, rect, active) in [
        (Icon::PanelLeft, actions.explorer, f.layout_visibility.explorer),
        (Icon::PanelBottom, actions.terminal, f.layout_visibility.terminal),
    ] {
        if rect.w <= 0.0 {
            continue;
        }
        let hovered = rect.contains(f.pointer.x, f.pointer.y);
        if hovered {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                4.0,
                f.theme.surface_hover,
                f.scale,
            );
        }
        icon(
            f,
            glyph,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
            14.0,
            if active { f.theme.accent } else { f.theme.text_muted },
        );
    }
    if f.pointer.click {
        if actions.explorer.w > 0.0 && actions.explorer.contains(f.pointer.x, f.pointer.y) {
            effects.action = Some(LayoutAction::ToggleExplorer);
        } else if actions.terminal.w > 0.0 && actions.terminal.contains(f.pointer.x, f.pointer.y) {
            effects.action = Some(LayoutAction::ToggleTerminal);
        }
    }
    effects
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

pub(super) fn explorer_action_rects(layout: &AppLayout) -> [Rect; 4] {
    let root = layout.explorer_root;
    let step = super::ui_size(18.0, layout.ui_scale);
    let hit_h = (root.h - 4.0).max(0.0).min(super::ui_size(20.0, layout.ui_scale));
    let first = root.x + root.w - 8.0 - 4.0 * step;
    std::array::from_fn(|index| Rect {
        x: first + index as f64 * step,
        y: root.y + (root.h - hit_h) / 2.0,
        w: step,
        h: hit_h,
    })
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
    let action_rects = explorer_action_rects(layout);
    let title_font_size = (super::ui_size(11.0, f.ui_scale) * f.scale) as f32;
    let title_metrics = f.text.mono_metrics(title_font_size, title_font_size * 1.35);
    let title_advance = f64::from(title_metrics.advance) / f.scale;
    let title_right = action_rects[0].x - 8.0;
    let title_chars = ((title_right - (root.x + 28.0)).max(0.0) / title_advance).floor() as usize;
    text(
        f,
        &super::truncate_chars(f.title, title_chars),
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
    for (index, (glyph, rect)) in actions.into_iter().zip(action_rects).enumerate() {
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
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
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

    effects
}

pub(super) struct NewItemLayout {
    pub(super) handle: Rect,
    pub(super) input: Rect,
}

pub(super) fn new_item_row(tree: &FileTree, parent: &Path) -> Option<(usize, usize)> {
    if parent == tree.root {
        return Some((0, 0));
    }
    let rows = tree.visible_rows();
    let (index, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.path == parent)?;
    Some((index + 1, row.depth + 1))
}

pub(super) fn new_item_layout(
    layout: &AppLayout,
    tree: &FileTree,
    parent: &Path,
) -> Option<NewItemLayout> {
    let panel = layout.tree_panel;
    if panel.w <= TRACK_W + 48.0 || panel.h < ROW_H {
        return None;
    }
    let (row_index, depth) = new_item_row(tree, parent)?;
    let row_y = panel.y + row_index as f64 * ROW_H - tree.offset;
    if row_y < panel.y || row_y + ROW_H > panel.y + panel.h {
        return None;
    }
    let input_x = panel.x + depth as f64 * INDENT_PX + CHEVRON_ZONE + 4.0;
    let input_w = (panel.x + panel.w - TRACK_W - 4.0 - input_x).max(0.0);
    if input_w < 40.0 {
        return None;
    }
    Some(NewItemLayout {
        handle: Rect {
            x: input_x - CHEVRON_ZONE,
            y: row_y + 1.0,
            w: CHEVRON_ZONE,
            h: ROW_H - 2.0,
        },
        input: Rect {
            x: input_x,
            y: row_y + 1.0,
            w: input_w,
            h: ROW_H - 2.0,
        },
    })
}

pub(super) fn draw_new_item_prompt(f: &mut Frame, layout: &AppLayout, tree: &FileTree) {
    let Some(prompt) = f.new_item.take() else {
        return;
    };
    let Some(l) = new_item_layout(layout, tree, &prompt.parent) else {
        f.new_item = Some(prompt);
        return;
    };
    icon(
        f,
        Icon::DragHandle,
        l.handle.x + l.handle.w / 2.0,
        l.handle.y + l.handle.h / 2.0,
        12.0,
        f.theme.text_muted,
    );
    prompt
        .input
        .draw_rounded(f.canvas, l.input, f.theme, f.text, f.scale, 4.0);
    if let Some(error) = &prompt.error {
        let error_y = l.input.y + l.input.h + 2.0;
        if error_y + 12.0 <= layout.tree_panel.y + layout.tree_panel.h {
            let max_chars = (l.input.w / 6.0).floor().max(1.0) as usize;
            text(
                f,
                &super::truncate_chars(error, max_chars),
                l.input.x,
                error_y,
                9.0,
                f.theme.git_deleted,
                false,
            );
        }
    }
    f.new_item = Some(prompt);
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

pub(super) fn editor_action_icons(width: f64, is_split: bool) -> &'static [Icon] {
    if is_split {
        if width >= 240.0 {
            &[Icon::Close, Icon::Split]
        } else {
            &[Icon::Close]
        }
    } else if width >= 640.0 {
        &[Icon::Split]
    } else {
        &[]
    }
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

    let actions = editor_action_icons(tabs.w, is_split);
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
    effects
}

pub(super) fn draw_terminal_resize_handle(f: &mut Frame, layout: &AppLayout) {
    let Some(rect) = super::terminal_resize_handle(layout) else {
        return;
    };
    let hovered = layout.terminal_resize.contains(f.pointer.x, f.pointer.y);
    f.canvas.fill_round_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        rect.h / 2.0,
        f.theme.border_subtle,
        f.scale,
    );
    f.canvas.fill_round_rect(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        (rect.h - 2.0) / 2.0,
        f.theme.surface_chrome,
        f.scale,
    );
    icon(
        f,
        Icon::DragHandle,
        rect.x + rect.w / 2.0,
        rect.y + rect.h / 2.0,
        10.0,
        if hovered {
            f.theme.accent
        } else {
            f.theme.text_muted
        },
    );
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TerminalActions {
    pub plus: Rect,
    pub menu: Rect,
    pub close: Rect,
}

impl Default for TerminalActions {
    fn default() -> Self {
        Self {
            plus: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            menu: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            close: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct TerminalChromeEffects {
    pub open: bool,
    pub close: Option<usize>,
    pub select: Option<usize>,
    pub menu_toggle: bool,
    pub menu_action: Option<TerminalMenuAction>,
    pub menu_dismiss: bool,
}

pub(super) fn terminal_action_icons() -> &'static [Icon] {
    &[Icon::Plus, Icon::ChevronDown, Icon::Trash]
}

pub(super) fn terminal_actions(layout: &AppLayout) -> TerminalActions {
    let pane = layout.term_dock;
    let toolbar = layout.terminal_toolbar;
    if pane.w <= 0.0 || pane.h <= 0.0 || toolbar.h < 20.0 {
        return TerminalActions::default();
    }
    let y = pane.y + (toolbar.h - 20.0) / 2.0;
    let close_x = pane.x + pane.w - 28.0;
    let menu_x = close_x - 24.0;
    let plus_x = menu_x - 24.0;
    TerminalActions {
        plus: Rect {
            x: plus_x,
            y,
            w: 20.0,
            h: 20.0,
        },
        menu: Rect {
            x: menu_x,
            y,
            w: 20.0,
            h: 20.0,
        },
        close: Rect {
            x: close_x,
            y,
            w: 20.0,
            h: 20.0,
        },
    }
}

pub(super) fn terminal_tab_rects(layout: &AppLayout, count: usize) -> Vec<Rect> {
    if count == 0 || layout.term_dock.w <= 0.0 || layout.terminal_toolbar.h < 20.0 {
        return Vec::new();
    }
    let actions = terminal_actions(layout);
    let toolbar = layout.terminal_toolbar;
    let start = toolbar.x + 8.0;
    let available = (actions.plus.x - 8.0 - start).max(0.0);
    if available < 24.0 {
        return Vec::new();
    }
    let gap = 2.0;
    let width = ((available - gap * (count - 1) as f64) / count as f64).clamp(24.0, 140.0);
    let total = width * count as f64 + gap * (count - 1) as f64;
    let width = if total > available {
        ((available - gap * (count - 1) as f64) / count as f64).max(1.0)
    } else {
        width
    };
    (0..count)
        .map(|index| Rect {
            x: start + index as f64 * (width + gap),
            y: toolbar.y + 4.0,
            w: width,
            h: toolbar.h - 8.0,
        })
        .collect()
}

pub(super) fn terminal_tab_close_rect(tab: Rect) -> Rect {
    Rect {
        x: tab.x + tab.w - 18.0,
        y: tab.y + 4.0,
        w: 14.0,
        h: tab.h - 8.0,
    }
}

pub(super) fn terminal_menu_layout(layout: &AppLayout) -> Option<(Rect, Vec<Rect>)> {
    let pane = layout.term_dock;
    if pane.w <= 0.0 || pane.h <= 0.0 {
        return None;
    }
    let actions = terminal_actions(layout);
    let width = 260.0_f64.min(pane.w - 16.0).max(120.0);
    let row_h = 28.0;
    let height = row_h * terminal_menu_options().len() as f64 + 8.0;
    let below = layout.terminal_toolbar.y + layout.terminal_toolbar.h + 4.0;
    let y = if below + height <= pane.y + pane.h - 4.0 {
        below
    } else {
        (layout.terminal_toolbar.y - 4.0 - height).max(layout.workbench.y + 4.0)
    };
    let x = (actions.menu.x + actions.menu.w / 2.0 - width / 2.0)
        .max(pane.x + 4.0)
        .min(pane.x + pane.w - width - 4.0);
    let panel = Rect {
        x,
        y,
        w: width,
        h: height,
    };
    let rows = terminal_menu_options()
        .iter()
        .enumerate()
        .map(|(index, _)| Rect {
            x: x + 4.0,
            y: y + 4.0 + index as f64 * row_h,
            w: width - 8.0,
            h: row_h,
        })
        .collect();
    Some((panel, rows))
}

#[allow(clippy::too_many_arguments)] // explicit raster context and tab state
fn draw_terminal_tab(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    ui_scale: f64,
    rect: Rect,
    label: &str,
    active: bool,
    closable: bool,
) {
    if active {
        canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.surface_hover, scale);
        canvas.fill_rect(
            rect.x,
            rect.y + rect.h - 2.0,
            rect.w,
            2.0,
            theme.accent,
            scale,
        );
    }
    let compact = rect.w < 64.0;
    let icon_x = if compact {
        rect.x + rect.w / 2.0
    } else {
        rect.x + 12.0
    };
    icons::draw(
        canvas.pixmap_mut(),
        Icon::Terminal,
        (icon_x * scale) as f32,
        ((rect.y + rect.h / 2.0) * scale) as f32,
        (super::ui_size(12.0, ui_scale) * scale) as f32,
        if active { theme.fg } else { theme.text_muted },
        scale,
    );
    if !compact {
        let right = rect.x + rect.w - if closable { 22.0 } else { 8.0 };
        let max_chars = ((right - icon_x - 12.0) / 6.6).floor().max(0.0) as usize;
        let display = super::truncate_chars(label, max_chars);
        let font_size = (super::ui_size(9.0, ui_scale) * scale) as f32;
        let metrics = text.mono_metrics(font_size, font_size * 1.35);
        let baseline =
            (rect.y + rect.h / 2.0) * scale + f64::from(metrics.ascent - metrics.descent) / 2.0;
        text.draw_line(
            canvas,
            &display,
            ((icon_x + 12.0) * scale) as f32,
            baseline as f32,
            TextStyle {
                color: if active { theme.fg } else { theme.text_muted },
                font_size,
                weight: Weight::NORMAL,
            },
        );
    }
    if closable && rect.w >= 44.0 {
        let close = terminal_tab_close_rect(rect);
        icons::draw(
            canvas.pixmap_mut(),
            Icon::Close,
            ((close.x + close.w / 2.0) * scale) as f32,
            ((close.y + close.h / 2.0) * scale) as f32,
            (super::ui_size(10.0, ui_scale) * scale) as f32,
            theme.text_muted,
            scale,
        );
    }
}

pub(super) fn draw_terminal_chrome(f: &mut Frame, layout: &AppLayout) -> TerminalChromeEffects {
    let mut effects = TerminalChromeEffects::default();
    let pane = layout.term_dock;
    if pane.h <= 0.0 {
        return effects;
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
    let has_terms = f.terms.as_ref().is_some_and(|terms| terms.is_open());
    let tab_count = f.terms.as_ref().map_or(1, |terms| terms.tabs.len().max(1));
    let active = f.terms.as_ref().map_or(0, |terms| terms.active);
    let menu_open = f.terms.as_ref().is_some_and(|terms| terms.menu_open);
    let tabs = terminal_tab_rects(layout, tab_count);
    if let Some(terms) = f.terms.as_ref() {
        for (index, rect) in tabs.iter().copied().enumerate() {
            if let Some(term) = terms.tabs.get(index) {
                draw_terminal_tab(
                    f.canvas,
                    f.text,
                    f.theme,
                    f.scale,
                    f.ui_scale,
                    rect,
                    term.label(),
                    index == active,
                    true,
                );
            }
        }
    } else {
        for rect in tabs.iter().copied() {
            draw_terminal_tab(
                f.canvas,
                f.text,
                f.theme,
                f.scale,
                f.ui_scale,
                rect,
                "Terminal",
                true,
                false,
            );
        }
    }

    let actions = terminal_actions(layout);
    let plus_hover = actions.plus.contains(f.pointer.x, f.pointer.y)
        || actions.menu.contains(f.pointer.x, f.pointer.y);
    if plus_hover {
        f.canvas.fill_round_rect(
            actions.plus.x - 2.0,
            actions.plus.y,
            44.0,
            actions.plus.h,
            4.0,
            f.theme.surface_hover,
            f.scale,
        );
    }
    for (glyph, rect) in terminal_action_icons()
        .iter()
        .copied()
        .zip([actions.plus, actions.menu, actions.close])
    {
        icon(
            f,
            glyph,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
            13.0,
            f.theme.text_muted,
        );
    }

    if f.pointer.click {
        if actions.plus.contains(f.pointer.x, f.pointer.y) {
            effects.open = true;
            effects.menu_dismiss = menu_open;
        } else if actions.menu.contains(f.pointer.x, f.pointer.y) {
            effects.menu_toggle = true;
        } else if actions.close.contains(f.pointer.x, f.pointer.y) && has_terms {
            effects.close = Some(active);
            effects.menu_dismiss = menu_open;
        } else {
            for (index, rect) in tabs.iter().copied().enumerate() {
                if !rect.contains(f.pointer.x, f.pointer.y) {
                    continue;
                }
                if has_terms
                    && index < tab_count
                    && terminal_tab_close_rect(rect).contains(f.pointer.x, f.pointer.y)
                {
                    effects.close = Some(index);
                } else if has_terms && index < tab_count {
                    effects.select = Some(index);
                }
                effects.menu_dismiss = menu_open;
                break;
            }
        }
    }

    effects
}

/// Draw the profile menu last so terminal glyphs can never paint over it.
pub(super) fn draw_terminal_menu(
    f: &mut Frame,
    layout: &AppLayout,
    effects: &mut TerminalChromeEffects,
) {
    if !f.terms.as_ref().is_some_and(|terms| terms.menu_open) {
        return;
    }
    let Some((panel, rows)) = terminal_menu_layout(layout) else {
        return;
    };
    card(f, panel);
    let shortcuts = ["Ctrl+Shift+`", "Ctrl+Shift+Alt+`", "", "", ""];
    for (index, (option, row)) in terminal_menu_options()
        .iter()
        .zip(rows.iter().copied())
        .enumerate()
    {
        if option.separator_before {
            f.canvas.fill_rect(
                row.x,
                row.y,
                row.w,
                1.0,
                f.theme.border_subtle,
                f.scale,
            );
        }
        let hovered = row.contains(f.pointer.x, f.pointer.y);
        if hovered {
            f.canvas
                .fill_rect(row.x, row.y, row.w, row.h, f.theme.surface_hover, f.scale);
        }
        text(
            f,
            option.label,
            row.x + 10.0,
            row.y + (row.h - 10.0) / 2.0,
            10.0,
            f.theme.fg,
            false,
        );
        if !shortcuts[index].is_empty() {
            let shortcut_w = shortcuts[index].len() as f64 * 6.2;
            text(
                f,
                shortcuts[index],
                row.x + row.w - shortcut_w - 10.0,
                row.y + (row.h - 9.0) / 2.0,
                9.0,
                f.theme.text_muted,
                false,
            );
        }
        if f.pointer.click && hovered {
            effects.menu_action = Some(option.action);
        }
    }
    let actions = terminal_actions(layout);
    let tab_count = f.terms.as_ref().map_or(1, |terms| terms.tabs.len().max(1));
    let tabs = terminal_tab_rects(layout, tab_count);
    let inside_menu = rows
        .iter()
        .any(|row| row.contains(f.pointer.x, f.pointer.y));
    let inside_actions = [actions.plus, actions.menu, actions.close]
        .iter()
        .any(|rect| rect.contains(f.pointer.x, f.pointer.y));
    let inside_tabs = tabs
        .iter()
        .any(|rect| rect.contains(f.pointer.x, f.pointer.y));
    if f.pointer.click
        && !inside_menu
        && !inside_actions
        && !inside_tabs
        && effects.menu_action.is_none()
    {
        effects.menu_dismiss = true;
    }
}

pub(super) fn draw_terminal_placeholder(f: &mut Frame, layout: &AppLayout) {
    let grid = layout.term_grid;
    if grid.w <= 0.0 || grid.h <= 0.0 {
        return;
    }
    let prompt = format!("ark@ark  in  repo:{}  on  main", f.title);
    text(
        f,
        &prompt,
        grid.x,
        grid.y + super::ui_size(8.0, f.ui_scale),
        11.0,
        f.theme.fg_dim,
        false,
    );
    text(
        f,
        "> Type a command to begin",
        grid.x,
        grid.y + super::ui_size(34.0, f.ui_scale),
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
        1 => 38.0,
        _ => 0.0,
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
    if width >= 640.0 {
        1
    } else {
        0
    }
}
