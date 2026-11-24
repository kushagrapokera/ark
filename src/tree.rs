//! File-tree state model and panel (M3): lazy per-directory listings cached
//! for expanded directories only, depth-first row projection, scroll
//! clamping and selection, plus the panel surface itself — viewport-only
//! row rendering, hit testing and pointer interaction.
//!
//! Memory rules (AGENTS.md guardrail 3, applied to the tree):
//! - listings are cached for EXPANDED directories only; collapsing a
//!   directory frees its cached listing (tested);
//! - `visible_rows` BORROWS the cached listings — the per-frame
//!   `Vec<RowRef>` is bounded by the visible node count, never by content.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use cosmic_text::Weight;

use crate::fs::{read_dir_sorted, DirListing};
use crate::git::{Badge, GitStatus};
use crate::icons::{self, Icon};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::ui::{Pointer, Rect};

/// Logical px per visible row.
pub const ROW_H: f64 = 22.0;
/// Logical px of horizontal indent per depth level.
pub const INDENT_PX: f64 = 16.0;
/// Width of the clickable chevron zone at a row's indent (dirs only).
pub const CHEVRON_ZONE: f64 = 16.0;
/// Nominal icon box edge (chevron and file icons).
pub const ICON_SIZE: f64 = 14.0;
/// Width of the scrollbar track occupying the panel's right edge; rows
/// (backgrounds and hit testing) stay left of it.
pub const TRACK_W: f64 = 8.0;
/// Width of the git-badge letter cell at the row's right edge.
pub const BADGE_W: f64 = 12.0;

/// One visible row: a BORROW of the tree's cached state (no per-frame
/// cloning of listings into rows).
#[derive(Clone, Debug)]
pub struct RowRef<'a> {
    pub path: &'a Path,
    pub name: &'a str,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

pub struct FileTree {
    pub root: PathBuf,
    /// Logical-px scroll offset (kept clamped by `scroll_by`/`set_offset`).
    pub offset: f64,
    /// Most recent listing failure (root at construction, any directory on
    /// toggle) for the status line; `None` while every listing succeeded.
    pub last_error: Option<String>,
    /// Directories currently expanded. Includes the implicitly expanded
    /// root; `expanded_dir_count` reports only user-expanded subdirectories.
    expanded: HashSet<PathBuf>,
    /// Cached listing per expanded directory. Collapsing FREES the entry.
    children: HashMap<PathBuf, DirListing>,
    /// Current selection. Survives collapse — the row highlight simply
    /// vanishes until the selected node is visible again.
    selected: Option<PathBuf>,
}

impl FileTree {
    /// List `root` eagerly; a failure surfaces in `last_error` and yields an
    /// empty tree (no panic).
    pub fn new(root: &Path) -> Self {
        let mut tree = FileTree {
            root: root.to_path_buf(),
            offset: 0.0,
            last_error: None,
            expanded: HashSet::new(),
            children: HashMap::new(),
            selected: None,
        };
        tree.expanded.insert(tree.root.clone()); // the root is implicitly expanded
        match read_dir_sorted(root) {
            Ok(listing) => {
                tree.children.insert(tree.root.clone(), listing);
            }
            Err(e) => {
                log::warn!("root listing failed for {}: {e}", root.display());
                tree.last_error = Some(format!("cannot list {}: {e}", root.display()));
                // Keep an empty root listing so the row walk stays total.
                tree.children.insert(
                    tree.root.clone(),
                    DirListing { entries: Vec::new(), truncated: false },
                );
            }
        }
        tree
    }

    /// Toggle a directory: expand (lazy listing, cached) ↔ collapse (FREES
    /// the cached listing). Returns whether anything changed; files never
    /// expand. A failed listing keeps the dir collapsed and surfaces in
    /// `last_error`.
    pub fn toggle(&mut self, path: &Path, is_dir: bool) -> bool {
        if !is_dir {
            return false;
        }
        if self.expanded.remove(path) {
            self.children.remove(path); // MEMORY RULE: collapse frees the cache
            log::debug!(
                "collapsed {} ({} cached listing{})",
                path.display(),
                self.children.len(),
                if self.children.len() == 1 { "" } else { "s" }
            );
            return true;
        }
        match read_dir_sorted(path) {
            Ok(listing) => {
                self.children.insert(path.to_path_buf(), listing);
                self.expanded.insert(path.to_path_buf());
                self.last_error = None; // every listing succeeded again
                log::debug!("expanded {}", path.display());
                true
            }
            Err(e) => {
                log::warn!("cannot list {}: {e}", path.display());
                self.last_error = Some(format!("cannot list {}: {e}", path.display()));
                false
            }
        }
    }

