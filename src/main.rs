//! ark — ultra-lightweight agent-first code editor.
//!
//! M3: the lazy virtualized file tree panel in the real app frame. `frame.rs`
//! owns layout + scene composition; `tree.rs` owns the tree state model and
//! its viewport-only panel rendering. `App` routes winit events into the
//! frame: tree scroll marks panel-level damage (partial presents), everything
//! else is full. The M2 demo scene survives as test-only `demo.rs`. HiDPI:
//! layout in logical px, raster ×scale.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use clap::Parser;
use log::{error, info};
use softbuffer::{Context, Surface};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

mod commit;
mod config;
mod configed;
mod frame;
mod fs;
mod git;
mod highlight;
mod icons;
mod memory;
mod render;
mod state;
mod term;
mod text;
mod tree;
mod ui;
mod viewer;
mod watch;
/// Test-only M2 demo scene.
#[cfg(test)]
mod demo;

use frame::{AppLayout, Frame, SplitDrag};
use git::{FileDiff as GitFileDiff, GitStatus};
use highlight::Highlighter;
use render::{Canvas, Damage, DamageRect, Theme};
use state::BufferCache;
use alacritty_terminal::term::TermMode;
use text::{TextSystem, UI_FONT_SIZE};
use tree::FileTree;
use ui::{Pointer, Rect, Scrollbar, Split};
use viewer::{Viewer, LINE_H};
use watch::{Wake, Watcher};
use configed::ConfigEditor;

