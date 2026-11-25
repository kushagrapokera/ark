//! Terminal tabs (M6): real PTYs (portable-pty) driven by alacritty's VT
//! model (alacritty_terminal), rendered by our font stack.
//!
//! Memory rules (AGENTS.md guardrail 3):
//! - the PTY reader thread pumps a BOUNDED channel (backpressure: the shell
//!   blocks on write when the buffer fills — no byte dropped, no growth);
//! - scrollback is capped by the model (`scrolling_history` = 2000);
//! - each wake drains a bounded number of chunks; rasters cap at ~30fps
//!   under output storms (the caller owns the budget).
//!
//! Lifecycle: dropping a `Terminal` drops the PTY master (SIGHUP kills the
//! shell's whole process group per POSIX), then `kill()` backstops and
//! `wait()` reaps.

use std::io::{Read, Write};
use std::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::{
    mpsc::{self, SyncSender, TryRecvError},
    Arc, Mutex,
};
use std::thread;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{self, cell::Flags, Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color as TermColor, NamedColor, Rgb};
use cosmic_text::Weight;
use portable_pty::{CommandBuilder, PtySize};
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{Key, NamedKey};

use crate::render::{Canvas, ColorRgb, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::ui::Rect;
use crate::viewer::LINE_H;
use crate::watch::Wake;

mod paste;
pub use paste::{encode_paste, MAX_PASTE_BYTES};

/// Terminal scrollback cap (memory rule 3: capped terminal scrollback).
pub const SCROLLBACK_LINES: usize = 2000;
/// Hard cap on open terminal tabs (shell, git commands and agents share it).
/// Each tab carries a PTY, grid + capped scrollback; without a cap the tab
/// vector would grow with clicks and could blow the memory budget.
pub const MAX_TERM_TABS: usize = 8;
/// Grid cell metrics: the viewer's row height and the shared mono font.
pub const TERM_LINE_H: f64 = LINE_H;

/// Grid dimensions (alacritty's own `TermSize` is test-only in 0.26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl GridSize {
    pub fn new(cols: usize, rows: usize) -> Self {
        GridSize { cols: cols.max(2), rows: rows.max(2) }
    }

    /// Fit a grid into a LOGICAL-px rect (whole cells only).
    pub fn fit(width: f64, height: f64, advance: f64, line_h: f64) -> GridSize {
        let cols = ((width / advance.max(1.0)).floor() as usize).max(2);
        let rows = ((height / line_h.max(1.0)).floor() as usize).max(2);
        GridSize::new(cols, rows)
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Shared PTY input sink. Terminal-model replies (Primary Device Attributes,
/// cursor reports, and similar queries) must be written back to the shell, not
/// dropped. Other model events remain no-ops because ark renders only the grid.
#[derive(Clone)]
struct TerminalListener {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl TerminalListener {
    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        write_terminal_input(&self.writer, bytes)
    }
}

impl EventListener for TerminalListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            if let Err(e) = self.write(text.as_bytes()) {
                // Writes race shell exit; logged, never silently dropped.
                log::debug!("terminal query response failed: {e}");
            }
        }
    }
}

fn write_terminal_input(
    writer: &Arc<Mutex<Box<dyn Write + Send>>>,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut writer = writer.lock().map_err(|_| {
        std::io::Error::other("terminal writer lock poisoned")
    })?;
    writer.write_all(bytes)?;
    writer.flush()
}

#[derive(Debug)]
pub enum TermError {
    Io(std::io::Error),
    Pty(anyhow::Error),
    NoActiveTerminal,
    TabLimit { max: usize },
}

impl std::fmt::Display for TermError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TermError::Io(e) => write!(f, "{e}"),
            TermError::Pty(e) => write!(f, "{e}"),
            TermError::NoActiveTerminal => write!(f, "no active terminal to duplicate"),
            TermError::TabLimit { max } => {
                write!(f, "terminal tab limit reached ({max}); close a tab first")
            }
        }
    }
}

impl std::error::Error for TermError {}

/// Shell registry (M6): fish first (ark's default), then `$SHELL`, then the
/// other common shells that exist on this system.
pub fn shell_registry() -> Vec<PathBuf> {
    let mut shells = Vec::new();
    for candidate in ["/usr/bin/fish", "/bin/fish"] {
        let p = PathBuf::from(candidate);
        if p.is_file() && !shells.contains(&p) {
            shells.push(p);
        }
    }
    if let Ok(s) = std::env::var("SHELL") {
        let p = PathBuf::from(s);
        if p.is_file() && !shells.contains(&p) {
            shells.push(p);
        }
    }
    for candidate in ["/bin/bash", "/usr/bin/bash", "/bin/zsh", "/usr/bin/zsh"] {
        let p = PathBuf::from(candidate);
        if p.is_file() && !shells.contains(&p) {
            shells.push(p);
        }
    }
    shells
}

