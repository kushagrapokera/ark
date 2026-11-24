//! The app frame: card-based workbench layout and scene composition. Layout
//! is computed in LOGICAL px from the window size; `draw_frame` rasterizes one
//! frame and reports what changed for damage tracking.

use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::commit::{self, CommitPanel};
use crate::configed::ConfigEditor;
use crate::editor_groups::EditorGroup;
use crate::git::{FileDiff, GitStatus};
use crate::highlight::Highlighter;
use crate::new_item::NewItemPrompt;
use crate::render::{Canvas, ColorRgb, Theme};
use crate::term::{self, TerminalMenuAction, TerminalTabs};
use crate::text::{TextStyle, TextSystem, UI_FONT_SIZE};
use crate::tree::{self, FileTree, ROW_H};
use crate::ui::{Pointer, Rect, Scrollbar, Split};
use crate::viewer::{self, Viewer, LINE_H};

mod chrome;

/// Card-layout metrics from the supplied workbench design.
const PAD: f64 = 8.0;
const GAP: f64 = 8.0;
const TITLE_H: f64 = 36.0;
const ACTIVITY_W: f64 = 48.0;
const EXPLORER_HEADER_H: f64 = 36.0;
const EXPLORER_ROOT_H: f64 = 28.0;
const EDITOR_TAB_H: f64 = 36.0;
const BREADCRUMB_H: f64 = 24.0;
const TERM_DOCK_H: f64 = 224.0;
const TERM_MIN_H: f64 = 120.0;
const TERM_EDITOR_MIN_H: f64 = 160.0;
const TERM_HANDLE_W: f64 = 36.0;
const TERM_HANDLE_H: f64 = 12.0;
const STATUS_H: f64 = 24.0;
const MINIMAP_W: f64 = 56.0;
const CARD_RADIUS: f64 = 6.0;
/// Split clamps: explorer card width and a usable editor minimum.
const SPLIT_MIN_PX: f64 = 224.0;
const SPLIT_MAX_MARGIN: f64 = 280.0;
/// Explorer card right edge (activity card + gap + 240 px explorer).
const SIDEBAR_DEFAULT_PX: f64 = 64.0 + 240.0;
const SIDEBAR_HANDLE_W: f64 = 20.0;
const SIDEBAR_HANDLE_H: f64 = 34.0;
/// Height of the "changed on disk" flash strip (viewer pane top edge).
pub const FLASH_STRIP_H: f64 = 3.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorGroupLayout {
    pub tabs: Rect,
    pub breadcrumb: Rect,
    pub body: Rect,
    pub minimap: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutVisibility {
    pub explorer: bool,
    pub terminal: bool,
}

impl Default for LayoutVisibility {
    fn default() -> Self {
        Self {
            explorer: true,
            terminal: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutAction {
    ToggleExplorer,
    ToggleTerminal,
}

/// Geometry of one frame in LOGICAL px, derived from the window size and
/// the current split position. Pure — tests pin it.
pub struct AppLayout {
    /// Top title/menu/layout-control bar.
    pub title_bar: Rect,
    /// Padded card area between title bar and status bar.
    pub workbench: Rect,
    /// Far-left activity card.
    pub activity_bar: Rect,
    /// Explorer card and its static chrome regions.
    pub explorer_card: Rect,
    pub explorer_header: Rect,
    pub explorer_root: Rect,
    /// Functional tree viewport inside the explorer card.
    pub tree_panel: Rect,
    pub scroll_track: Rect,
    /// Narrow drag hit-area between explorer and editor cards.
    pub split_area: Rect,
    /// Editor card, tab strip, breadcrumbs, body and minimap.
    pub editor_card: Rect,
    #[cfg_attr(not(test), allow(dead_code))]
    pub editor_tabs: Rect,
    #[cfg_attr(not(test), allow(dead_code))]
    pub breadcrumb: Rect,
    #[cfg_attr(not(test), allow(dead_code))]
    pub viewer_pane: Rect,
    #[cfg_attr(not(test), allow(dead_code))]
    pub minimap: Rect,
    /// Per-editor-group chrome/body rects. Index 1 is degenerate when unsplit.
    pub editor_groups: [EditorGroupLayout; 2],
    pub editor_group_count: usize,
    /// Draggable divider strip between the two editor groups.
    pub editor_group_split: Rect,
    /// Bottom terminal card and its chrome/body regions.
    pub term_dock: Rect,
    /// Drag area between editor and terminal; zero when the dock is hidden.
    pub terminal_resize: Rect,
    pub terminal_toolbar: Rect,
    pub term_grid: Rect,
    /// Damage envelope of the status line.
    pub status: Rect,
}

#[allow(dead_code)] // test/layout compatibility surface; the App uses the commit variant
pub fn app_layout(w: f64, h: f64, split_x: f64) -> AppLayout {
    app_layout_with_commit(w, h, split_x, false, false)
}

/// [`app_layout`] with the optional terminal dock and one editor group.
pub fn app_layout_with_commit(
    w: f64,
    h: f64,
    split_x: f64,
    show_commit: bool,
    show_term: bool,
) -> AppLayout {
    app_layout_with_groups(w, h, split_x, show_commit, show_term, 0.5, 1)
}

/// Full workbench layout with one or two side-by-side editor groups.
pub fn app_layout_with_groups(
    w: f64,
    h: f64,
    split_x: f64,
    show_commit: bool,
    show_term: bool,
    group_divider: f64,
    group_count: usize,
) -> AppLayout {
    app_layout_with_groups_and_terminal(
        w,
        h,
        split_x,
        show_commit,
        show_term,
        group_divider,
        group_count,
        None,
    )
}

/// Clamp a user-selected terminal height while preserving a usable editor.
/// Tiny windows shrink the minimum proportionally instead of overflowing.
pub fn clamp_terminal_height(requested: f64, work_h: f64, gap: f64) -> f64 {
    if !requested.is_finite() {
        return 0.0;
    }
    let min = TERM_MIN_H.min((work_h * 0.35).max(0.0));
    let max = (work_h - TERM_EDITOR_MIN_H - gap).max(min);
    requested.clamp(min, max)
}

/// Full workbench layout with an optional user-adjusted terminal height.
#[allow(clippy::too_many_arguments)] // compatibility layout inputs
pub fn app_layout_with_groups_and_terminal(
    w: f64,
    h: f64,
    split_x: f64,
    _show_commit: bool,
    show_term: bool,
    group_divider: f64,
    group_count: usize,
    term_height: Option<f64>,
) -> AppLayout {
    app_layout_with_visibility(
        w,
        h,
        split_x,
        group_divider,
        group_count,
        LayoutVisibility {
            explorer: true,
            terminal: show_term,
        },
        term_height,
    )
}

/// Full workbench layout with explicit panel visibility.
pub fn app_layout_with_visibility(
    w: f64,
    h: f64,
    split_x: f64,
    group_divider: f64,
    group_count: usize,
    visibility: LayoutVisibility,
    term_height: Option<f64>,
) -> AppLayout {
    let w = w.max(0.0);
    let h = h.max(0.0);
    let narrow = w < 560.0;
    let compact = w < 760.0;
    let short = h < 620.0;
    let pad = if narrow { 4.0 } else { PAD };
    let gap = if narrow { 4.0 } else { GAP };
    let activity_w = if narrow { 40.0 } else { ACTIVITY_W };
    let status_y = (h - STATUS_H).max(TITLE_H);
    let work_y = TITLE_H + pad;
    let work_h = (status_y - work_y - pad).max(0.0);

    let activity = Rect {
        x: pad,
        y: work_y,
        w: activity_w,
        h: work_h,
    };
    let explorer_x = pad + activity_w + gap;
    let show_explorer = visibility.explorer && !compact;
    let explorer_w = if show_explorer {
        (split_x - explorer_x).clamp(160.0, (w - explorer_x - gap).max(160.0))
    } else {
        0.0
    };
    let explorer = Rect {
        x: explorer_x,
        y: work_y,
        w: explorer_w,
        h: work_h,
    };
    let explorer_header_h = if show_explorer {
        EXPLORER_HEADER_H
    } else {
        0.0
    };
    let explorer_root_h = if show_explorer { EXPLORER_ROOT_H } else { 0.0 };
    let explorer_header = Rect {
        x: explorer.x,
        y: explorer.y,
        w: explorer.w,
        h: explorer_header_h,
    };
    let explorer_root = Rect {
        x: explorer.x,
        y: explorer.y + explorer_header_h,
        w: explorer.w,
        h: explorer_root_h,
    };
    let tree_panel = Rect {
        x: explorer.x + pad,
        y: explorer_root.y + explorer_root.h,
        w: (explorer.w - 2.0 * pad).max(0.0),
        h: (explorer.y + explorer.h - pad - explorer_root.y - explorer_root.h).max(0.0),
    };
    let scroll_track = Rect {
        x: tree_panel.x + (tree_panel.w - tree::TRACK_W).max(0.0),
        y: tree_panel.y,
        w: if tree_panel.w > 0.0 {
            tree::TRACK_W
        } else {
            0.0
        },
        h: tree_panel.h,
    };

    let editor_x = if show_explorer {
        explorer.x + explorer.w + gap
    } else {
        activity.x + activity.w + gap
    };
    let editor_w = (w - pad - editor_x).max(0.0);
    let dock_h = if visibility.terminal {
        match term_height {
            Some(height) => clamp_terminal_height(height, work_h, gap),
            None => if short || compact {
                140.0_f64.min(work_h * 0.35)
            } else {
                TERM_DOCK_H.min(work_h * 0.40)
            }
            .max(0.0),
        }
    } else {
        0.0
    };
    let editor_card_h = (work_h - dock_h - if dock_h > 0.0 { gap } else { 0.0 }).max(0.0);
    let editor_card = Rect {
        x: editor_x,
        y: work_y,
        w: editor_w,
        h: editor_card_h,
    };
    let editor_tab_h = if narrow { 32.0 } else { EDITOR_TAB_H };
    let breadcrumb_h = if narrow { 20.0 } else { BREADCRUMB_H };
    let editor_tabs = Rect {
        x: editor_card.x,
        y: editor_card.y,
        w: editor_card.w,
        h: editor_tab_h,
    };
    let breadcrumb = Rect {
        x: editor_card.x,
        y: editor_tabs.y + editor_tab_h,
        w: editor_card.w,
        h: breadcrumb_h,
    };
    let minimap_w = if editor_w >= 560.0 {
        MINIMAP_W
    } else {
        0.0
    };
    let viewer_pane = Rect {
        x: editor_card.x + pad,
        y: breadcrumb.y + breadcrumb_h,
        w: (editor_card.w - 2.0 * pad - minimap_w - if minimap_w > 0.0 { gap } else { 0.0 })
            .max(0.0),
        h: (editor_card.h - editor_tab_h - breadcrumb_h - pad).max(0.0),
    };
    let minimap = Rect {
        x: editor_card.x + editor_card.w - pad - minimap_w,
        y: viewer_pane.y,
        w: minimap_w,
        h: viewer_pane.h,
    };

    let empty = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    let mut editor_groups = [
        EditorGroupLayout {
            tabs: editor_tabs,
            breadcrumb,
            body: viewer_pane,
            minimap,
        },
        EditorGroupLayout {
            tabs: empty,
            breadcrumb: empty,
            body: empty,
            minimap: empty,
        },
    ];
    let mut editor_group_split = Rect {
        x: editor_card.x,
        y: editor_card.y,
        w: 0.0,
        h: editor_card.h,
    };
    if group_count >= 2 && editor_card.w > 0.0 {
        let usable = (editor_card.w - gap).max(0.0);
        let min_w = 180.0_f64.min(usable / 2.0);
        let min_ratio = if usable > 0.0 { min_w / usable } else { 0.0 };
        let divider = if group_divider.is_finite() {
            group_divider.clamp(min_ratio, 1.0 - min_ratio)
        } else {
            0.5
        };
        let left_w = (usable * divider).floor();
        let right_w = usable - left_w;
        let left = Rect {
            x: editor_card.x,
            y: editor_card.y,
            w: left_w,
            h: editor_card.h,
        };
        let right = Rect {
            x: editor_card.x + left_w + gap,
            y: editor_card.y,
            w: right_w,
            h: editor_card.h,
        };
        editor_groups = [
            editor_group_layout(left, editor_tab_h, breadcrumb_h, pad, gap),
            editor_group_layout(right, editor_tab_h, breadcrumb_h, pad, gap),
        ];
        editor_group_split = Rect {
            x: editor_card.x + left_w,
            y: editor_card.y,
            w: gap,
            h: editor_card.h,
        };
    }
    let editor_tabs = editor_groups[0].tabs;
    let breadcrumb = editor_groups[0].breadcrumb;
    let viewer_pane = editor_groups[0].body;
    let minimap = editor_groups[0].minimap;
    let editor_group_count = if group_count >= 2 { 2 } else { 1 };

    let terminal = Rect {
        x: editor_card.x,
        y: editor_card.y + editor_card.h + if dock_h > 0.0 { gap } else { 0.0 },
        w: editor_card.w,
        h: dock_h,
    };
    let terminal_resize = if dock_h > 0.0 {
        Rect {
            x: terminal.x,
            y: editor_card.y + editor_card.h,
            w: terminal.w,
            h: gap,
        }
    } else {
        empty
    };
    let terminal_toolbar_h = if short { 30.0 } else { EDITOR_TAB_H };
    let terminal_toolbar = Rect {
        x: terminal.x,
        y: terminal.y,
        w: terminal.w,
        h: terminal_toolbar_h,
    };
    let term_grid = Rect {
        x: terminal.x + pad,
        y: terminal_toolbar.y + terminal_toolbar.h,
        w: (terminal.w - 2.0 * pad).max(0.0),
        h: (terminal.h - terminal_toolbar.h - pad).max(0.0),
    };

    AppLayout {
        title_bar: Rect {
            x: 0.0,
            y: 0.0,
            w,
            h: TITLE_H,
        },
        workbench: Rect {
            x: pad,
            y: work_y,
            w: (w - 2.0 * pad).max(0.0),
            h: work_h,
        },
        activity_bar: activity,
        explorer_card: explorer,
        explorer_header,
        explorer_root,
        tree_panel,
        scroll_track,
        split_area: Rect {
            x: explorer.x + explorer.w,
            y: work_y,
            w: if show_explorer { gap } else { 0.0 },
            h: work_h,
        },
        editor_card,
        editor_tabs,
        breadcrumb,
        viewer_pane,
        minimap,
        editor_groups,
        editor_group_count,
        editor_group_split,
        term_dock: terminal,
        terminal_resize,
        terminal_toolbar,
        term_grid,
        status: Rect {
            x: 0.0,
            y: status_y,
            w,
            h: STATUS_H,
        },
    }
}

fn editor_group_layout(
    card: Rect,
    tab_h: f64,
    breadcrumb_h: f64,
    pad: f64,
    gap: f64,
) -> EditorGroupLayout {
    let tabs = Rect {
        x: card.x,
        y: card.y,
        w: card.w,
        h: tab_h,
    };
    let breadcrumb = Rect {
        x: card.x,
        y: tabs.y + tab_h,
        w: card.w,
        h: breadcrumb_h,
    };
    let minimap_w = if card.w >= 480.0 {
        MINIMAP_W
    } else {
        0.0
    };
    let body = Rect {
        x: card.x + pad,
        y: breadcrumb.y + breadcrumb_h,
        w: (card.w - 2.0 * pad - minimap_w - if minimap_w > 0.0 { gap } else { 0.0 }).max(0.0),
        h: (card.h - tab_h - breadcrumb_h - pad).max(0.0),
    };
    let minimap = Rect {
        x: card.x + card.w - pad - minimap_w,
        y: body.y,
        w: minimap_w,
        h: body.h,
    };
    EditorGroupLayout {
        tabs,
        breadcrumb,
        body,
        minimap,
    }
}

/// Body rect for the Source Control card. Wide windows use the left card;
/// compact windows use the editor body so the feature remains reachable.
#[cfg(test)]
pub fn source_control_panel(layout: &AppLayout) -> Option<Rect> {
    source_control_panel_for_group(layout, 0)
}

/// Source Control body rect for a specific active editor group.
pub fn source_control_panel_for_group(layout: &AppLayout, active_group: usize) -> Option<Rect> {
    if layout.explorer_card.w > 0.0 && layout.explorer_header.h > 0.0 {
        Some(Rect {
            x: layout.explorer_card.x + 1.0,
            y: layout.explorer_header.y + layout.explorer_header.h,
            w: (layout.explorer_card.w - 2.0).max(0.0),
            h: (layout.explorer_card.h - layout.explorer_header.h - 1.0).max(0.0),
        })
    } else {
        layout
            .editor_groups
            .get(active_group)
            .map(|group| group.body)
            .filter(|rect| rect.w > 0.0 && rect.h > 0.0)
    }
}

/// Floating drag handle centered on the editor/terminal divider.
pub fn terminal_resize_handle(layout: &AppLayout) -> Option<Rect> {
    let area = layout.terminal_resize;
    if area.w <= 0.0 || area.h <= 0.0 {
        return None;
    }
    Some(Rect {
        x: area.x + (area.w - TERM_HANDLE_W) / 2.0,
        y: area.y + (area.h - TERM_HANDLE_H) / 2.0,
        w: TERM_HANDLE_W,
        h: TERM_HANDLE_H,
    })
}

/// Three-dot resize handle centered on the visible split divider.
fn sidebar_handle(layout: &AppLayout, split_x: f64) -> Option<Rect> {
    if layout.split_area.w <= 0.0 {
        return None;
    }
    Some(Rect {
        x: split_x + layout.split_area.w / 2.0 - SIDEBAR_HANDLE_W / 2.0,
        y: layout.workbench.y + (layout.workbench.h - SIDEBAR_HANDLE_H) / 2.0,
        w: SIDEBAR_HANDLE_W,
        h: SIDEBAR_HANDLE_H,
    })
}

fn mix_rgb(a: ColorRgb, b: ColorRgb, t: f64) -> ColorRgb {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t).round() as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t).round() as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t).round() as u8,
    )
}