/// Logical px scrolled per wheel notch.
const WHEEL_PX: f64 = 40.0;
/// Viewer LRU capacity (memory rule: bounded open buffers).
const BUFFER_CACHE_CAP: usize = 8;
/// Double-click window (ms) and click slop (logical px).
const DBL_CLICK_MS: u128 = 400;
const DBL_CLICK_SLOP: f64 = 6.0;
/// M5 fs pipeline: tree re-lists coalesce to one per debounce window; the
/// flash is a bounded two-frame animation (ON for the deadline, one redraw
/// clears it).
const TREE_REFRESH_DEBOUNCE: Duration = Duration::from_millis(400);
const FLASH_DURATION: Duration = Duration::from_millis(900);
/// Terminal raster floor under output storms (~30fps).
const TERM_FRAME_BUDGET: Duration = Duration::from_millis(33);

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
/// ONLY the panel + status rects — header, split and viewer pane are
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
    damage.mark(damage_rect(layout.status, scale, cw, ch));
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
    /// The currently viewed file, if any (always `buffers`-resident).
    open_path: Option<PathBuf>,
    /// Most recent failed open attempt (rendered in the pane, logged).
    viewer_error: Option<String>,
    viewer_scrollbar: Scrollbar,
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
    /// Last full-scene raster (30fps terminal budget reference).
    last_render: Option<Instant>,
    /// A terminal-area repaint was deferred past the frame budget.
    term_paint_due: bool,
    /// "Changed on disk" flash deadline (cleared by a scheduled redraw).
    flash_until: Option<Instant>,
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
    /// Right-pane mode (M9): viewer vs hidden-terminal config editing.
    pane: PaneMode,
    /// Active workbench activity panel (M12; focus/highlight only).
    activity: frame::ActivityPanel,
    /// Human-writable whitelist from `ark.toml` (defaults on absence).
    settings: config::Config,
    /// The open config editor (one at a time; M9).
    config_edits: Option<ConfigEditor>,
    /// Most recent config-open failure (rendered in the pane).
    config_error: Option<String>,
    /// Window IME state (follows PaneMode::Config).
    ime_allowed: bool,
    /// Last input/fs/git/PTY activity (M11 idle trim gate).
    last_activity: Instant,
    /// Last successful idle heap trim (M11 min-interval gate).
    last_trim: Option<Instant>,
    /// A git status subprocess is running (one at a time).
    git_inflight: bool,
    /// fs activity arrived while the status was in flight.
    git_pending: bool,
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
            open_path: None,
            viewer_error: None,
            viewer_scrollbar: Scrollbar::default(),
            highlighter: Highlighter::new(),
            modifiers: winit::event::Modifiers::default(),
            last_click: None,
            pending_dbl: false,
            watcher,
            terms: term::TerminalTabs::default(),
            term_focused: false,
            last_render: None,
            term_paint_due: false,
            flash_until: None,
            last_tree_refresh: None,
            fs_dirty: false,
            git: None,
            file_diff: None,
            commit: commit::CommitPanel::default(),
            pane: PaneMode::Viewer,
            activity: frame::ActivityPanel::Explorer,
            settings,
            config_edits: None,
            config_error: None,
            ime_allowed: false,
            last_activity: Instant::now(),
            last_trim: None,
            git_inflight: false,
            git_pending: false,
            smoke,
            pending_exit: false,
        }
    }

    /// Request one redraw when a window exists.
    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
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
        Some(match &self.git {
            // M8: a repo root gets the commit panel under the tree; M12 keeps
            // the terminal dock visible whenever tabs exist.
            Some(_) => frame::app_layout_with_commit(
                size.width as f64 / self.scale,
                size.height as f64 / self.scale,
                self.split.position,
                true,
                true,
            ),
            None => frame::app_layout_with_commit(
                size.width as f64 / self.scale,
                size.height as f64 / self.scale,
                self.split.position,
                false,
                true,
            ),
        })
    }

    /// Terminal grid as if the dock were open (new tab spawn size).
    fn terminal_grid_for_new_tab(&self) -> Option<Rect> {
        let window = self.window.as_ref()?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some(
            frame::app_layout_with_commit(
                size.width as f64 / self.scale,
                size.height as f64 / self.scale,
                self.split.position,
                self.git.is_some(),
                true,
            )
            .term_grid,
        )
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
        // The config editor is the only IME surface (M9).
        self.sync_ime();
        // app_layout borrows all of self; it must finish before the surface
        // borrow below.
        let Some(layout) = self.app_layout() else { return };
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        // The window size is guaranteed non-zero here (app_layout checked).
        let size = window.inner_size();
        if self.surface_size != (size.width, size.height) {
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
        let Some(rects) = self.damage.take(self.canvas.width(), self.canvas.height()) else {
            return; // nothing changed since the last present
        };

        // Raster the full scene per damage wave (partial damage bounds the
        // PRESENT, not the raster). The frame consumes the armed click and
        // double-click flags exactly once.
        let pointer = self.pointer;
        self.pointer.click = false;
        let dbl = std::mem::take(&mut self.pending_dbl);
        // Tab strip order: LRU files (most-recent first) + the one config
        // editor. Bounded by the cache cap plus one.
        let mut open_files = self.buffers.lru_paths();
        if let Some(editor) = &self.config_edits {
            let config_path = editor.path().to_path_buf();
            if !open_files.contains(&config_path) {
                open_files.push(config_path);
            }
        }
        let active_path = match self.pane {
            PaneMode::Config => self.config_edits.as_ref().map(|e| e.path().to_path_buf()),
            _ => self.open_path.clone(),
        };
        // The open viewer (editor body) — borrowed from the LRU cache; the
        // open file is always most-recently-used, so the cache never evicts
        // it mid-frame. Terminal tabs no longer replace the editor body.
        let viewer = match (self.pane, self.open_path.as_deref()) {
            (PaneMode::Config, _) => None,
            (_, Some(p)) => self.buffers.get_or_open(Path::new(p)).ok(),
            (_, None) => None,
        };
        let terms = if self.terms.is_open() {
            Some(&mut self.terms)
        } else {
            None
        };
        let config = if self.pane == PaneMode::Config {
            self.config_edits.as_mut()
        } else {
            None
        };
        let config_error = if self.pane == PaneMode::Config {
            self.config_error.as_deref()
        } else {
            None
        };
        let viewer_error = self.viewer_error.as_deref();
        let git = self.git.as_deref();
        let file_diff = self
            .file_diff
            .as_ref()
            .filter(|(p, _)| Some(p) == self.open_path.as_ref())
            .map(|(_, d)| d.as_ref());
        let effects = frame::draw_frame(
            &mut Frame {
                canvas: &mut self.canvas,
                text: &mut self.text,
                theme: &self.theme,
                scale: self.scale,
                title: &self.title,
                root: &self.root,
                open_files: &open_files,
                active_path: active_path.as_deref(),
                activity: self.activity,
                pointer,
                dbl_click: dbl,
                viewer,
                viewer_error,
                viewer_flash: self.flash_until.is_some_and(|d| Instant::now() < d),
                highlighter: &self.highlighter,
                git,
                file_diff,
                commit: Some(&mut self.commit),
                config,
                config_error,
                terms,
                term_focused: self.term_focused,
            },
            &layout,
            &mut self.tree,
            &mut self.split,
            &mut self.split_drag,
            &mut self.scrollbar,
            &mut self.viewer_scrollbar,
        );
        // Post-raster damage for state changes made during the frame (the
        // marks schedule the next present).
        if effects.tree_changed {
            self.damage.mark_full();
        }
        if effects.scrolled {
            let (cw, ch) = (self.canvas.width(), self.canvas.height());
            self.damage.mark(damage_rect(layout.tree_panel, self.scale, cw, ch));
            self.damage.mark(damage_rect(layout.status, self.scale, cw, ch));
        }
        if effects.viewer_scrolled {
            let (cw, ch) = (self.canvas.width(), self.canvas.height());
            self.damage.mark(damage_rect(layout.viewer_pane, self.scale, cw, ch));
        }
        let mut need_redraw = false;
        let mut pending_config: Option<PathBuf> = None;
        if let Some(path) = effects.opened {
            if config::is_editable(&self.root, &path, &self.settings.editable) {
                // Whitelisted config → open the enforced editor AFTER the
                // present (the method needs &mut self, which the surface
                // borrow above still holds).
                pending_config = Some(path);
            } else {
                // Double-click open: goes through the LRU cache; a failure
                // (Io / binary refusal) surfaces in the pane and logs.
                match self.buffers.get_or_open(&path) {
                    Ok(_) => {
                        info!("viewer: opened {}", path.display());
                        self.open_path = Some(path);
                        self.viewer_error = None;
                        self.pane = PaneMode::Viewer;
                        self.activity = frame::ActivityPanel::Explorer;
                        self.term_focused = false;
                        self.commit.message.focused = false;
                    }
                    Err(e) => {
                        error!("viewer: {e}");
                        self.viewer_error = Some(e.to_string());
                        self.pane = PaneMode::Viewer;
                    }
                }
                self.damage.mark_full();
            }
            need_redraw = true;
        }

        // Blit + present (failures are logged, never silent).
        if render::present(surface, &self.canvas, &rects) && self.smoke {
            self.pending_exit = true;
        }
        self.last_render = Some(Instant::now());
        if effects.open_terminal {
            self.open_terminal_tab();
        }
        if effects.close_terminal {
            self.close_terminal_tab();
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
        if let Some(panel) = effects.set_activity {
            self.set_activity(panel);
        }
        if let Some(path) = effects.select_tab {
            self.select_tab(path);
        }
        if let Some(path) = effects.close_tab {
            self.close_tab(path);
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
        let Some(p) = self.open_path.clone() else {
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
        let Some(proxy) = self.proxy.clone() else {
            error!("terminal open failed: no wake proxy");
            return;
        };
        let Some(grid) = self.terminal_grid_for_new_tab() else {
            return;
        };
        let m = self.text.mono_metrics(UI_FONT_SIZE, UI_FONT_SIZE * 1.35);
        let advance = f64::from(m.advance);
        let size = term::GridSize::fit(grid.w, grid.h, advance, viewer::LINE_H);
        match self.terms.open(&self.root, size, proxy) {
            Ok(()) => {
                self.activity = frame::ActivityPanel::Terminal;
                self.term_focused = true;
                self.commit.message.focused = false;
                self.damage.mark_full();
                self.request_redraw();
            }
            Err(e) => error!("terminal open failed: {e}"),
        }
    }

    /// Close the active terminal tab and return focus to the remaining editor
    /// surface when the last shell is gone.
    fn close_terminal_tab(&mut self) {
        self.terms.close_active();
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
                if self.config_edits.is_some() {
                    self.pane = PaneMode::Config;
                }
                self.term_focused = false;
                self.commit.message.focused = false;
            }
            frame::ActivityPanel::Explorer => {
                if self.config_edits.is_some() || self.pane == PaneMode::Config {
                    self.pane = PaneMode::Viewer;
                }
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

    /// M12: select a file/config tab. Config paths enter the config editor;
    /// every other path opens through the LRU cache.
    fn select_tab(&mut self, path: PathBuf) {
        if Some(path.as_path()) == self.config_edits.as_ref().map(|e| e.path()) {
            self.pane = PaneMode::Config;
            self.activity = frame::ActivityPanel::Config;
            self.term_focused = false;
            self.damage.mark_full();
            self.request_redraw();
            return;
        }
        match self.buffers.get_or_open(&path) {
            Ok(_) => {
                self.open_path = Some(path);
                self.viewer_error = None;
                self.pane = PaneMode::Viewer;
                self.activity = frame::ActivityPanel::Explorer;
                self.term_focused = false;
                self.request_diff_refresh();
            }
            Err(e) => {
                error!("viewer: {e}");
                self.viewer_error = Some(e.to_string());
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    /// M12: close a file/config tab. Closing the active viewer tab selects
    /// the most recently used remaining file.
    fn close_tab(&mut self, path: PathBuf) {
        if Some(path.as_path()) == self.config_edits.as_ref().map(|e| e.path()) {
            self.close_config();
            self.activity = frame::ActivityPanel::Explorer;
            return;
        }
        let was_active = self.open_path.as_deref() == Some(path.as_path());
        self.buffers.evict(&path);
        if was_active {
            self.open_path = self
                .buffers
                .lru_paths()
                .into_iter()
                .next()
                .filter(|p| Some(p.as_path()) != self.config_edits.as_ref().map(|e| e.path()));
            if self.open_path.is_none() {
                self.viewer_error = None;
            }
            self.pane = PaneMode::Viewer;
            self.request_diff_refresh();
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Open a whitelisted config file in the enforced editor. Failures render
    /// in the pane (never a silent viewer fallback).
    fn open_config(&mut self, path: PathBuf) {        match ConfigEditor::open(&self.root, &path, &self.settings.editable) {
            Ok(editor) => {
                info!("config editor: {} ({:?})", path.display(), editor.kind());
                self.config_edits = Some(editor);
                self.config_error = None;
                self.pane = PaneMode::Config;
                self.activity = frame::ActivityPanel::Config;
                self.term_focused = false;
                self.commit.message.focused = false;
                self.viewer_error = None;
            }
            Err(e) => {
                error!("config editor: {e}");
                self.config_edits = None;
                self.config_error = Some(e.to_string());
                self.pane = PaneMode::Config;
                self.activity = frame::ActivityPanel::Config;
                self.term_focused = false;
            }
        }
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Close the config editor and return to the last viewed file.
    fn close_config(&mut self) {
        self.config_edits = None;
        self.config_error = None;
        self.pane = PaneMode::Viewer;
        self.activity = frame::ActivityPanel::Explorer;
        self.damage.mark_full();
        self.request_redraw();
    }

    /// Save the open config through the enforced atomic boundary.
    fn save_config(&mut self) {
        let mut saved_path = None;
        if let Some(editor) = &mut self.config_edits {
            match editor.save() {
                Ok(()) => info!("config saved: {}", editor.path().display()),
                Err(e) => error!("config save failed: {e}"),
            }
            saved_path = Some(editor.path().to_path_buf());
        }
        // Editing ark.toml itself applies the new whitelist immediately.
        if saved_path.as_deref() == Some(self.root.join("ark.toml").as_path()) {
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
    }

    fn reload_config(&mut self) {
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

    /// Keep the window IME state in sync with the config pane (M9).
    fn sync_ime(&mut self) {
        let want = self.pane == PaneMode::Config;
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
        if self.config_edits.is_some() || self.config_error.is_some() {
            PaneMode::Config
        } else {
            PaneMode::Viewer
        }
    }

    /// M8: run an exact git action in a terminal tab so its output streams
    /// (the M6 PTY reader does the streaming — ark keeps no capture buffer).
    /// Returns whether the tab opened.
    fn run_git_command(&mut self, label: &str, command: &str) -> bool {
        let Some(proxy) = self.proxy.clone() else {
            error!("{label}: cannot run without an event-loop proxy");
            self.commit.error = Some(format!("{label}: no event loop"));
            self.damage.mark_full();
            self.request_redraw();
            return false;
        };
        let Some(grid) = self.terminal_grid_for_new_tab() else {
            return false;
        };
        let m = self.text.mono_metrics(UI_FONT_SIZE, UI_FONT_SIZE * 1.35);
        let advance = f64::from(m.advance);
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

    /// Encode + write a mouse report to the active terminal when the running
    /// app has enabled mouse reporting. Returns whether a report was sent.
    fn term_mouse_report(&mut self, state: ElementState) -> bool {
        let Some(grid) = self.terminal_grid_for_new_tab() else {
            return false;
        };
        if !grid.contains(self.pointer.x, self.pointer.y) {
            return false;
        }
        let Some(term) = self.terms.active_mut() else {
            return false;
        };
        let mode = term.mode();
        let m = self.text.mono_metrics(UI_FONT_SIZE, UI_FONT_SIZE * 1.35);
        let advance = f64::from(m.advance);
        let col = ((self.pointer.x - grid.x) / advance).floor().max(0.0) as usize;
        let row = ((self.pointer.y - grid.y) / viewer::LINE_H).floor().max(0.0) as usize;
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
        // Hidden terminals keep their scrollback current but do not repaint
        // another pane. The workbench dock repaints whenever tabs exist.
        if self.terms.is_open() {
            if let Some(layout) = self.app_layout() {
                let (cw, ch) = (self.canvas.width(), self.canvas.height());
                self.damage
                    .mark(damage_rect(layout.term_grid, self.scale, cw, ch));
            }
        }
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
        if let Some(ed) = &mut self.config_edits {
            if ed.refresh_if_clean() {
                if ed.external_change() {
                    info!("config changed on disk while dirty (edits kept)");
                } else {
                    info!("config reloaded (changed on disk): {}", ed.path().display());
                }
                viewer_changed = true;
            }
        }
        if let Some(p) = self.open_path.clone() {
            let disk_changed = self
                .buffers
                .get_or_open(&p)
                .map(|v| v.changed_on_disk())
                .unwrap_or(false);
            if disk_changed {
                match self.buffers.refresh(&p) {
                    Ok(_) => {
                        info!("viewer refreshed (changed on disk): {}", p.display());
                        self.flash_until = Some(Instant::now() + FLASH_DURATION);
                        viewer_changed = true;
                        self.request_diff_refresh();
                    }
                    Err(e) => {
                        // Keep the old view: a vanished/binary file never
                        // blanks the pane.
                        error!("viewer refresh failed (keeping the old view): {e}");
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
        let mut earliest: Option<Instant> = self.flash_until;
        if self.fs_dirty {
            let at = self.last_tree_refresh.unwrap_or_else(Instant::now) + TREE_REFRESH_DEBOUNCE;
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
                self.scale = window.scale_factor();
                info!("window ready (root: {}, scale {:.2}x)", self.root.display(), self.scale);
                self.context = Some(context);
                self.surface = Some(surface);
                self.window = Some(window);
                self.damage.mark_full();
                self.request_redraw();
                // M7: status on open — dispatched here (the loop is live);
                // send_event before run_app is dropped by winit.
                self.request_git_refresh();
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
            Wake::PtyData => self.on_pty_data(event_loop),
            Wake::GitStatus(status) => self.apply_git_status(status),
            Wake::GitDiff { path, diff } => {
                // Stale-diff guard: only the currently open file applies.
                if self.open_path.as_deref() == Some(path.as_path()) {
                    self.file_diff = Some((path, diff));
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
        let now = Instant::now();
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        self.last_activity = Instant::now();
        match event {
            WindowEvent::CloseRequested => {
                info!("close requested — exiting");
                event_loop.exit();
            }
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                self.damage.mark_full();
                info!("scale factor changed to {scale_factor:.2}x");
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Full repaint on pointer movement: hover backgrounds and the
                // split divider react to position; simple and correct in M3.
                let logical = position.to_logical(self.scale);
                self.pointer.x = logical.x;
                self.pointer.y = logical.y;
                self.damage.mark_full();
                self.request_redraw();
            }
            WindowEvent::MouseInput { state, .. } => {
                // Terminal grid: a click focuses the pane; when the running
                // app enables mouse reporting, press/release are encoded to
                // the shell instead of local interaction.
                let layout = self.app_layout();
                let over_commit = self.activity == frame::ActivityPanel::SourceControl
                    && layout
                        .as_ref()
                        .and_then(frame::source_control_panel)
                        .is_some_and(|rect| rect.contains(self.pointer.x, self.pointer.y));
                let over_grid = layout
                    .as_ref()
                    .map(|l| l.term_grid.contains(self.pointer.x, self.pointer.y))
                    .unwrap_or(false);
                if over_commit {
                    // A commit-panel click never leaves the shell keyboard-
                    // focused (the message field / buttons own this surface).
                    self.term_focused = false;
                }
                if self.pane == PaneMode::Config {
                    // Config-pane clicks (textarea/env form) never focus the
                    // hidden shell either.
                    self.term_focused = false;
                    self.commit.message.focused = false;
                }
                if self.terms.is_open() && over_grid {
                    self.term_focused = true;
                    self.commit.message.focused = false;
                    if self.term_mouse_report(state) {
                        self.damage.mark_full();
                        self.request_redraw();
                    }
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
                self.damage.mark_full();
                self.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(dy) = wheel_scroll_px(&delta) else { return };
                let Some(layout) = self.app_layout() else { return };
                // Wheel scrolling applies over the whole panel under the
                // pointer (its scrollbar track scrolls too, matching editors).
                let (x, y) = (self.pointer.x, self.pointer.y);
                // Source Control owns the wheel while its card is active,
                // whether it is hosted in the sidebar or compact editor body.
                if self.activity == frame::ActivityPanel::SourceControl {
                    if let Some(rect) = frame::source_control_panel(&layout)
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
                // M9: wheel over the config pane scrolls its list/textarea.
                if self.pane == PaneMode::Config && layout.viewer_pane.contains(x, y) {
                    if let Some(editor) = &mut self.config_edits {
                        if editor.scroll_by(dy) {
                            let (cw, ch) = (self.canvas.width(), self.canvas.height());
                            let rect = damage_rect(layout.viewer_pane, self.scale, cw, ch);
                            self.damage.mark(rect);
                            self.request_redraw();
                        }
                    }
                    return;
                }
                let grid = layout.term_grid;
                let (scale, canvas, damage) = (self.scale, &self.canvas, &mut self.damage);
                let scrolled = if layout.tree_panel.contains(x, y) {
                    route_wheel_scroll(&mut self.tree, dy, &layout, scale, canvas, damage)
                } else if grid.contains(x, y) && self.terms.is_open() {
                    route_wheel_term(self, dy, &grid)
                } else if layout.viewer_pane.contains(x, y) {
                    match self.open_path.as_deref() {
                        Some(p) => match self.buffers.get_or_open(Path::new(p)) {
                            Ok(v) => route_wheel_viewer(v, dy, &layout, scale, canvas, damage),
                            Err(_) => false,
                        },
                        None => false,
                    }
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
                    self.damage.mark_full();
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                let ctrl = self.modifiers.state().control_key();
                // App-level shortcuts: work whether or not the shell has focus.
                if ctrl {
                    if let winit::keyboard::Key::Character(s) = &event.logical_key {
                        match s.as_str() {
                            "t" | "T" => {
                                self.open_terminal_tab();
                                return;
                            }
                            "w" | "W" => {
                                if self.pane == PaneMode::Config {
                                    self.close_config();
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
                    if self.commit.handle_key(&event.logical_key, event.text.as_deref()) {
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
                            self.damage.mark_full();
                            self.request_redraw();
                        }
                    }
                    return;
                }
                // Focused terminal: every other key goes to the shell (Ctrl+C
                // included — it is SIGINT there, not copy).
                if self.terms.is_open() && self.term_focused {
                    if let Some(term) = self.terms.active_mut() {
                        let mode = term.mode();
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
                if !is_c {
                    return;
                }
                let copied = self
                    .open_path
                    .as_deref()
                    .and_then(|p| self.buffers.get_or_open(Path::new(p)).ok())
                    .and_then(|v| v.selection_text())
                    .map(|text| {
                        let n = text.chars().count();
                        viewer::copy_to_clipboard(&text).map(|_| n)
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