    #[allow(dead_code)] // frozen-interface surface; consumers arrive with M4 keyboard nav
    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Status-line + memory-test surface: user-expanded directories (root
    /// excluded — it is always implicitly expanded).
    #[allow(dead_code)] // retained for future explorer/status detail
    pub fn expanded_dir_count(&self) -> usize {
        self.expanded.iter().filter(|p| p.as_path() != self.root.as_path()).count()
    }

    /// Collapse every user-expanded directory and free all non-root listings.
    /// Uses in-place `retain` so the action allocates nothing.
    pub fn collapse_all(&mut self) -> bool {
        let changed = self.expanded.len() > 1;
        if !changed {
            return false;
        }
        self.expanded.retain(|path| path == &self.root);
        self.children.retain(|path, _| path == &self.root);
        self.offset = 0.0;
        log::debug!("collapsed all Explorer directories");
        true
    }

    /// Re-list the root and every user-expanded directory (M5 live refresh).
    /// Failed sub-listings collapse that dir (vanished/unreadable — normal
    /// during refresh, not an error surface); a failed ROOT listing keeps
    /// the `last_error` surface and empties the tree. The scroll offset is
    /// re-clamped by the next draw's self-heal.
    pub fn refresh(&mut self) {
        match read_dir_sorted(&self.root) {
            Ok(listing) => {
                self.children.insert(self.root.clone(), listing);
                self.last_error = None;
            }
            Err(e) => {
                log::warn!("root re-list failed for {}: {e}", self.root.display());
                self.last_error = Some(format!("cannot list {}: {e}", self.root.display()));
                self.children.insert(
                    self.root.clone(),
                    DirListing { entries: Vec::new(), truncated: false },
                );
            }
        }
        let expanded: Vec<PathBuf> = self
            .expanded
            .iter()
            .filter(|p| p.as_path() != self.root.as_path())
            .cloned()
            .collect();
        for dir in expanded {
            match read_dir_sorted(&dir) {
                Ok(listing) => {
                    self.children.insert(dir, listing);
                }
                Err(e) => {
                    log::debug!("collapsed vanished dir {}: {e}", dir.display());
                    self.expanded.remove(&dir);
                    self.children.remove(&dir);
                }
            }
        }
    }

    /// Depth-first rows over the expanded set, root children first at
    /// depth 0. Borrows the cached listings — no cloning.
    pub fn visible_rows(&self) -> Vec<RowRef<'_>> {
        let mut rows = Vec::new();
        self.push_rows(&self.root, 0, &mut rows);
        rows
    }

    pub fn visible_count(&self) -> usize {
        self.visible_rows().len()
    }

    /// Recursive row walk over one directory level.
    fn push_rows<'a>(&'a self, dir: &Path, depth: usize, rows: &mut Vec<RowRef<'a>>) {
        let Some(listing) = self.children.get(dir) else {
            return;
        };
        for entry in &listing.entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            rows.push(RowRef {
                path: &entry.path,
                name: &entry.name,
                is_dir: entry.is_dir,
                depth,
                expanded,
            });
            if expanded {
                self.push_rows(&entry.path, depth + 1, rows);
            }
        }
    }

    /// Scroll by `dy` logical px, clamped to [0, max(0, count×ROW_H −
    /// viewport)]; returns whether the offset actually moved.
    pub fn scroll_by(&mut self, dy: f64, viewport: f64) -> bool {
        if !dy.is_finite() {
            return false;
        }
        let before = self.offset;
        self.offset += dy;
        self.clamp_offset(viewport);
        self.offset != before
    }

    /// Set the offset (clamped like `scroll_by`); returns whether it moved.
    pub fn set_offset(&mut self, v: f64, viewport: f64) -> bool {
        if !v.is_finite() {
            return false;
        }
        let before = self.offset;
        self.offset = v;
        self.clamp_offset(viewport);
        self.offset != before
    }

    /// Clamp `offset` into [0, max(0, count×ROW_H − viewport)] — forced 0
    /// when there is no overflow. Mirrors `ui::Scrollbar::clamp` (whose
    /// non-finite discipline the pub offset field needs too).
    fn clamp_offset(&mut self, viewport: f64) {
        let max = self.visible_count() as f64 * ROW_H - viewport;
        if !max.is_finite() || max <= 0.0 {
            self.offset = 0.0;
        } else {
            self.offset = if self.offset.is_finite() {
                self.offset.clamp(0.0, max)
            } else {
                0.0
            };
        }
    }

    /// Select a row. Does NOT expand directories (only `toggle` does); the
    /// selection survives collapse — the highlight vanishes until the node
    /// is visible again.
    pub fn select(&mut self, path: &Path) {
        self.selected = Some(path.to_path_buf());
    }

    #[allow(dead_code)] // frozen-interface surface; M4 keyboard nav (Esc) clears the selection
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Current selection, if any.
    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    /// Direct assertion surface for the cache-eviction memory test
    /// (cfg(test)-gated per the handoff).
    #[cfg(test)]
    fn cached_listing_count(&self) -> usize {
        self.children.len()
    }
}