fn draw_sidebar_handle(f: &mut Frame, rect: Rect, dragging: bool) {
    let hovered = rect.contains(f.pointer.x, f.pointer.y);
    // Floating pill keeps the dots legible over both adjacent cards.
    f.canvas.fill_round_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        rect.w / 2.0,
        f.theme.border_subtle,
        f.scale,
    );
    f.canvas.fill_round_rect(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        (rect.w - 2.0) / 2.0,
        f.theme.surface_chrome,
        f.scale,
    );
    let pulse = f.activity_pulse.clamp(0.0, 1.0) as f64;
    let emphasis = if dragging || hovered { 1.0 } else { pulse };
    let wave = 0.5 + 0.5 * (f.animation_time * 10.0).sin();
    let dot = 3.0 + emphasis * (0.5 + wave * 0.7);
    let gap = 4.0;
    let total = dot * 3.0 + gap * 2.0;
    let x = rect.x + (rect.w - dot) / 2.0;
    let y = rect.y + (rect.h - total) / 2.0;
    let color = if dragging || hovered {
        f.theme.accent
    } else {
        mix_rgb(
            f.theme.fg_dim,
            f.theme.accent,
            emphasis * (0.35 + 0.65 * wave),
        )
    };
    for index in 0..3 {
        f.canvas.fill_round_rect(
            x,
            y + index as f64 * (dot + gap),
            dot,
            dot,
            dot / 2.0,
            color,
            f.scale,
        );
    }
}

