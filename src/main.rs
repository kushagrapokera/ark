//! ark — ultra-lightweight agent-first code editor.
//!
//! M3: the lazy virtualized file tree panel in the real app frame. `frame.rs`
//! owns layout + scene composition; `tree.rs` owns the tree state model and
//! its viewport-only panel rendering. `App` routes winit events into the
//! frame: tree scrolls and PTY output use partial/terminal-only damage,
//! divider drags coalesce to one frame, and scene changes repaint fully.
//! The M2 demo scene survives as test-only `demo.rs`. HiDPI: layout in
//! logical px, raster ×scale.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use clap::Parser;
use log::{error, info};
use softbuffer::{Context, Surface};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

mod caret;
mod clipboard;
mod commit;
mod config;
mod configed;
mod editor_groups;
mod frame;
mod fs;
mod fs_create;
mod git;
mod highlight;
mod icons;
mod memory;
mod new_item;
mod render;
mod markdown;
mod search;
mod state;
mod term;
mod text;
mod tree;
mod ui;
mod update_process;
mod update_protocol;
mod viewer;
mod watch;
mod zoom;
/// Test-only M2 demo scene.
#[cfg(test)]
mod demo;

use caret::CaretBlink;
use configed::ConfigEditor;
use editor_groups::EditorGroups;
use frame::{AppLayout, Frame, SplitDrag};
use git::{FileDiff as GitFileDiff, GitStatus};
use highlight::Highlighter;
use new_item::{NewItemKind, NewItemPrompt};
use markdown::MarkdownState;
use render::{Canvas, Damage, DamageRect, Theme};
use state::BufferCache;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::TermMode;
use text::{TextSystem, UI_FONT_SIZE};
use tree::FileTree;
use ui::{Pointer, Rect, Scrollbar, Split};
use viewer::{Viewer, LINE_H};
use watch::{Wake, Watcher};

/// Logical px scrolled per wheel notch.
const WHEEL_PX: f64 = 40.0;
/// Viewer LRU capacity (memory rule: bounded open buffers).
const BUFFER_CACHE_CAP: usize = 8;
/// Double-click window (ms) and click slop (logical px).
const DBL_CLICK_MS: u128 = 400;
const DBL_CLICK_SLOP: f64 = 6.0;
/// Duration of the sidebar-handle emphasis pulse after input.
const HANDLE_PULSE: Duration = Duration::from_millis(800);

fn handle_activity_pulse(now: Instant, last_activity: Instant) -> f32 {
    (1.0 - now.saturating_duration_since(last_activity).as_secs_f32() / HANDLE_PULSE.as_secs_f32())
        .clamp(0.0, 1.0)
}
/// M5 fs pipeline: tree re-lists coalesce to one per debounce window; the
/// flash is a bounded two-frame animation (ON for the deadline, one redraw
/// clears it).
const TREE_REFRESH_DEBOUNCE: Duration = Duration::from_millis(400);
const FLASH_DURATION: Duration = Duration::from_millis(900);
/// Terminal raster floor under output storms (~30fps).
const TERM_FRAME_BUDGET: Duration = Duration::from_millis(33);
/// Keyword search waits for a short typing pause before walking the project.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// Config edits save automatically after a short typing pause.
const CONFIG_AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(750);

/// Which surface owns the right pane. Terminal tabs keep running while
/// hidden; the explicit mode makes config editing (M9) reachable even when a
/// terminal tab exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PaneMode {
    #[default]
    Viewer,
    Config,
}

/// Parsed command line: `ark [PATH]` — the `code .`-style entry point.
#[derive(Parser, Debug)]
#[command(name = "ark", version, about = "ark — ultra-lightweight agent-first code editor")]
struct Cli {
    /// Project directory to open (defaults to the current directory)
    path: Option<PathBuf>,
}

/// Wheel delta → logical px for [`FileTree::scroll_by`]; `None` for unhandled
/// kinds (PixelDelta, e.g. touchpads). winit's `LineDelta` y is positive when
/// content moves DOWN, so the sign flips: wheel down scrolls toward the end
/// at [`WHEEL_PX`] per notch.
fn wheel_scroll_px(delta: &MouseScrollDelta) -> Option<f64> {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => Some(-f64::from(*y) * WHEEL_PX),
        _ => None,
    }
}

/// Logical rect → physical damage rect, clamped to the canvas.
fn damage_rect(rect: Rect, scale: f64, canvas_w: u32, canvas_h: u32) -> DamageRect {
    let x0 = ((rect.x * scale).floor().max(0.0)) as u32;
    let y0 = ((rect.y * scale).floor().max(0.0)) as u32;
    let x1 = (((rect.x + rect.w) * scale).ceil() as u32).min(canvas_w);
    let y1 = (((rect.y + rect.h) * scale).ceil() as u32).min(canvas_h);
    DamageRect {
        x: x0.min(x1),
        y: y0.min(y1),
        width: x1.saturating_sub(x0),
        height: y1.saturating_sub(y0),
    }
}

/// M3 damage contract for tree scrolls (wheel): scroll the tree and mark
/// ONLY the panel rect — header, split, viewer, and window edges are
/// untouched by scrolling. Returns whether anything was damaged.
fn route_wheel_scroll(
    tree: &mut FileTree,
    dy: f64,
    layout: &AppLayout,
    scale: f64,
    canvas: &Canvas,
    damage: &mut Damage,
) -> bool {
    if !tree.scroll_by(dy, layout.tree_panel.h) {
        return false;
    }
    let (cw, ch) = (canvas.width(), canvas.height());
    damage.mark(damage_rect(layout.tree_panel, scale, cw, ch));
    true
}

/// M8 wheel over the commit panel: convert the wheel's pixel delta into
/// whole changed-file rows and scroll the viewport-only list. Returns
/// whether the window moved.
fn route_wheel_commit(
    panel: &mut commit::CommitPanel,
    dy: f64,
    rect: Rect,
    total: usize,
) -> bool {
    let visible = commit::layout(rect).rows;
    let rows = (dy / commit::ROW_H).round() as i64;
    panel.scroll_by(rows, visible, total)
}

