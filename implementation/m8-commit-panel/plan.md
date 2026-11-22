# M8 — Commit & push panel

Master plan: `plan.md` (root), milestone M8. Feature 5: changed-file
checkboxes → stage the checked paths (`git add -- <paths>`, per-file
granularity) → commit message → Commit → Push; output streams into a git
terminal tab. Clicks only, zero commands typed by the human.

## Carry-forwards

1. `arboard` X11 clipboard fallback — still deferred.
2. Oversized cohesive files (term.rs/main.rs) — accepted per user directive.
3. IME in the commit-message field — deferred to M9 (single-line ASCII/UTF-8
   text input only, same `TextInput` model the config editor will extend).

## New dependencies

NONE. Git still runs as a subprocess (here: through the existing PTY
terminal, so output streams); `git add/commit/push` command lines are built
with shell quoting in `git.rs`.

## Constraints (AGENTS.md — binding)

- Fixed-size state: the changed-file list is the bounded `GitStatus`
  snapshot (100k cap); the panel keeps only user-UNCHECKED paths (pruned to
  the snapshot on every status apply) and one bounded `TextInput` message.
  The visible list is viewport-only (`scroll` is a row index).
- Git runs out of the UI thread. Commit/push run in a PTY tab (the M6
  machinery), so output is streamed by the reader thread — no capture
  buffers in ark.
- Clicks only: no command input surface. File rows toggle, the message field
  is the only typed text, Commit/Push are buttons.
- Shell quoting is an injection boundary: repo-relative paths and the
  commit message are single-quoted with embedded `'` escaped.

## Frozen interfaces (implement verbatim)

```rust
// src/commit.rs — panel state + pure command validation
pub const PANEL_H: f64 = 236.0;
pub const ROW_H: f64 = 20.0;

pub enum CommitError { NoSelection, EmptyMessage }   // Display + Error

pub struct CommitPanel {
    pub message: TextInput,      // 128-char cap (ui::TextInput)
    pub scroll: usize,           // first visible change row
    pub error: Option<String>,   // validation message, cleared on edit
    /* private: unchecked: HashSet<PathBuf> */
}
impl CommitPanel {
    pub fn is_checked(&self, path: &Path) -> bool;      // default true
    pub fn toggle(&mut self, path: &Path);
    pub fn selected_paths(&self, status: &GitStatus) -> Vec<PathBuf>;  // repo-relative
    pub fn retain(&mut self, status: &GitStatus);       // prune unchecked + scroll
    pub fn scroll_by(&mut self, rows: i64, visible: usize, total: usize) -> bool;
    pub fn commit_command(&self, status: &GitStatus) -> Result<String, CommitError>;
    pub fn handle_key(&mut self, key: &winit::keyboard::Key, text: Option<&str>) -> bool;
}

pub struct PanelLayout { title, list, rows, field, commit_button, push_button: Rect/usize }
pub fn layout(panel: Rect) -> PanelLayout;   // pure, pinned

pub struct PanelEffects { pub toggled: bool, pub commit: bool, pub push: bool }
impl CommitPanel {
    pub fn draw(&mut self, canvas, text, theme, scale, panel: Rect,
                status: &GitStatus, pointer: Pointer) -> PanelEffects;
}

// src/git.rs — command builders (still subprocess-only)
pub fn shell_quote(s: &str) -> String;
pub fn stage_and_commit_command(paths: &[PathBuf], message: &str) -> String;
pub const PUSH_COMMAND: &str = "git push";

// src/term.rs — command tabs (stream output through the existing PTY)
impl Terminal {
    pub fn spawn_command(root, shell, label: &str, command: &str, size,
                         proxy) -> Result<Terminal, TermError>;
    pub fn label(&self) -> &str;
}
impl TerminalTabs {
    pub fn open_command(&mut self, root, label, command, size, proxy)
        -> Result<(), TermError>;
}

// src/frame.rs
pub fn app_layout_with_commit(w, h, split_x, show_commit: bool) -> AppLayout;
// AppLayout gains `commit_panel: Rect` (zero-height when hidden).
// Frame gains `commit: &mut CommitPanel`; FrameEffects gains
// `commit_toggled`, `commit_clicked`, `push_clicked`.
```

## Layout / rendering

- The left column splits: tree on top, commit panel (236 logical px, capped
  at 60% of the content height) at the bottom when `git.is_some()`.
  Non-repo roots keep the full-height tree.
- Panel: title `Changes <n>[ · <k> sel]` (or the validation error in
  `git_deleted`), viewport-only file rows (checkbox + badge letter + relative
  path), message field, Commit/Push buttons side by side.
- Checkboxes default checked; the whole row is the click target. Badge
  colors reuse the M7 `Theme` git tokens via `tree::badge_glyph`.
- The message field takes keyboard focus on click; any other click blurs it.
  Escape blurs. Typing clears the validation error.
- Commit: checked paths + a non-empty message are required. The button
  opens a `git commit` terminal tab running
  `git add -- <checked> && git commit -m <message>`.
- Push: opens a `git push` terminal tab. Both tabs stream output through the
  M6 PTY reader and become active so the outcome is visible.

## Tasks (inline; TDD, tests first)

1. `git.rs` command builders + quoting tests.
2. `commit.rs` pure model (selection, retain, scroll, validation, key
   routing) + live stage/commit/push tests against temp repos.
3. Panel layout + raster/hit-test tests (pixels, toggle, focus, buttons).
4. `term.rs` command tabs (+ live `echo` stream test, label).
5. Frame/App wiring: layout split, effects, commit/push dispatch, keyboard
   routing, wheel scrolling, status pruning.
6. Gates + live verification (real repo: edit two files, commit selected,
   push to a bare remote through the panel's command strings).

## Acceptance criteria

- `cargo test` green (baseline PTY timing test included), clippy clean,
  release build 0 warnings, `ARK_SMOKE=1` exit 0.
- Panel shows changed files with checked boxes; unchecking excludes a path
  from `git add`; an empty message or empty selection shows a validation
  error and spawns nothing.
- Commit creates exactly the selected content with the typed message; Push
  updates the remote; both stream into a labelled git terminal tab.
- Non-repo root: no panel, tree unchanged.
- RSS re-measured; ≤100MB intact. No unbounded state added.