/// Workbench activity panels (M12). `Copy` state only — no per-panel data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityPanel {
    Explorer,
    SourceControl,
    Terminal,
    Config,
}

/// File-tab rects across the editor tab strip. Tabs are bounded by the LRU
/// cache (plus at most one config editor), so the strip never grows without
/// bound.
fn file_tab_rects(pane: Rect, count: usize) -> Vec<Rect> {
    if count == 0 || pane.w <= 0.0 {
        return Vec::new();
    }
    // Keep every drawn tab inside the strip. The cache is bounded, but a
    // narrow window can still show fewer tabs than the cache holds.
    let visible = count.min(((pane.w / 48.0).floor() as usize).max(1));
    let tab_w = (pane.w / visible as f64)
        .floor()
        .clamp(36.0, 160.0)
        .min(pane.w);
    (0..visible)
        .map(|i| Rect {
            x: pane.x + i as f64 * tab_w,
            y: pane.y,
            w: tab_w,
            h: pane.h,
        })
        .collect()
}

/// Draw a root-relative path breadcrumb for one editor group.
fn draw_path_breadcrumb(f: &mut Frame, pane: Rect, path: &Path) {
    if pane.w <= 0.0 || pane.h <= 0.0 {
        return;
    }
    f.canvas
        .fill_rect(pane.x, pane.y, pane.w, pane.h, f.theme.bg, f.scale);
    let rel = path.strip_prefix(f.root).unwrap_or(path);
    let font_size = (11.0 * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, font_size * 1.35);
    let advance = f64::from(m.advance) / f.scale;
    let mut x = pane.x + 12.0;
    let baseline = (pane.y + pane.h / 2.0) * f.scale + f64::from(m.ascent - m.descent) / 2.0;
    let parts: Vec<_> = rel.components().collect();
    let total: f64 = parts
        .iter()
        .map(|part| part.as_os_str().to_string_lossy().chars().count() as f64 * advance + 8.0)
        .sum::<f64>()
        + parts.len().saturating_sub(1) as f64 * (advance + 8.0);
    let available = (pane.w - 24.0).max(0.0);
    if total > available {
        // Narrow windows keep the file name visible and drop parent crumbs.
        if parts.len() > 1 && available > advance {
            f.text.draw_line(
                f.canvas,
                "…",
                (x * f.scale) as f32,
                baseline as f32,
                TextStyle {
                    color: f.theme.fg_dim,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
            x += advance + 8.0;
        }
        let Some(part) = parts.last() else { return };
        let name = part.as_os_str().to_string_lossy();
        let remaining = (pane.x + pane.w - 12.0 - x).max(0.0);
        let max_chars = (remaining / advance).floor().max(0.0) as usize;
        let label: String = name.chars().take(max_chars).collect();
        f.text.draw_line(
            f.canvas,
            &label,
            (x * f.scale) as f32,
            baseline as f32,
            TextStyle {
                color: f.theme.fg,
                font_size,
                weight: Weight::NORMAL,
            },
        );
        return;
    }
    for (i, part) in parts.iter().enumerate() {
        let text = part.as_os_str().to_string_lossy();
        let color = if i + 1 == parts.len() {
            f.theme.fg
        } else {
            f.theme.fg_dim
        };
        f.text.draw_line(
            f.canvas,
            &text,
            (x * f.scale) as f32,
            baseline as f32,
            TextStyle {
                color,
                font_size,
                weight: Weight::NORMAL,
            },
        );
        x += advance * text.chars().count() as f64 + 8.0;
        if i + 1 < parts.len() {
            f.text.draw_line(
                f.canvas,
                ">",
                (x * f.scale) as f32,
                baseline as f32,
                TextStyle {
                    color: f.theme.fg_dim,
                    font_size,
                    weight: Weight::NORMAL,
                },
            );
            x += advance + 8.0;
        }
    }
}

/// Split-divider drag latch (grab offset + sticky flag), owned by the app
/// and driven by `draw_frame` — the M2 pattern, preserved as-is.
#[derive(Debug, Default)]
pub struct SplitDrag {
    pub dragging: bool,
    pub grab_dx: f64,
}

/// Everything one frame borrows to raster + interact.
pub struct Frame<'a> {
    pub canvas: &'a mut Canvas,
    pub text: &'a mut TextSystem,
    pub theme: &'a Theme,
    pub scale: f64,
    /// Recent input emphasis in `[0,1]` for the sidebar handle pulse.
    pub activity_pulse: f32,
    /// Current title-bar panel visibility.
    pub layout_visibility: LayoutVisibility,
    /// Monotonic animation phase in seconds.
    pub animation_time: f64,
    /// Project dir name for the header (App owns it, precomputed).
    pub title: &'a str,
    /// Project root (breadcrumbs strip it from open paths).
    pub root: &'a Path,
    /// Cached/config file paths in tab order (most-recent first).
    pub open_files: &'a [PathBuf],
    /// The active tab path (viewer file or config file).
    pub active_path: Option<&'a Path>,
    /// Active workbench panel (activity bar highlight).
    pub activity: ActivityPanel,
    /// Editor groups. `None` selects the legacy single-pane test surface.
    pub groups: Option<&'a [EditorGroup]>,
    pub active_group: usize,
    pub group_viewers: [Option<&'a mut Viewer>; 2],
    pub group_flash: [bool; 2],
    pub group_scrollbars: Option<&'a mut [Scrollbar]>,
    pub group_split: Option<&'a mut f64>,
    pub group_split_drag: Option<&'a mut SplitDrag>,
    pub pointer: Pointer,
    /// Armed together with `pointer.click`: this click completes a
    /// double-click (the App tracks time+distance).
    pub dbl_click: bool,
    /// The open viewer (right pane), if a file is open and loaded.
    pub viewer: Option<&'a mut Viewer>,
    /// Most recent failed open attempt (rendered in the pane + logged by
    /// the App — never silent).
    pub viewer_error: Option<&'a str>,
    /// "Changed on disk" flash active (the App owns the deadline; ~900 ms).
    pub viewer_flash: bool,
    /// Shared regex highlighter (compiled once at startup).
    pub highlighter: &'a Highlighter,
    /// Git status snapshot (M7): None = not a repo / git unavailable.
    pub git: Option<&'a GitStatus>,
    /// The open file's diff marks (gutter strip).
    pub file_diff: Option<&'a FileDiff>,
    /// Per-file commit/push state, shown in the Source Control card.
    pub commit: Option<&'a mut CommitPanel>,
    /// The open config editor (M9), shown in the right pane instead of the
    /// viewer while the App's pane mode is Config.
    pub config: Option<&'a mut ConfigEditor>,
    /// Most recent config-open failure (rendered instead of the editor).
    pub config_error: Option<&'a str>,
    pub config_error_path: Option<&'a Path>,
    /// Terminal tabs to draw (M6): Some only while the App's pane mode is
    /// Terminals; tabs keep running while hidden.
    pub terms: Option<&'a mut TerminalTabs>,
    /// Terminal pane focused (keys route to the shell; cursor visible).
    pub term_focused: bool,
    /// Explorer New File/New Folder name prompt, when open.
    pub new_item: Option<&'a mut NewItemPrompt>,
}

/// What `draw_frame` changed this frame — the caller marks damage AFTER the
/// raster pass (the changes are already painted; the marks schedule the
/// next present).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameEffects {
    /// Tree offset moved (scrollbar drag or collapse-clamp): panel + status
    /// rect damage only.
    pub scrolled: bool,
    /// Tree state changed (expand/collapse or selection): full damage.
    pub tree_changed: bool,
    /// Viewer offset moved (scrollbar drag): viewer rect damage only.
    pub viewer_scrolled: bool,
    /// A file row was double-clicked: the App opens it through the LRU
    /// cache after the raster pass.
    pub opened: Option<PathBuf>,
    /// The "+" button opened a new terminal tab.
    pub open_terminal: bool,
    /// A terminal tab's close button was clicked.
    pub close_terminal: Option<usize>,
    /// A terminal tab was selected.
    pub terminal_select: Option<usize>,
    /// The terminal profile dropdown arrow was clicked.
    pub terminal_menu_toggle: bool,
    /// A terminal profile menu action was selected.
    pub terminal_menu_action: Option<TerminalMenuAction>,
    /// A click outside the open terminal menu dismissed it.
    pub terminal_menu_dismiss: bool,
    /// A Source Control file row toggled its checkbox.
    pub commit_toggled: bool,
    /// The Commit button was clicked.
    pub commit_clicked: bool,
    /// The Push button was clicked.
    pub push_clicked: bool,
    /// Explorer root New File action clicked.
    pub new_file: bool,
    /// Explorer root New Folder action clicked.
    pub new_folder: bool,
    /// Explorer root Refresh action clicked.
    pub refresh_tree: bool,
    /// Explorer root Collapse All action clicked.
    pub collapse_tree: bool,
    /// Config editor header Save/Reload clicked.
    pub config_save: bool,
    pub config_reload: bool,
    /// The config body changed (env add/remove/apply).
    pub config_changed: bool,
    /// An activity-bar icon was clicked (M12).
    pub set_activity: Option<ActivityPanel>,
    /// A file/config tab was clicked.
    pub select_tab: Option<(usize, PathBuf)>,
    /// A file/config tab close button was clicked.
    pub close_tab: Option<(usize, PathBuf)>,
    /// The secondary editor group's close action was clicked.
    pub close_group: Option<usize>,
    /// An editor group body/tab was clicked.
    pub focus_group: Option<usize>,
    /// The Split editor action was clicked.
    pub toggle_split: bool,
    /// The editor-group divider moved.
    pub group_split_moved: bool,
    /// A title-bar layout action was selected.
    pub layout_action: Option<LayoutAction>,
}

/// Draw the existing per-file Commit & Push panel in the active Source
/// Control card. The panel owns its bounded selection/message state; the frame
/// only supplies the current responsive rect and git snapshot.
fn draw_source_control(f: &mut Frame, layout: &AppLayout) -> commit::PanelEffects {
    let effects = commit::PanelEffects::default();
    let Some(rect) = source_control_panel_for_group(layout, f.active_group) else {
        return effects;
    };
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return effects;
    }
    f.canvas
        .fill_rect(rect.x, rect.y, rect.w, rect.h, f.theme.surface, f.scale);
    let Some(status) = f.git else {
        draw_pane_message(
            f,
            Rect {
                x: rect.x + 8.0,
                y: rect.y + 8.0,
                w: (rect.w - 16.0).max(0.0),
                h: (rect.h - 16.0).max(0.0),
            },
            "Source Control —",
            "git status unavailable",
        );
        return effects;
    };
    let Some(panel) = f.commit.as_deref_mut() else {
        return effects;
    };
    panel.draw(f.canvas, f.text, f.theme, f.scale, rect, status, f.pointer)
}

