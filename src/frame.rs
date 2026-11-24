//! The app frame: real layout (header, info line, tree|viewer split, status
//! line) and the scene composition. Layout is computed in LOGICAL px from
//! the window size; `draw_frame` rasterizes one frame and reports what
//! changed so the caller can decide damage (scroll → the scrolled panel's
//! rect, everything else → full; see FrameEffects).

use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::commit::CommitPanel;
use crate::configed::ConfigEditor;
use crate::git::{FileDiff, GitStatus};
use crate::highlight::Highlighter;
use crate::render::{Canvas, Theme};
use crate::term::{self, TerminalTabs};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::tree::{self, FileTree, ROW_H};
use crate::ui::{draw_button, Pointer, Press, Rect, Scrollbar, Split, TabBar, TAB_HEIGHT};
use crate::viewer::{self, Viewer, LINE_H};

/// Left margin shared by header, info line, tree panel and status line.
const LEFT_MARGIN: f64 = 8.0;
/// Baselines (logical px from the window top).
const HEADER_BASELINE: f64 = 40.0;
/// Top of the split area (below header + info line).
const CONTENT_TOP: f64 = 88.0;
/// Split clamps: a readable minimum tree width, and a placeholder that
/// never fully disappears on the right.
const SPLIT_MIN_PX: f64 = 160.0;
const SPLIT_MAX_MARGIN: f64 = 220.0;
/// Sidebar resize handle (M12 polish): VS Code-style three vertical dots.
const SIDEBAR_DEFAULT_PX: f64 = 280.0;
const SIDEBAR_HANDLE_W: f64 = 16.0;
const SIDEBAR_HANDLE_H: f64 = 24.0;
const SIDEBAR_HANDLE_Y: f64 = CONTENT_TOP - SIDEBAR_HANDLE_H - 4.0;
/// Height of the "changed on disk" flash strip (viewer pane top edge).
pub const FLASH_STRIP_H: f64 = 3.0;
/// Workbench chrome (M12): activity bar, editor tabs, breadcrumb, dock.
const ACTIVITY_W: f64 = 48.0;
const EDITOR_TAB_H: f64 = 30.0;
const BREADCRUMB_H: f64 = 22.0;
const TERM_DOCK_H: f64 = 220.0;
const STATUS_H: f64 = 24.0;
/// Geometry of one frame in LOGICAL px, derived from the window size and
/// the current split position. Pure — tests pin it.
pub struct AppLayout {
    /// Far-left activity strip (Explorer / Source Control / Terminal / Config).
    pub activity_bar: Rect,
    pub tree_panel: Rect,
    pub scroll_track: Rect,
    /// Commit & push panel (M8) under the tree; zero-height when hidden.
    pub commit_panel: Rect,
    pub split_area: Rect,
    /// File/config tab strip above the editor body.
    pub editor_tabs: Rect,
    /// Path breadcrumb strip under the file tabs.
    pub breadcrumb: Rect,
    /// The editor body: read-only viewer or config editor.
    pub viewer_pane: Rect,
    /// Bottom terminal dock; zero-height when no terminal tabs are open.
    pub term_dock: Rect,
    /// Terminal tab strip inside the dock.
    pub term_tabbar: Rect,
    /// The terminal grid rect inside the dock, below the tab strip.
    pub term_grid: Rect,
    /// Damage envelope of the status line (its ink sits well inside).
    pub status: Rect,
}

#[allow(dead_code)] // test/layout compatibility surface; the App uses the commit variant
pub fn app_layout(w: f64, h: f64, split_x: f64) -> AppLayout {
    app_layout_with_commit(w, h, split_x, false, false)
}

