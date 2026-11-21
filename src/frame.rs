//! The app frame: real layout (header, info line, tree|viewer split, status
//! line) and the scene composition. Layout is computed in LOGICAL px from
//! the window size; `draw_frame` rasterizes one frame and reports what
//! changed so the caller can decide damage (scroll → the scrolled panel's
//! rect, everything else → full; see FrameEffects).

use std::path::PathBuf;

use cosmic_text::Weight;

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
const INFO_BASELINE: f64 = 64.0;
/// Top of the split area (below header + info line).
const CONTENT_TOP: f64 = 88.0;
/// Gap kept clear above the status line at the window bottom.
const BOTTOM_INSET: f64 = 48.0;
/// Status line baseline sits this far above the window bottom.
const STATUS_BASELINE_INSET: f64 = 28.0;
/// Split clamps: a readable minimum tree width, and a placeholder that
/// never fully disappears on the right.
const SPLIT_MIN_PX: f64 = 160.0;
const SPLIT_MAX_MARGIN: f64 = 220.0;
/// Height of the "changed on disk" flash strip (viewer pane top edge).
pub const FLASH_STRIP_H: f64 = 3.0;

/// Geometry of one frame in LOGICAL px, derived from the window size and
/// the current split position. Pure — tests pin it.
pub struct AppLayout {
    pub tree_panel: Rect,
    pub scroll_track: Rect,
    pub split_area: Rect,
    /// The right pane: terminals (M6) while tabs are open, else the viewer /
    /// placeholder.
    pub viewer_pane: Rect,
    /// The terminal grid rect (inside the right pane, below the tab strip).
    pub term_grid: Rect,
    /// Damage envelope of the status line (its ink sits well inside).
    pub status: Rect,
}

pub fn app_layout(w: f64, h: f64, split_x: f64) -> AppLayout {
    let area = Rect { x: 0.0, y: CONTENT_TOP, w: w.max(0.0), h: (h - BOTTOM_INSET - CONTENT_TOP).max(0.0) };
    let panel_w = (split_x - 16.0).max(0.0);
    AppLayout {
        tree_panel: Rect { x: LEFT_MARGIN, y: CONTENT_TOP, w: panel_w, h: area.h },
        // The 8px scrollbar track occupies the tree panel's right edge.
        scroll_track: Rect {
            x: LEFT_MARGIN + (panel_w - tree::TRACK_W).max(0.0),
            y: CONTENT_TOP,
            w: tree::TRACK_W,
            h: area.h,
        },
        viewer_pane: Rect {
            x: split_x + 16.0,
            y: CONTENT_TOP,
            w: (w - split_x - 24.0).max(0.0),
            h: area.h,
        },
        term_grid: Rect {
            x: split_x + 16.0,
            y: CONTENT_TOP + TAB_HEIGHT + 4.0,
            w: (w - split_x - 24.0).max(0.0),
            h: (area.h - TAB_HEIGHT - 4.0).max(0.0),
        },
        split_area: area,
        status: Rect {
            x: 0.0,
            y: (h - STATUS_BASELINE_INSET - 14.0).max(0.0),
            w: w.max(0.0),
            h: 20.0,
        },
    }
}