/// Compose and raster one app frame (M3 layout + M4 viewer pane). State
/// transitions happen here, right before the widgets draw (the M2 pattern,
/// preserved).
pub fn draw_frame(
    f: &mut Frame,
    layout: &AppLayout,
    tree: &mut FileTree,
    split: &mut Split,
    split_drag: &mut SplitDrag,
    scrollbar: &mut Scrollbar,
    _viewer_scrollbar: &mut Scrollbar,
) -> FrameEffects {
    let mut effects = FrameEffects::default();
    if !f.scale.is_finite() || f.scale <= 0.0 {
        return effects;
    }
    let lw = f.canvas.width() as f64 / f.scale;

    // Production frames supply editor groups. The legacy fields remain as a
    // single-group adapter for existing frame tests and callers.
    let legacy_groups;
    let groups: &[EditorGroup] = if let Some(groups) = f.groups {
        groups
    } else {
        legacy_groups = [EditorGroup {
            tabs: f.open_files.to_vec(),
            active: f.active_path.map(Path::to_path_buf),
            error: f.viewer_error.map(str::to_owned),
        }];
        &legacy_groups
    };
    let mut group_viewers = std::mem::take(&mut f.group_viewers);
    let mut group_scrollbars_state = std::mem::take(&mut f.group_scrollbars);
    let mut group_split = std::mem::take(&mut f.group_split);
    let mut group_split_drag = std::mem::take(&mut f.group_split_drag);
    let mut legacy_viewers = [None, None];
    let viewers: &mut [Option<&mut Viewer>] = if f.groups.is_some() {
        &mut group_viewers
    } else {
        legacy_viewers[0] = f.viewer.take();
        &mut legacy_viewers
    };
    let mut fallback_scrollbars = [Scrollbar::default(), Scrollbar::default()];
    let group_scrollbars: &mut [Scrollbar] = if f.groups.is_some() {
        group_scrollbars_state
            .as_deref_mut()
            .unwrap_or(&mut fallback_scrollbars)
    } else {
        std::slice::from_mut(_viewer_scrollbar)
    };

    f.canvas.fill(f.theme.surface_chrome);
    chrome::draw_cards(f, layout);
    let title_fx = chrome::draw_title_bar(f, layout);

    // Sidebar drag: the narrow card gap and the three-dot handle share the
    // same sticky latch as the old divider. Both disappear with Explorer.
    let handle = sidebar_handle(layout, split.position);
    let handle_hover = handle.is_some_and(|rect| rect.contains(f.pointer.x, f.pointer.y));
    let gap_hover = layout.split_area.contains(f.pointer.x, f.pointer.y);
    if handle.is_none() {
        split_drag.dragging = false;
    }
    if f.pointer.down && (split_drag.dragging || gap_hover || handle_hover) {
        if !split_drag.dragging {
            split_drag.grab_dx = f.pointer.x - split.position;
        }
        split_drag.dragging = true;
        let max = (lw - SPLIT_MAX_MARGIN).max(SPLIT_MIN_PX);
        split.drag_to(f.pointer.x - split_drag.grab_dx, SPLIT_MIN_PX, max);
    } else if !f.pointer.down {
        split_drag.dragging = false;
    }
    if split_drag.dragging || gap_hover || handle_hover {
        f.canvas.fill_rect(
            layout.split_area.x + layout.split_area.w / 2.0 - 1.0,
            layout.workbench.y,
            2.0,
            layout.workbench.h,
            f.theme.accent,
            f.scale,
        );
    }
    if f.dbl_click && (gap_hover || handle_hover) {
        let hi = (lw - SPLIT_MAX_MARGIN).max(SPLIT_MIN_PX);
        split.drag_to(SIDEBAR_DEFAULT_PX, SPLIT_MIN_PX, hi);
    }
    if let Some(handle) = handle {
        draw_sidebar_handle(f, handle, split_drag.dragging);
    }

    // Editor-group divider: independent from the Explorer resize handle.
    if layout.editor_group_count == 2 && layout.editor_group_split.w > 0.0 {
        if let (Some(divider), Some(drag)) =
            (group_split.as_deref_mut(), group_split_drag.as_deref_mut())
        {
            let area = layout.editor_group_split;
            let hovered = area.contains(f.pointer.x, f.pointer.y);
            let usable = (layout.editor_card.w - area.w).max(0.0);
            let min_w = 180.0_f64.min(usable / 2.0);
            let min_ratio = if usable > 0.0 { min_w / usable } else { 0.0 };
            if f.pointer.down && (drag.dragging || hovered) {
                if !drag.dragging {
                    let divider_x = layout.editor_card.x + usable * *divider;
                    drag.grab_dx = f.pointer.x - divider_x;
                }
                drag.dragging = true;
                let next = (f.pointer.x - drag.grab_dx - layout.editor_card.x) / usable;
                let clamped = next.clamp(min_ratio, 1.0 - min_ratio);
                if (*divider - clamped).abs() > f64::EPSILON {
                    *divider = clamped;
                    effects.group_split_moved = true;
                }
            } else if !f.pointer.down {
                drag.dragging = false;
            }
            if drag.dragging || hovered {
                f.canvas
                    .fill_rect(area.x, area.y, area.w, area.h, f.theme.accent, f.scale);
            }
            if f.dbl_click && hovered {
                *divider = 0.5;
                effects.group_split_moved = true;
            }
        }
    }

    if f.activity == ActivityPanel::SourceControl {
        let _ = chrome::draw_explorer_chrome(f, layout);
        let fx = draw_source_control(f, layout);
        effects.commit_toggled = fx.toggled;
        effects.commit_clicked = fx.commit;
        effects.push_clicked = fx.push;
    } else {
        let fx = chrome::draw_explorer_chrome(f, layout);
        effects.new_file = fx.new_file;
        effects.new_folder = fx.new_folder;
        effects.refresh_tree = fx.refresh;
        effects.collapse_tree = fx.collapse;

        // Tree panel: the inline name field blocks clicks/hover underneath it
        // and inserts one virtual row at the target folder's child position.
        let new_item_row = f
            .new_item
            .as_ref()
            .and_then(|prompt| chrome::new_item_row(tree, &prompt.parent))
            .map(|(index, _)| index);
        let extra_rows = if new_item_row.is_some() {
            f.new_item
                .as_ref()
                .map_or(0, |prompt| 1 + usize::from(prompt.error.is_some()))
        } else {
            0
        };
        let tree_pointer = if f.new_item.is_some() {
            Pointer::default()
        } else {
            f.pointer
        };
        match tree::handle_pointer(tree, layout.tree_panel, tree_pointer, f.dbl_click) {
            tree::TreeAction::None => {}
            tree::TreeAction::Toggled | tree::TreeAction::Selected => effects.tree_changed = true,
            tree::TreeAction::Opened(path) => effects.opened = Some(path),
        }
        if tree.set_offset_with_extra(tree.offset, layout.tree_panel.h, extra_rows)
            && !effects.tree_changed
        {
            effects.scrolled = true;
        }
        tree::draw_tree_panel_with_insertion(
            f.canvas,
            f.text,
            f.theme,
            f.scale,
            layout.tree_panel,
            tree,
            tree_pointer,
            f.git,
            new_item_row,
        );
        if layout.tree_panel.w > tree::TRACK_W {
            let viewport = layout.tree_panel.h;
            let content = (tree.visible_count() + extra_rows) as f64 * ROW_H;
            scrollbar.offset = tree.offset;
            if scrollbar.on_pointer(layout.scroll_track, tree_pointer, content, viewport) {
                tree.set_offset(scrollbar.offset, viewport);
                effects.scrolled = true;
            }
            scrollbar.draw(
                f.canvas,
                layout.scroll_track,
                content,
                viewport,
                f.theme,
                f.scale,
            );
        }
    }

    if let Some(panel) = chrome::draw_activity_bar(f, layout) {
        effects.set_activity = Some(panel);
    }

    // Per-group tabs, breadcrumbs, actions and minimaps.
    let active_group = f.active_group.min(groups.len().saturating_sub(1));
    for index in 0..layout.editor_group_count.min(groups.len()) {
        let gx = chrome::draw_editor_group_chrome(
            f,
            layout.editor_groups[index],
            &groups[index],
            index,
            active_group,
            layout.editor_group_count == 2,
            f.activity == ActivityPanel::SourceControl && layout.explorer_card.w <= 0.0,
        );
        if let Some(path) = gx.select {
            effects.select_tab = Some((index, path));
            effects.focus_group = Some(index);
        }
        if let Some(path) = gx.close {
            effects.close_tab = Some((index, path));
        }
        if gx.close_group {
            effects.close_group = Some(index);
        }
        effects.toggle_split |= gx.toggle_split;
        effects.focus_group = effects.focus_group.or(gx.focus.then_some(index));
    }

    // Terminal card: tab strip, profile menu, actions, and the active PTY.
    chrome::draw_terminal_resize_handle(f, layout);
    let mut terminal_fx = chrome::draw_terminal_chrome(f, layout);
    if !f.terms.as_ref().is_some_and(|t| t.is_open()) {
        chrome::draw_terminal_placeholder(f, layout);
    }
    if let Some(terms) = f.terms.as_deref_mut().filter(|t| t.is_open()) {
        let grid = layout.term_grid;
        if let Some(t) = terms.active_mut() {
            let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
            let m = f.text.mono_metrics(font_size, font_size * 1.35);
            let advance = f64::from(m.advance) / f.scale;
            t.resize(term::GridSize::fit(grid.w, grid.h, advance, LINE_H));
            t.draw(f.canvas, f.text, f.theme, f.scale, grid, f.term_focused);
        }
    }
    chrome::draw_terminal_menu(f, layout, &mut terminal_fx);
    effects.open_terminal = terminal_fx.open;
    effects.close_terminal = terminal_fx.close;
    effects.terminal_select = terminal_fx.select;
    effects.terminal_menu_toggle = terminal_fx.menu_toggle;
    effects.terminal_menu_action = terminal_fx.menu_action;
    effects.terminal_menu_dismiss = terminal_fx.menu_dismiss;

    // Per-group editor bodies. Config is a tab in one group; every other
    // active tab borrows its pinned viewer from the shared cache.
    for index in 0..layout.editor_group_count.min(groups.len()) {
        let pane = layout.editor_groups[index].body;
        let group = &groups[index];
        if pane.w <= 0.0 || pane.h <= 0.0 {
            continue;
        }
        if f.pointer.click && pane.contains(f.pointer.x, f.pointer.y) {
            effects.focus_group = Some(index);
        }
        if f.activity == ActivityPanel::SourceControl
            && layout.explorer_card.w <= 0.0
            && index == active_group
        {
            let fx = draw_source_control(f, layout);
            effects.commit_toggled |= fx.toggled;
            effects.commit_clicked |= fx.commit;
            effects.push_clicked |= fx.push;
            continue;
        }
        let is_config = if f.groups.is_some() {
            f.config
                .as_ref()
                .is_some_and(|config| Some(config.path()) == group.active.as_deref())
        } else {
            f.config.is_some()
        };
        if is_config {
            let config = f.config.as_deref_mut().expect("checked config path");
            let fx = config.draw(
                f.canvas,
                f.text,
                f.theme,
                f.scale,
                pane,
                f.pointer,
                f.highlighter,
            );
            effects.config_save |= fx.save;
            effects.config_reload |= fx.reload;
            effects.config_changed |= fx.changed;
        } else if index == active_group
            && viewers[index].is_none()
            && (group.active.is_none()
                || f.config_error_path
                    .is_some_and(|path| Some(path) == group.active.as_deref()))
        {
            if let Some(err) = f.config_error {
                draw_pane_message(f, pane, "config editor failed —", &truncate_chars(err, 60));
            } else {
                draw_placeholder(f, pane, group.error.as_deref());
            }
        } else if let Some(viewer) = viewers[index].as_deref_mut() {
            let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
            let m = f
                .text
                .mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
            let advance = f64::from(m.advance) / f.scale;
            let gutter_w = viewer::gutter_width(viewer.line_count(), advance);
            let content_w = (pane.x + pane.w - tree::TRACK_W - (pane.x + gutter_w) - 4.0).max(0.0);
            if viewer.sync_horizontal(content_w, advance) {
                effects.viewer_scrolled = true;
            }
            let horizontal_overflow =
                content_w > 0.0 && viewer.max_line_chars() as f64 * advance > content_w;
            let code_h = pane.h
                - if horizontal_overflow {
                    crate::ui::HSCROLL_H
                } else {
                    0.0
                };
            let viewport_lines = (code_h / LINE_H).floor() as usize;
            if viewer.set_offset_lines(viewer.offset_lines(), viewport_lines) {
                effects.viewer_scrolled = true;
            }
            let horizontal_consumed =
                viewer::handle_horizontal_pointer(viewer, pane, f.pointer, content_w, advance);
            if horizontal_consumed {
                effects.viewer_scrolled = true;
            } else if viewer::handle_pointer(viewer, pane, f.pointer, advance) {
                effects.tree_changed = true;
            }
            let diff = if index == active_group {
                f.file_diff
            } else {
                None
            };
            viewer::draw_viewer_panel(
                f.canvas,
                f.text,
                f.theme,
                f.scale,
                pane,
                viewer,
                f.highlighter,
                diff,
            );
            if pane.w > tree::TRACK_W {
                let content = viewer.line_count() as f64 * LINE_H;
                let scroll_track = Rect {
                    x: pane.x + pane.w - tree::TRACK_W,
                    y: pane.y,
                    w: tree::TRACK_W,
                    h: code_h,
                };
                let scrollbar = &mut group_scrollbars[index];
                scrollbar.offset = viewer.offset_lines() as f64 * LINE_H;
                if scrollbar.on_pointer(scroll_track, f.pointer, content, code_h) {
                    let lines = (scrollbar.offset / LINE_H).ceil() as usize;
                    if viewer.set_offset_lines(lines, viewport_lines) {
                        effects.viewer_scrolled = true;
                    }
                }
                scrollbar.draw(f.canvas, scroll_track, content, code_h, f.theme, f.scale);
            }
            if (f.groups.is_some() && f.group_flash[index])
                || (f.groups.is_none() && index == 0 && f.viewer_flash)
            {
                f.canvas.fill_rect(
                    pane.x,
                    pane.y,
                    pane.w,
                    FLASH_STRIP_H,
                    f.theme.accent,
                    f.scale,
                );
            }
        } else {
            draw_placeholder(f, pane, group.error.as_deref());
        }
    }

    if f.groups.is_none() {
        f.viewer = legacy_viewers[0].take();
    }
    f.group_viewers = group_viewers;
    f.group_scrollbars = group_scrollbars_state;
    f.group_split = group_split;
    f.group_split_drag = group_split_drag;

    chrome::draw_new_item_prompt(f, layout, tree);
    effects.layout_action = title_fx.action;

    chrome::draw_status_bar(f, layout);
    effects
}