/// [`app_layout`] with the M8 commit panel shown under the tree and the
/// optional M12 terminal dock at the bottom of the editor column.
pub fn app_layout_with_commit(
    w: f64,
    h: f64,
    split_x: f64,
    show_commit: bool,
    show_term: bool,
) -> AppLayout {
    let status_y = (h - STATUS_H).max(CONTENT_TOP);
    let area_h = (status_y - CONTENT_TOP).max(0.0);
    let area = Rect { x: 0.0, y: CONTENT_TOP, w: w.max(0.0), h: area_h };
    let panel_w = (split_x - ACTIVITY_W - 16.0).max(0.0);
    let commit_h = if show_commit {
        crate::commit::PANEL_H.min(area.h * 0.6).max(0.0)
    } else {
        0.0
    };
    let tree_h = (area.h - commit_h).max(0.0);
    let dock_h = if show_term { TERM_DOCK_H.min(area.h * 0.4).max(0.0) } else { 0.0 };
    let editor_x = split_x + 16.0;
    let editor_w = (w - split_x - 24.0).max(0.0);
    let tabs_bottom = CONTENT_TOP + EDITOR_TAB_H + BREADCRUMB_H;
    let editor_h = (status_y - tabs_bottom - dock_h).max(0.0);
    let dock_y = status_y - dock_h;
    AppLayout {
        activity_bar: Rect { x: 0.0, y: CONTENT_TOP, w: ACTIVITY_W, h: area.h },
        tree_panel: Rect { x: ACTIVITY_W + LEFT_MARGIN, y: CONTENT_TOP, w: panel_w, h: tree_h },
        scroll_track: Rect {
            x: ACTIVITY_W + LEFT_MARGIN + (panel_w - tree::TRACK_W).max(0.0),
            y: CONTENT_TOP,
            w: tree::TRACK_W,
            h: tree_h,
        },
        commit_panel: Rect {
            x: ACTIVITY_W + LEFT_MARGIN,
            y: CONTENT_TOP + tree_h,
            w: panel_w,
            h: commit_h,
        },
        editor_tabs: Rect { x: editor_x, y: CONTENT_TOP, w: editor_w, h: EDITOR_TAB_H },
        breadcrumb: Rect {
            x: editor_x,
            y: CONTENT_TOP + EDITOR_TAB_H,
            w: editor_w,
            h: BREADCRUMB_H,
        },
        viewer_pane: Rect { x: editor_x, y: tabs_bottom, w: editor_w, h: editor_h },
        term_dock: Rect { x: editor_x, y: dock_y, w: editor_w, h: dock_h },
        term_tabbar: Rect {
            x: editor_x,
            y: dock_y,
            w: (editor_w - 60.0).max(0.0),
            h: TAB_HEIGHT,
        },
        term_grid: Rect {
            x: editor_x,
            y: dock_y + TAB_HEIGHT,
            w: editor_w,
            h: (dock_h - TAB_HEIGHT).max(0.0),
        },
        split_area: area,
        status: Rect { x: 0.0, y: status_y, w: w.max(0.0), h: STATUS_H },
    }
}

/// Three-dot resize handle centered on the split divider in the header gap.
fn sidebar_handle(split_x: f64) -> Rect {
    Rect {
        x: split_x - SIDEBAR_HANDLE_W / 2.0,
        y: SIDEBAR_HANDLE_Y,
        w: SIDEBAR_HANDLE_W,
        h: SIDEBAR_HANDLE_H,
    }
}

fn draw_sidebar_handle(f: &mut Frame, rect: Rect, dragging: bool) {
    let hovered = rect.contains(f.pointer.x, f.pointer.y);
    let color = if dragging || hovered {
        f.theme.accent
    } else {
        f.theme.fg_dim
    };
    let dot = 2.0;
    let gap = 3.0;
    let total = dot * 3.0 + gap * 2.0;
    let x = rect.x + (rect.w - dot) / 2.0;
    let y = rect.y + (rect.h - total) / 2.0;
    for index in 0..3 {
        f.canvas.fill_rect(
            x,
            y + index as f64 * (dot + gap),
            dot,
            dot,
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

impl ActivityPanel {
    pub const ALL: [ActivityPanel; 4] = [
        ActivityPanel::Explorer,
        ActivityPanel::SourceControl,
        ActivityPanel::Terminal,
        ActivityPanel::Config,
    ];

    fn icon(self) -> crate::icons::Icon {
        match self {
            ActivityPanel::Explorer => crate::icons::Icon::File,
            ActivityPanel::SourceControl => crate::icons::Icon::SourceControl,
            ActivityPanel::Terminal => crate::icons::Icon::Terminal,
            ActivityPanel::Config => crate::icons::Icon::Config,
        }
    }
}

/// Activity-bar icon rects (top-aligned, one per panel).
fn activity_rects(bar: Rect) -> Vec<(ActivityPanel, Rect)> {
    ActivityPanel::ALL
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                *p,
                Rect {
                    x: bar.x + 6.0,
                    y: bar.y + 8.0 + i as f64 * 52.0,
                    w: bar.w - 12.0,
                    h: 40.0,
                },
            )
        })
        .collect()
}