/// The terminal grid rect inside the right pane (App-side damage marking).
pub fn term_grid_rect(pane: Rect) -> Rect {
    Rect {
        x: pane.x,
        y: pane.y + TAB_HEIGHT + 4.0,
        w: pane.w,
        h: (pane.h - TAB_HEIGHT - 4.0).max(0.0),
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
    /// Terminal tabs (M6) — the right pane shows them while open.
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
    let (lw, lh) = (
        f.canvas.width() as f64 / f.scale,
        f.canvas.height() as f64 / f.scale,
    );
    f.canvas.fill(f.theme.bg);

    // Header + info line.
    let header_size = 16.0 * f.scale as f32;
    let info_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
    let status_size = (12.0 * f.scale) as f32;
    f.text.draw_line(
        f.canvas,
        &format!("ark — {}", f.title),
        (LEFT_MARGIN * f.scale) as f32,
        (HEADER_BASELINE * f.scale) as f32,
        TextStyle { color: f.theme.accent, font_size: header_size, weight: Weight::BOLD },
    );
    f.text.draw_line(
        f.canvas,
        &format!(
            "scale {:.2}x · canvas {}x{}px · rows {} · expanded {}",
            f.scale,
            f.canvas.width(),
            f.canvas.height(),
            tree.visible_count(),
            tree.expanded_dir_count(),
        ),
        (LEFT_MARGIN * f.scale) as f32,
        (INFO_BASELINE * f.scale) as f32,
        TextStyle { color: f.theme.fg_dim, font_size: info_size, weight: Weight::NORMAL },
    );

    // Split: grab within ±4px while the pointer is down, drag until release
    // (sticky latch so the divider can be pulled beyond the grab band).
    if f.pointer.down
        && (split_drag.dragging || split.hit_divider(layout.split_area, f.pointer, f.scale))
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
    if split_drag.dragging {
        // Accent overlay while dragging (draw painted the resting strip).
        f.canvas.fill_rect(
            split.position - 1.0,
            layout.split_area.y,
            2.0,
            layout.split_area.h,
            f.theme.accent,
            f.scale,
        );
    }

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

    // Right pane: terminal tabs (M6) while open, else the viewer /
    // placeholder.
    if let Some(terms) = f.terms.as_deref_mut().filter(|t| t.is_open()) {
        let pane = layout.viewer_pane;
        let tabbar_rect = Rect { x: pane.x, y: pane.y, w: (pane.w - 60.0).max(0.0), h: TAB_HEIGHT };
        let plus = Rect { x: pane.x + pane.w - 56.0, y: pane.y + 3.0, w: 26.0, h: 24.0 };
        let close = Rect { x: pane.x + pane.w - 28.0, y: pane.y + 3.0, w: 24.0, h: 24.0 };

        // Sync the retained widget bar with the terminal tabs, process the
        // frame's click, then draw.
        let labels: Vec<String> = terms.tabs.iter().map(|t| t.shell_name().to_string()).collect();
        if f.tabbar.tabs != labels {
            f.tabbar.tabs = labels;
        }
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

    // Status line (mono 12px, dim).
    f.text.draw_line(
        f.canvas,
        &frame_status(tree, f.viewer.as_deref(), f.terms.as_deref(), f.git, layout.tree_panel.h),
        (LEFT_MARGIN * f.scale) as f32,
        ((lh - STATUS_BASELINE_INSET) * f.scale) as f32,
        TextStyle { color: f.theme.fg_dim, font_size: status_size, weight: Weight::NORMAL },
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
/// Terminal status segment: `term <shell> <k>/<n>[ · exited]` while tabs
/// are open; `None` otherwise.
fn terms_status(terms: Option<&TerminalTabs>) -> Option<String> {
    let terms = terms?;
    if !terms.is_open() {
        return None;
    }
    let exited = terms.active().map(|t| t.exited()).unwrap_or(false);
    let exit = if exited { " · exited" } else { "" };
    let shell = terms.active().map(|t| t.shell_name()).unwrap_or("?");
    Some(format!("term {shell} {}/{n}{exit}", terms.active + 1, n = terms.tabs.len()))
}

fn frame_status(
    tree: &FileTree,
    viewer: Option<&Viewer>,
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
    if rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    f.canvas.stroke_rect_strips(rect.x, rect.y, rect.w, rect.h, f.theme.border, f.scale);
    let font_size = (UI_FONT_SIZE as f64 * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let cx = (rect.x + rect.w / 2.0) * f.scale;
    let cy = (rect.y + rect.h / 2.0) * f.scale;
    let style = TextStyle { color: f.theme.fg_dim, font_size, weight: Weight::NORMAL };
    let lines: (&str, &str) = match error {
        Some(err) => {
            // Char-boundary truncation (byte slicing could split a codepoint).
            let cut = err.char_indices().nth(44).map(|(i, _)| i).unwrap_or(err.len());
            (&err[..cut], "double-click a text file in the tree")
        }
        None => ("M4 viewer lands here —", "double-click a file in the tree"),
    };
    for (dy, line) in [(-0.5, lines.0), (0.5, lines.1)] {
        // Monospace advance: width = chars × advance; center horizontally.
        let x = cx - f64::from(m.advance) * line.chars().count() as f64 / 2.0;
        let baseline = cy + dy * f64::from(m.line_height) + f64::from(m.ascent - m.descent) / 2.0;
        f.text.draw_line(f.canvas, line, x as f32, baseline as f32, style);
    }
}

// The frame tests live in the module subdirectory (impl + tests together
// would pass the ~400-line guidance).
#[cfg(test)]
mod tests;