pub fn default_shell() -> PathBuf {
    shell_registry().into_iter().next().unwrap_or_else(|| PathBuf::from("/bin/bash"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalProfile {
    Bash,
    Fish,
    JavaScriptDebug,
}

impl TerminalProfile {
    pub fn program(self) -> Option<PathBuf> {
        match self {
            Self::Bash => ["/bin/bash", "/usr/bin/bash"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.is_file()),
            Self::Fish => ["/usr/bin/fish", "/bin/fish"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.is_file()),
            Self::JavaScriptDebug => program_on_path("node"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::JavaScriptDebug => "JavaScript Debug Terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMenuAction {
    NewTerminal,
    NewTerminalWindow,
    OpenProfile(TerminalProfile),
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalMenuOption {
    pub label: &'static str,
    pub action: TerminalMenuAction,
    pub separator_before: bool,
}

pub fn terminal_menu_options() -> &'static [TerminalMenuOption] {
    &[
        TerminalMenuOption {
            label: "New Terminal",
            action: TerminalMenuAction::NewTerminal,
            separator_before: false,
        },
        TerminalMenuOption {
            label: "New Terminal Window",
            action: TerminalMenuAction::NewTerminalWindow,
            separator_before: false,
        },
        TerminalMenuOption {
            label: "bash",
            action: TerminalMenuAction::OpenProfile(TerminalProfile::Bash),
            separator_before: true,
        },
        TerminalMenuOption {
            label: "fish",
            action: TerminalMenuAction::OpenProfile(TerminalProfile::Fish),
            separator_before: false,
        },
        TerminalMenuOption {
            label: "JavaScript Debug Terminal",
            action: TerminalMenuAction::OpenProfile(TerminalProfile::JavaScriptDebug),
            separator_before: false,
        },
    ]
}

pub fn program_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

pub fn external_terminal_program() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("TERMINAL") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
        if let Some(path) = program_on_path(path.to_str()?) {
            return Some(path);
        }
    }
    ["konsole", "gnome-terminal", "alacritty", "kitty", "xterm"]
        .into_iter()
        .find_map(program_on_path)
}

pub fn configure_external_terminal(command: &mut std::process::Command, program: &Path, root: &Path) {
    command.current_dir(root);
    match program.file_name().and_then(|name| name.to_str()) {
        Some("konsole") => {
            command.arg("--workdir").arg(root);
        }
        Some("gnome-terminal") => {
            command.arg(format!("--working-directory={}", root.display()));
        }
        Some("alacritty") => {
            command.arg("--working-directory").arg(root);
        }
        Some("kitty") => {
            command.arg("-d").arg(root);
        }
        _ => {}
    }
}

/// One terminal tab: PTY + model + bounded output channel.
pub struct Terminal {
    shell: PathBuf,
    /// Executable actually launched (shell, node, or a git action's shell).
    program: PathBuf,
    /// Launch arguments retained so `+` can clone the active terminal.
    args: Vec<String>,
    /// Tab label: the shell's file name for interactive tabs, or the action
    /// name for command tabs ("git commit", "git push").
    label: String,
    /// Kept for `resize`; dropped FIRST on close (SIGHUP to the group).
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// `None` once the Drop teardown reaper owns it.
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    /// The shell's process group (== its pid: session leader), captured at
    /// spawn for the teardown group kill.
    pgid: Option<u32>,
    term: Term<TerminalListener>,
    processor: ansi::Processor,
    rx: mpsc::Receiver<Vec<u8>>,
    /// Visible/model grid size; may lead the PTY during a divider drag.
    size: GridSize,
    /// Last size acknowledged by the PTY. Kept separate so a drag sends one
    /// SIGWINCH at release instead of one per pointer movement.
    pty_size: GridSize,
    exited: bool,
}

impl Drop for Terminal {
    /// Teardown (deterministic process-group kill): dropping the master
    /// SIGHUPs the foreground group, then a detached reaper SIGTERMs the
    /// shell's whole process group (the shell is the session leader, so its
    /// pgid == its pid — verified via /proc before the group kill), SIGKILLs
    /// after a short grace, and reaps — all off the UI thread. Without a
    /// pgid match (never a group leader) it falls back to child.kill().
    fn drop(&mut self) {
        self.master.take();
        let pgid = self.pgid;
        if let Some(mut child) = self.child.take() {
            let spawned = thread::Builder::new()
                .name("ark-term-reaper".into())
                .spawn(move || {
                    // Every process in the shell's SESSION: interactive bash
                    // gives background jobs their own process group, so a
                    // group kill can't reach them — the session sweep does.
                    // Bounded: one /proc pass, then TERM → grace → KILL.
                    let victims = session_members(pgid);
                    for pid in &victims {
                        unsafe { libc::kill(*pid, libc::SIGTERM) };
                    }
                    thread::sleep(Duration::from_millis(80));
                    for pid in &victims {
                        unsafe { libc::kill(*pid, libc::SIGKILL) };
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    log::debug!(
                        "terminal torn down ({} session process(es) killed)",
                        victims.len()
                    );
                });
            if spawned.is_err() {
                log::warn!("terminal reaper thread unavailable — dropping the child");
            }
        }
    }
}

/// All pids whose SESSION equals the shell's (the shell + its descendants,
/// including background jobs in their own groups). One bounded /proc pass.
fn session_members(pgid: Option<u32>) -> Vec<i32> {
    let Some(sid) = pgid else { return Vec::new() };
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in dirs.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue; // non-numeric entries
        };
        if let Some(s) = session_id(pid) {
            if s == sid {
                out.push(pid as i32);
            }
        }
    }
    out
}

/// The session id of `pid` (field 6 of /proc/<pid>/stat — fields after the
/// comm paren: state, ppid, pgrp, session, …). None when gone/unreadable.
fn session_id(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit(')').next()?;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    fields.get(3)?.parse().ok()
}


impl Terminal {
    /// Spawn `shell` in a PTY rooted at `root`, sized to `size`. Starts the
    /// reader thread (bounded channel + one wake per chunk).
    pub fn spawn(
        root: &Path,
        shell: &Path,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<Terminal, TermError> {
        Self::spawn_with(root, shell, size, move || {
            let _ = proxy.send_event(Wake::PtyData);
        })
    }

    /// Spawn `shell -c command` in a PTY rooted at `root` — the M8 git
    /// action tabs. Output streams through the same reader thread as an
    /// interactive shell; the tab carries an action `label`.
    pub fn spawn_command(
        root: &Path,
        shell: &Path,
        label: &str,
        command: &str,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<Terminal, TermError> {
        let args = ["-c", command];
        Self::spawn_inner(root, shell, Some(label), size, &args, move || {
            let _ = proxy.send_event(Wake::PtyData);
        })
    }

    /// Spawn a labeled interactive program (for example Node's JavaScript
    /// console) in a PTY rooted at `root`.
    pub fn spawn_program(
        root: &Path,
        program: &Path,
        label: &str,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<Terminal, TermError> {
        Self::spawn_launch_with(root, program, &[], Some(label), size, move || {
            let _ = proxy.send_event(Wake::PtyData);
        })
    }

    fn spawn_launch_with(
        root: &Path,
        program: &Path,
        args: &[String],
        label: Option<&str>,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Terminal, TermError> {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Self::spawn_inner(root, program, label, size, &args, wake)
    }

    /// The spawn core with a pluggable wake (tests pass a no-op).
    fn spawn_with(
        root: &Path,
        shell: &Path,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Terminal, TermError> {
        Self::spawn_inner(root, shell, None, size, &[], wake)
    }

    /// Test-only direct-program spawn (no event-loop proxy).
    #[cfg(test)]
    pub(crate) fn spawn_program_with(
        root: &Path,
        program: &Path,
        label: &str,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Terminal, TermError> {
        Self::spawn_inner(root, program, Some(label), size, &[], wake)
    }

    /// Test-only command spawn (no event-loop proxy).
    #[cfg(test)]
    fn spawn_command_with(
        root: &Path,
        shell: &Path,
        label: &str,
        command: &str,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Terminal, TermError> {
        let args = ["-c", command];
        Self::spawn_inner(root, shell, Some(label), size, &args, wake)
    }

    /// The shared PTY spawn: `args` empty = interactive shell; otherwise the
    /// shell runs the command and exits (the tab keeps the output).
    fn spawn_inner(
        root: &Path,
        shell: &Path,
        label: Option<&str>,
        size: GridSize,
        args: &[&str],
        wake: impl Fn() + Send + 'static,
    ) -> Result<Terminal, TermError> {
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: size.rows as u16,
                cols: size.cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(TermError::Pty)?;
        let mut cmd = CommandBuilder::new(shell);
        if !args.is_empty() {
            cmd.args(args);
        }
        cmd.cwd(root);
        // TERM/COLORTERM: running apps need them for colors + capabilities.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pair.slave.spawn_command(cmd).map_err(TermError::Pty)?;
        // The shell is the session leader: pgid == pid. Captured NOW (the
        // /proc entry is gone once the teardown reaps the shell).
        let pgid = child.process_id();
        let child = Some(child);
        // The slave lives in the child's session now; drop our copy so the
        // shell's exit produces EOF on the master.
        drop(pair.slave);

        let writer = Arc::new(Mutex::new(
            pair.master.take_writer().map_err(TermError::Pty)?,
        ));
        let mut reader = pair.master.try_clone_reader().map_err(TermError::Pty)?;
        let (tx, rx): (SyncSender<Vec<u8>>, _) = mpsc::sync_channel(128);
        thread::Builder::new()
            .name("ark-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF: the session is gone
                        Ok(n) => {
                            // Bounded send: when the app lags, this BLOCKS
                            // (backpressure to the shell) — no byte dropped.
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break; // the tab closed; stop pumping
                            }
                            wake();
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(TermError::Io)?;

        let config = Config { scrolling_history: SCROLLBACK_LINES, ..Config::default() };
        let term = Term::new(
            config,
            &size,
            TerminalListener {
                writer: writer.clone(),
            },
        );
        let label = label.map(str::to_owned).unwrap_or_else(|| {
            shell
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "shell".into())
        });
        Ok(Terminal {
            shell: shell.to_path_buf(),
            program: shell.to_path_buf(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            label,
            master: Some(pair.master),
            writer,
            child,
            term,
            processor: ansi::Processor::new(),
            rx,
            size,
            pty_size: size,
            pgid,
            exited: false,
        })
    }

    pub fn shell_name(&self) -> &str {
        self.shell
            .file_name()
            .map(|n| n.to_str().unwrap_or("?"))
            .unwrap_or("?")
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The tab-strip label (shell name or the action label for command tabs).
    pub fn label(&self) -> &str {
        &self.label
    }

    #[allow(dead_code)] // test/inspection surface; the frame drives resize directly
    pub fn size(&self) -> GridSize {
        self.size
    }

    #[allow(dead_code)] // retained for future terminal status chrome
    pub fn exited(&self) -> bool {
        self.exited
    }

    /// Size currently acknowledged by the PTY. During a divider drag this
    /// intentionally lags [`Self::size`] until the final size is flushed.
    #[allow(dead_code)] // resize contract/inspection surface
    pub fn pty_size(&self) -> GridSize {
        self.pty_size
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Resize the model + the PTY immediately. Returns whether anything changed.
    #[allow(dead_code)] // immediate-resize compatibility API; frame rendering defers explicitly
    pub fn resize(&mut self, size: GridSize) -> bool {
        let changed = self.resize_visual(size);
        if changed {
            self.flush_pty_resize();
        }
        changed
    }

    /// Resize only the visible model. The shell keeps its previous dimensions
    /// until [`Self::flush_pty_resize`] runs, avoiding a SIGWINCH feedback loop
    /// while the divider is moving.
    pub fn resize_visual(&mut self, size: GridSize) -> bool {
        if self.size == size {
            return false;
        }
        self.term.resize(size);
        self.size = size;
        true
    }

    /// Send the final visual size to the PTY once. Returns whether the kernel
    /// PTY dimensions changed. Failures are logged and left retryable.
    pub fn flush_pty_resize(&mut self) -> bool {
        if self.pty_size == self.size {
            return false;
        }
        let Some(master) = &self.master else {
            // An exited/reaped PTY has no kernel size left to synchronize.
            self.pty_size = self.size;
            return false;
        };
        if let Err(e) = master.resize(PtySize {
            rows: self.size.rows as u16,
            cols: self.size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            log::error!(
                "terminal PTY resize to {}×{} failed: {e}",
                self.size.cols,
                self.size.rows
            );
            return false;
        }
        self.pty_size = self.size;
        log::debug!("terminal PTY resized to {}×{}", self.size.cols, self.size.rows);
        true
    }

    /// Write raw input bytes to the shell (already encoded by the caller).
    pub fn key_input(&mut self, bytes: &[u8]) {
        if let Err(e) = write_terminal_input(&self.writer, bytes) {
            // Writes race the shell's exit — logged, never fatal.
            log::debug!("terminal write failed: {e}");
        }
    }

    /// Scroll the viewport by `lines` (positive = toward history).
    pub fn scroll_display(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    /// Drain up to `max_chunks` output chunks into the model. `true` when
    /// any chunk was parsed (the grid may have changed).
    pub fn drain_output(&mut self, max_chunks: usize) -> bool {
        let mut parsed = false;
        for _ in 0..max_chunks {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.processor.advance(&mut self.term, &chunk);
                    parsed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.exited = true; // reader EOF: the session ended
                    break;
                }
            }
        }
        parsed
    }

    /// Render the visible grid into `rect` (LOGICAL px). Viewport-only:
    /// same-style runs share one shaped text call. The cursor paints when
    /// `show_cursor` (the pane is focused) and is on-screen.
    #[allow(clippy::too_many_arguments)] // explicit raster context and window scale
    pub fn draw(
        &self,
        canvas: &mut Canvas,
        text: &mut TextSystem,
        theme: &Theme,
        scale: f64,
        ui_scale: f64,
        rect: Rect,
        show_cursor: bool,
    ) {
        if rect.w <= 0.0
            || rect.h <= 0.0
            || !scale.is_finite()
            || scale <= 0.0
            || !ui_scale.is_finite()
            || ui_scale <= 0.0
        {
            return;
        }
        let content = self.term.renderable_content();
        let font_size = (UI_FONT_SIZE as f64 * scale * ui_scale) as f32;
        let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
        let advance = f64::from(m.advance) / scale;
        let rows = ((rect.h / TERM_LINE_H).floor() as usize).min(self.size.rows);

        // Collect the viewport: [row][col] → (char, fg, bg, flags) with bg
        // fills resolved to per-cell colors (default bg cells stay clean).
        let mut grid: Vec<Vec<CellView>> = (0..rows).map(|_| Vec::new()).collect();
        for indexed in content.display_iter {
            let Some(vp) = term::point_to_viewport(content.display_offset, indexed.point) else {
                continue;
            };
            if vp.line >= rows {
                continue; // below the painted area (model rows > shown rows)
            }
            let cell = indexed.cell;
            let (fg, bg) = resolve_pair(theme, content.colors, cell.fg, cell.bg, cell.flags);
            let mut ch = cell.c;
            if cell.flags.contains(Flags::HIDDEN) {
                ch = ' ';
            }
            grid[vp.line].push(CellView { col: vp.column.0, ch, fg, bg, flags: cell.flags });
        }
        // Rows may arrive sparse — sort by column (the iterator is in order,
        // but a skipped WIDE_CHAR_SPACER keeps it stable; sort is a no-op).
        for row in &mut grid {
            row.sort_by_key(|c| c.col);
        }

        // Backgrounds first (whole-cell fills), then text runs.
        let baseline_off = f64::from(m.ascent - m.descent) / 2.0;
        for (i, row) in grid.iter().enumerate() {
            let row_y = rect.y + i as f64 * TERM_LINE_H;
            for cell in row {
                if cell.bg != theme.bg {
                    canvas.fill_rect(
                        rect.x + cell.col as f64 * advance,
                        row_y,
                        advance,
                        TERM_LINE_H,
                        cell.bg,
                        scale,
                    );
                }
            }
            // Text runs: consecutive cells with the same (fg, bold) merge
            // into one shaped call — a uniform row is a single run.
            let mut run = String::new();
            let mut run_start = 0usize;
            let mut run_fg = theme.fg;
            let mut run_bold = false;
            let mut run_underline = false;
            for cell in row {
                let bold = cell.flags.contains(Flags::BOLD);
                let same_style = cell.fg == run_fg && bold == run_bold;
                if !same_style && !run.is_empty() {
                    flush_run(canvas, text, theme, scale, rect, row_y, advance, baseline_off, &run, run_start, run_fg, run_bold, run_underline);
                    run.clear();
                    run_underline = false;
                }
                if run.is_empty() {
                    run_start = cell.col;
                    run_fg = cell.fg;
                    run_bold = bold;
                }
                run_underline |= cell.flags.contains(Flags::UNDERLINE);
                run.push(cell.ch);
            }
            if !run.is_empty() {
                flush_run(canvas, text, theme, scale, rect, row_y, advance, baseline_off, &run, run_start, run_fg, run_bold, run_underline);
            }
        }

        // Exited sessions render a dim note (the tab stays until closed).
        if self.exited {
            let m2 = text.mono_metrics((12.0 * scale) as f32, (12.0 * 1.35 * scale) as f32);
            let baseline = (rect.y + 12.0) * scale + f64::from(m2.ascent - m2.descent) / 2.0;
            text.draw_line(
                canvas,
                "(process exited)",
                (rect.x * scale) as f32,
                baseline as f32,
                TextStyle { color: theme.fg_dim, font_size: (12.0 * scale) as f32, weight: Weight::NORMAL },
            );
        }

        // Cursor: block in the accent color, character in bg (focused only;
        // hidden when scrolled off-screen or the app hides it).
        if show_cursor && content.cursor.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden {
            if let Some(vp) = term::point_to_viewport(content.display_offset, content.cursor.point) {
                if vp.line < rows {
                    let x = rect.x + vp.column.0 as f64 * advance;
                    let y = rect.y + vp.line as f64 * TERM_LINE_H;
                    canvas.fill_rect(x, y, advance, TERM_LINE_H, theme.accent, scale);
                    let cursor_char = self
                        .term
                        .grid()[Point::new(Line(vp.line as i32 - content.display_offset as i32), Column(vp.column.0))]
                        .c;
                    let baseline = (y + TERM_LINE_H / 2.0) * scale + baseline_off;
                    text.draw_line(
                        canvas,
                        &cursor_char.to_string(),
                        (x * scale) as f32,
                        baseline as f32,
                        TextStyle { color: theme.bg, font_size, weight: Weight::NORMAL },
                    );
                }
            }
        }
    }
}

struct CellView {
    col: usize,
    ch: char,
    fg: ColorRgb,
    bg: ColorRgb,
    flags: Flags,
}

#[allow(clippy::too_many_arguments)]
fn flush_run(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    _theme: &Theme,
    scale: f64,
    rect: Rect,
    row_y: f64,
    advance: f64,
    baseline_off: f64,
    run: &str,
    run_start: usize,
    fg: ColorRgb,
    bold: bool,
    underline: bool,
) {
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let baseline = (row_y + TERM_LINE_H / 2.0) * scale + baseline_off;
    text.draw_line(
        canvas,
        run,
        ((rect.x + run_start as f64 * advance) * scale) as f32,
        baseline as f32,
        TextStyle { color: fg, font_size, weight: if bold { Weight::BOLD } else { Weight::NORMAL } },
    );
    if underline {
        let y = row_y + TERM_LINE_H - 1.0;
        let w = run.chars().count() as f64 * advance;
        canvas.fill_rect(rect.x + run_start as f64 * advance, y, w, 1.0, fg, scale);
    }
}

/// Catppuccin Mocha ANSI palette (normal 0-7, bright 8-15).
const ANSI_PALETTE: [ColorRgb; 16] = [
    (0x45, 0x47, 0x5A), // black (surface1)
    (0xF3, 0x8B, 0xA8), // red
    (0xA6, 0xE3, 0xA1), // green
    (0xF9, 0xE2, 0xAF), // yellow
    (0x89, 0xB4, 0xFA), // blue
    (0xF5, 0xC2, 0xE7), // magenta (pink)
    (0x94, 0xE2, 0xD5), // cyan (teal)
    (0xBA, 0xC2, 0xDE), // white (subtext1)
    (0x58, 0x5B, 0x70), // bright black (overlay0)
    (0xF3, 0x8B, 0xA8),
    (0xA6, 0xE3, 0xA1),
    (0xF9, 0xE2, 0xAF),
    (0x89, 0xB4, 0xFA),
    (0xF5, 0xC2, 0xE7),
    (0x94, 0xE2, 0xD5),
    (0xA6, 0xAD, 0xC2), // bright white (subtext0)
];

/// Blend `c` toward `bg` by `t` (0 = c, 1 = bg) — used for DIM.
fn blend_toward(c: ColorRgb, bg: ColorRgb, t: f64) -> ColorRgb {
    let f = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    (f(c.0, bg.0), f(c.1, bg.1), f(c.2, bg.2))
}

/// xterm 256-color: 0-15 ANSI, 16-231 the 6×6×6 cube, 232-255 grays.
fn xterm256(i: u8) -> ColorRgb {
    let i = i as usize;
    if i < 16 {
        return ANSI_PALETTE[i];
    }
    if i < 232 {
        let n = i - 16;
        let levels = [0u8, 95, 135, 175, 215, 255];
        let r = levels[n / 36];
        let g = levels[(n % 36) / 6];
        let b = levels[n % 6];
        return (r, g, b);
    }
    let gray = (8 + 10 * (i - 232)) as u8;
    (gray, gray, gray)
}

/// REVERSE-aware pair resolution for one cell: (fg, bg).
fn resolve_pair(
    theme: &Theme,
    overrides: &alacritty_terminal::term::color::Colors,
    fg: TermColor,
    bg: TermColor,
    flags: Flags,
) -> (ColorRgb, ColorRgb) {
    if flags.contains(Flags::INVERSE) {
        let f = resolve_color_plain(theme, overrides, bg, flags);
        let b = resolve_color_plain(theme, overrides, fg, flags);
        (f, b)
    } else {
        let f = resolve_color_plain(theme, overrides, fg, flags);
        let b = resolve_color_plain(theme, overrides, bg, flags);
        (f, b)
    }
}

fn resolve_color_plain(
    theme: &Theme,
    overrides: &alacritty_terminal::term::color::Colors,
    c: TermColor,
    flags: Flags,
) -> ColorRgb {
    let mut rgb = match c {
        TermColor::Spec(Rgb { r, g, b }) => (r, g, b),
        TermColor::Named(named) => {
            let over = overrides[named];
            if let Some(Rgb { r, g, b }) = over {
                (r, g, b)
            } else {
                match named {
                    NamedColor::Foreground => theme.fg,
                    NamedColor::Background => theme.bg,
                    NamedColor::Cursor => theme.accent,
                    NamedColor::DimBlack => blend_toward(ANSI_PALETTE[0], theme.bg, 0.4),
                    NamedColor::DimRed => blend_toward(ANSI_PALETTE[1], theme.bg, 0.4),
                    NamedColor::DimGreen => blend_toward(ANSI_PALETTE[2], theme.bg, 0.4),
                    NamedColor::DimYellow => blend_toward(ANSI_PALETTE[3], theme.bg, 0.4),
                    NamedColor::DimBlue => blend_toward(ANSI_PALETTE[4], theme.bg, 0.4),
                    NamedColor::DimMagenta => blend_toward(ANSI_PALETTE[5], theme.bg, 0.4),
                    NamedColor::DimCyan => blend_toward(ANSI_PALETTE[6], theme.bg, 0.4),
                    NamedColor::DimWhite => blend_toward(ANSI_PALETTE[7], theme.bg, 0.4),
                    NamedColor::DimForeground => blend_toward(theme.fg, theme.bg, 0.4),
                    NamedColor::BrightForeground => theme.fg,
                    n if (n as u8) < 16 => ANSI_PALETTE[n as u8 as usize],
                    _ => theme.fg,
                }
            }
        }
        TermColor::Indexed(i) => {
            let over = overrides[i as usize];
            if let Some(Rgb { r, g, b }) = over {
                (r, g, b)
            } else {
                xterm256(i)
            }
        }
    };
    if flags.contains(Flags::DIM) {
        rgb = blend_toward(rgb, theme.bg, 0.4);
    }
    rgb
}

// ---- keyboard + mouse encoding ----

/// xterm modifier parameter for CSI sequences: 1 + shift + alt*2 + ctrl*4.
fn csi_mod(mods: &winit::keyboard::ModifiersState) -> Option<u8> {
    let m = (mods.shift_key() as u8)
        + (mods.alt_key() as u8) * 2
        + (mods.control_key() as u8) * 4;
    if m == 0 {
        None
    } else {
        Some(1 + m)
    }
}

/// Encode one keyboard event into PTY bytes against the model's mode.
pub fn encode_key(
    logical: &Key,
    text: Option<&str>,
    mods: &winit::keyboard::ModifiersState,
    mode: &TermMode,
) -> Vec<u8> {
    let ctrl = mods.control_key();
    let alt = mods.alt_key();
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    if let Key::Named(named) = logical {
        let seq: Vec<u8> = match named {
            NamedKey::Enter => vec![b'\r'],
            NamedKey::Backspace => vec![0x7f],
            NamedKey::Tab => vec![b'\t'],
            NamedKey::Escape => vec![0x1b],
            NamedKey::Delete => b"\x1b[3~".to_vec(),
            NamedKey::PageUp => b"\x1b[5~".to_vec(),
            NamedKey::PageDown => b"\x1b[6~".to_vec(),
            NamedKey::Home => {
                if app_cursor {
                    b"\x1bOH".to_vec()
                } else {
                    b"\x1b[H".to_vec()
                }
            }
            NamedKey::End => {
                if app_cursor {
                    b"\x1bOF".to_vec()
                } else {
                    b"\x1b[F".to_vec()
                }
            }
            NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::ArrowRight
            | NamedKey::ArrowLeft => {
                let c = match named {
                    NamedKey::ArrowUp => b'A',
                    NamedKey::ArrowDown => b'B',
                    NamedKey::ArrowRight => b'C',
                    _ => b'D',
                };
                match csi_mod(mods) {
                    Some(m) => format!("\x1b[1;{m}{}", c as char).into_bytes(),
                    None if app_cursor => vec![b'\x1b', b'O', c],
                    None => vec![b'\x1b', b'[', c],
                }
            }
            NamedKey::F1 => b"\x1bOP".to_vec(),
            NamedKey::F2 => b"\x1bOQ".to_vec(),
            NamedKey::F3 => b"\x1bOR".to_vec(),
            NamedKey::F4 => b"\x1bOS".to_vec(),
            NamedKey::F5 => b"\x1b[15~".to_vec(),
            NamedKey::F6 => b"\x1b[17~".to_vec(),
            NamedKey::F7 => b"\x1b[18~".to_vec(),
            NamedKey::F8 => b"\x1b[19~".to_vec(),
            NamedKey::F9 => b"\x1b[20~".to_vec(),
            NamedKey::F10 => b"\x1b[21~".to_vec(),
            NamedKey::F11 => b"\x1b[23~".to_vec(),
            NamedKey::F12 => b"\x1b[24~".to_vec(),
            _ => Vec::new(),
        };
        if !seq.is_empty() {
            return seq;
        }
    }

    // Ctrl+letter/`@`/space → the classic C0 control byte.
    if ctrl {
        if *logical == Key::Named(NamedKey::Space) {
            return vec![0]; // Ctrl+Space = NUL
        }
        if let Key::Character(s) = logical {
            if let Some(c) = s.bytes().next() {
                let c = c.to_ascii_lowercase();
                if c.is_ascii_lowercase() {
                    return vec![c - b'a' + 1];
                }
                if c == b'@' {
                    return vec![0];
                }
            }
        }
    }

    // Printable text (UTF-8); Alt prefixes ESC (meta). Control characters
    // carried in `text` are never typed (they belong to named keys above).
    if let Some(t) = text {
        if !t.chars().any(char::is_control) {
            let mut out = Vec::new();
            if alt {
                out.push(0x1b);
            }
            out.extend_from_slice(t.as_bytes());
            return out;
        }
    }
    Vec::new()
}

/// Encode one mouse event against the model's mode: SGR (1006) when
/// enabled, else X10. `button`: 0 left, 1 middle, 2 right, 64 wheel up,
/// 65 wheel down. `release` only differs for SGR (X10 reports button 3).
/// `None` when the app has not enabled mouse reporting.
pub fn encode_mouse_report(
    mode: &TermMode,
    button: u8,
    col: usize,
    row: usize,
    release: bool,
) -> Option<Vec<u8>> {
    if !mode.intersects(
        TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
    ) {
        return None;
    }
    let col = (col + 1).min(1023);
    let row = (row + 1).min(1023);
    if mode.contains(TermMode::SGR_MOUSE) {
        let final_ch = if release { 'm' } else { 'M' };
        Some(format!("\x1b[<{button};{col};{row}{final_ch}").into_bytes())
    } else {
        let btn = if release { 3 } else { button };
        Some(vec![
            b'\x1b',
            b'[',
            b'M',
            32 + btn,
            32 + col.min(95) as u8,
            32 + row.min(95) as u8,
        ])
    }
}

// ---- tab set ----

/// All open terminal tabs + the active index.
#[derive(Default)]
pub struct TerminalTabs {
    pub tabs: Vec<Terminal>,
    pub active: usize,
    /// Terminal profile dropdown visibility (bounded UI state).
    pub menu_open: bool,
}

impl TerminalTabs {
    pub fn is_open(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// Hard cap check shared by shell, command and agent tabs.
    fn ensure_capacity(&self) -> Result<(), TermError> {
        Self::capacity_for(self.tabs.len())
    }

    /// Pure cap kernel (tested without spawning processes).
    fn capacity_for(open_tabs: usize) -> Result<(), TermError> {
        if open_tabs >= MAX_TERM_TABS {
            return Err(TermError::TabLimit { max: MAX_TERM_TABS });
        }
        Ok(())
    }

    /// Spawn a new tab with the registry's default shell. Becomes active.
    pub fn open(
        &mut self,
        root: &Path,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), TermError> {
        let shell = default_shell();
        self.open_shell(root, &shell, size, proxy)
    }

    /// Spawn a specific shell as a new active tab.
    pub fn open_shell(
        &mut self,
        root: &Path,
        shell: &Path,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), TermError> {
        self.ensure_capacity()?;
        let term = Terminal::spawn(root, shell, size, proxy)?;
        self.push_tab(term);
        Ok(())
    }

    /// Spawn a labeled interactive program as a new active tab.
    pub fn open_program(
        &mut self,
        root: &Path,
        program: &Path,
        label: &str,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), TermError> {
        self.ensure_capacity()?;
        let term = Terminal::spawn_program(root, program, label, size, proxy)?;
        self.push_tab(term);
        Ok(())
    }

    /// Clone the active terminal's executable, arguments, and label.
    pub fn duplicate_active(
        &mut self,
        root: &Path,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), TermError> {
        self.duplicate_active_inner(root, size, move || {
            let _ = proxy.send_event(Wake::PtyData);
        })
    }

    #[cfg(test)]
    pub fn duplicate_active_with(
        &mut self,
        root: &Path,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), TermError> {
        self.duplicate_active_inner(root, size, wake)
    }

    fn duplicate_active_inner(
        &mut self,
        root: &Path,
        size: GridSize,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), TermError> {
        self.ensure_capacity()?;
        let active = self.active().ok_or(TermError::NoActiveTerminal)?;
        let program = active.program().to_path_buf();
        let args = active.args.clone();
        let label = active.label.clone();
        let term = Terminal::spawn_launch_with(root, &program, &args, Some(&label), size, wake)?;
        self.push_tab(term);
        Ok(())
    }

    /// Spawn a command tab (`shell -c command`) — the M8 git actions stream
    /// their output here. Becomes active.
    pub fn open_command(
        &mut self,
        root: &Path,
        label: &str,
        command: &str,
        size: GridSize,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), TermError> {
        self.ensure_capacity()?;
        let shell = default_shell();
        let term = Terminal::spawn_command(root, &shell, label, command, size, proxy)?;
        self.push_tab(term);
        Ok(())
    }

    /// Append a spawned tab, log it, and make it active.
    fn push_tab(&mut self, term: Terminal) {
        log::info!(
            "terminal tab opened ({}: {} @ {}×{})",
            term.label(),
            term.shell_name(),
            term.size.cols,
            term.size.rows
        );
        self.tabs.push(term);
        self.active = self.tabs.len() - 1;
    }

    /// Close the active tab: the Terminal's Drop tears down the PTY (SIGHUP
    /// to the process group + kill backstop + reap).
    /// Close a tab by index, clamping the active selection to a valid neighbor.
    pub fn close_at(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        log::info!("terminal tab closed ({} left)", self.tabs.len());
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if self.active > index {
            self.active -= 1;
        }
        if self.tabs.is_empty() {
            self.menu_open = false;
        }
    }

    pub fn active(&self) -> Option<&Terminal> {
        self.tabs.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Terminal> {
        self.tabs.get_mut(self.active)
    }

    /// Apply a deferred visual resize to the active PTY. Background tabs are
    /// resized when they become active and the next frame is composed.
    pub fn flush_active_resize(&mut self) -> bool {
        self.active_mut().is_some_and(Terminal::flush_pty_resize)
    }

    /// Drain every tab (background tabs keep their scrollback current).
    /// Returns whether any grid changed.
    pub fn drain_all(&mut self, max_chunks_per_tab: usize) -> bool {
        let mut changed = false;
        for tab in &mut self.tabs {
            changed |= tab.drain_output(max_chunks_per_tab);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::testutil::temp_dir;
    use std::path::Path;
    use std::time::{Duration, Instant};
    use winit::keyboard::ModifiersState;

    fn mods(ctrl: bool, alt: bool) -> ModifiersState {
        let mut m = ModifiersState::empty();
        if ctrl {
            m |= ModifiersState::CONTROL;
        }
        if alt {
            m |= ModifiersState::ALT;
        }
        m
    }

    // ---- keyboard encoding ----

    #[test]
    fn encode_key_printable_named_and_ctrl() {
        let mode = TermMode::empty();
        let plain = Key::Character("a".into());

        // Printable text → UTF-8 bytes.
        assert_eq!(encode_key(&plain, Some("a"), &mods(false, false), &mode), b"a");
        // Named Enter → CR (never the text's \r).
        assert_eq!(encode_key(&Key::Named(NamedKey::Enter), Some("\r"), &mods(false, false), &mode), b"\r");
        // Backspace → DEL (xterm convention).
        assert_eq!(encode_key(&Key::Named(NamedKey::Backspace), Some("\x08"), &mods(false, false), &mode), &[0x7f]);
        // Escape / Tab.
        assert_eq!(encode_key(&Key::Named(NamedKey::Escape), None, &mods(false, false), &mode), b"\x1b");
        assert_eq!(encode_key(&Key::Named(NamedKey::Tab), Some("\t"), &mods(false, false), &mode), b"\t");

        // Ctrl+letter → the C0 control byte (Ctrl+C = 0x03 = SIGINT).
        assert_eq!(
            encode_key(&Key::Character("c".into()), Some("c"), &mods(true, false), &mode),
            vec![0x03]
        );
        // Ctrl+Space → NUL.
        assert_eq!(
            encode_key(&Key::Named(NamedKey::Space), Some(" "), &mods(true, false), &mode),
            vec![0]
        );
        // Alt prefixes ESC (meta).
        assert_eq!(
            encode_key(&plain, Some("a"), &mods(false, true), &mode),
            vec![0x1b, b'a']
        );
        // Unlabeled keys produce nothing.
        assert!(encode_key(&Key::Named(NamedKey::F13), None, &mods(false, false), &mode).is_empty());
        // Control chars in text without a named key are never typed.
        assert!(encode_key(&Key::Named(NamedKey::F14), Some("\r"), &mods(false, false), &mode).is_empty());
    }

    #[test]
    fn encode_key_arrows_follow_cursor_mode_and_modifiers() {
        let normal = TermMode::empty();
        let app = TermMode::APP_CURSOR;
        let up = Key::Named(NamedKey::ArrowUp);

        // Normal mode: CSI A.
        assert_eq!(encode_key(&up, None, &mods(false, false), &normal), b"\x1b[A");
        // Application cursor mode: SS3 A.
        assert_eq!(encode_key(&up, None, &mods(false, false), &app), b"\x1bOA");
        // Ctrl+arrow: the xterm modifier parameter (1 + ctrl*4 = 5).
        assert_eq!(encode_key(&up, None, &mods(true, false), &normal), b"\x1b[1;5A");
        // Down/Left/Right.
        assert_eq!(encode_key(&Key::Named(NamedKey::ArrowDown), None, &mods(false, false), &normal), b"\x1b[B");
        assert_eq!(encode_key(&Key::Named(NamedKey::ArrowLeft), None, &mods(false, false), &normal), b"\x1b[D");
        assert_eq!(encode_key(&Key::Named(NamedKey::ArrowRight), None, &mods(false, false), &normal), b"\x1b[C");
        // Home/End/PgUp/PgDn/Del.
        assert_eq!(encode_key(&Key::Named(NamedKey::Home), None, &mods(false, false), &normal), b"\x1b[H");
        assert_eq!(encode_key(&Key::Named(NamedKey::End), None, &mods(false, false), &normal), b"\x1b[F");
        assert_eq!(encode_key(&Key::Named(NamedKey::PageUp), None, &mods(false, false), &normal), b"\x1b[5~");
        assert_eq!(encode_key(&Key::Named(NamedKey::Delete), None, &mods(false, false), &normal), b"\x1b[3~");
    }

    // ---- mouse encoding ----

    #[test]
    fn mouse_reports_require_mouse_mode() {
        let mode = TermMode::empty();
        assert!(encode_mouse_report(&mode, 0, 3, 2, false).is_none(), "no mouse mode → no report");
    }

    #[test]
    fn mouse_reports_sgr_when_enabled() {
        let mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        // Press: <M;col+1;row+1
        assert_eq!(
            encode_mouse_report(&mode, 0, 3, 2, false),
            Some(b"\x1b[<0;4;3M".to_vec())
        );
        // Release: m
        assert_eq!(
            encode_mouse_report(&mode, 0, 3, 2, true),
            Some(b"\x1b[<0;4;3m".to_vec())
        );
        // Wheel buttons (64 up / 65 down).
        assert_eq!(
            encode_mouse_report(&mode, 64, 0, 0, false),
            Some(b"\x1b[<64;1;1M".to_vec())
        );
    }

    #[test]
    fn mouse_reports_x10_fallback() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        // X10: ESC [ M <32+button> <32+col+1> <32+row+1>; release reports button 3.
        assert_eq!(
            encode_mouse_report(&mode, 0, 0, 0, false),
            Some(vec![b'\x1b', b'[', b'M', 32, 33, 33])
        );
        assert_eq!(
            encode_mouse_report(&mode, 0, 0, 0, true),
            Some(vec![b'\x1b', b'[', b'M', 35, 33, 33])
        );
    }

    // ---- registry ----

    #[test]
    fn registry_defaults_to_a_real_shell() {
        let reg = shell_registry();
        assert!(!reg.is_empty(), "this machine has a shell");
        assert_eq!(reg[0], default_shell(), "registry[0] is the default");
        assert!(reg[0].is_file(), "the default shell exists");
        // No duplicates.
        let mut sorted = reg.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), reg.len());
    }

    #[test]
    fn terminal_menu_excludes_github_and_split() {
        let labels: Vec<_> = terminal_menu_options()
            .iter()
            .map(|option| option.label)
            .collect();
        assert_eq!(
            labels,
            vec![
                "New Terminal",
                "New Terminal Window",
                "bash",
                "fish",
                "JavaScript Debug Terminal",
            ]
        );
    }

    #[test]
    fn duplicate_active_reuses_the_current_launch_program() {
        let d = temp_dir("duplicate-terminal");
        let size = GridSize { cols: 80, rows: 24 };
        let mut tabs = TerminalTabs::default();
        tabs.tabs.push(
            Terminal::spawn_program_with(d.path(), Path::new("/bin/cat"), "first", size, || {})
                .expect("spawn test terminal"),
        );
        tabs.active = 0;

        tabs.duplicate_active_with(d.path(), size, || {})
            .expect("duplicate active terminal");
        assert_eq!(tabs.tabs.len(), 2);
        assert_eq!(tabs.active, 1);
        assert_eq!(tabs.active().expect("active").label(), "first");
        assert_eq!(tabs.active().expect("active").program(), Path::new("/bin/cat"));
    }

    #[test]
    fn closing_a_tab_by_index_selects_a_valid_neighbor() {
        let d = temp_dir("close-terminal-index");
        let size = GridSize { cols: 80, rows: 24 };
        let mut tabs = TerminalTabs::default();
        for label in ["first", "second", "third"] {
            tabs.tabs.push(
                Terminal::spawn_program_with(d.path(), Path::new("/bin/cat"), label, size, || {})
                    .expect("spawn test terminal"),
            );
        }
        tabs.active = 2;
        tabs.close_at(1);
        assert_eq!(tabs.tabs.len(), 2);
        assert_eq!(tabs.active, 1);
        assert_eq!(tabs.active().expect("active").label(), "third");
    }

    #[test]
    fn external_terminal_command_uses_the_project_root() {
        let root = Path::new("/tmp/ark-terminal-window");
        let mut command = std::process::Command::new("konsole");
        configure_external_terminal(&mut command, Path::new("/usr/bin/konsole"), root);
        assert_eq!(command.get_current_dir(), Some(root));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["--workdir", "/tmp/ark-terminal-window"]));
    }

    #[test]
    fn default_shell_prefers_fish_when_installed() {
        let fish = Path::new("/usr/bin/fish");
        if fish.is_file() {
            assert_eq!(default_shell(), fish);
        }
    }

    #[test]
    fn terminal_tab_cap_rejects_beyond_the_limit() {
        assert!(TerminalTabs::capacity_for(0).is_ok());
        assert!(TerminalTabs::capacity_for(MAX_TERM_TABS - 1).is_ok());
        let err = TerminalTabs::capacity_for(MAX_TERM_TABS).expect_err("at the cap");
        assert!(matches!(err, TermError::TabLimit { max } if max == MAX_TERM_TABS));
        assert!(TerminalTabs::capacity_for(MAX_TERM_TABS + 5).is_err());
        assert!(err.to_string().contains("tab limit"), "{err}");
    }

    // ---- LIVE PTY round-trip (a real bash, a real echo) ----

    /// Drain with a timeout: poll until the grid contains `needle` (the shell
    /// needs a moment to start + run the command).
    fn wait_for_text(term: &mut Terminal, needle: &str, budget: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < budget {
            term.drain_output(256);
            if visible_text(term).contains(needle) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        visible_text(term).contains(needle)
    }

    /// The visible viewport as text (rows joined with \n) — test-only read.
    fn visible_text(term: &Terminal) -> String {
        let content = term.term.renderable_content();
        let mut rows: Vec<Vec<char>> = vec![Vec::new(); term.size.rows];
        for indexed in content.display_iter {
            if let Some(vp) = term::point_to_viewport(content.display_offset, indexed.point) {
                if vp.line < rows.len() {
                    let row = &mut rows[vp.line];
                    if row.len() <= vp.column.0 {
                        row.resize(vp.column.0 + 1, ' ');
                    }
                    row[vp.column.0] = indexed.cell.c;
                }
            }
        }
        rows.into_iter()
            .map(|r| r.into_iter().collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn primary_device_attribute_query_writes_response_to_pty() {
        let d = temp_dir("pty-primary-da");
        let response = d.path().join("da-response");
        let command = format!(
            "stty raw -echo; printf '\\033[0c'; dd bs=1 count=5 of={} 2>/dev/null",
            crate::git::shell_quote(&response.to_string_lossy())
        );
        let mut term = Terminal::spawn_command_with(
            d.path(),
            Path::new("/bin/sh"),
            "device attributes",
            &command,
            GridSize::new(80, 24),
            || {},
        )
        .expect("spawn the query responder");

        let start = Instant::now();
        let mut reply = Vec::new();
        while start.elapsed() < Duration::from_secs(2) {
            term.drain_output(256);
            if let Ok(bytes) = std::fs::read(&response) {
                if bytes.len() == 5 {
                    reply = bytes;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            reply, b"\x1b[?6c",
            "the terminal model's Primary Device Attribute reply reaches the shell; grid={:?}",
            visible_text(&term)
        );
    }

    #[test]
    fn command_tab_streams_output_and_carries_its_label() {
        let d = temp_dir("cmd-tab");
        let mut term = Terminal::spawn_command_with(
            d.path(),
            &default_shell(),
            "git commit",
            "echo ark-command-ok",
            GridSize::new(80, 24),
            || {},
        )
        .expect("spawn the command tab");

        assert_eq!(term.label(), "git commit", "the tab carries the action label");
        assert!(
            wait_for_text(&mut term, "ark-command-ok", Duration::from_secs(5)),
            "command output missing: {:?}",
            visible_text(&term)
        );
    }

    #[cfg(unix)]
    #[test]
    fn program_tab_runs_in_the_project_dir_under_its_label() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp_dir("program-tab");
        let script = d.path().join("program");
        std::fs::write(&script, "#!/bin/sh\npwd\necho ark-program-ok\n").expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let mut term = Terminal::spawn_program_with(
            d.path(),
            &script,
            "program",
            GridSize::new(80, 24),
            || {},
        )
        .expect("spawn the program");
        assert_eq!(term.label(), "program");
        assert!(
            wait_for_text(&mut term, "ark-program-ok", Duration::from_secs(5)),
            "program output missing: {:?}",
            visible_text(&term)
        );
        let text = visible_text(&term);
        assert!(
            text.contains(&d.path().to_string_lossy().to_string()),
            "agent runs in the project root; saw {text:?}"
        );
    }

    #[test]
    fn visual_resize_defers_and_coalesces_the_pty_resize() {
        let d = temp_dir("pty-visual-resize");
        let initial = GridSize::new(80, 24);
        let mut term = Terminal::spawn_program_with(
            d.path(),
            Path::new("/bin/cat"),
            "test",
            initial,
            || {},
        )
        .expect("spawn test terminal");

        assert!(term.resize_visual(GridSize::new(100, 30)));
        assert_eq!(term.size(), GridSize::new(100, 30), "the model follows the panel");
        assert_eq!(term.pty_size(), initial, "fish still sees the old grid during a drag");

        assert!(term.resize_visual(GridSize::new(120, 40)));
        assert!(term.flush_pty_resize(), "the final drag size reaches the PTY once");
        assert_eq!(term.pty_size(), GridSize::new(120, 40));
        assert!(!term.flush_pty_resize(), "an unchanged PTY size is a no-op");
    }

    #[test]
    fn pty_round_trip_echo_and_group_kill() {
        let d = temp_dir("pty");
        let size = GridSize::new(80, 24);
        // This test exercises the PTY + process-group teardown, not the user's
        // shell dialect: `$!` and `&` are POSIX, while fish (a common $SHELL)
        // spells the last pid `$last_pid`. Use /bin/sh when present.
        let shell = if Path::new("/bin/sh").is_file() {
            PathBuf::from("/bin/sh")
        } else {
            default_shell()
        };
        let mut term =
            Terminal::spawn_with(d.path(), &shell, size, || {}).expect("spawn the shell");

        // A real command echoes into the grid.
        term.key_input(b"echo ark-pty-ok\r");
        assert!(
            wait_for_text(&mut term, "ark-pty-ok", Duration::from_secs(5)),
            "output missing: {:?}",
            visible_text(&term)
        );

        // Resize round-trip (model + pty size).
        assert!(term.resize(GridSize::new(60, 20)));
        assert_eq!(term.size(), GridSize::new(60, 20));
        assert!(!term.resize(GridSize::new(60, 20)), "same size: no-op");

        // Process-group teardown: a background sleep dies with the tab.
        term.key_input(b"sleep 300 & echo $!\r");
        let mut pid = None;
        let start = Instant::now();
        while pid.is_none() && start.elapsed() < Duration::from_secs(5) {
            term.drain_output(256);
            for line in visible_text(&term).lines() {
                let t = line.trim();
                if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
                    pid = t.parse::<u32>().ok();
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        let pid = pid.expect("background job pid printed");
        let proc = format!("/proc/{pid}");
        assert!(Path::new(&proc).exists(), "sleep {pid} alive before teardown");
        let shell_pid = term.child.as_ref().and_then(|c| c.process_id());
        drop(term);
        // Poll for BOTH pids (the reaper grants the shell a short HUP-
        // forwarding grace, then kills + reaps — delivery races assertions).
        let mut gone = vec![pid];
        if let Some(sp) = shell_pid {
            gone.push(sp);
        }
        let start = Instant::now();
        loop {
            if gone.iter().all(|p| !Path::new(&format!("/proc/{p}")).exists()) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(4),
                "the process group died with the tab (jobs + shell); gone={gone:?} alive={:?}",
                gone.iter().filter(|p| Path::new(&format!("/proc/{p}")).exists()).collect::<Vec<_>>()
            );
            thread::sleep(Duration::from_millis(25));
        }
    }
}