/// Draw the far-left activity strip. Count badges ride the existing git and
/// terminal state — no new panel state is introduced.
fn draw_activity_bar(
    f: &mut Frame,
    bar: Rect,
    term_count: usize,
) -> Option<ActivityPanel> {
    if bar.w <= 0.0 || bar.h <= 0.0 {
        return None;
    }
    f.canvas.fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.bg_elevated, f.scale);
    let mut clicked = None;
    for (panel, rect) in activity_rects(bar) {
        let active = f.activity == panel;
        if active {
            f.canvas.fill_rect(bar.x, rect.y, 3.0, rect.h, f.theme.accent, f.scale);
        }
        let color = if active { f.theme.accent } else { f.theme.fg_dim };
        crate::icons::draw(
            f.canvas.pixmap_mut(),
            panel.icon(),
            (rect.x + rect.w / 2.0) as f32,
            (rect.y + rect.h / 2.0 - 4.0) as f32,
            20.0,
            color,
            f.scale,
        );
        let count = match panel {
            ActivityPanel::SourceControl => f.git.map(|g| g.changed_count()).unwrap_or(0),
            ActivityPanel::Terminal => term_count,
            _ => 0,
        };
        if count > 0 {
            let badge = Rect { x: rect.x + rect.w - 18.0, y: rect.y + rect.h - 14.0, w: 16.0, h: 14.0 };
            f.canvas.fill_rect(badge.x, badge.y, badge.w, badge.h, f.theme.accent, f.scale);
            let font = (10.0 * f.scale) as f32;
            let m = f.text.mono_metrics(font, font * 1.35);
            let baseline = (badge.y + badge.h / 2.0) * f.scale
                + f64::from(m.ascent - m.descent) / 2.0;
            f.text.draw_line(
                f.canvas,
                &count.to_string(),
                (badge.x * f.scale) as f32,
                baseline as f32,
                TextStyle { color: f.theme.on_accent, font_size: font, weight: Weight::BOLD },
            );
        }
        if f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
            clicked = Some(panel);
        }
    }
    clicked
}

/// File-tab rects across the editor tab strip. Tabs are bounded by the LRU
/// cache (plus at most one config editor), so the strip never grows without
/// bound.
fn file_tab_rects(pane: Rect, count: usize) -> Vec<Rect> {
    if count == 0 || pane.w <= 0.0 {
        return Vec::new();
    }
    let tab_w = (pane.w / count as f64).floor().clamp(48.0, 160.0);
    (0..count)
        .map(|i| Rect {
            x: pane.x + i as f64 * tab_w,
            y: pane.y,
            w: tab_w,
            h: pane.h,
        })
        .collect()
}

