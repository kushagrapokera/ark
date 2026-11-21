# M6 — Terminals

Master plan: `plan.md` (root), milestone M6. Real PTYs, the alacritty grid
rendered with our font stack, keyboard/mouse routing, a tab strip, shell
registry, and frame budgeting under output storms.

## M4/M5 carry-forwards

1. `arboard` (X11 clipboard fallback) — still deferred.
2. `main.rs` size — accepted per user directive where concerns are cohesive.

## New dependencies (frozen stack, RAM justifications)

- `portable-pty 0.9` — PTY lifecycle; one reader thread per terminal tab
  pumping a BOUNDED channel (backpressure: the child blocks on write when
  the buffer fills — no byte dropped, no unbounded buffer).
- `alacritty_terminal 0.26` (pulls `vte`) — VT parser + grid model with a
  capped scrollback (`scrolling_history` = 2000, memory rule 3); ark renders
  the grid itself (viewport-only runs of cells).

## Constraints (AGENTS.md — binding)

- Fixed-size state: grid = cols×rows (pane-derived), scrollback capped
  (2000), PTY buffer bounded, no per-event growth anywhere.
- Viewport-only rendering: only visible grid rows are shaped, grouped into
  same-style runs (a row of uniform cells is ONE shaped run).
- Frame budgeting: terminal rasters cap at ~30fps under output storms
  (33 ms floor between terminal-area rasters; pending damage waits).
- Closing a tab tears down the PTY: dropping the master sends SIGHUP to the
  session (POSIX process-group death) + `child.kill()` backstop + `wait()`
  reaps.
- No new deps beyond the frozen stack.

## Frozen interfaces (implement verbatim)

```rust
// src/watch.rs — Wake becomes an enum (fs + pty share the proxy)
pub enum Wake { FsChanged, PtyData }

// src/term.rs
pub struct GridSize { pub cols: usize, pub rows: usize }         // impl Dimensions
pub struct NoopListener;                                          // impl EventListener
pub fn default_shell() -> PathBuf;                                // $SHELL, else /bin/bash
pub fn shell_registry() -> Vec<PathBuf>;                          // existing: $SHELL, bash, zsh, fish

#[derive(Debug)] pub enum TermError { Io(std::io::Error), Pty(anyhow::Error) }
impl Display + Error for TermError

pub struct Terminal { /* master, writer, child, term, processor, rx, size, exited */ }
impl Terminal {
    pub fn spawn(root: &Path, shell: &Path, size: GridSize, proxy: EventLoopProxy<Wake>)
        -> Result<Terminal, TermError>;      // pty + shell + reader thread
    pub fn resize(&mut self, size: GridSize) -> bool;   // pty + term; false when unchanged
    pub fn key_input(&mut self, bytes: &[u8]);          // writer + flush
    pub fn drain_output(&mut self, max_chunks: usize);  // parse bounded chunks into the grid
    pub fn exited(&self) -> bool;
    pub fn shell_name(&self) -> &str;
    pub fn size(&self) -> GridSize;
    pub fn scroll_display(&mut self, lines: i32);       // scrollback, clamped by the model
    pub fn mode(&self) -> TermMode;
}
impl Drop for Terminal;                                   // master drop + kill + wait (reap)

pub struct TerminalTabs { pub tabs: Vec<Terminal>, pub active: usize }
impl TerminalTabs {
    pub fn is_open(&self) -> bool;
    pub fn open(&mut self, root: &Path, size: GridSize, proxy: EventLoopProxy<Wake>) -> Result<(), TermError>;
    pub fn close_active(&mut self);                       // kill + reap + fix index
    pub fn active_mut(&mut self) -> Option<&mut Terminal>;
    pub fn active(&self) -> Option<&Terminal>;
    pub fn drain_all(&mut self, max_chunks_per_tab: usize) -> bool;   // true: any grid changed
}

// keyboard encoding (pure, tested against TermMode)
pub fn encode_key(logical: &Key, text: Option<&str>, mods: &ModifiersState, mode: &TermMode) -> Vec<u8>;
// mouse encoding (pure, tested)
pub fn encode_mouse_report(mode: &TermMode, button: u8, col: usize, row: usize, release: bool) -> Option<Vec<u8>>;
// grid → canvas (viewport-only runs)
pub fn draw_terminal_grid(canvas, text, theme, scale, rect, term: &Terminal, show_cursor: bool);
```

## App wiring (main.rs)

- `Wake` enum: `FsChanged` (M5 path unchanged) / `PtyData` (drain pump).
- `Ctrl+T` opens a terminal tab (shell registry default, root cwd);
  `Ctrl+W` closes the active tab. Keys route to the PTY only while the
  terminal pane is focused (click inside the grid focuses; click elsewhere
  unfocuses). Focused: every key (Ctrl+C included → 0x03) goes to the shell.
  Unfocused: M4 app keys (Ctrl+C copy).
- Mouse over the grid: when the app enables mouse reporting → encoded
  reports (SGR 1006 / X10 fallback); otherwise wheel → scrollback.
- Frame budget: terminal-area damage waits for a 33 ms floor between
  rasters under storms (pending damage persists; the next wake paints).
- Right pane: terminal area (tab strip + close/plus buttons + grid) while
  tabs exist, else the M4 viewer. Status branch: `… · term <shell> <k>/<n>
  [exited]`.

## Acceptance criteria (milestone)

- `cargo test` all green (+ encoding/registry/grid tests + a LIVE PTY test:
  spawn bash, run `echo ark-pty-ok`, drain, assert the grid shows it);
  clippy clean; release build 0 warnings; ARK_SMOKE exit 0.
- Live run: Ctrl+T opens a shell; typing works (`ls`, `echo`); wheel
  scrolls scrollback; `vim`/`less` (mouse-reporting apps) receive wheel as
  reports; closing the tab kills the shell process group.
- Memory: scrollback capped at 2000 lines; PTY channel bounded; RSS
  re-measured ≤100MB.
