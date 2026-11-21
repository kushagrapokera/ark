# M4 — Read-only viewer

Master plan: `plan.md` (root), milestone M4. This is the first genuinely
usable ark: double-click a file in the tree, read it with highlighting,
select and copy text.

## M3 carry-forwards (binding)

1. `src/text.rs` (438 lines, M1/M2 pre-existing) exceeds the ~400 guidance —
   split when this milestone touches it, otherwise carry to M5. (User note:
   crossing 400 is acceptable "if it's worth it" — no forced split.)
2. Gated frozen surfaces (`is_expanded`, `clear_selection`, `RowHit.y`,
   `Scrollbar::scroll_by`) stay gated until their consumers land.

## New dependencies (frozen stack per plan.md — RAM justifications)

| Crate | Purpose | RAM justification |
|---|---|---|
| `memmap2 0.9` | mmap file bytes (viewer) | zero-copy; RSS grows only by touched pages; no transitive deps |
| `regex 1` | token highlighting | compiled rule set built once (bounded); matching runs on VISIBLE lines only (~40/frame) |
| `wl-clipboard-rs 0.9` | selection copy (native Wayland data-control) | serving thread only while ownership held; native protocol, no subprocess poll |

`arboard` (X11/XWayland fallback per the frozen stack) is deliberately
deferred: the target platform is KDE Wayland; the fallback joins when an X11
session actually needs support — the cheapest memory is a library never
loaded. Recorded as a deviation from the stack table.

## Constraints (AGENTS.md — binding)

- Viewer is READ-ONLY: no editing affordances of any kind (guardrail 2).
- Fixed-size state: line index is `Vec<u32>` (view capped at 64 MiB);
  LRU cache holds ≤ 8 viewers; eviction DROPS mmap + index (tested).
- Viewport-only rendering: only visible lines are shaped/rasterized.
- No file watcher (M5), no git marks (M7 — the gutter strip is reserved,
  unpainted), no tabs (M6+), no chunked paging UI: >64 MiB views a preview
  of the first 64 MiB with a `truncated` flag.
- Line-based scrolling (integer lines, no partial-line rows) — the
  monospace-grid choice that avoids all text-clipping machinery; pixel
  smoothness is deliberately traded away in M4.

## Frozen interfaces (implement verbatim)

```rust
// src/viewer.rs — read-only viewer state model
pub const VIEW_CAP_BYTES: u64 = 64 * 1024 * 1024;
pub const BINARY_SNIFF_BYTES: usize = 8 * 1024;
/// Logical px per viewer text line (UI_FONT_SIZE × 1.35).
pub const LINE_H: f64 = 18.9;

#[derive(Debug)]
pub enum ViewerError { Binary { path: PathBuf }, Io(std::io::Error) }  // Display + Error impls

pub struct Viewer { /* map: Mmap, view_len, truncated, starts: Vec<u32>, offset_lines, sel */ }
impl Viewer {
    pub fn open(path: &Path) -> Result<Viewer, ViewerError>;              // uses VIEW_CAP_BYTES
    pub fn open_with_cap(path: &Path, cap: u64) -> Result<Viewer, ViewerError>;
    pub fn path(&self) -> &Path;
    pub fn truncated(&self) -> bool;
    pub fn line_count(&self) -> usize;                                    // empty file → 0
    pub fn line_text(&self, line: usize) -> Option<Cow<str>>;             // lossy, trailing \r stripped
    pub fn offset_lines(&self) -> usize;
    pub fn max_offset_lines(&self, viewport_lines: usize) -> usize;
    pub fn scroll_by_lines(&mut self, dy: i64, viewport_lines: usize) -> bool;
    pub fn set_offset_lines(&mut self, v: usize, viewport_lines: usize) -> bool;
    pub fn selection(&self) -> Option<(usize, usize, usize, usize)>;      // (a_line, a_col, h_line, h_col)
    pub fn selection_len(&self) -> usize;                                 // chars
    pub fn selection_text(&self) -> Option<String>;                       // lines joined \n, bytes verbatim for full lines
    pub fn select_at(&mut self, line: usize, col: usize);                 // anchor=head (collapse), clamped
    pub fn extend_select(&mut self, line: usize, col: usize);             // head moves, clamped
    pub fn pos_at(&self, line: usize, col: usize) -> (usize, usize);      // clamped to content
}
pub fn copy_to_clipboard(text: &str) -> Result<(), std::io::Error>;       // wl-clipboard-rs, serving thread

// src/state.rs — LRU buffer cache
pub struct BufferCache { /* entries: HashMap<PathBuf, Viewer>, order: VecDeque<PathBuf>, cap */ }
impl BufferCache {
    pub fn new(cap: usize) -> Self;
    pub fn get_or_open(&mut self, path: &Path) -> Result<&mut Viewer, ViewerError>;  // hit promotes
    pub fn contains(&self, path: &Path) -> bool;
    pub fn len(&self) -> usize;
}

// src/highlight.rs — regex token highlighter (visible lines only)
pub enum Token { Keyword, String, Number, Comment }
pub struct Span { pub start: usize, pub end: usize, pub token: Token }    // char ranges
pub struct Highlighter { /* compiled rule set, built once */ }
impl Highlighter {
    pub fn new() -> Self;
    pub fn line_spans(&self, line: &str) -> Vec<Span>;                    // non-overlapping, sorted
}

// src/tree.rs — interaction evolution (was `-> bool`)
pub enum TreeAction { None, Toggled, Selected, Opened(PathBuf) }
pub fn handle_pointer(tree: &mut FileTree, panel: Rect, pointer: Pointer, double: bool) -> TreeAction;
```

## Tasks (inline implementer; TDD, tests first)

- **Task 1 — viewer core + LRU + highlighter:** viewer state model (open
  with cap, binary sniff, line index, scroll, selection) + BufferCache +
  Highlighter + Theme syntax tokens, each TDD.
- **Task 2 — viewer panel + wiring:** draw_viewer_panel (gutter with
  reserved mark strip, viewport line grid, selection bgs, token colors) +
  frame integration (viewer replaces the placeholder when a file is open;
  status-line branches) + tree double-click opens files through the LRU
  cache + Ctrl+C copy + viewer scrollbar/wheel + damage contract
  (viewer scroll → viewer rect only; selection rides event-level full marks).

## Acceptance criteria (milestone)

- `cargo test` — all previous tests green + new viewer/state/highlight/panel
  tests; `cargo clippy --release --tests -- -D warnings` clean;
  `cargo build --release` clean, 0 warnings.
- Double-click a tree file → content renders with line-number gutter and
  token colors; scrolling only moves the viewport (viewport-only ink test);
  drag-select renders the selection background; Ctrl+C copies it
  (manual wl-paste verification; ignored-by-default compositor test).
- Binary files refused; >64 MiB files truncate with flag (cap-parametrized
  tests); LRU eviction drops closed buffers (len bounded, tested).
- RSS re-measured (≤100MB budget intact); ARK_SMOKE exit 0; screenshot.
- No editing affordances anywhere (read-only guardrail).