/// M6 wheel over the terminal grid: mouse-reporting apps receive encoded
/// wheel buttons; otherwise the scrollback scrolls. Marks the grid rect.
/// Returns whether anything changed.
fn route_wheel_term(app: &mut App, dy: f64, grid: &Rect) -> bool {
    let Some(term) = app.terms.active_mut() else {
        return false;
    };
    let mode = term.mode();
    let m = app.text.mono_metrics(UI_FONT_SIZE, UI_FONT_SIZE * 1.35);
    let advance = f64::from(m.advance);
    let col = ((app.pointer.x - grid.x) / advance).floor().max(0.0) as usize;
    let row = ((app.pointer.y - grid.y) / viewer::LINE_H).floor().max(0.0) as usize;
    let lines = (dy / viewer::LINE_H).round() as i32;
    if mode.intersects(TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
    {
        let btn: u8 = if dy < 0.0 { 64 } else { 65 };
        if let Some(bytes) = term::encode_mouse_report(&mode, btn, col, row, false) {
            term.key_input(&bytes);
        }
        return true;
    }
    // Wheel up (dy < 0) scrolls into history; wheel down returns to the edge.
    term.scroll_display(-lines);
    true
}

/// M4 damage contract for viewer scrolls (wheel): convert the wheel's pixel
/// delta into whole lines, scroll the viewer, and mark ONLY the viewer pane
/// rect (the status line carries no viewer scroll info). Returns whether
/// anything was damaged.
#[cfg(test)]
fn route_wheel_viewer(
    viewer: &mut Viewer,
    dy: f64,
    layout: &AppLayout,
    scale: f64,
    canvas: &Canvas,
    damage: &mut Damage,
) -> bool {
    let viewport_lines = (layout.viewer_pane.h / LINE_H).floor() as usize;
    let dy_lines = (dy / LINE_H).round() as i64;
    if !viewer.scroll_by_lines(dy_lines, viewport_lines) {
        return false;
    }
    let (cw, ch) = (canvas.width(), canvas.height());
    damage.mark(damage_rect(layout.viewer_pane, scale, cw, ch));
    true
}

#[derive(Debug, Default)]
struct TerminalDrag {
    dragging: bool,
    grab_dy: f64,
    /// Pointer movement since the last requested frame.
    dirty: bool,
    /// The drag's stable editor/terminal envelope is already damaged.
    damage_marked: bool,
}

/// A left-button press is held briefly so a drag can select locally without
/// leaking press/release reports to a mouse-reporting terminal application.
#[derive(Clone, Copy, Debug)]
struct TerminalMousePress {
    x: f64,
    y: f64,
    point: Point,
}

/// One pending present wave. PTY output normally stays terminal-only; layout,
/// input, menus, and window changes use the normal scene path.
#[derive(Debug, PartialEq, Eq)]
enum RenderWave {
    Scene(Vec<DamageRect>),
    Terminal(DamageRect),
    None,
}

/// Per-window render state. `Arc<Window>` clones keep the `Surface` (which
/// owns softbuffer's display/window handles) alive independently of `window`.
struct App {
    root: PathBuf,
    /// Wake proxy: fs watcher + terminal reader threads send app wake-ups.
    /// `None` only in tests (no event loop).
    proxy: Option<EventLoopProxy<Wake>>,
    /// Project dir name for the window title and the frame header.
    title: String,
    theme: Theme,
    text: TextSystem,
    /// Window/device scale before user zoom.
    device_scale: f64,
    /// Session-only whole-UI zoom multiplier.
    zoom: f64,
    /// Effective render scale: device scale × user zoom.
    scale: f64,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    canvas: Canvas,
    /// Size the surface buffer was last resized to, to skip redundant
    /// reallocations on Wayland resize event storms.
    surface_size: (u32, u32),
    damage: Damage,
    /// Logical pointer position + button state for the current frame. The
    /// click flag is armed by a completed press→release and consumed by the
    /// next render pass.
    pointer: Pointer,
    /// The lazy file tree (root listed eagerly at startup).
    tree: FileTree,
    split: Split,
    split_drag: SplitDrag,
    scrollbar: Scrollbar,
    /// LRU cache of open viewers (bounded, M4).
    buffers: BufferCache,
    /// Bounded per-group tabs and the active editor group.
    editor_groups: EditorGroups,
    group_scrollbars: [Scrollbar; 2],
    group_split_drag: SplitDrag,
    /// Shared regex highlighter (compiled once, bounded).
    highlighter: Highlighter,
    /// Active keyboard modifiers (Ctrl+C copy routing).
    modifiers: winit::event::Modifiers,
    /// Last armed click (time + position) for double-click detection.
    last_click: Option<(Instant, f64, f64)>,
    /// Armed with the click flag when that click completes a double-click.
    pending_dbl: bool,
    /// Live-refresh watcher (M5); `None` only if watching failed to start
    /// (logged at startup — the app runs without live refresh).
    watcher: Option<Watcher>,
    /// Terminal tabs (M6); the right pane shows them while open.
    terms: term::TerminalTabs,
    /// Terminal pane focused (keys route to the shell).
    term_focused: bool,
    /// User-selected terminal height; `None` keeps the responsive default.
    term_height: Option<f64>,
    /// Sticky editor/terminal divider drag latch.
    term_drag: TerminalDrag,
    /// Pending left press in the terminal grid (click versus local drag).
    terminal_mouse_press: Option<TerminalMousePress>,
    /// Last full-scene raster (30fps terminal budget reference).
    last_render: Option<Instant>,
    /// A terminal-area repaint was deferred past the frame budget.
    term_paint_due: bool,
    /// Optional frame-timing diagnostics (`ARK_TERM_PROFILE=1`), sampled once
    /// at startup so the render hot path does not read the environment.
    term_profile: bool,
    /// PTY output changed a visible terminal since the last present. Kept
    /// separate from scene damage so repeated chunks cannot degrade the fast
    /// path into full-frame damage.
    terminal_dirty: bool,
    /// "Changed on disk" flash deadline (cleared by a scheduled redraw).
    flash_until: Option<Instant>,
    /// Group/path that owns the current change flash.
    flash_path: Option<PathBuf>,
    /// Last tree re-list (debounce reference point).
    last_tree_refresh: Option<Instant>,
    /// fs activity arrived while the tree refresh was debounced away.
    fs_dirty: bool,
    /// Git status snapshot (M7): None = not a repo / git unavailable.
    git: Option<Arc<GitStatus>>,
    /// One open-file diff (bounded: replaced, never accumulated).
    file_diff: Option<(PathBuf, Arc<GitFileDiff>)>,
    /// Commit & push panel state (M8; checked-path set + message).
    commit: commit::CommitPanel,
    /// Bounded project keyword-search state (no persistent index).
    search: search::SearchState,
    search_inflight: bool,
    search_cancel: Option<Arc<AtomicBool>>,
    search_generation: u64,
    search_queued: Option<(String, u64)>,
    search_debounce: Option<(String, Instant)>,
    /// Right-pane mode (M9): viewer vs hidden-terminal config editing.
    pane: PaneMode,
    /// Active workbench activity panel (M12; focus/highlight only).
    activity: frame::ActivityPanel,
    /// Title-bar panel visibility (responsive auto-hide still applies).
    layout_visibility: frame::LayoutVisibility,
    /// Human-writable whitelist from `ark.toml` (defaults on absence).
    settings: config::Config,
    /// The open config editor (one at a time; M9).
    config_edits: Option<ConfigEditor>,
    /// Pending debounced automatic save for the config editor.
    config_autosave_at: Option<Instant>,
    /// Streamed formatted preview state for the active Markdown tab.
    markdown: MarkdownState,
    /// Most recent config-open failure (rendered in the pane).
    config_error: Option<String>,
    /// Tab path that owns a config-open failure.
    config_error_path: Option<PathBuf>,
    /// One capped Explorer New File/New Folder name prompt.
    new_item: Option<NewItemPrompt>,
    /// Window IME state (follows PaneMode::Config).
    ime_allowed: bool,
    /// Shared blink clock for terminal and text-editor carets.
    caret_blink: CaretBlink,
    /// Last input/fs/git/PTY activity (M11 idle trim gate).
    last_activity: Instant,
    /// Last successful idle heap trim (M11 min-interval gate).
    last_trim: Option<Instant>,
    /// A git status subprocess is running (one at a time).
    git_inflight: bool,
    /// fs activity arrived while the status was in flight.
    git_pending: bool,
    /// Update helper controller and line-oriented UI state.
    updater: update_process::UpdateController,
    /// Delayed first automatic update check for this app session.
    auto_update_check_at: Option<Instant>,
    /// Set only after the user explicitly requests restart after an update.
    restart_requested: bool,
    /// Smoke-test mode (`ARK_SMOKE=1`): exit cleanly after the first
    /// successful present so automated verification never blocks on a human.
    smoke: bool,
    pending_exit: bool,
}

impl App {
    fn new(
        root: PathBuf,
        smoke: bool,
        watcher: Option<Watcher>,
        proxy: Option<EventLoopProxy<Wake>>,
    ) -> Self {
        let title = root.file_name().and_then(|n| n.to_str()).unwrap_or("ark").to_owned();
        let tree = FileTree::new(&root);
        if let Some(err) = &tree.last_error {
            // No silent failures: logged here AND shown in the status line.
            error!("tree: {err}");
        }
        let settings = match config::load(&root) {
            Ok(c) => c,
            Err(e) => {
                // Never silently widen/shrink the write whitelist: log the
                // broken ark.toml and continue with defaults.
                error!("ark.toml: {e} — using default editable whitelist");
                config::Config::default()
            }
        };
        Self {
            root,
            title,
            theme: Theme { ..Theme::DARK_PLUS },
            text: TextSystem::new(),
            device_scale: 1.0,
            zoom: 1.0,
            scale: 1.0,
            window: None,
            context: None,
            surface: None,
            canvas: Canvas::new(1, 1),
            surface_size: (0, 0),
            damage: Damage::new(64),
            pointer: Pointer::default(),
            proxy,
            tree,
            split: Split { position: 304.0 },
            split_drag: SplitDrag::default(),
            scrollbar: Scrollbar::default(),
            buffers: BufferCache::new(BUFFER_CACHE_CAP),
            editor_groups: EditorGroups::new(),
            group_scrollbars: [Scrollbar::default(), Scrollbar::default()],
            group_split_drag: SplitDrag::default(),
            highlighter: Highlighter::new(),
            modifiers: winit::event::Modifiers::default(),
            last_click: None,
            pending_dbl: false,
            watcher,
            terms: term::TerminalTabs::default(),
            term_focused: false,
            term_height: None,
            term_drag: TerminalDrag::default(),
            terminal_mouse_press: None,
            last_render: None,
            term_paint_due: false,
            term_profile: std::env::var_os("ARK_TERM_PROFILE").is_some(),
            terminal_dirty: false,
            flash_until: None,
            flash_path: None,
            last_tree_refresh: None,
            fs_dirty: false,
            git: None,
            file_diff: None,
            commit: commit::CommitPanel::default(),
            search: search::SearchState::default(),
            search_inflight: false,
            search_cancel: None,
            search_generation: 0,
            search_queued: None,
            search_debounce: None,
            pane: PaneMode::Viewer,
            activity: frame::ActivityPanel::Explorer,
            layout_visibility: frame::LayoutVisibility::default(),
            settings,
            config_edits: None,
            config_autosave_at: None,
            markdown: MarkdownState::default(),
            config_error: None,
            config_error_path: None,
            new_item: None,
            ime_allowed: false,
            caret_blink: CaretBlink::default(),
            last_activity: Instant::now(),
            last_trim: None,
            git_inflight: false,
            git_pending: false,
            updater: update_process::UpdateController::detect(),
            auto_update_check_at: None,
            restart_requested: false,
            smoke,
            pending_exit: false,
        }
    }

    /// Active editor-group index, clamped to the current group count.
    fn active_group_index(&self) -> usize {
        self.editor_groups
            .active
            .min(self.editor_groups.groups.len().saturating_sub(1))
    }

    fn active_path(&self) -> Option<PathBuf> {
        self.editor_groups.active_path().map(Path::to_path_buf)
    }

    fn config_path(&self) -> Option<PathBuf> {
        self.config_edits
            .as_ref()
            .map(|editor| editor.path().to_path_buf())
            .or_else(|| self.config_error_path.clone())
    }

    fn new_item_parent(&self) -> PathBuf {
        let Some(selected) = self.tree.selected() else {
            return self.root.clone();
        };
        // Match the tree's no-symlink-follow rule: a symlink is a file row,
        // even when its target happens to be a directory.
        let is_dir = match std::fs::symlink_metadata(selected) {
            Ok(meta) => meta.is_dir(),
            Err(e) => {
                log::debug!("selected entry unavailable at {}: {e}", selected.display());
                false
            }
        };
        if is_dir {
            selected.to_path_buf()
        } else {
            selected.parent().unwrap_or(&self.root).to_path_buf()
        }
    }

    fn request_new_item(&mut self, kind: NewItemKind) {
        let parent = self.new_item_parent();
        if parent != self.root && !self.tree.expand_to(&parent) {
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        let viewport = self.app_layout().map(|layout| layout.tree_panel.h);
        if parent == self.root {
            self.tree.set_offset(0.0, viewport.unwrap_or(0.0));
        } else if let Some(viewport) = viewport {
            let rows = self.tree.visible_rows();
            if let Some(index) = rows.iter().position(|row| row.path == parent) {
                let top = index as f64 * tree::ROW_H - self.tree.offset;
                let prompt_bottom = (index + 2) as f64 * tree::ROW_H - self.tree.offset;
                if top < 0.0 {
                    self.tree.set_offset_with_extra(
                        index as f64 * tree::ROW_H,
                        viewport,
                        1,
                    );
                } else if prompt_bottom > viewport {
                    self.tree.set_offset_with_extra(prompt_bottom - viewport, viewport, 1);
                }
            }
        }
        self.new_item = Some(NewItemPrompt::new(kind, parent));
        self.activity = frame::ActivityPanel::Explorer;
        self.term_focused = false;
        self.commit.message.focused = false;
        self.damage.mark_full();
        self.request_redraw();
    }

    fn reveal_new_item_error(&mut self, parent: &Path) {
        let Some(viewport) = self.app_layout().map(|layout| layout.tree_panel.h) else {
            return;
        };
        let rows = self.tree.visible_rows();
        let Some(index) = rows.iter().position(|row| row.path == parent) else {
            return;
        };
        // Parent row + inline field + error line.
        let error_bottom = (index + 3) as f64 * tree::ROW_H - self.tree.offset;
        if error_bottom > viewport {
            self.tree
                .set_offset_with_extra(error_bottom - viewport, viewport, 2);
        }
    }

    fn handle_new_item_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if self.new_item.is_none() {
            return false;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                self.submit_new_item();
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.cancel_new_item();
                true
            }
            _ => self
                .new_item
                .as_mut()
                .expect("prompt checked above")
                .handle_key(key, text),
        }
    }

    fn handle_search_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.search.input.focused = false;
                self.set_activity(frame::ActivityPanel::Explorer);
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.search_debounce = None;
                let query = self.search.input.text.trim().to_owned();
                self.start_search(query);
                true
            }
            _ => {
                let keys = ui::TextInput::keys_from_logical_and_text(key, text);
                let mut changed = false;
                for edit in keys {
                    changed |= self.search.input.handle_key(edit);
                }
                if changed {
                    self.search.scroll = 0.0;
                    self.request_search(self.search.input.text.clone());
                    self.damage.mark_full();
                    self.request_redraw();
                }
                changed
            }
        }
    }

    fn cancel_new_item(&mut self) {
        if self.new_item.take().is_some() {
            self.damage.mark_full();
            self.request_redraw();
        }
    }

    fn submit_new_item(&mut self) {
        let Some(prompt) = self.new_item.as_ref() else {
            return;
        };
        let kind = prompt.kind;
        let parent = prompt.parent.clone();
        let name = prompt.input.text.trim().to_owned();
        let result = match kind {
            NewItemKind::File => fs_create::create_file_in(&self.root, &parent, &name),
            NewItemKind::Folder => fs_create::create_dir_in(&self.root, &parent, &name),
        };
        match result {
            Ok(path) => {
                info!("created {} {} in {}", kind.title(), path.display(), parent.display());
                self.new_item = None;
                self.tree.refresh();
                if parent != self.root && !self.tree.is_expanded(&parent) {
                    self.tree.toggle(&parent, true);
                }
                self.tree.select(&path);
                if kind == NewItemKind::File
                    && config::is_editable(&self.root, &path, &self.settings.editable)
                {
                    self.open_config(path);
                } else if kind == NewItemKind::File {
                    self.open_viewer(path, self.active_group_index());
                }
                self.damage.mark_full();
                self.request_redraw();
            }
            Err(e) => {
                error!("create {} failed: {e}", kind.title());
                if let Some(prompt) = self.new_item.as_mut() {
                    prompt.set_error(&e.to_string());
                }
                self.reveal_new_item_error(&parent);
                self.damage.mark_full();
                self.request_redraw();
            }
        }
    }

    fn refresh_explorer(&mut self) {
        self.tree.refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    fn collapse_explorer(&mut self) {
        if self.tree.collapse_all() {
            self.damage.mark_full();
            self.request_redraw();
        }
    }

    /// Open/pin every visible viewer, then lend distinct mutable borrows to the
    /// frame. The return lifetime is tied only to the buffer cache.
    fn prepare_group_viewers<'a>(
        buffers: &'a mut BufferCache,
        groups: &mut EditorGroups,
        config_path: Option<PathBuf>,
    ) -> [Option<&'a mut Viewer>; 2] {
        let desired: Vec<PathBuf> = groups
            .visible_paths()
            .into_iter()
            .filter(|path| Some(*path) != config_path.as_deref())
            .map(Path::to_path_buf)
            .collect();
        let pins: Vec<&Path> = desired.iter().map(PathBuf::as_path).collect();
        for path in &desired {
            match buffers.get_or_open_pinned(path, &pins) {
                Ok(_) => {
                    if let Some(group) = groups
                        .groups
                        .iter_mut()
                        .find(|group| group.active.as_deref() == Some(path.as_path()))
                    {
                        group.error = None;
                    }
                }
                Err(e) => {
                    error!("viewer: {e}");
                    if let Some(group) = groups
                        .groups
                        .iter_mut()
                        .find(|group| group.active.as_deref() == Some(path.as_path()))
                    {
                        group.error = Some(e.to_string());
                    }
                }
            }
        }
        let ready: Vec<PathBuf> = desired
            .into_iter()
            .filter(|path| buffers.contains(path))
            .collect();
        let refs: Vec<&Path> = ready.iter().map(PathBuf::as_path).collect();
        let mut viewers = buffers.viewers_mut(&refs).into_iter();
        let mut out = [None, None];
        for (index, group) in groups.groups.iter().enumerate() {
            if index >= out.len() {
                break;
            }
            if group
                .active
                .as_ref()
                .is_some_and(|path| ready.contains(path))
            {
                out[index] = viewers.next();
            }
        }
        out
    }

    fn evict_group_tabs(&mut self, paths: Vec<PathBuf>) {
        let config_path = self.config_path();
        for path in paths {
            if Some(path.as_path()) != config_path.as_deref() {
                self.buffers.evict(&path);
            }
        }
    }

    fn sync_active_pane(&mut self) {
        let is_config = (self.config_edits.is_some() || self.config_error.is_some())
            && self.config_path().as_deref() == self.editor_groups.active_path();
        self.pane = if is_config {
            PaneMode::Config
        } else {
            PaneMode::Viewer
        };
    }

    /// Apply one bounded whole-UI zoom step.
    fn adjust_zoom(&mut self, step: i32) -> bool {
        let next = zoom::next_zoom(self.zoom, step);
        let next_scale = self.device_scale * next;
        if !self.update_effective_scale(next_scale) {
            return false;
        }
        self.zoom = next;
        self.damage.mark_full();
        info!("ui zoom changed to {:.0}%", self.zoom * 100.0);
        self.request_redraw();
        true
    }

    /// Change the window/device scale while preserving session zoom.
    fn set_device_scale(&mut self, device_scale: f64) -> bool {
        if !device_scale.is_finite() || device_scale <= 0.0 {
            return false;
        }
        let changed = self.update_effective_scale(device_scale * self.zoom);
        self.device_scale = device_scale;
        changed
    }

    /// Update raster scale while keeping the pointer at the same physical
    /// position. A scale change invalidates click/double-click geometry.
    fn update_effective_scale(&mut self, next_scale: f64) -> bool {
        if !next_scale.is_finite()
            || next_scale <= 0.0
            || (next_scale - self.scale).abs() < f64::EPSILON
        {
            return false;
        }
        let ratio = self.scale / next_scale;
        self.pointer.x *= ratio;
        self.pointer.y *= ratio;
        self.pointer.click = false;
        self.pointer.down = false;
        self.last_click = None;
        self.pending_dbl = false;
        self.terminal_mouse_press = None;
        self.split_drag = SplitDrag::default();
        self.group_split_drag = SplitDrag::default();
        self.term_drag = TerminalDrag::default();
        self.scrollbar = Scrollbar::default();
        self.group_scrollbars = [Scrollbar::default(); 2];
        self.scale = next_scale;
        true
    }

    /// Request one redraw when a window exists.
    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Select the smallest correct render wave. Terminal-only painting is safe
    /// only when no scene interaction is pending and the grid is visible.
    fn take_render_wave(&mut self, layout: &AppLayout) -> RenderWave {
        // This frame consumes any damage marked by the drag. Re-arm the
        // one-region guard so subsequent pointer movement paints again.
        self.term_drag.damage_marked = false;
        let terminal_was_dirty = self.terminal_dirty;
        let terminal_chrome_hovered = layout.terminal_toolbar.contains(self.pointer.x, self.pointer.y)
            || layout.terminal_resize.contains(self.pointer.x, self.pointer.y);
        let terminal_only = self.terminal_dirty
            && self.damage.is_empty()
            && self.layout_visibility.terminal
            && layout.term_grid.w > 0.0
            && layout.term_grid.h > 0.0
            && self.terms.is_open()
            && !self.terms.menu_open
            && !terminal_chrome_hovered
            && !self.term_drag.dragging
            && !self.pointer.click
            && !self.pending_dbl
            && self.scale.is_finite()
            && self.scale > 0.0;
        self.terminal_dirty = false;
        if terminal_only {
            return RenderWave::Terminal(damage_rect(
                layout.term_grid,
                self.scale,
                self.canvas.width(),
                self.canvas.height(),
            ));
        }
        // A tooltip over terminal chrome must survive PTY repaints. Use a full
        // scene wave there instead of clearing the grid beneath the tooltip.
        if self.damage.is_empty()
            && terminal_was_dirty
            && terminal_chrome_hovered
            && self.layout_visibility.terminal
            && self.terms.is_open()
            && !self.terms.menu_open
            && !self.term_drag.dragging
        {
            self.damage.mark_full();
        }
        match self.damage.take(self.canvas.width(), self.canvas.height()) {
            Some(rects) => RenderWave::Scene(rects),
            None => RenderWave::None,
        }
    }

    /// Current frame layout from the window's logical size, or `None` before
    /// the window exists / while collapsed.
    fn app_layout(&self) -> Option<AppLayout> {
        let window = self.window.as_ref()?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some(frame::app_layout_with_visibility_and_zoom(
            size.width as f64 / self.scale,
            size.height as f64 / self.scale,
            self.split.position,
            self.editor_groups.divider,
            self.editor_groups.groups.len(),
            self.layout_visibility,
            self.term_height,
            self.zoom,
        ))
    }

    fn begin_terminal_resize(&mut self, layout: &AppLayout) -> bool {
        if layout.terminal_resize.w <= 0.0 || layout.terminal_resize.h <= 0.0 {
            return false;
        }
        self.term_drag.dragging = true;
        self.term_drag.grab_dy = self.pointer.y - layout.term_dock.y;
        self.term_drag.dirty = false;
        self.term_drag.damage_marked = false;
        log::debug!("terminal resize drag started");
        true
    }

    fn resize_terminal_to(&mut self, pointer_y: f64, layout: &AppLayout) -> bool {
        if !self.term_drag.dragging || !pointer_y.is_finite() {
            return false;
        }
        let bottom = layout.workbench.y + layout.workbench.h;
        let requested = bottom - (pointer_y - self.term_drag.grab_dy);
        let height = frame::clamp_terminal_height(
            requested,
            layout.workbench.h,
            layout.terminal_resize.h,
        );
        if self.term_height == Some(height) {
            return false;
        }
        self.term_height = Some(height);
        self.term_drag.dirty = true;
        log::debug!("terminal height set to {height:.1}px");
        true
    }

    /// Damage the editor/terminal column once per drag. Its geometry is
    /// stable while only the divider moves, so repeated pointer events do not
    /// accumulate rectangles or degrade into full-frame damage.
    fn mark_terminal_resize_damage(&mut self, layout: &AppLayout) {
        if self.term_drag.damage_marked {
            return;
        }
        let region = Rect {
            x: layout.editor_card.x,
            y: layout.workbench.y,
            w: layout.editor_card.w,
            h: layout.workbench.h,
        };
        let (cw, ch) = (self.canvas.width(), self.canvas.height());
        self.damage.mark(damage_rect(region, self.scale, cw, ch));
        self.term_drag.damage_marked = true;
    }

    /// Consume the coalesced resize redraw request. Called after a batch of
    /// pointer movements so one event-loop turn requests at most one frame.
    fn take_terminal_resize_redraw(&mut self) -> bool {
        std::mem::take(&mut self.term_drag.dirty)
    }

    /// End a divider drag and send fish its final grid size exactly once.
    /// When release arrives before the last visual frame, fit the model to
    /// the final layout here so a stale intermediate size is never sent.
    fn finish_terminal_resize(&mut self, layout: Option<&AppLayout>) -> bool {
        if !self.term_drag.dragging {
            return false;
        }
        self.term_drag.dragging = false;
        self.term_drag.dirty = false;
        if let Some(layout) = layout {
            if let Some(size) = frame::terminal_grid_size(
                &mut self.text,
                self.scale,
                layout.ui_scale,
                layout.term_grid,
            ) {
                if let Some(term) = self.terms.active_mut() {
                    term.resize_visual(size);
                }
            }
        }
        self.terms.flush_active_resize();
        log::debug!("terminal resize drag finished");
        true
    }

    /// Terminal grid as if the dock were open (new tab spawn size).
    fn terminal_grid_for_new_tab(&mut self) -> Option<(Rect, f64)> {
        let layout = self.app_layout()?;
        Some((layout.term_grid, self.terminal_advance(layout.ui_scale)))
    }

    fn terminal_advance(&mut self, ui_scale: f64) -> f64 {
        let font_size = UI_FONT_SIZE * self.scale as f32 * ui_scale as f32;
        let m = self.text.mono_metrics(font_size, font_size * 1.35);
        f64::from(m.advance) / self.scale
    }

    fn begin_terminal_mouse_press(&mut self, grid: Rect, advance: f64) -> bool {
        self.terminal_mouse_press = None;
        let Some(point) = self
            .terms
            .active()
            .and_then(|term| term.point_at(grid, self.pointer.x, self.pointer.y, advance))
        else {
            return false;
        };
        if let Some(term) = self.terms.active_mut() {
            term.clear_selection();
        }
        self.terminal_mouse_press = Some(TerminalMousePress {
            x: self.pointer.x,
            y: self.pointer.y,
            point,
        });
        true
    }

    fn update_terminal_mouse_selection(&mut self, grid: Rect, advance: f64) -> bool {
        let Some(press) = self.terminal_mouse_press else {
            return false;
        };
        if !self.pointer.x.is_finite()
            || !self.pointer.y.is_finite()
            || (self.pointer.x - press.x).hypot(self.pointer.y - press.y) < DBL_CLICK_SLOP
        {
            return false;
        }
        let Some(point) = self
            .terms
            .active()
            .and_then(|term| term.point_at(grid, self.pointer.x, self.pointer.y, advance))
        else {
            return false;
        };
        let Some(term) = self.terms.active_mut() else {
            return false;
        };
        if !term.is_selecting() {
            term.begin_selection(press.point);
        }
        term.extend_selection(point);
        true
    }

    /// Finish a pending left press. Returns whether it became a local
    /// selection. A click is delivered to the PTY only on release, and only
    /// when both press and release remain inside the terminal grid.
    fn finish_terminal_mouse_press(&mut self, grid: Rect, advance: f64) -> bool {
        let Some(press) = self.terminal_mouse_press.take() else {
            return false;
        };
        if self
            .terms
            .active()
            .is_some_and(|term| term.is_selecting())
        {
            if let Some(term) = self.terms.active_mut() {
                term.end_selection();
            }
            return true;
        }
        if grid.contains(self.pointer.x, self.pointer.y) {
            self.term_mouse_report_at(ElementState::Pressed, press.x, press.y, grid, advance);
            self.term_mouse_report_at(
                ElementState::Released,
                self.pointer.x,
                self.pointer.y,
                grid,
                advance,
            );
        }
        false
    }

    /// Scroll the viewer in the group under the pointer, pinning both visible
    /// paths while the cache is touched.
    fn route_wheel_viewer_group(
        &mut self,
        group: usize,
        rect: Rect,
        dy: f64,
        horizontal: bool,
    ) -> bool {
        let Some(path) = self.editor_groups.groups.get(group).and_then(|g| g.active.clone()) else {
            return false;
        };
        if Some(path.as_path()) == self.config_path().as_deref() {
            return false;
        }
        let visible: Vec<PathBuf> = self
            .editor_groups
            .visible_paths()
            .into_iter()
            .filter(|path| Some(*path) != self.config_path().as_deref())
            .map(Path::to_path_buf)
            .collect();
        let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
        let Ok(viewer) = self.buffers.get_or_open_pinned(&path, &pins) else {
            return false;
        };
        let font_size = (UI_FONT_SIZE as f64 * self.scale) as f32;
        let m = self.text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
        let advance = f64::from(m.advance) / self.scale;
        let gutter_w = viewer::gutter_width(viewer.line_count(), advance);
        let content_w = (rect.x + rect.w - tree::TRACK_W - (rect.x + gutter_w) - 4.0).max(0.0);
        let changed = if horizontal {
            let viewport_cols = (content_w / advance).floor().max(1.0) as usize;
            let cols = (dy / advance).round() as i64;
            viewer.scroll_horizontal_by(cols, viewport_cols)
        } else {
            let overflow = content_w > 0.0 && viewer.max_line_chars() as f64 * advance > content_w;
            let code_h = rect.h - if overflow { ui::HSCROLL_H } else { 0.0 };
            let viewport_lines = (code_h / LINE_H).floor() as usize;
            let dy_lines = (dy / LINE_H).round() as i64;
            viewer.scroll_by_lines(dy_lines, viewport_lines)
        };
        if !changed {
            return false;
        }
        let (cw, ch) = (self.canvas.width(), self.canvas.height());
        self.damage.mark(damage_rect(rect, self.scale, cw, ch));
        true
    }

    /// Whether the tracked pointer is inside the window (logical px) — for
    /// arming the click flag on release.
    fn pointer_in_window(&self) -> bool {        let Some(window) = &self.window else { return false };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return false;
        }
        let (lw, lh) = (size.width as f64 / self.scale, size.height as f64 / self.scale);
        self.pointer.x >= 0.0 && self.pointer.y >= 0.0 && self.pointer.x < lw && self.pointer.y < lh
    }

    /// Present one frame: raster pending damage onto the persistent canvas,
    /// blit into the softbuffer buffer, present with damage rects.
    fn render(&mut self) {
        let profile = self.term_profile;
        let render_start = Instant::now();
        // The config editor is the only IME surface (M9).
        self.sync_ime();
        // app_layout borrows all of self; it must finish before the surface
        // borrow below.
        let Some(layout) = self.app_layout() else { return };
        let Some(window) = &self.window else { return };
        // The window size is guaranteed non-zero here (app_layout checked).
        let size = window.inner_size();
        if self.surface_size != (size.width, size.height) {
            let Some(surface) = self.surface.as_mut() else { return };
            if let Err(e) = surface.resize(
                NonZeroU32::new(size.width.max(1)).expect("nonzero width"),
                NonZeroU32::new(size.height.max(1)).expect("nonzero height"),
            ) {
                error!("surface resize to {}x{} failed: {e}", size.width, size.height);
                return;
            }
            self.surface_size = (size.width, size.height);
            self.canvas.resize(size.width, size.height);
            self.damage.mark_full();
            info!("surface resized to {}x{}", size.width, size.height);
        }
        let rects = match self.take_render_wave(&layout) {
            RenderWave::None => return,
            RenderWave::Scene(rects) => rects,
            RenderWave::Terminal(rect) => {
                // PTY output repaints only the grid. No viewer preparation,
                // chrome composition, or whole-canvas raster is needed.
                log::trace!(
                    "terminal-only frame {}x{}",
                    rect.width,
                    rect.height
                );
                frame::draw_terminal_grid(
                    &mut self.canvas,
                    &mut self.text,
                    &self.theme,
                    self.scale,
                    layout.ui_scale,
                    layout.term_grid,
                    &mut self.terms,
                    self.term_focused,
                );
                if !self.term_drag.dragging {
                    self.terms.flush_active_resize();
                }
                let raster_elapsed = render_start.elapsed();
                let Some(surface) = self.surface.as_mut() else { return };
                if render::present(surface, &self.canvas, &[rect]) && self.smoke {
                    self.pending_exit = true;
                }
                if profile {
                    let interval = self
                        .last_render
                        .map(|last| render_start.duration_since(last))
                        .unwrap_or_default();
                    info!(
                        "terminal frame: interval={interval:?} raster={raster_elapsed:?} present={:?} total={:?}",
                        render_start.elapsed() - raster_elapsed,
                        render_start.elapsed()
                    );
                }
                self.last_render = Some(Instant::now());
                return;
            }
        };

        let active_group = self.active_group_index();
        let active_path = self.active_path();
        let config_path = self.config_path();
        let group_viewers = Self::prepare_group_viewers(
            &mut self.buffers,
            &mut self.editor_groups,
            config_path,
        );
        let Some(surface) = self.surface.as_mut() else { return };

        // Raster the full scene per damage wave (partial damage bounds the
        // PRESENT, not the raster). The frame consumes the armed click and
        // double-click flags exactly once.
        let pointer = self.pointer;
        self.pointer.click = false;
        let dbl = std::mem::take(&mut self.pending_dbl);
        let now = Instant::now();
        let activity_pulse = handle_activity_pulse(now, self.last_activity);
        let animation_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let terms = if self.terms.is_open() {
            Some(&mut self.terms)
        } else {
            None
        };
        let config = self.config_edits.as_mut();
        let config_error = self.config_error.as_deref();
        let config_error_path = self.config_error_path.as_deref();
        let git = self.git.as_deref();
        let file_diff = self
            .file_diff
            .as_ref()
            .filter(|(path, _)| Some(path) == active_path.as_ref())
            .map(|(_, diff)| diff.as_ref());
        let flash_path = if self.flash_until.is_some_and(|deadline| Instant::now() < deadline) {
            self.flash_path.clone()
        } else {
            None
        };
        let mut group_flash = [false; 2];
        for (index, group) in self.editor_groups.groups.iter().enumerate() {
            if index >= group_flash.len() {
                break;
            }
            group_flash[index] = group.active.as_deref() == flash_path.as_deref();
        }
        let effects = frame::draw_frame(
            &mut Frame {
                canvas: &mut self.canvas,
                text: &mut self.text,
                theme: &self.theme,
                scale: self.scale,
                ui_scale: layout.ui_scale,
                activity_pulse,
                layout_visibility: self.layout_visibility,
                animation_time,
                title: &self.title,
                root: &self.root,
                open_files: &[],
                active_path: active_path.as_deref(),
                activity: self.activity,
                groups: Some(&self.editor_groups.groups),
                active_group,
                group_viewers,
                group_flash,
                group_scrollbars: Some(&mut self.group_scrollbars),
                group_split: Some(&mut self.editor_groups.divider),
                group_split_drag: Some(&mut self.group_split_drag),
                pointer,
                dbl_click: dbl,
                viewer: None,
                viewer_error: None,
                viewer_flash: self.flash_until.is_some_and(|d| Instant::now() < d),
                highlighter: &self.highlighter,
                git,
                file_diff,
                commit: Some(&mut self.commit),
                config,
                config_error,
                config_error_path,
                terms,
                term_focused: self.term_focused,
                new_item: self.new_item.as_mut(),
                icon_tooltip: None,
                search: Some(&mut self.search),
                markdown: Some(&mut self.markdown),
                update: Some(&self.updater),
            },
            &layout,
            &mut self.tree,
            &mut self.split,
            &mut self.split_drag,
            &mut self.scrollbar,
            &mut Scrollbar::default(),
        );
        // Window/layout changes resize the PTY immediately. Only an active
        // divider drag defers SIGWINCH until release.
        if !self.term_drag.dragging {
            self.terms.flush_active_resize();
        }
        let raster_elapsed = render_start.elapsed();
        // Post-raster damage for state changes made during the frame (the
        // marks schedule the next present).
        if effects.tree_changed {
            self.damage.mark_full();
        }
        if effects.scrolled {
            let (cw, ch) = (self.canvas.width(), self.canvas.height());
            self.damage.mark(damage_rect(layout.tree_panel, self.scale, cw, ch));
        }
        if effects.viewer_scrolled {
            let (cw, ch) = (self.canvas.width(), self.canvas.height());
            for group in layout.editor_groups.iter().take(layout.editor_group_count) {
                self.damage.mark(damage_rect(group.body, self.scale, cw, ch));
            }
        }
        let mut need_redraw = false;
        let mut pending_config: Option<PathBuf> = None;
        let mut pending_viewer: Option<(PathBuf, usize)> = None;
        if let Some(path) = effects.opened {
            if config::is_editable(&self.root, &path, &self.settings.editable) {
                pending_config = Some(path);
            } else {
                pending_viewer = Some((path, active_group));
            }
            need_redraw = true;
        }

        // Blit + present (failures are logged, never silent).
        if render::present(surface, &self.canvas, &rects) && self.smoke {
            self.pending_exit = true;
        }
        if profile {
            let interval = self
                .last_render
                .map(|last| render_start.duration_since(last))
                .unwrap_or_default();
            info!(
                "scene frame: interval={interval:?} raster={raster_elapsed:?} present={:?} total={:?}",
                render_start.elapsed() - raster_elapsed,
                render_start.elapsed()
            );
        }
        self.last_render = Some(Instant::now());
        if effects.open_terminal {
            self.duplicate_terminal_tab();
        }
        if let Some(index) = effects.close_terminal {
            self.close_terminal_tab_at(index);
        }
        if let Some(index) = effects.terminal_select {
            if index < self.terms.tabs.len() {
                self.terms.active = index;
                self.terms.menu_open = false;
                self.term_focused = true;
                self.activity = frame::ActivityPanel::Terminal;
                self.damage.mark_full();
                self.request_redraw();
            }
        }
        if effects.terminal_menu_toggle {
            self.terms.menu_open = !self.terms.menu_open;
            self.term_focused = !self.terms.menu_open && self.terms.is_open();
            self.damage.mark_full();
            self.request_redraw();
        }
        if effects.terminal_menu_dismiss && self.terms.menu_open {
            self.terms.menu_open = false;
            self.term_focused = self.terms.is_open();
            self.damage.mark_full();
            self.request_redraw();
        }
        if let Some(action) = effects.terminal_menu_action {
            self.terms.menu_open = false;
            match action {
                term::TerminalMenuAction::NewTerminal => self.open_terminal_tab(),
                term::TerminalMenuAction::NewTerminalWindow => self.open_terminal_window(),
                term::TerminalMenuAction::OpenProfile(profile) => {
                    self.open_terminal_profile(profile)
                }
            }
        }
        if let Some(action) = effects.layout_action {
            self.apply_layout_action(action);
        }
        if let Some(action) = effects.update_action {
            self.apply_update_action(action);
        }
        if let Some((path, line)) = effects.search_open {
            self.open_search_result(path, line, active_group);
        }
        if effects.new_file {
            self.request_new_item(NewItemKind::File);
        }
        if effects.new_folder {
            self.request_new_item(NewItemKind::Folder);
        }
        if effects.refresh_tree {
            self.refresh_explorer();
        }
        if effects.collapse_tree {
            self.collapse_explorer();
        }
        if effects.commit_clicked {
            self.commit_now();
        }
        if effects.push_clicked {
            self.push_now();
        }
        if effects.config_save {
            self.save_config();
        }
        if effects.config_reload {
            self.reload_config();
        }
        if effects.config_changed {
            self.schedule_config_autosave();
        }
        if let Some(panel) = effects.set_activity {
            self.set_activity(panel);
        }
        if effects.commit_toggled || effects.group_split_moved {
            self.damage.mark_full();
        }
        if let Some(group) = effects.focus_group {
            self.focus_editor_group(group);
        }
        if effects.toggle_split {
            self.toggle_editor_split();
        }
        if effects.toggle_markdown {
            self.toggle_markdown_preview();
        }
        if let Some(group) = effects.close_group {
            self.close_editor_group(group);
        }
        if let Some((group, path)) = effects.select_tab {
            self.select_tab(group, path);
        }
        if let Some((group, path)) = effects.close_tab {
            self.close_tab(group, path);
        }
        if let Some((path, group)) = pending_viewer {
            self.open_viewer(path, group);
        }
        if let Some(path) = pending_config {
            self.open_config(path);
        }
        // The redraw for the just-opened file waits until the surface borrow
        // has ended (request_redraw borrows all of self).
        if need_redraw {
            self.request_redraw();
        }
    }

    /// Queue a project keyword search after a short typing pause.
    fn request_search(&mut self, query: String) {
        let query = query.trim().to_owned();
        self.search_debounce = Some((query, Instant::now() + SEARCH_DEBOUNCE));
    }

    /// Start a due debounced search. Returns whether one was launched.
    fn start_search_if_due(&mut self, now: Instant) -> bool {
        let Some((query, due)) = self.search_debounce.take() else {
            return false;
        };
        if now < due {
            self.search_debounce = Some((query, due));
            return false;
        }
        self.start_search(query);
        true
    }

    fn start_search(&mut self, query: String) {
        self.search_generation = self.search_generation.saturating_add(1);
        let generation = self.search_generation;
        if query.is_empty() {
            if let Some(cancel) = self.search_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.search_inflight = false;
            self.search_queued = None;
            self.search.input.text.clear();
            self.search.input.cursor = 0;
            self.search.clear();
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        if self.search_inflight {
            if let Some(cancel) = &self.search_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            self.search_queued = Some((query.clone(), generation));
            self.search.begin(&query);
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        self.launch_search(query, generation);
    }

    fn launch_search(&mut self, query: String, generation: u64) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(cancel.clone());
        self.search_inflight = true;
        self.search.begin(&query);
        let root = self.root.clone();
        match self.proxy.clone() {
            Some(proxy) => {
                let spawned = thread::Builder::new()
                    .name("ark-keyword-search".into())
                    .spawn(move || {
                        let report = search::run_search(&root, &query, generation, &cancel);
                        let _ = proxy.send_event(Wake::SearchFinished(Box::new(report)));
                    });
                if spawned.is_err() {
                    self.search_inflight = false;
                    self.search_cancel = None;
                    self.search.running = false;
                    self.search.status = "Search worker unavailable".into();
                    error!("keyword search worker spawn failed");
                }
            }
            None => {
                let report = search::run_search(&self.root, &query, generation, &cancel);
                self.apply_search_report(report);
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn apply_search_report(&mut self, report: search::SearchReport) {
        if report.generation == self.search_generation {
            self.search_inflight = false;
            self.search_cancel = None;
            if report.query == self.search.input.text {
                self.search.apply(report);
                self.damage.mark_full();
                self.request_redraw();
            }
        } else if self.search_queued.is_some() {
            // This is the cancelled worker that the queued query replaces.
            // Unrelated older reports must not clear a newer active worker.
            self.search_inflight = false;
            self.search_cancel = None;
        } else {
            return;
        }
        if let Some((query, generation)) = self.search_queued.take() {
            self.launch_search(query, generation);
        }
    }

    /// Dispatch a git status subprocess (M7): one in flight at a time; fs
    /// bursts during flight set a pending flag, re-dispatched on completion.
    /// Without a proxy (tests) the status runs inline.
    fn request_git_refresh(&mut self) {
        if self.git_inflight {
            self.git_pending = true;
            return;
        }
        self.git_inflight = true;
        let root = self.root.clone();
        match self.proxy.clone() {
            Some(proxy) => {
                let spawned = thread::Builder::new()
                    .name("ark-git-status".into())
                    .spawn(move || {
                        let status = match git::status(&root) {
                            Ok(s) => Some(Arc::new(s)),
                            Err(e) => {
                                log::debug!("git status unavailable: {e}");
                                None
                            }
                        };
                        let _ = proxy.send_event(Wake::GitStatus(status));
                    });
                if spawned.is_err() {
                    self.git_inflight = false;
                    self.git_pending = true;
                }
            }
            None => {
                let status = git::status(&root).ok().map(Arc::new);
                self.apply_git_status(status);
            }
        }
    }

    /// Apply a finished status snapshot (None = not a repo): refresh the
    /// open-file diff and repaint.
    fn apply_git_status(&mut self, status: Option<Arc<GitStatus>>) {
        self.git_inflight = false;
        self.git = status;
        if let Some(status) = &self.git {
            // M8: prune unchecks/scroll against the new snapshot (bounded).
            self.commit.retain(status);
        }
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
        if self.git_pending {
            self.git_pending = false;
            self.request_git_refresh();
        }
    }

    /// Re-run the per-file diff for the OPEN viewer file (lazy, M7): a
    /// background subprocess lands via Wake::GitDiff; a stale result (file
    /// no longer open) is dropped on arrival. Without a proxy (tests) it
    /// runs inline.
    fn request_diff_refresh(&mut self) {
        let Some(p) = self.active_path() else {
            self.file_diff = None;
            return;
        };
        if self.git.is_none() {
            self.file_diff = None;
            return;
        }
        let root = self.root.clone();
        match self.proxy.clone() {
            Some(proxy) => {
                let spawned = thread::Builder::new()
                    .name("ark-git-diff".into())
                    .spawn(move || {
                        let diff = git::diff_file(&root, &p).unwrap_or_default();
                        let _ = proxy.send_event(Wake::GitDiff { path: p, diff: Arc::new(diff) });
                    });
                if spawned.is_err() {
                    log::warn!("git diff thread spawn failed");
                }
            }
            None => {
                let diff = git::diff_file(&root, &p).unwrap_or_default();
                self.file_diff = Some((p, Arc::new(diff)));
            }
        }
    }

    /// Open a new terminal tab (registry's default shell, root cwd).
    fn open_terminal_tab(&mut self) {
        self.layout_visibility.terminal = true;
        let Some(proxy) = self.proxy.clone() else {
            error!("terminal open failed: no wake proxy");
            return;
        };
        let Some((grid, advance)) = self.terminal_grid_for_new_tab() else {
            return;
        };
        let size = term::GridSize::fit(grid.w, grid.h, advance, viewer::LINE_H);
        match self.terms.open(&self.root, size, proxy) {
            Ok(()) => self.focus_terminal(),
            Err(e) => error!("terminal open failed: {e}"),
        }
    }

    /// `+` clones the active terminal; with no tabs it opens the default shell.
    fn duplicate_terminal_tab(&mut self) {
        self.layout_visibility.terminal = true;
        if !self.terms.is_open() {
            self.open_terminal_tab();
            return;
        }
        let Some(proxy) = self.proxy.clone() else {
            error!("terminal duplicate failed: no wake proxy");
            return;
        };
        let Some((grid, advance)) = self.terminal_grid_for_new_tab() else {
            return;
        };
        let size = term::GridSize::fit(grid.w, grid.h, advance, viewer::LINE_H);
        match self.terms.duplicate_active(&self.root, size, proxy) {
            Ok(()) => self.focus_terminal(),
            Err(e) => error!("terminal duplicate failed: {e}"),
        }
    }

    fn open_terminal_profile(&mut self, profile: term::TerminalProfile) {
        self.layout_visibility.terminal = true;
        let Some(program) = profile.program() else {
            error!("terminal profile {} is unavailable", profile.label());
            return;
        };
        let Some(proxy) = self.proxy.clone() else {
            error!("terminal profile open failed: no wake proxy");
            return;
        };
        let Some((grid, advance)) = self.terminal_grid_for_new_tab() else {
            return;
        };
        let size = term::GridSize::fit(grid.w, grid.h, advance, viewer::LINE_H);
        match self
            .terms
            .open_program(&self.root, &program, profile.label(), size, proxy)
        {
            Ok(()) => self.focus_terminal(),
            Err(e) => error!("terminal profile {} failed: {e}", profile.label()),
        }
    }

    fn open_terminal_window(&mut self) {
        let Some(program) = term::external_terminal_program() else {
            error!("terminal window open failed: no terminal emulator found");
            return;
        };
        let mut command = std::process::Command::new(&program);
        term::configure_external_terminal(&mut command, &program, &self.root);
        match command.spawn() {
            Ok(_) => info!("opened terminal window with {}", program.display()),
            Err(e) => error!("terminal window open failed: {e}"),
        }
    }

    fn focus_terminal(&mut self) {
        self.terms.menu_open = false;
        self.activity = frame::ActivityPanel::Terminal;
        self.term_focused = true;
        self.commit.message.focused = false;
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close a terminal tab by index and restore focus when the last one exits.
    fn close_terminal_tab_at(&mut self, index: usize) {
        self.terms.menu_open = false;
        self.terms.close_at(index);
        if !self.terms.is_open() {
            self.pane = self.pane_after_terminals();
            self.activity = if self.pane == PaneMode::Config {
                frame::ActivityPanel::Config
            } else {
                frame::ActivityPanel::Explorer
            };
            self.term_focused = false;
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close the active terminal tab and return focus to the remaining editor
    /// surface when the last shell is gone.
    fn close_terminal_tab(&mut self) {
        if self.terms.is_open() {
            self.close_terminal_tab_at(self.terms.active);
        }
    }

    /// M12: activity-bar focus routing. The bar highlights the active panel;
    /// Terminal/Config also switch the editor surface when one exists.
    fn set_activity(&mut self, panel: frame::ActivityPanel) {
        self.activity = panel;
        match panel {
            frame::ActivityPanel::Terminal => {
                if self.terms.is_open() {
                    self.term_focused = true;
                    self.commit.message.focused = false;
                }
            }
            frame::ActivityPanel::Config => {
                if let Some(path) = self.config_path() {
                    if let Some(group) = self.editor_groups.group_owning(&path) {
                        self.editor_groups.active = group;
                        self.sync_active_pane();
                    }
                }
                if self.config_edits.is_some() || self.config_error.is_some() {
                    self.pane = PaneMode::Config;
                }
                self.term_focused = false;
                self.commit.message.focused = false;
            }
            frame::ActivityPanel::Explorer => {
                if let Some(group) = self
                    .editor_groups
                    .groups
                    .iter()
                    .position(|group| {
                        group.active.is_some()
                            && group.active.as_deref() != self.config_path().as_deref()
                    })
                {
                    self.editor_groups.active = group;
                }
                self.sync_active_pane();
                self.term_focused = false;
                self.commit.message.focused = false;
            }
            frame::ActivityPanel::Search => {
                self.search.input.focused = true;
                self.term_focused = false;
                self.commit.message.focused = false;
            }
            frame::ActivityPanel::SourceControl => {
                self.term_focused = false;
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn apply_layout_action(&mut self, action: frame::LayoutAction) {
        match action {
            frame::LayoutAction::ToggleExplorer => {
                self.layout_visibility.explorer = !self.layout_visibility.explorer;
                if !self.layout_visibility.explorer {
                    self.cancel_new_item();
                }
            }
            frame::LayoutAction::ToggleTerminal => {
                if self.layout_visibility.terminal && self.terms.is_open() {
                    self.layout_visibility.terminal = false;
                    self.terms.menu_open = false;
                    self.term_focused = false;
                    self.finish_terminal_resize(None);
                    self.term_paint_due = false;
                    self.terminal_dirty = false;
                } else {
                    self.layout_visibility.terminal = true;
                    self.terms.menu_open = false;
                    if self.terms.is_open() {
                        self.activity = frame::ActivityPanel::Terminal;
                        self.term_focused = true;
                        self.commit.message.focused = false;
                    } else {
                        self.open_terminal_tab();
                    }
                }
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn apply_update_action(&mut self, action: update_process::UpdateAction) {
        match action {
            update_process::UpdateAction::TogglePanel => {
                self.updater.ui.open = !self.updater.ui.open;
            }
            update_process::UpdateAction::Later => self.updater.ui.later(),
            update_process::UpdateAction::Check => {
                self.start_update(update_process::UpdateCommand::Check, false);
            }
            update_process::UpdateAction::Install => {
                if let Some(command) = self.updater.install_command() {
                    self.start_update(command, false);
                }
            }
            update_process::UpdateAction::Restart => {
                self.restart_after_update();
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn start_update(&mut self, command: update_process::UpdateCommand, automatic: bool) {
        let Some(proxy) = self.proxy.clone() else {
            self.updater.ui.fail(
                "event-loop-unavailable",
                "updates require a running ark event loop",
                automatic,
            );
            return;
        };
        if let Err(error) = self.updater.start(command, automatic, proxy) {
            error!("update helper: {error}");
        }
    }

    fn schedule_auto_update_check(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke || !self.updater.supported() || self.updater.is_busy() {
            return;
        }
        if !self.updater.automatic_due(std::time::SystemTime::now()) {
            return;
        }
        self.auto_update_check_at = Some(Instant::now() + Duration::from_millis(500));
        self.schedule_followups(event_loop);
    }

    fn start_auto_update_if_due(&mut self, now: Instant) -> bool {
        let Some(due) = self.auto_update_check_at.take() else {
            return false;
        };
        if now < due {
            self.auto_update_check_at = Some(due);
            return false;
        }
        if self.updater.supported()
            && !self.updater.is_busy()
            && self.updater.automatic_due(std::time::SystemTime::now())
        {
            self.start_update(update_process::UpdateCommand::Check, true);
        }
        true
    }

    fn restart_after_update(&mut self) -> bool {
        if !self.flush_config_autosave() {
            self.updater.ui.fail(
                "restart-failed",
                "save or reload pending config changes before restarting",
                false,
            );
            self.damage.mark_full();
            self.request_redraw();
            return false;
        }
        let target = match self.updater.restart_target() {
            Ok(target) => target,
            Err(error) => {
                self.updater
                    .ui.fail("restart-failed", error, false);
                self.damage.mark_full();
                self.request_redraw();
                return false;
            }
        };
        match std::process::Command::new(target)
            .arg(&self.root)
            .spawn()
        {
            Ok(_) => {
                info!("restarting into updated ark at {}", self.root.display());
                self.restart_requested = true;
                true
            }
            Err(error) => {
                error!("could not restart updated ark: {error}");
                self.updater.ui.fail(
                    "restart-failed",
                    format!("could not restart ark: {error}"),
                    false,
                );
                self.damage.mark_full();
                self.request_redraw();
                false
            }
        }
    }

    /// Open a viewer in a group, or focus the group that already owns it.
    fn open_viewer(&mut self, path: PathBuf, group: usize) {
        self.markdown.preview = false;
        self.markdown.scroll = 0.0;
        if group < self.editor_groups.groups.len() {
            self.editor_groups.active = group;
        }
        let result = self.editor_groups.open(path.clone());
        let owner = match result {
            editor_groups::OpenResult::Opened { group, evicted } => {
                if let Some(evicted) = evicted {
                    self.buffers.evict(&evicted);
                }
                group
            }
            editor_groups::OpenResult::FocusedExisting(group) => group,
        };
        self.editor_groups.active = owner;
        let visible: Vec<PathBuf> = self
            .editor_groups
            .visible_paths()
            .into_iter()
            .filter(|path| Some(*path) != self.config_path().as_deref())
            .map(Path::to_path_buf)
            .collect();
        let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
        match self.buffers.get_or_open_pinned(&path, &pins) {
            Ok(viewer) => {
                viewer.clear_search_match();
                info!("viewer: opened {}", path.display());
                self.editor_groups.groups[owner].error = None;
            }
            Err(e) => {
                error!("viewer: {e}");
                self.editor_groups.groups[owner].error = Some(e.to_string());
            }
        }
        self.pane = PaneMode::Viewer;
        self.activity = frame::ActivityPanel::Explorer;
        self.term_focused = false;
        self.commit.message.focused = false;
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Open a search result and jump to its 1-based matching line while keeping
    /// the Search sidebar active.
    fn open_search_result(&mut self, path: PathBuf, line: usize, group: usize) {
        let activity = self.activity;
        if config::is_editable(&self.root, &path, &self.settings.editable) {
            let query = self.search.input.text.trim().to_owned();
            self.open_config(path);
            if let Some(editor) = &mut self.config_edits {
                editor.select_search_match(line, &query);
            }
            self.activity = activity;
            self.search.input.focused = false;
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        self.open_viewer(path.clone(), group);
        self.activity = activity;
        self.search.input.focused = false;
        let viewport_lines = self
            .app_layout()
            .map(|layout| {
                (layout.editor_groups[group.min(layout.editor_group_count.saturating_sub(1))].body.h
                    / viewer::LINE_H)
                    .floor() as usize
            })
            .unwrap_or(1);
        let visible: Vec<PathBuf> = self
            .editor_groups
            .visible_paths()
            .into_iter()
            .filter(|path| Some(*path) != self.config_path().as_deref())
            .map(Path::to_path_buf)
            .collect();
        let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
        let query = self.search.input.text.trim().to_owned();
        if let Ok(viewer) = self.buffers.get_or_open_pinned(&path, &pins) {
            viewer.set_offset_lines(line.saturating_sub(1), viewport_lines.max(1));
            viewer.set_search_match(line, &query);
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn focus_editor_group(&mut self, group: usize) {
        if group >= self.editor_groups.groups.len() {
            return;
        }
        self.editor_groups.active = group;
        self.sync_active_pane();
        self.activity = if self.pane == PaneMode::Config {
            frame::ActivityPanel::Config
        } else {
            frame::ActivityPanel::Explorer
        };
        self.term_focused = false;
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    fn toggle_editor_split(&mut self) {
        if !self.flush_config_autosave() {
            return;
        }
        let was_split = self.editor_groups.is_split();
        if !was_split {
            if let Some(layout) = self.app_layout() {
                if layout.editor_card.w < 640.0 {
                    return;
                }
            }
        }
        let closed = self.editor_groups.toggle_split();
        let closed_config = self
            .config_path()
            .is_some_and(|path| closed.contains(&path));
        self.evict_group_tabs(closed);
        if closed_config {
            let _ = self.close_config();
        }
        if was_split {
            self.editor_groups.active = 0;
        }
        self.sync_active_pane();
        self.activity = if self.pane == PaneMode::Config {
            frame::ActivityPanel::Config
        } else {
            frame::ActivityPanel::Explorer
        };
        self.term_focused = false;
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    fn toggle_markdown_preview(&mut self) {
        if !self.active_path().is_some_and(|path| markdown::is_markdown(&path)) {
            return;
        }
        self.markdown.preview = !self.markdown.preview;
        self.markdown.scroll = 0.0;
        self.damage.mark_full();
        self.request_redraw();
    }

    fn close_editor_group(&mut self, group: usize) {
        if group != 1 || !self.editor_groups.is_split() {
            return;
        }
        if !self.flush_config_autosave() {
            return;
        }
        let closed = self.editor_groups.close_secondary_group();
        let closed_config = self
            .config_path()
            .is_some_and(|path| closed.contains(&path));
        self.evict_group_tabs(closed);
        if closed_config {
            let _ = self.close_config();
        }
        self.sync_active_pane();
        self.activity = if self.pane == PaneMode::Config {
            frame::ActivityPanel::Config
        } else {
            frame::ActivityPanel::Explorer
        };
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close the active group's tab. Returns false when both groups are empty.
    fn close_active_editor_tab(&mut self) -> bool {
        let group = self.active_group_index();
        let Some(path) = self
            .editor_groups
            .groups
            .get(group)
            .and_then(|group| group.active.clone())
        else {
            return false;
        };
        if Some(path.as_path()) == self.config_path().as_deref() {
            return self.close_config();
        } else {
            self.close_tab(group, path);
        }
        true
    }

    /// Select a tab in its owning group. Config paths enter the enforced editor.
    fn select_tab(&mut self, group: usize, path: PathBuf) {
        self.markdown.preview = false;
        self.markdown.scroll = 0.0;
        let Some(owner) = self.editor_groups.select(group, &path) else {
            return;
        };
        if Some(path.as_path()) == self.config_path().as_deref() {
            self.pane = PaneMode::Config;
            self.activity = frame::ActivityPanel::Config;
        } else {
            let visible: Vec<PathBuf> = self
                .editor_groups
                .visible_paths()
                .into_iter()
                .filter(|path| Some(*path) != self.config_path().as_deref())
                .map(Path::to_path_buf)
                .collect();
            let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
            if let Err(e) = self.buffers.get_or_open_pinned(&path, &pins) {
                error!("viewer: {e}");
                self.editor_groups.groups[owner].error = Some(e.to_string());
            }
            self.pane = PaneMode::Viewer;
            self.activity = frame::ActivityPanel::Explorer;
            self.request_diff_refresh();
        }
        self.term_focused = false;
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close one tab and evict its viewer after the last group reference.
    fn close_tab(&mut self, group: usize, path: PathBuf) {
        if Some(path.as_path()) == self.config_path().as_deref() {
            let _ = self.close_config();
            return;
        }
        if self.editor_groups.close_tab(group, &path).is_none() {
            return;
        }
        self.buffers.evict(&path);
        self.sync_active_pane();
        self.activity = if self.pane == PaneMode::Config {
            frame::ActivityPanel::Config
        } else {
            frame::ActivityPanel::Explorer
        };
        self.request_diff_refresh();
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Open a whitelisted config file as a tab in the active editor group.
    fn open_config(&mut self, path: PathBuf) {
        if !self.flush_config_autosave() {
            return;
        }
        self.config_autosave_at = None;
        self.markdown.preview = false;
        self.markdown.scroll = 0.0;
        if let Some(old) = self.config_path() {
            for group in &mut self.editor_groups.groups {
                if let Some(index) = group.tabs.iter().position(|tab| tab == &old) {
                    group.tabs.remove(index);
                    if group.active.as_ref() == Some(&old) {
                        group.active = group.tabs.get(index).or_else(|| index.checked_sub(1).and_then(|i| group.tabs.get(i))).cloned();
                    }
                }
            }
        }
        if let editor_groups::OpenResult::Opened { evicted: Some(evicted), .. } =
            self.editor_groups.open(path.clone())
        {
            self.buffers.evict(&evicted);
        }
        self.buffers.evict(&path);
        self.config_error_path = Some(path.clone());
        match ConfigEditor::open(&self.root, &path, &self.settings.editable) {
            Ok(editor) => {
                info!("config editor: {} ({:?})", path.display(), editor.kind());
                self.config_edits = Some(editor);
                self.config_error = None;
                self.config_error_path = None;
            }
            Err(e) => {
                error!("config editor: {e}");
                self.config_edits = None;
                self.config_error = Some(e.to_string());
            }
        }
        self.pane = PaneMode::Config;
        self.activity = frame::ActivityPanel::Config;
        self.term_focused = false;
        self.commit.message.focused = false;
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close the config tab and return focus to the group's remaining tab.
    /// Pending edits are flushed first; a failed save keeps the editor open.
    fn close_config(&mut self) -> bool {
        if !self.flush_config_autosave() {
            return false;
        }
        self.config_autosave_at = None;
        let path = self.config_path();
        if let Some(path) = path {
            for group in &mut self.editor_groups.groups {
                if let Some(index) = group.tabs.iter().position(|tab| tab == &path) {
                    group.tabs.remove(index);
                    if group.active.as_ref() == Some(&path) {
                        group.active = group.tabs.get(index).or_else(|| index.checked_sub(1).and_then(|i| group.tabs.get(i))).cloned();
                    }
                }
            }
        }
        self.config_edits = None;
        self.config_error = None;
        self.config_error_path = None;
        self.sync_active_pane();
        self.activity = if self.pane == PaneMode::Config {
            frame::ActivityPanel::Config
        } else {
            frame::ActivityPanel::Explorer
        };
        self.damage.mark_full();
        self.request_redraw();
        true
    }

    fn schedule_config_autosave(&mut self) {
        if self
            .config_edits
            .as_ref()
            .is_some_and(|editor| editor.is_dirty() && !editor.external_change())
        {
            self.config_autosave_at = Some(Instant::now() + CONFIG_AUTOSAVE_DEBOUNCE);
        } else {
            self.config_autosave_at = None;
        }
    }

    fn autosave_config_if_due(&mut self, now: Instant) -> bool {
        let Some(due) = self.config_autosave_at else {
            return false;
        };
        if now < due {
            return false;
        }
        let _ = self.flush_config_autosave();
        true
    }

    /// Flush pending edits now. Returns false when an external-change conflict
    /// or I/O error prevents a safe save; the editor stays open and dirty.
    fn flush_config_autosave(&mut self) -> bool {
        self.config_autosave_at = None;
        let Some(editor) = &mut self.config_edits else {
            return true;
        };
        if !editor.is_dirty() {
            return true;
        }
        if editor.changed_on_disk() {
            editor.refresh_if_clean();
            if editor.external_change() {
                info!("autosave paused: config changed externally at {}", editor.path().display());
                self.damage.mark_full();
                self.request_redraw();
                return false;
            }
        }
        self.save_config()
    }

    /// Save the open config through the enforced atomic boundary. Returns true
    /// when there was nothing to save or the atomic write succeeded.
    fn save_config(&mut self) -> bool {
        self.config_autosave_at = None;
        let mut saved_path = None;
        let mut ok = true;
        if let Some(editor) = &mut self.config_edits {
            match editor.save() {
                Ok(()) => {
                    saved_path = Some(editor.path().to_path_buf());
                    info!("config saved: {}", editor.path().display());
                }
                Err(e) => {
                    ok = false;
                    error!("config save failed: {e}");
                }
            }
        }
        // Editing ark.toml itself applies the new whitelist immediately.
        if ok && saved_path.as_deref() == Some(self.root.join("ark.toml").as_path()) {
            match config::load(&self.root) {
                Ok(settings) => {
                    self.settings = settings;
                    info!("ark.toml reloaded: {} editable pattern(s)", self.settings.editable.len());
                }
                Err(e) => error!("ark.toml reload failed (keeping old whitelist): {e}"),
            }
        }
        self.damage.mark_full();
        self.request_redraw();
        ok
    }

    fn reload_config(&mut self) {
        self.config_autosave_at = None;
        let Some(editor) = &mut self.config_edits else {
            return;
        };
        match editor.reload() {
            Ok(()) => info!("config reloaded: {}", editor.path().display()),
            Err(e) => error!("config reload failed: {e}"),
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    fn reset_caret_blink(&mut self) {
        self.caret_blink.reset(Instant::now());
        self.apply_caret_visibility();
    }

    fn apply_caret_visibility(&mut self) {
        let visible = self.caret_blink.visible();
        self.search.input.caret_visible = visible;
        self.commit.message.caret_visible = visible;
        if let Some(prompt) = &mut self.new_item {
            prompt.input.caret_visible = visible;
        }
        if let Some(editor) = &mut self.config_edits {
            editor.set_caret_visible(visible);
        }
        self.terms.set_caret_visible(visible);
    }

    fn has_focused_caret(&self) -> bool {
        (self.terms.is_open() && self.term_focused)
            || self.search.input.focused
            || self.commit.message.focused
            || self.new_item.as_ref().is_some_and(|prompt| prompt.input.focused)
            || (self.pane == PaneMode::Config && self.config_edits.is_some())
    }

    fn advance_caret_blink(&mut self, now: Instant) -> bool {
        if !self.caret_blink.tick(now) {
            return false;
        }
        if !self.has_focused_caret() {
            self.caret_blink.stop();
        }
        self.apply_caret_visibility();
        if self.caret_blink.due().is_some() {
            self.damage.mark_full();
            self.request_redraw();
            true
        } else {
            false
        }
    }

    /// Keep the window IME state in sync with the config pane (M9).
    fn sync_ime(&mut self) {
        let want = self.pane == PaneMode::Config || self.new_item.is_some();
        if want == self.ime_allowed {
            return;
        }
        if let Some(window) = &self.window {
            window.set_ime_allowed(want);
        }
        self.ime_allowed = want;
    }

    /// Pane to show after the last terminal tab closes (config wins if open).
    fn pane_after_terminals(&self) -> PaneMode {
        if (self.config_edits.is_some() || self.config_error.is_some())
            && self.config_path().as_deref() == self.editor_groups.active_path()
        {
            PaneMode::Config
        } else {
            PaneMode::Viewer
        }
    }

    /// M8: run an exact git action in a terminal tab so its output streams
    /// (the M6 PTY reader does the streaming — ark keeps no capture buffer).
    /// Returns whether the tab opened.
    fn run_git_command(&mut self, label: &str, command: &str) -> bool {
        self.layout_visibility.terminal = true;
        let Some(proxy) = self.proxy.clone() else {
            error!("{label}: cannot run without an event-loop proxy");
            self.commit.error = Some(format!("{label}: no event loop"));
            self.damage.mark_full();
            self.request_redraw();
            return false;
        };
        let Some((grid, advance)) = self.terminal_grid_for_new_tab() else {
            return false;
        };
        let size = term::GridSize::fit(grid.w, grid.h, advance, viewer::LINE_H);
        match self.terms.open_command(&self.root, label, command, size, proxy) {
            Ok(()) => {
                info!("{label}: command tab opened (output streams there)");
                self.activity = frame::ActivityPanel::Terminal;
                self.term_focused = true;
                self.commit.message.focused = false;
                self.damage.mark_full();
                self.request_redraw();
                true
            }
            Err(e) => {
                error!("{label}: command tab failed: {e}");
                self.commit.error = Some(format!("{label}: {e}"));
                self.damage.mark_full();
                self.request_redraw();
                false
            }
        }
    }

    fn copy_terminal_selection(&mut self) -> bool {
        let Some(text) = self.terms.active().and_then(|term| term.selection_text()) else {
            return false;
        };
        match clipboard::copy_text(&text) {
            Ok(()) => {
                info!("copied {} terminal chars", text.chars().count());
                true
            }
            Err(e) => {
                error!("terminal copy failed: {e}");
                false
            }
        }
    }

    /// M8 Commit: validate the panel, then stage the checked paths and commit
    /// with the typed message in a streaming terminal tab.
    fn commit_now(&mut self) {
        let Some(status) = self.git.clone() else {
            self.commit.error = Some("not a git repository".into());
            self.damage.mark_full();
            self.request_redraw();
            return;
        };
        match self.commit.commit_command(&status) {
            Ok(command) => {
                let n = self.commit.selected_paths(&status).len();
                if self.run_git_command("git commit", &command) {
                    info!("commit: staged {n} selected path(s)");
                    self.commit.message.text.clear();
                    self.commit.message.cursor = 0;
                    self.commit.error = None;
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
            Err(e) => {
                // No silent failure: the panel title shows the reason.
                error!("commit blocked: {e}");
                self.commit.error = Some(e.to_string());
                self.damage.mark_full();
                self.request_redraw();
            }
        }
    }

    /// M8 Push: `git push` in a streaming terminal tab.
    fn push_now(&mut self) {
        if self.git.is_none() {
            self.commit.error = Some("not a git repository".into());
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        self.run_git_command("git push", git::PUSH_COMMAND);
    }

    /// Encode + write a mouse report at a logical position when the running
    /// app has enabled mouse reporting. Returns whether a report was sent.
    fn term_mouse_report_at(
        &mut self,
        state: ElementState,
        x: f64,
        y: f64,
        grid: Rect,
        advance: f64,
    ) -> bool {
        if !grid.contains(x, y) || !(advance > 0.0 && advance.is_finite()) {
            return false;
        }
        let Some(term) = self.terms.active_mut() else {
            return false;
        };
        if !term.mouse_reporting_enabled() {
            return false;
        }
        let mode = term.mode();
        let size = term.size();
        let col = ((x - grid.x) / advance)
            .floor()
            .clamp(0.0, size.cols.saturating_sub(1) as f64) as usize;
        let row = ((y - grid.y) / term::TERM_LINE_H)
            .floor()
            .clamp(0.0, size.rows.saturating_sub(1) as f64) as usize;
        let press = matches!(state, ElementState::Pressed);
        let Some(bytes) = term::encode_mouse_report(&mode, 0, col, row, !press) else {
            return false;
        };
        term.key_input(&bytes);
        true
    }

    /// PTY data ready: drain every tab (bounded), paint the terminal area at
    /// the frame budget (~30fps under output storms).
    fn on_pty_data(&mut self, event_loop: &ActiveEventLoop) {
        let parsed = self.terms.drain_all(64);
        if !parsed {
            return;
        }
        // Hidden terminals keep their scrollback current but do not schedule
        // paints for a surface that is not visible.
        if !self.layout_visibility.terminal || !self.terms.is_open() {
            return;
        }
        self.terminal_dirty = true;
        let now = Instant::now();
        let due = self.last_render.is_none_or(|t| now.duration_since(t) >= TERM_FRAME_BUDGET);
        if due {
            self.term_paint_due = false;
            self.request_redraw();
        } else {
            // Stay within the frame budget: schedule the paint.
            self.term_paint_due = true;
            if let Some(t) = self.last_render {
                event_loop.set_control_flow(ControlFlow::WaitUntil(t + TERM_FRAME_BUDGET));
            }
        }
    }

    /// Handle one coalesced fs-activity signal (M5). The open viewer
    /// refreshes immediately (leading-edge stat check — also minimizes the
    /// truncated-mmap exposure window); the tree re-list is debounced to the
    /// next due point (`refresh_tree_if_due`, completed in `about_to_wait`).
    /// Returns whether the viewer changed.
    fn process_fs_change(&mut self) -> bool {
        self.fs_dirty = true;
        self.request_git_refresh();
        let mut viewer_changed = false;
        let mut config_conflict = false;
        if let Some(ed) = &mut self.config_edits {
            if ed.refresh_if_clean() {
                config_conflict = ed.external_change();
                if config_conflict {
                    info!("config changed on disk while dirty (edits kept)");
                } else {
                    info!("config reloaded (changed on disk): {}", ed.path().display());
                }
                viewer_changed = true;
            }
        }
        if config_conflict {
            self.config_autosave_at = None;
        }
        let config_path = self.config_path();
        let visible: Vec<PathBuf> = self
            .editor_groups
            .visible_paths()
            .into_iter()
            .filter(|path| Some(*path) != config_path.as_deref())
            .map(Path::to_path_buf)
            .collect();
        let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
        for path in &visible {
            let disk_changed = self
                .buffers
                .get_or_open_pinned(path, &pins)
                .map(|viewer| viewer.changed_on_disk())
                .unwrap_or(false);
            if disk_changed {
                match self.buffers.refresh_pinned(path, &pins) {
                    Ok(_) => {
                        info!("viewer refreshed: {}", path.display());
                        self.flash_until = Some(Instant::now() + FLASH_DURATION);
                        self.flash_path = Some(path.clone());
                        viewer_changed = true;
                        self.request_diff_refresh();
                    }
                    Err(e) => {
                        error!("viewer refresh failed (keeping old view): {e}");
                    }
                }
            }
        }
        viewer_changed
    }

    /// Re-list the tree if the debounce window has elapsed and fs activity
    /// happened since the last refresh. Returns whether it ran.
    fn refresh_tree_if_due(&mut self, now: Instant) -> bool {
        if !self.fs_dirty {
            return false;
        }
        let due = self
            .last_tree_refresh
            .map(|t| now.duration_since(t) >= TREE_REFRESH_DEBOUNCE)
            .unwrap_or(true);
        if !due {
            return false;
        }
        self.fs_dirty = false;
        self.last_tree_refresh = Some(now);
        self.tree.refresh();
        self.damage.mark_full();
        self.request_redraw();
        true
    }

    /// Schedule wake-ups for the pending follow-ups (flash clear + debounced
    /// tree refresh).
    fn schedule_followups(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let mut earliest: Option<Instant> = self.flash_until;
        if let Some(due) = self.auto_update_check_at {
            earliest = Some(earliest.map_or(due, |d| d.min(due)));
        }
        if self.fs_dirty {
            let at = self.last_tree_refresh.unwrap_or(now) + TREE_REFRESH_DEBOUNCE;
            earliest = Some(earliest.map_or(at, |d| d.min(at)));
        }
        if let Some((_, due)) = &self.search_debounce {
            earliest = Some(earliest.map_or(*due, |d| d.min(*due)));
        }
        if let Some(due) = self.config_autosave_at {
            earliest = Some(earliest.map_or(due, |d| d.min(due)));
        }
        if let Some(due) = self.caret_blink.due() {
            earliest = Some(earliest.map_or(due, |d| d.min(due)));
        }
        if self.term_paint_due {
            if let Some(at) = self.last_render.map(|t| t + TERM_FRAME_BUDGET) {
                earliest = Some(earliest.map_or(at, |d| d.min(at)));
            }
        }
        if now.checked_duration_since(self.last_activity).is_none_or(|d| d < memory::TRIM_IDLE) {
            let at = self.last_activity + memory::TRIM_IDLE;
            earliest = Some(earliest.map_or(at, |d| d.min(at)));
        }
        if now.checked_duration_since(self.last_activity).is_some_and(|d| d < HANDLE_PULSE) {
            let at = now + Duration::from_millis(33);
            earliest = Some(earliest.map_or(at, |d| d.min(at)));
        }
        if let Some(d) = earliest {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // already bootstrapped
        }
        let attrs = Window::default_attributes()
            .with_title(format!("ark — {}", self.title))
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(400.0, 300.0));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!("window creation failed: {e}");
                event_loop.exit();
                return;
            }
        };
        let context = match Context::new(window.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("softbuffer context creation failed: {e}");
                event_loop.exit();
                return;
            }
        };
        match Surface::new(&context, window.clone()) {
            Ok(surface) => {
                self.set_device_scale(window.scale_factor());
                info!(
                    "window ready (root: {}, scale {:.2}x, zoom {:.2}x)",
                    self.root.display(),
                    self.scale,
                    self.zoom
                );
                self.context = Some(context);
                self.surface = Some(surface);
                self.window = Some(window);
                self.damage.mark_full();
                self.request_redraw();
                // M7: status on open — dispatched here (the loop is live);
                // send_event before run_app is dropped by winit.
                self.request_git_refresh();
                self.schedule_auto_update_check(event_loop);
                // Catch fs events that fired during startup (their wake was
                // coalesced away before the loop existed).
                if self.watcher.as_ref().map(Watcher::take_pending).unwrap_or(false) {
                    if self.process_fs_change() {
                        self.damage.mark_full();
                    }
                    self.schedule_followups(event_loop);
                }
            }
            Err(e) => {
                error!("surface creation failed: {e}");
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        self.last_activity = Instant::now();
        match event {
            Wake::FsChanged => {
                let changed = self.watcher.as_ref().map(Watcher::take_pending).unwrap_or(false);
                if !changed {
                    return;
                }
                if self.process_fs_change() {
                    self.damage.mark_full();
                }
                // Schedule the flash-clear frame (and any debounced tree refresh).
                self.schedule_followups(event_loop);
                self.request_redraw();
            }
            Wake::PtyData => {
                self.on_pty_data(event_loop);
                self.reset_caret_blink();
            }
            Wake::SearchFinished(report) => self.apply_search_report(*report),
            Wake::GitStatus(status) => self.apply_git_status(status),
            Wake::GitDiff { path, diff } => {
                // Stale-diff guard: only the currently open file applies.
                if self.active_path().as_deref() == Some(path.as_path()) {
                    self.file_diff = Some((path, diff));
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
            Wake::UpdateEvent { generation, event } => {
                if !matches!(event.as_ref(), update_protocol::UpdateEvent::Progress(_)) {
                    info!("update helper event: {event:?}");
                }
                if self.updater.handle_event(generation, *event) {
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
            Wake::UpdateFinished {
                generation,
                command,
                success,
                error,
            } => {
                if self.updater.finish(generation, command, success, error) {
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.pending_exit {
            info!("smoke present ok — exiting");
            event_loop.exit();
            return;
        }
        if self.restart_requested {
            info!("restart requested — exiting current ark process");
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        self.start_auto_update_if_due(now);
        self.start_search_if_due(now);
        if self.search_debounce.is_some() {
            self.schedule_followups(event_loop);
        }
        self.autosave_config_if_due(now);
        self.advance_caret_blink(now);
        if self.config_autosave_at.is_some() || self.caret_blink.due().is_some() {
            self.schedule_followups(event_loop);
        }
        // M11: return free heap pages after a quiet period (glibc only).
        if memory::should_trim(now, self.last_activity, self.last_trim) {
            let trimmed = memory::trim_heap();
            self.last_trim = Some(now);
            if let Some(r) = memory::report() {
                if std::env::var_os("ARK_MEM_REPORT").is_some() {
                    info!(
                        "memory: trimmed={trimmed} rss={} KiB hwm={} KiB pss={} KiB",
                        r.rss_kib, r.hwm_kib, r.pss_kib
                    );
                } else {
                    log::debug!(
                        "memory: trimmed={trimmed} rss={} KiB hwm={} KiB pss={} KiB",
                        r.rss_kib, r.hwm_kib, r.pss_kib
                    );
                }
            }
        } else if now
            .checked_duration_since(self.last_activity)
            .is_none_or(|d| d < memory::TRIM_IDLE)
        {
            // Wake for the idle deadline; any earlier follow-up scheduling
            // below overrides this to its own (sooner) deadline.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.last_activity + memory::TRIM_IDLE,
            ));
        }
        // Flash clear frame: one full redraw without the strip, then back to
        // waiting for events.
        if let Some(d) = self.flash_until {
            if now >= d {
                self.flash_until = None;
                self.flash_path = None;
                self.damage.mark_full();
                self.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(d));
            }
        }
        // Debounced tree refresh due → run it (and reschedule if still dirty).
        if self.refresh_tree_if_due(now) || self.fs_dirty {
            self.schedule_followups(event_loop);
        }
        // Deferred terminal paint due → raster now.
        if self.term_paint_due {
            let due = self.last_render.is_none_or(|t| now.duration_since(t) >= TERM_FRAME_BUDGET);
            if due {
                self.term_paint_due = false;
                self.request_redraw();
            } else if let Some(t) = self.last_render {
                event_loop.set_control_flow(ControlFlow::WaitUntil(t + TERM_FRAME_BUDGET));
            }
        }
        // Sidebar-handle pulse: keep painting for a short window after input,
        // then stop automatically. Redraw events themselves do not extend it.
        if now
            .checked_duration_since(self.last_activity)
            .is_some_and(|elapsed| elapsed < HANDLE_PULSE)
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(33)));
            self.request_redraw();
        }
        // Reapply the earliest deadline after all follow-ups so a later idle
        // wake-up cannot postpone a due caret/autosave/terminal repaint.
        self.schedule_followups(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.last_activity = Instant::now();
        }
        match event {
            WindowEvent::CloseRequested => {
                if !self.flush_config_autosave() {
                    info!("close cancelled: config auto-save needs attention");
                    return;
                }
                info!("close requested — exiting");
                event_loop.exit();
            }
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.set_device_scale(scale_factor);
                self.damage.mark_full();
                info!(
                    "scale factor changed to {:.2}x (ui zoom {:.2}x)",
                    self.scale, self.zoom
                );
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Full repaint on pointer movement: hover backgrounds and the
                // split divider react to position; simple and correct in M3.
                let logical = position.to_logical(self.scale);
                self.pointer.x = logical.x;
                self.pointer.y = logical.y;
                if self.term_drag.dragging {
                    if let Some(layout) = self.app_layout() {
                        if self.resize_terminal_to(logical.y, &layout) {
                            self.mark_terminal_resize_damage(&layout);
                        }
                    }
                    // One request per event-loop turn, not per raw pointer
                    // event. The PTY itself stays at its pre-drag size.
                    if self.take_terminal_resize_redraw() {
                        self.request_redraw();
                    }
                    return;
                }
                if self.terminal_mouse_press.is_some() {
                    if let Some(layout) = self.app_layout() {
                        let advance = self.terminal_advance(layout.ui_scale);
                        if self.update_terminal_mouse_selection(layout.term_grid, advance) {
                            let (cw, ch) = (self.canvas.width(), self.canvas.height());
                            self.damage.mark(damage_rect(layout.term_grid, self.scale, cw, ch));
                            self.request_redraw();
                        }
                    }
                    return;
                }
                // Full repaint on pointer movement: hover backgrounds and the
                // split divider react to position; simple and correct in M3.
                self.damage.mark_full();
                self.request_redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed {
                    self.reset_caret_blink();
                }
                // Terminal grid: left presses wait for release so a plain
                // drag can select locally even when the app owns the mouse.
                // A click is then reported as press+release; other buttons
                // retain immediate mouse reporting.
                let layout = self.app_layout();
                let over_commit = self.activity == frame::ActivityPanel::SourceControl
                    && layout
                        .as_ref()
                        .and_then(|layout| {
                            frame::source_control_panel_for_group(
                                layout,
                                self.active_group_index(),
                            )
                        })
                        .is_some_and(|rect| rect.contains(self.pointer.x, self.pointer.y));
                let over_grid = layout
                    .as_ref()
                    .map(|l| l.term_grid.contains(self.pointer.x, self.pointer.y))
                    .unwrap_or(false);
                if state == ElementState::Pressed && !over_grid {
                    self.terminal_mouse_press = None;
                    if let Some(term) = self.terms.active_mut() {
                        term.clear_selection();
                    }
                }
                if state == ElementState::Pressed
                    && layout.as_ref().is_some_and(|l| {
                        l.terminal_resize.contains(self.pointer.x, self.pointer.y)
                    })
                {
                    if let Some(layout) = layout.as_ref() {
                        self.begin_terminal_resize(layout);
                    }
                }
                let terminal_drag_finished = state == ElementState::Released
                    && self.finish_terminal_resize(layout.as_ref());
                if over_commit {
                    // A commit-panel click never leaves the shell keyboard-
                    // focused (the message field / buttons own this surface).
                    self.term_focused = false;
                }
                if self.new_item.is_some() {
                    self.term_focused = false;
                    self.commit.message.focused = false;
                }
                if self.pane == PaneMode::Config {
                    // Config-pane clicks (textarea/env form) never focus the
                    // hidden shell either.
                    self.term_focused = false;
                    self.commit.message.focused = false;
                }
                if self.terms.is_open() {
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            self.terminal_mouse_press = None;
                            if over_grid {
                                if let Some(layout) = layout.as_ref() {
                                    let advance = self.terminal_advance(layout.ui_scale);
                                    self.begin_terminal_mouse_press(layout.term_grid, advance);
                                }
                            }
                        } else {
                            if let Some(layout) = layout.as_ref() {
                                let advance = self.terminal_advance(layout.ui_scale);
                                self.finish_terminal_mouse_press(layout.term_grid, advance);
                            } else {
                                self.terminal_mouse_press = None;
                                if let Some(term) = self.terms.active_mut() {
                                    if term.is_selecting() {
                                        term.end_selection();
                                    }
                                }
                            }
                        }
                    } else if over_grid {
                        if let Some(layout) = layout.as_ref() {
                            let advance = self.terminal_advance(layout.ui_scale);
                            self.term_mouse_report_at(
                                state,
                                self.pointer.x,
                                self.pointer.y,
                                layout.term_grid,
                                advance,
                            );
                        }
                    }
                }
                if self.terms.is_open() && over_grid {
                    self.term_focused = true;
                    self.commit.message.focused = false;
                }
                match state {
                    ElementState::Pressed => self.pointer.down = true,
                    ElementState::Released => {
                        // A completed press→release arms `pointer.click` for
                        // the next render pass (render clears it). A click
                        // completing a double-click (same spot, inside the
                        // time window) arms `pending_dbl` alongside.
                        let was_down = self.pointer.down;
                        self.pointer.down = false;
                        if was_down && self.pointer_in_window() {
                            self.pointer.click = true;
                            let now = Instant::now();
                            let (x, y) = (self.pointer.x, self.pointer.y);
                            self.pending_dbl = match self.last_click {
                                Some((t, lx, ly)) => {
                                    now.duration_since(t).as_millis() < DBL_CLICK_MS
                                        && (lx - x).abs() < DBL_CLICK_SLOP
                                        && (ly - y).abs() < DBL_CLICK_SLOP
                                }
                                None => false,
                            };
                            self.last_click = Some((now, x, y));
                        }
                    }
                }
                if terminal_drag_finished {
                    if let Some(layout) = layout.as_ref() {
                        self.mark_terminal_resize_damage(layout);
                    }
                } else {
                    self.damage.mark_full();
                }
                self.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.modifiers.state().control_key() {
                    if let Some(step) = zoom::wheel_step(&delta) {
                        self.adjust_zoom(step);
                    }
                    return;
                }
                let Some(dy) = wheel_scroll_px(&delta) else { return };
                let Some(layout) = self.app_layout() else { return };
                // Wheel scrolling applies over the whole panel under the
                // pointer (its scrollbar track scrolls too, matching editors).
                let (x, y) = (self.pointer.x, self.pointer.y);
                if self.activity == frame::ActivityPanel::Search {
                    let rect = if layout.explorer_card.w > 0.0 {
                        layout.explorer_card
                    } else {
                        layout.editor_groups[self
                            .active_group_index()
                            .min(layout.editor_group_count.saturating_sub(1))]
                        .body
                    };
                    if rect.contains(x, y) && self.search.scroll_by(dy) {
                        let (cw, ch) = (self.canvas.width(), self.canvas.height());
                        self.damage.mark(damage_rect(rect, self.scale, cw, ch));
                        self.request_redraw();
                    }
                    return;
                }
                // Source Control owns the wheel while its card is active,
                // whether it is hosted in the sidebar or compact editor body.
                if self.activity == frame::ActivityPanel::SourceControl {
                    if let Some(rect) = frame::source_control_panel_for_group(
                        &layout,
                        self.active_group_index(),
                    )
                    .filter(|rect| rect.contains(x, y))
                    {
                        if self.git.is_some() {
                            let total = self.git.as_ref().map_or(0, |g| g.entries.len());
                            if route_wheel_commit(&mut self.commit, dy, rect, total) {
                                let (cw, ch) = (self.canvas.width(), self.canvas.height());
                                let damage = damage_rect(rect, self.scale, cw, ch);
                                self.damage.mark(damage);
                                self.request_redraw();
                            }
                        }
                        return;
                    }
                }
                // Config wheel routing follows the group under the pointer.
                if self.pane == PaneMode::Config {
                    for (index, group_layout) in layout
                        .editor_groups
                        .iter()
                        .take(layout.editor_group_count)
                        .enumerate()
                    {
                        if !group_layout.body.contains(x, y) {
                            continue;
                        }
                        let is_config = self
                            .editor_groups
                            .groups
                            .get(index)
                            .and_then(|group| group.active.as_deref())
                            == self.config_path().as_deref();
                        if is_config {
                            if let Some(editor) = &mut self.config_edits {
                                if editor.scroll_by(dy) {
                                    let (cw, ch) = (self.canvas.width(), self.canvas.height());
                                    let rect = damage_rect(group_layout.body, self.scale, cw, ch);
                                    self.damage.mark(rect);
                                    self.request_redraw();
                                }
                            }
                            return;
                        }
                    }
                }
                if self.markdown.preview
                    && self.active_path().is_some_and(|path| markdown::is_markdown(&path))
                {
                    let group = self
                        .active_group_index()
                        .min(layout.editor_group_count.saturating_sub(1));
                    let rect = layout.editor_groups[group].body;
                    if rect.contains(x, y) && self.markdown.scroll_by(dy) {
                        let (cw, ch) = (self.canvas.width(), self.canvas.height());
                        self.damage.mark(damage_rect(rect, self.scale, cw, ch));
                        self.request_redraw();
                    }
                    return;
                }
                let grid = layout.term_grid;
                let scrolled = if layout.tree_panel.contains(x, y) {
                    route_wheel_scroll(
                        &mut self.tree,
                        dy,
                        &layout,
                        self.scale,
                        &self.canvas,
                        &mut self.damage,
                    )
                } else if grid.contains(x, y) && self.terms.is_open() {
                    let handled = route_wheel_term(self, dy, &grid);
                    if handled {
                        // Scrollback and mouse-report-driven updates repaint
                        // only the terminal grid.
                        self.terminal_dirty = true;
                    }
                    handled
                } else if let Some(group) = layout
                    .editor_groups
                    .iter()
                    .take(layout.editor_group_count)
                    .position(|group| group.body.contains(x, y))
                {
                    self.route_wheel_viewer_group(
                        group,
                        layout.editor_groups[group].body,
                        dy,
                        self.modifiers.state().shift_key(),
                    )
                } else {
                    false
                };
                if scrolled {
                    self.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            }
            WindowEvent::Ime(ime) => {
                self.reset_caret_blink();
                if let Some(prompt) = &mut self.new_item {
                    if let winit::event::Ime::Commit(text) = ime {
                        if prompt.input.insert(&text) {
                            prompt.error = None;
                            self.damage.mark_full();
                            self.request_redraw();
                        }
                    }
                    return;
                }
                if self.activity == frame::ActivityPanel::Search && self.search.input.focused {
                    if let winit::event::Ime::Commit(text) = ime {
                        if self.search.input.insert(&text) {
                            self.request_search(self.search.input.text.clone());
                            self.damage.mark_full();
                            self.request_redraw();
                        }
                    }
                    return;
                }
                if self.pane != PaneMode::Config {
                    return;
                }
                let changed = match ime {
                    winit::event::Ime::Commit(text) => {
                        self.config_edits.as_mut().is_some_and(|e| e.on_ime_commit(&text))
                    }
                    winit::event::Ime::Preedit(text, _) => {
                        self.config_edits.as_mut().is_some_and(|e| e.on_ime_preedit(&text))
                    }
                    _ => false,
                };
                if changed {
                    self.schedule_config_autosave();
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                self.reset_caret_blink();
                if let Some(step) = zoom::key_step(&event.logical_key, self.modifiers.state()) {
                    self.adjust_zoom(step);
                    return;
                }
                if self.new_item.is_some() {
                    let mods = self.modifiers.state();
                    if mods.control_key()
                        && matches!(
                            &event.logical_key,
                            winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v")
                        )
                    {
                        let result = clipboard::paste_single_line(ui::TEXT_INPUT_MAX_CHARS)
                            .map(|text| {
                                let prompt = self.new_item.as_mut().expect("checked prompt");
                                let changed = prompt.input.insert_with_cap(&text, ui::TEXT_INPUT_MAX_CHARS);
                                if changed {
                                    prompt.error = None;
                                }
                                changed
                            });
                        match result {
                            Ok(_) => {}
                            Err(e) => {
                                if let Some(prompt) = &mut self.new_item {
                                    prompt.error = Some(format!("Clipboard paste failed: {e}"));
                                }
                                error!("new-item clipboard paste failed: {e}");
                            }
                        }
                        self.damage.mark_full();
                        self.request_redraw();
                        return;
                    }
                    let handled = self.handle_new_item_key(
                        &event.logical_key,
                        event.text.as_deref(),
                    );
                    if handled {
                        self.damage.mark_full();
                        self.request_redraw();
                    }
                    return;
                }
                if self.terms.menu_open {
                    self.terms.menu_open = false;
                    self.term_focused = self.terms.is_open();
                    self.damage.mark_full();
                    self.request_redraw();
                    return;
                }
                let ctrl = self.modifiers.state().control_key();
                // App-level shortcuts: work whether or not the shell has focus.
                if ctrl {
                    let state = self.modifiers.state();
                    if state.shift_key()
                        && matches!(
                            &event.logical_key,
                            winit::keyboard::Key::Character(s) if s == "`" || s == "~"
                        )
                    {
                        if state.alt_key() {
                            self.open_terminal_window();
                        } else {
                            self.open_terminal_tab();
                        }
                        return;
                    }
                    if let winit::keyboard::Key::Character(s) = &event.logical_key {
                        match s.as_str() {
                            "t" | "T" => {
                                self.open_terminal_tab();
                                return;
                            }
                            "w" | "W" => {
                                if self.close_active_editor_tab() {
                                    return;
                                }
                                self.close_terminal_tab();
                                return;
                            }
                            "s" | "S" if self.pane == PaneMode::Config => {
                                self.save_config();
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                // M8: the focused commit-message field consumes editing keys
                // (Escape blurs). It sits in the left panel and can hold focus
                // while a config editor is shown on the right.
                if self.commit.message.focused {
                    let is_paste = ctrl
                        && matches!(
                            &event.logical_key,
                            winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v")
                        );
                    if is_paste {
                        match clipboard::paste_single_line(ui::TEXT_INPUT_MAX_CHARS) {
                            Ok(text) => {
                                let changed = self.commit.message.insert_with_cap(
                                    &text,
                                    ui::TEXT_INPUT_MAX_CHARS,
                                );
                                if changed {
                                    self.commit.error = None;
                                }
                            }
                            Err(e) => {
                                self.commit.error = Some(format!("Clipboard paste failed: {e}"));
                                error!("commit clipboard paste failed: {e}");
                            }
                        }
                        self.damage.mark_full();
                        self.request_redraw();
                        return;
                    }
                    if self.commit.handle_key(&event.logical_key, event.text.as_deref()) {
                        self.damage.mark_full();
                        self.request_redraw();
                    }
                    return;
                }
                if self.activity == frame::ActivityPanel::Search && self.search.input.focused {
                    let is_paste = ctrl
                        && matches!(
                            &event.logical_key,
                            winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v")
                        );
                    if is_paste {
                        match clipboard::paste_single_line(ui::TEXT_INPUT_MAX_CHARS) {
                            Ok(text) => {
                                if self.search.input.insert_with_cap(&text, ui::TEXT_INPUT_MAX_CHARS) {
                                    self.search.scroll = 0.0;
                                    self.request_search(self.search.input.text.clone());
                                }
                            }
                            Err(e) => {
                                self.search.status = format!("Clipboard paste failed: {e}");
                                error!("search clipboard paste failed: {e}");
                            }
                        }
                        self.damage.mark_full();
                        self.request_redraw();
                        return;
                    }
                    if self.handle_search_key(&event.logical_key, event.text.as_deref()) {
                        self.damage.mark_full();
                        self.request_redraw();
                    }
                    return;
                }
                // M9: the config editor owns the keyboard while its pane is
                // active (Ctrl+Z/Y/A/C are handled inside).
                if self.pane == PaneMode::Config {
                    let mods = self.modifiers.state();
                    if let Some(editor) = &mut self.config_edits {
                        if editor.handle_key(&event.logical_key, event.text.as_deref(), &mods) {
                            self.schedule_config_autosave();
                            self.damage.mark_full();
                            self.request_redraw();
                        }
                    }
                    return;
                }
                // Focused terminal: every other key goes to the shell (Ctrl+C
                // included — it is SIGINT there, not copy).
                if self.terms.is_open() && self.term_focused {
                    let is_copy = ctrl
                        && self.modifiers.state().shift_key()
                        && matches!(
                            &event.logical_key,
                            winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("c")
                        );
                    if is_copy {
                        if !self.copy_terminal_selection() {
                            info!("nothing selected to copy from terminal");
                        }
                        return;
                    }
                    if let Some(term) = self.terms.active_mut() {
                        let mode = term.mode();
                        let is_paste = ctrl
                            && self.modifiers.state().shift_key()
                            && matches!(
                                &event.logical_key,
                                winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v")
                            );
                        if is_paste {
                            match clipboard::paste_text(term::MAX_PASTE_BYTES)
                                .and_then(|text| term::encode_paste(&mode, &text))
                            {
                                Ok(bytes) => term.key_input(&bytes),
                                Err(e) => error!("terminal clipboard paste failed: {e}"),
                            }
                            return;
                        }
                        let bytes = term::encode_key(
                            &event.logical_key,
                            event.text.as_deref(),
                            &self.modifiers.state(),
                            &mode,
                        );
                        if !bytes.is_empty() {
                            term.key_input(&bytes);
                        }
                    }
                    return;
                }
                // M4 keyboard surface: Ctrl+C copies the viewer selection.
                if !ctrl {
                    return;
                }
                let is_c = matches!(&event.logical_key, winit::keyboard::Key::Character(s) if s == "c");
                let is_v = matches!(&event.logical_key, winit::keyboard::Key::Character(s) if s == "v");
                if is_v {
                    info!("paste ignored: code and Markdown source are read-only");
                    return;
                }
                if !is_c {
                    return;
                }
                let copied = self.active_path().map(|path| {
                    let config_path = self.config_path();
                    let visible: Vec<PathBuf> = self
                        .editor_groups
                        .visible_paths()
                        .into_iter()
                        .filter(|path| Some(*path) != config_path.as_deref())
                        .map(Path::to_path_buf)
                        .collect();
                    let pins: Vec<&Path> = visible.iter().map(PathBuf::as_path).collect();
                    self.buffers
                        .get_or_open_pinned(&path, &pins)
                        .map_err(std::io::Error::other)
                        .and_then(|viewer| clipboard::copy_viewer_selection(&*viewer))
                });
                match copied {
                    Some(Ok(n)) => info!("copied {n} chars to the clipboard"),
                    Some(Err(e)) => error!("copy failed: {e}"),
                    None => info!("nothing selected to copy"),
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();

    let root: &Path = cli.path.as_deref().unwrap_or(Path::new("."));
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot open project path {}", root.display()))?;
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }

    info!("ark {} starting (root: {})", env!("CARGO_PKG_VERSION"), root.display());

    // Automated-verification hook: `ARK_SMOKE=1` exits after the first frame
    // is presented (see App::render / about_to_wait).
    let smoke = std::env::var("ARK_SMOKE").is_ok();
    if smoke {
        info!("smoke mode enabled — will exit after first present");
    }

    let event_loop = EventLoop::<Wake>::with_user_event()
        .build()
        .context("event loop creation")?;
    let proxy: EventLoopProxy<Wake> = event_loop.create_proxy();
    let watcher = match Watcher::new(&root, proxy.clone()) {
        Ok(w) => Some(w),
        Err(e) => {
            // Live refresh is a feature, not a dependency: log and continue.
            error!("fs watcher disabled (no live refresh): {e}");
            None
        }
    };
    let mut app = App::new(root, smoke, watcher, Some(proxy));
    event_loop.run_app(&mut app).context("event loop error")?;
    Ok(())
}

// Crate-root tests live in their own file (the 400-line gate).
#[cfg(test)]
mod tests;