/// Draw the file/config tab strip. Clicks report select/close; no second
/// open-files model exists (labels come from the LRU + config editor).
fn draw_file_tabs(f: &mut Frame, pane: Rect) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut select = None;
    let mut close = None;
    if pane.w <= 0.0 || pane.h <= 0.0 {
        return (select, close);
    }
    f.canvas.fill_rect(pane.x, pane.y, pane.w, pane.h, f.theme.bg_elevated, f.scale);
    let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let advance = f64::from(m.advance) / f.scale;
    let rects = file_tab_rects(pane, f.open_files.len());
    for (rect, path) in rects.iter().zip(f.open_files.iter()) {
        let active = f.active_path == Some(path.as_path());
        if active {
            f.canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, f.theme.bg, f.scale);
            f.canvas.fill_rect(rect.x, rect.y + rect.h - 2.0, rect.w, 2.0, f.theme.accent, f.scale);
        } else if rect.contains(f.pointer.x, f.pointer.y) {
            f.canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, f.theme.selection, f.scale);
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(unset)".into());
        let max_chars = (((rect.w - 44.0) / advance).floor()).max(1.0) as usize;
        let label: String = name.chars().take(max_chars).collect();
        let baseline = (rect.y + rect.h / 2.0) * f.scale
            + f64::from(m.ascent - m.descent) / 2.0;
        f.text.draw_line(
            f.canvas,
            &label,
            ((rect.x + 10.0) * f.scale) as f32,
            baseline as f32,
            TextStyle { color: f.theme.fg, font_size, weight: Weight::NORMAL },
        );
        if let Some(badge) = f.git.and_then(|g| g.badge_for(path)) {
            let (letter, color) = crate::tree::badge_glyph(badge, f.theme);
            f.text.draw_line(
                f.canvas,
                letter,
                ((rect.x + rect.w - 30.0) * f.scale) as f32,
                baseline as f32,
                TextStyle { color, font_size, weight: Weight::BOLD },
            );
        }
        let close_rect = Rect {
            x: rect.x + rect.w - 22.0,
            y: rect.y + (rect.h - 16.0) / 2.0,
            w: 16.0,
            h: 16.0,
        };
        f.text.draw_line(
            f.canvas,
            "\u{d7}",
            ((close_rect.x + 4.0) * f.scale) as f32,
            baseline as f32,
            TextStyle { color: f.theme.fg_dim, font_size, weight: Weight::NORMAL },
        );
        if f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
            if close_rect.contains(f.pointer.x, f.pointer.y) {
                close = Some(path.clone());
            } else {
                select = Some(path.clone());
            }
        }
    }
    // READ-ONLY badge at the strip's right edge (not for the writable config
    // editor tab).
    if f.open_files.is_empty() {
        return (select, close);
    }
    let active_is_config = f
        .config
        .as_ref()
        .is_some_and(|c| f.active_path == Some(c.path()));
    if active_is_config {
        return (select, close);
    }
    let badge_w = 92.0;
    let badge = Rect {
        x: pane.x + pane.w - badge_w - 8.0,
        y: pane.y + 5.0,
        w: badge_w,
        h: pane.h - 10.0,
    };
    if badge.x > pane.x {
        f.canvas.fill_rect(badge.x, badge.y, badge.w, badge.h, f.theme.bg, f.scale);
        f.canvas.stroke_rect_strips(badge.x, badge.y, badge.w, badge.h, f.theme.border, f.scale);
        let baseline = (badge.y + badge.h / 2.0) * f.scale
            + f64::from(m.ascent - m.descent) / 2.0;
        f.text.draw_line(
            f.canvas,
            "READ-ONLY",
            ((badge.x + 12.0) * f.scale) as f32,
            baseline as f32,
            TextStyle { color: f.theme.fg_dim, font_size: (11.0 * f.scale) as f32, weight: Weight::BOLD },
        );
    }
    (select, close)
}