// ---- Panel surface: hit testing, interaction, viewport-only rendering ----

/// A row under the pointer, with its panel-space geometry.
#[derive(Clone, Debug)]
pub struct RowHit<'a> {
    pub row: RowRef<'a>,
    /// Row's top edge y in LOGICAL px (panel.y + index×ROW_H − offset).
    #[allow(dead_code)] // interaction-contract surface (row_y); M4 hover affordances consume it
    pub y: f64,
    /// Pointer is inside the row's chevron zone (dirs only): a chevron
    /// click toggles, a row-body click selects.
    pub on_chevron: bool,
}

/// Hit-test one pointer position against the visible rows: maps the pointer
/// y through the scroll offset to the visible row index. `None` outside the
/// panel, inside the scrollbar track zone, or past the last row.
pub fn row_at<'a>(panel: Rect, x: f64, y: f64, tree: &'a FileTree) -> Option<RowHit<'a>> {
    if !panel.contains(x, y) {
        return None;
    }
    if x >= panel.x + panel.w - TRACK_W {
        return None; // the scrollbar track never hits a row
    }
    let rel = y - panel.y + tree.offset;
    if !rel.is_finite() || rel < 0.0 {
        return None; // non-finite offset (pub field corruption) hits nothing
    }
    let idx = (rel / ROW_H).floor();
    if !idx.is_finite() {
        return None;
    }
    let rows = tree.visible_rows();
    let row = rows.get(idx as usize)?;
    let row_y = panel.y + idx * ROW_H - tree.offset;
    let indent = row.depth as f64 * INDENT_PX;
    let on_chevron =
        row.is_dir && x >= panel.x + indent && x < panel.x + indent + CHEVRON_ZONE;
    Some(RowHit { row: row.clone(), y: row_y, on_chevron })
}

/// What one pointer frame did to the tree (M2 pattern: state transitions at
/// draw time; the caller decides damage / follow-ups).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeAction {
    None,
    /// Expansion toggled (row set changed → full damage).
    Toggled,
    /// Selection changed (full damage).
    Selected,
    /// A file row was double-clicked: open it in the viewer (M4).
    Opened(PathBuf),
}

/// Pointer interaction for the tree panel (M2 pattern: state transitions
/// happen at draw time, right before the widgets draw). A completed click on
/// a dir row's chevron toggles expansion; a click anywhere else on a row
/// selects it (dirs included — selection never expands); a double-click on a
/// file row opens it in the viewer. Returns what changed.
pub fn handle_pointer(tree: &mut FileTree, panel: Rect, pointer: Pointer, double: bool) -> TreeAction {
    if !pointer.click {
        return TreeAction::None;
    }
    let Some(hit) = row_at(panel, pointer.x, pointer.y, tree) else {
        return TreeAction::None;
    };
    // Copy what the click needs out of the borrowed hit so the tree can
    // mutate again.
    let (path, is_dir, on_chevron) =
        (hit.row.path.to_path_buf(), hit.row.is_dir, hit.on_chevron);
    if on_chevron {
        return if tree.toggle(&path, is_dir) {
            TreeAction::Toggled
        } else {
            TreeAction::None
        };
    }
    if double && !is_dir {
        return TreeAction::Opened(path);
    }
    if tree.selected() == Some(path.as_path()) {
        return TreeAction::None; // re-selecting the same row changes nothing
    }
    tree.select(&path);
    TreeAction::Selected
}