/// The live status line. Without an open viewer (M3 format):
/// `visible N · expanded M · selected <name|none> · scroll <pct>%`.
/// With one open: `visible N · expanded M · scroll <pct>% · <file> <L>
/// lines[ · truncated] · sel <K chars|none>`. A tree listing failure
/// replaces the whole line (the tree's error surface).
/// Terminal status segment: `term <shell> <k>/<n>[ · exited]` while tabs
/// are open; `None` otherwise.
#[cfg_attr(not(test), allow(dead_code))] // retained status detail for tests/future inspector
fn terms_status(terms: Option<&TerminalTabs>) -> Option<String> {
    let terms = terms?;
    if !terms.is_open() {
        return None;
    }
    let exited = terms.active().map(|t| t.exited()).unwrap_or(false);
    let exit = if exited { " · exited" } else { "" };
    let shell = terms.active().map(|t| t.label()).unwrap_or("?");
    Some(format!(
        "term {shell} {}/{n}{exit}",
        terms.active + 1,
        n = terms.tabs.len()
    ))
}

#[cfg_attr(not(test), allow(dead_code))] // retained status detail for tests/future inspector
fn frame_status(
    tree: &FileTree,
    viewer: Option<&Viewer>,
    config: Option<&ConfigEditor>,
    terms: Option<&TerminalTabs>,
    git: Option<&GitStatus>,
    viewport: f64,
) -> String {
    if let Some(err) = &tree.last_error {
        return err.clone();
    }
    let pct = {
        let max = tree.visible_count() as f64 * ROW_H - viewport;
        if max > 0.0 {
            (tree.offset / max * 100.0).round().clamp(0.0, 100.0)
        } else {
            0.0
        }
    };
    let head = format!(
        "visible {} · expanded {}",
        tree.visible_count(),
        tree.expanded_dir_count()
    );
    // Git tail (all modes): changed-entry count, or the not-a-repo note.
    let git_tail = match git {
        Some(g) => format!(" · git {}", g.changed_count()),
        None => " · no git".to_string(),
    };
    // With terminals open the viewer segment is replaced by the terminal one.
    if terms_status(terms).is_some() {
        return format!(
            "{head} · scroll {pct:.0}% · {}{git_tail}",
            terms_status(terms).expect("checked above")
        );
    }
    if let Some(c) = config {
        let state = if c.is_dirty() { " · modified" } else { "" };
        return format!(
            "{head} · scroll {pct:.0}% · config {}{state}{git_tail}",
            c.display_name()
        );
    }
    match viewer {
        None => {
            let selected = tree
                .selected()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "none".into());
            format!("{head} · selected {selected} · scroll {pct:.0}%{git_tail}")
        }
        Some(v) => {
            let name = v
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let trunc = if v.truncated() { " · truncated" } else { "" };
            let sel = match v.selection_len() {
                0 => "none".to_string(),
                n => format!("{n} chars"),
            };
            format!(
                "{head} · scroll {pct:.0}% · {name} {} lines{trunc} · sel {sel}{git_tail}",
                v.line_count()
            )
        }
    }
}