/// Draw the breadcrumb for the active path, relative to the project root.
fn draw_breadcrumb(f: &mut Frame, pane: Rect) {
    if pane.w <= 0.0 || pane.h <= 0.0 {
        return;
    }
    f.canvas.fill_rect(pane.x, pane.y, pane.w, pane.h, f.theme.bg, f.scale);
    let Some(path) = f.active_path else { return };
    let rel = path.strip_prefix(f.root).unwrap_or(path);
    let font_size = (11.0 * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, font_size * 1.35);
    let mut x = pane.x + 12.0;
    let baseline = (pane.y + pane.h / 2.0) * f.scale + f64::from(m.ascent - m.descent) / 2.0;
    let parts: Vec<_> = rel.components().collect();
    for (i, part) in parts.iter().enumerate() {
        let text = part.as_os_str().to_string_lossy();
        let color = if i + 1 == parts.len() { f.theme.fg } else { f.theme.fg_dim };
        f.text.draw_line(
            f.canvas,
            &text,
            (x * f.scale) as f32,
            baseline as f32,
            TextStyle { color, font_size, weight: Weight::NORMAL },
        );
        x += f64::from(m.advance) * text.chars().count() as f64 + 8.0;
        if i + 1 < parts.len() {
            f.text.draw_line(
                f.canvas,
                ">",
                (x * f.scale) as f32,
                baseline as f32,
                TextStyle { color: f.theme.fg_dim, font_size, weight: Weight::NORMAL },
            );
            x += f64::from(m.advance) + 8.0;
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
    /// Commit & push panel state (M8; drawn when `git` is Some).
    pub commit: &'a mut CommitPanel,
    /// The open config editor (M9), shown in the right pane instead of the
    /// viewer while the App's pane mode is Config.
    pub config: Option<&'a mut ConfigEditor>,
    /// Most recent config-open failure (rendered instead of the editor).
    pub config_error: Option<&'a str>,
    /// Terminal tabs to draw (M6): Some only while the App's pane mode is
    /// Terminals; tabs keep running while hidden.
    pub terms: Option<&'a mut TerminalTabs>,
    /// Terminal pane focused (keys route to the shell; cursor visible).
    pub term_focused: bool,
    /// The retained tab-strip widget (App-owned; synced with the tabs).
    pub tabbar: &'a mut TabBar,
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
    /// The "×" button closed the active terminal tab.
    pub close_terminal: bool,
    /// A commit-panel file row toggled (panel repaint).
    pub commit_toggled: bool,
    /// The Commit button was clicked (the App dispatches after the frame).
    pub commit_clicked: bool,
    /// The Push button was clicked.
    pub push_clicked: bool,
    /// Config editor header Save/Reload clicked.
    pub config_save: bool,
    pub config_reload: bool,
    /// The config body changed (env add/remove/apply).
    pub config_changed: bool,
    /// An activity-bar icon was clicked (M12).
    pub set_activity: Option<ActivityPanel>,
    /// A file/config tab was clicked.
    pub select_tab: Option<PathBuf>,
    /// A file/config tab close button was clicked.
    pub close_tab: Option<PathBuf>,
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
    viewer_scrollbar: &mut Scrollbar,
) -> FrameEffects {
    let mut effects = FrameEffects::default();
    if !f.scale.is_finite() || f.scale <= 0.0 {
        return effects;
    }
    let (lw, _lh) = (
        f.canvas.width() as f64 / f.scale,
        f.canvas.height() as f64 / f.scale,
    );
    f.canvas.fill(f.theme.bg);

    // Header. The debug scale/canvas line and agent launchers were removed;
    // the gap below the title is intentionally quiet.
    let header_size = 16.0 * f.scale as f32;
    let status_size = (12.0 * f.scale) as f32;
    f.text.draw_line(
        f.canvas,
        &format!("ark — {}", f.title),
        (LEFT_MARGIN * f.scale) as f32,
        (HEADER_BASELINE * f.scale) as f32,
        TextStyle { color: f.theme.accent, font_size: header_size, weight: Weight::BOLD },
    );

    // Split: grab within ±4px while the pointer is down, drag until release
    // (sticky latch so the divider can be pulled beyond the grab band).
    let handle = sidebar_handle(split.position);
    let handle_hover = handle.contains(f.pointer.x, f.pointer.y);
    if f.pointer.down
        && (split_drag.dragging
            || split.hit_divider(layout.split_area, f.pointer, f.scale)
            || handle_hover)
    {
        if !split_drag.dragging {
            split_drag.grab_dx = f.pointer.x - split.position;
        }
        split_drag.dragging = true;
        let max = (lw - SPLIT_MAX_MARGIN).max(SPLIT_MIN_PX);
        split.drag_to(f.pointer.x - split_drag.grab_dx, SPLIT_MIN_PX, max);
    } else if !f.pointer.down {
        split_drag.dragging = false;
    }
    split.draw(f.canvas, layout.split_area, f.theme, f.scale);
    let split_hover =
        split.hit_divider(layout.split_area, f.pointer, f.scale) || handle_hover;
    if split_drag.dragging || split_hover {
        // Accent overlay while dragging or hovering the grab band.
        f.canvas.fill_rect(
            split.position - 1.0,
            layout.split_area.y,
            2.0,
            layout.split_area.h,
            f.theme.accent,
            f.scale,
        );
    }
    // Double-click the divider or the three-dot handle resets the sidebar.
    if f.dbl_click && split_hover {
        let hi = (lw - SPLIT_MAX_MARGIN).max(SPLIT_MIN_PX);
        split.drag_to(SIDEBAR_DEFAULT_PX, SPLIT_MIN_PX, hi);
    }
    draw_sidebar_handle(f, handle, split_drag.dragging);

    // Tree panel: process the frame's click, then draw the rows.
    match tree::handle_pointer(tree, layout.tree_panel, f.pointer, f.dbl_click) {
        tree::TreeAction::None => {}
        tree::TreeAction::Toggled | tree::TreeAction::Selected => effects.tree_changed = true,
        tree::TreeAction::Opened(path) => effects.opened = Some(path),
    }
    // Collapse-while-scrolled self-heal: re-clamp the offset to the current
    // content (no-op when already in range; a move is scroll damage too).
    if tree.set_offset(tree.offset, layout.tree_panel.h) && !effects.tree_changed {
        effects.scrolled = true;
    }
    tree::draw_tree_panel(f.canvas, f.text, f.theme, f.scale, layout.tree_panel, tree, f.pointer, f.git);

    // Scrollbar (tree panel's right edge): the tree offset is the single
    // source of truth — sync in, interact, apply back.
    if layout.tree_panel.w > tree::TRACK_W {
        let viewport = layout.tree_panel.h;
        let content = tree.visible_count() as f64 * ROW_H;
        scrollbar.offset = tree.offset;
        if scrollbar.on_pointer(layout.scroll_track, f.pointer, content, viewport) {
            tree.set_offset(scrollbar.offset, viewport);
            effects.scrolled = true;
        }
        scrollbar.draw(f.canvas, layout.scroll_track, content, viewport, f.theme, f.scale);
    }

    // Commit & push panel (M8) under the tree when a repo is open.
    if let Some(git) = f.git {
        let panel = f
            .commit
            .draw(f.canvas, f.text, f.theme, f.scale, layout.commit_panel, git, f.pointer);
        effects.commit_toggled |= panel.toggled;
        effects.commit_clicked |= panel.commit;
        effects.push_clicked |= panel.push;
    }

    // Workbench activity bar (M12): four fixed panels, existing state only.
    let term_count = f.terms.as_ref().map(|t| t.tabs.len()).unwrap_or(0);
    if let Some(panel) = draw_activity_bar(f, layout.activity_bar, term_count) {
        effects.set_activity = Some(panel);
    }

    // File/config tab strip + breadcrumb (M12). Tabs come from the LRU order
    // (plus the one config editor), never from a second open-files model.
    let (select_tab, close_tab) = draw_file_tabs(f, layout.editor_tabs);
    if select_tab.is_some() {
        effects.select_tab = select_tab;
    }
    if close_tab.is_some() {
        effects.close_tab = close_tab;
    }
    draw_breadcrumb(f, layout.breadcrumb);

    // Bottom terminal dock (M12): always visible while tabs exist; the
    // editor body above never switches away for a terminal.
    if let Some(terms) = f.terms.as_deref_mut().filter(|t| t.is_open()) {
        let pane = layout.term_dock;
        let tabbar_rect = layout.term_tabbar;
        let plus = Rect { x: pane.x + pane.w - 56.0, y: pane.y + 3.0, w: 26.0, h: 24.0 };
        let close = Rect { x: pane.x + pane.w - 28.0, y: pane.y + 3.0, w: 24.0, h: 24.0 };
        f.canvas.fill_rect(pane.x, pane.y, pane.w, pane.h, f.theme.bg_elevated, f.scale);

        // Sync the retained widget bar with the terminal tabs, process the
        // frame's click, then draw. A tab-list change (open/close, including
        // the M8 git command tabs) carries the model's active index into the
        // widget, which is the per-frame source of truth for clicks.
        sync_term_tabbar(f.tabbar, terms.tabs.iter().map(|t| t.label().to_string()).collect(), terms.active);
        f.tabbar.on_pointer(tabbar_rect, f.pointer);
        terms.active = f.tabbar.active.min(terms.tabs.len().saturating_sub(1));
        f.tabbar.draw(f.canvas, tabbar_rect, f.theme, f.text, f.scale);
        if draw_button(f.canvas, "+", plus, f.theme, f.text, f.pointer, f.scale) == Press::Clicked {
            effects.open_terminal = true;
        }
        if draw_button(f.canvas, "\u{d7}", close, f.theme, f.text, f.pointer, f.scale)
            == Press::Clicked
        {
            effects.close_terminal = true;
        }

        // Grid: resize the model+PTY to the fitted size, draw the active tab.
        let grid = layout.term_grid;
        if let Some(t) = terms.active_mut() {
            let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
            let m = f.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
            let advance = f64::from(m.advance) / f.scale;
            t.resize(term::GridSize::fit(grid.w, grid.h, advance, LINE_H));
            t.draw(f.canvas, f.text, f.theme, f.scale, grid, f.term_focused);
        }
    }

    // Editor body: config editor while its pane is active, else the viewer /
    // placeholder. Terminal tabs no longer replace this surface.
    if let Some(config) = f.config.as_deref_mut() {
        let fx = config.draw(
            f.canvas,
            f.text,
            f.theme,
            f.scale,
            layout.viewer_pane,
            f.pointer,
            f.highlighter,
        );
        effects.config_save |= fx.save;
        effects.config_reload |= fx.reload;
        effects.config_changed |= fx.changed;
    } else if let Some(err) = f.config_error {
        draw_pane_message(
            f,
            layout.viewer_pane,
            "config editor failed —",
            &truncate_chars(err, 60),
        );
    } else if f.viewer.is_some() {
        let viewer = f.viewer.as_deref_mut().expect("checked");
            let pane = layout.viewer_pane;
            let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
            let m = f.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
            let advance = f64::from(m.advance) / f.scale;
            // Offset self-heal: a refresh (or any row-set shrink) may leave
            // the raw offset past the content — re-clamp before hit-testing.
            let viewport_lines = (pane.h / LINE_H).floor() as usize;
            if viewer.set_offset_lines(viewer.offset_lines(), viewport_lines) {
                effects.viewer_scrolled = true;
            }
            // Selection drag (press → move → release) inside the pane.
            if viewer::handle_pointer(viewer, pane, f.pointer, advance) {
                effects.tree_changed = true; // selection rides event-level full damage
            }
            viewer::draw_viewer_panel(
                f.canvas,
                f.text,
                f.theme,
                f.scale,
                pane,
                viewer,
                f.highlighter,
                f.file_diff,
            );
            // Viewer scrollbar: lines ↔ px via LINE_H (single source of
            // truth is the viewer's integer line offset).
            if pane.w > tree::TRACK_W {
                let viewport_lines = (pane.h / LINE_H).floor() as usize;
                let content = viewer.line_count() as f64 * LINE_H;
                viewer_scrollbar.offset = viewer.offset_lines() as f64 * LINE_H;
                if viewer_scrollbar.on_pointer(pane_scroll_track(pane), f.pointer, content, pane.h) {
                    let lines = (viewer_scrollbar.offset / LINE_H).round() as usize;
                    if viewer.set_offset_lines(lines, viewport_lines) {
                        effects.viewer_scrolled = true;
                    }
                }
                viewer_scrollbar.draw(
                    f.canvas,
                    pane_scroll_track(pane),
                    content,
                    pane.h,
                    f.theme,
                    f.scale,
                );
            }
            // "Changed on disk" flash: a 3px accent strip across the pane's
            // top edge while active (bounded two-frame animation: the App
            // schedules one redraw at the deadline to clear it).
            if f.viewer_flash {
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
        draw_placeholder(f, layout.viewer_pane, f.viewer_error);
    }

    // Status bar (M12): accent-filled band with on-accent text.
    f.canvas.fill_rect(
        layout.status.x,
        layout.status.y,
        layout.status.w,
        layout.status.h,
        f.theme.accent,
        f.scale,
    );
    let status_baseline = (layout.status.y + layout.status.h - 8.0) * f.scale;
    f.text.draw_line(
        f.canvas,
        &frame_status(tree, f.viewer.as_deref(), f.config.as_deref(), f.terms.as_deref(), f.git, layout.tree_panel.h),
        (LEFT_MARGIN * f.scale) as f32,
        status_baseline as f32,
        TextStyle { color: f.theme.on_accent, font_size: status_size, weight: Weight::NORMAL },
    );
    effects
}

/// The viewer scrollbar track: the pane's right edge (8px), mirroring the
/// tree panel's scrollbar placement.
fn pane_scroll_track(pane: Rect) -> Rect {
    Rect {
        x: pane.x + (pane.w - tree::TRACK_W).max(0.0),
        y: pane.y,
        w: tree::TRACK_W,
        h: pane.h,
    }
}

/// The live status line. Without an open viewer (M3 format):
/// `visible N · expanded M · selected <name|none> · scroll <pct>%`.
/// With one open: `visible N · expanded M · scroll <pct>% · <file> <L>
/// lines[ · truncated] · sel <K chars|none>`. A tree listing failure
/// replaces the whole line (the tree's error surface).
/// Sync the retained tab bar when the terminal tab list changed (open/close,
/// including M8's git command tabs). Carrying `active` in is essential: the
/// bar's index is the per-frame source of truth for clicks, so a freshly
/// opened tab must win over the bar's previous index.
fn sync_term_tabbar(tabbar: &mut TabBar, labels: Vec<String>, active: usize) {
    if tabbar.tabs != labels {
        tabbar.tabs = labels;
        tabbar.active = active;
    }
}

/// Terminal status segment: `term <shell> <k>/<n>[ · exited]` while tabs
/// are open; `None` otherwise.
fn terms_status(terms: Option<&TerminalTabs>) -> Option<String> {
    let terms = terms?;
    if !terms.is_open() {
        return None;
    }
    let exited = terms.active().map(|t| t.exited()).unwrap_or(false);
    let exit = if exited { " · exited" } else { "" };
    let shell = terms.active().map(|t| t.label()).unwrap_or("?");
    Some(format!("term {shell} {}/{n}{exit}", terms.active + 1, n = terms.tabs.len()))
}

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
        return format!("{head} · scroll {pct:.0}% · {}{git_tail}", terms_status(terms).expect("checked above"));
    }
    if let Some(c) = config {
        let state = if c.is_dirty() { " · modified" } else { "" };
        return format!("{head} · scroll {pct:.0}% · config {}{state}{git_tail}", c.display_name());
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
            let name = v.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let trunc = if v.truncated() { " · truncated" } else { "" };
            let sel = match v.selection_len() {
                0 => "none".to_string(),
                n => format!("{n} chars"),
            };
            format!("{head} · scroll {pct:.0}% · {name} {} lines{trunc} · sel {sel}{git_tail}", v.line_count())
        }
    }
}