/// Render the visible rows of `tree` into `panel` (LOGICAL px, rastered at
/// `scale`). Viewport-only: only the rows the scroll offset exposes are
/// drawn. Per row: depth-indented chevron (dirs; ChevronDown when expanded)
/// or File icon, name text at indent+20 — text may cross the panel's right
/// edge and is left unclipped (no horizontal scroll in M3). Hovered rows
/// paint `bg_elevated`, selected rows `selection` (selection wins).
#[allow(clippy::too_many_arguments)] // git snapshot joins raster borrows; bundling would obscure the call
pub fn draw_tree_panel(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    panel: Rect,
    tree: &FileTree,
    pointer: Pointer,
    git: Option<&GitStatus>,
) {
    if panel.w <= 0.0 || panel.h <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return; // degenerate panel or scale: nothing to raster
    }
    if !tree.offset.is_finite() {
        return; // corrupted pub offset: render nothing (the next scroll/set
                // repairs it — same discipline as ui::Scrollbar::clamp)
    }
    let rows = tree.visible_rows();
    // Viewport-only: iterate just the rows the offset exposes (+1 covers a
    // fractional offset straddling two rows).
    let first = ((tree.offset / ROW_H).floor().max(0.0)) as usize;
    let band_rows = ((panel.h / ROW_H).ceil() as usize) + 1;
    let hovered = row_at(panel, pointer.x, pointer.y, tree).map(|hit| hit.row.path);
    let selected = tree.selected();
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    let row_w = (panel.w - TRACK_W).max(0.0);
    for (i, row) in rows.iter().enumerate().skip(first).take(band_rows) {
        let row_y = panel.y + i as f64 * ROW_H - tree.offset;
        if row_y + ROW_H <= panel.y || row_y >= panel.y + panel.h {
            continue; // straddler outside the vertical span
        }
        if hovered == Some(row.path) {
            canvas.fill_rect(panel.x, row_y, row_w, ROW_H, theme.bg_elevated, scale);
        }
        if selected == Some(row.path) {
            canvas.fill_rect(panel.x, row_y, row_w, ROW_H, theme.selection, scale);
        }
        let indent = row.depth as f64 * INDENT_PX;
        let cy = (row_y + ROW_H / 2.0) as f32;
        if row.is_dir {
            let icon = if row.expanded { Icon::ChevronDown } else { Icon::ChevronRight };
            icons::draw(
                canvas.pixmap_mut(),
                icon,
                (panel.x + indent + CHEVRON_ZONE / 2.0) as f32,
                cy,
                ICON_SIZE as f32,
                theme.fg,
                scale,
            );
        } else {
            icons::draw(
                canvas.pixmap_mut(),
                Icon::File,
                (panel.x + indent + 2.0 + ICON_SIZE / 2.0) as f32,
                cy,
                ICON_SIZE as f32,
                theme.fg,
                scale,
            );
        }
        // Baseline centers the line in the row; the metrics term is already
        // physical and must NOT be scaled again (the M2 fix-round-2 lesson).
        let baseline = (row_y + ROW_H / 2.0) * scale + f64::from(m.ascent - m.descent) / 2.0;
        let badge = git.and_then(|g| g.badge_for(row.path));
        let text_x = panel.x + indent + 20.0;
        let text_right = panel.x + panel.w - TRACK_W - 4.0
            - if badge.is_some() { BADGE_W + 4.0 } else { 0.0 };
        let advance = f64::from(m.advance) / scale;
        let max_chars = ((text_right - text_x).max(0.0) / advance).floor() as usize;
        let display = elide_name(row.name, max_chars);
        if !display.is_empty() {
            text.draw_line(
                canvas,
                &display,
                (text_x * scale) as f32,
                baseline as f32,
                TextStyle { color: theme.fg, font_size, weight: Weight::NORMAL },
            );
        }

        // Git badge (M/A/D/U letter) at the row's right edge.
        if let Some(b) = badge {
            let (letter, color) = badge_glyph(b, theme);
            let badge_x = panel.x + panel.w - TRACK_W - BADGE_W;
            text.draw_line(
                canvas,
                letter,
                (badge_x * scale) as f32,
                baseline as f32,
                TextStyle { color, font_size, weight: Weight::BOLD },
            );
        }
    }
}

/// Char-boundary-safe row-label truncation. Short names borrow the cached
/// listing directly; only clipped labels allocate for the current frame.
fn elide_name(name: &str, max_chars: usize) -> Cow<'_, str> {
    let count = name.chars().count();
    if count <= max_chars {
        return Cow::Borrowed(name);
    }
    if max_chars == 0 {
        return Cow::Borrowed("");
    }
    let mut out = String::with_capacity(max_chars);
    out.extend(name.chars().take(max_chars - 1));
    out.push('…');
    Cow::Owned(out)
}

/// Badge letter + color per kind (shared with the M8 commit panel).
pub(crate) fn badge_glyph(b: Badge, theme: &Theme) -> (&'static str, crate::render::ColorRgb) {
    match b {
        Badge::Modified => ("M", theme.git_modified),
        Badge::Added => ("A", theme.git_added),
        Badge::Deleted => ("D", theme.git_deleted),
        Badge::Untracked => ("U", theme.git_untracked),
    }
}

// The state tests and the panel tests live in the module's subdirectory:
// impl + all tests together would pass the ~400-line guidance.
#[cfg(test)]
mod tests;
#[cfg(test)]
mod panel_tests;