/// Bordered placeholder with two centered dim lines (or the open-failure
/// message when the last attempt errored).
fn draw_placeholder(f: &mut Frame, rect: Rect, error: Option<&str>) {
    let lines: (String, &str) = match error {
        Some(err) => (
            truncate_chars(err, 44),
            "double-click a text file in the tree",
        ),
        None => (
            "M4 viewer lands here —".to_string(),
            "double-click a file in the tree",
        ),
    };
    draw_pane_message(f, rect, &lines.0, lines.1);
}

/// Bordered pane message used by the placeholder and config-open errors.
fn draw_pane_message(f: &mut Frame, rect: Rect, top: &str, bottom: &str) {
    if rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    f.canvas
        .stroke_rect_strips(rect.x, rect.y, rect.w, rect.h, f.theme.border, f.scale);
    let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
    let m = f
        .text
        .mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let cx = (rect.x + rect.w / 2.0) * f.scale;
    let cy = (rect.y + rect.h / 2.0) * f.scale;
    let style = TextStyle {
        color: f.theme.fg_dim,
        font_size,
        weight: Weight::NORMAL,
    };
    for (dy, line) in [(-0.5, top), (0.5, bottom)] {
        // Monospace advance: width = chars × advance; center horizontally.
        let x = cx - f64::from(m.advance) * line.chars().count() as f64 / 2.0;
        let baseline = cy + dy * f64::from(m.line_height) + f64::from(m.ascent - m.descent) / 2.0;
        f.text
            .draw_line(f.canvas, line, x as f32, baseline as f32, style);
    }
}

/// Char-boundary-safe truncation (byte slicing could split a codepoint).
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// The frame tests live in the module subdirectory (impl + tests together
// would pass the ~400-line guidance).
#[cfg(test)]
mod tests;