/// Bordered placeholder with two centered dim lines (or the open-failure
/// message when the last attempt errored).
fn draw_placeholder(f: &mut Frame, rect: Rect, error: Option<&str>) {
    let lines: (String, &str) = match error {
        Some(err) => (truncate_chars(err, 44), "double-click a text file in the tree"),
        None => ("M4 viewer lands here —".to_string(), "double-click a file in the tree"),
    };
    draw_pane_message(f, rect, &lines.0, lines.1);
}

/// Bordered pane message used by the placeholder and config-open errors.
fn draw_pane_message(f: &mut Frame, rect: Rect, top: &str, bottom: &str) {
    if rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    f.canvas.stroke_rect_strips(rect.x, rect.y, rect.w, rect.h, f.theme.border, f.scale);
    let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let cx = (rect.x + rect.w / 2.0) * f.scale;
    let cy = (rect.y + rect.h / 2.0) * f.scale;
    let style = TextStyle { color: f.theme.fg_dim, font_size, weight: Weight::NORMAL };
    for (dy, line) in [(-0.5, top), (0.5, bottom)] {
        // Monospace advance: width = chars × advance; center horizontally.
        let x = cx - f64::from(m.advance) * line.chars().count() as f64 / 2.0;
        let baseline = cy + dy * f64::from(m.line_height) + f64::from(m.ascent - m.descent) / 2.0;
        f.text.draw_line(f.canvas, line, x as f32, baseline as f32, style);
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
