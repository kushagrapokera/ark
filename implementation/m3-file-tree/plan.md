# M3 — Lazy virtualized file tree panel

Master plan: `plan.md` (root), milestone M3.

## M2 carry-forwards (binding on this milestone)

1. **Split `src/ui.rs`** (1,519 lines > 400-line guidance) into `src/ui/`
   modules with identical public API (re-exports; call sites untouched).
2. **Partial damage**: tree scroll marks the tree-panel rect via
   `Damage::mark` — removes the `#[allow(dead_code)]` on `render.rs::Damage`.
3. **main.rs ≤ 400 lines** after the demo scene is replaced by the real
   layout (scene glue extracted).

## Constraints (AGENTS.md — binding)

- **ZERO new dependencies.** Directory listing = `std::fs::read_dir`.
- Fixed-size state + content-bounded caches only: the tree holds listings
  for **expanded directories only**; collapsing a directory frees its
  cached listing (tested).
- No file watcher in M3 (that is M5 — plan order is deliberate). The tree
  is allowed to go stale until then.
- No git badges in M3 (that is M7). Hidden files and `.git` are listed
  honestly; filtering/dimming comes with git integration.
- Keyboard tree navigation deferred to M4 (mouse-only tree in M3).

## Frozen interfaces (implement verbatim)

```rust
// src/fs.rs — lazy, one directory level at a time
pub struct DirEntry { pub name: String, pub path: PathBuf, pub is_dir: bool }
pub struct DirListing { pub entries: Vec<DirEntry>, pub truncated: bool }
/// Guard against pathological directories; truncation sets `truncated`.
pub const MAX_ENTRIES_PER_DIR: usize = 50_000;
/// Sorted dirs-first, then files, each case-insensitive alphabetical.
/// Includes hidden files. Broken symlinks: is_dir=false, never followed
/// as dirs (no cycle risk: listings are per-expansion, never recursive).
pub fn read_dir_sorted_with_cap(dir: &Path, cap: usize) -> io::Result<DirListing>;
pub fn read_dir_sorted(dir: &Path) -> io::Result<DirListing>;  // uses the const

// src/tree.rs — state model (rendering joins in Task 2)
pub const ROW_H: f64 = 22.0;  // logical px per visible row
#[derive(Clone, Debug)]
pub struct RowRef<'a> {
    pub path: &'a Path, pub name: &'a str,
    pub is_dir: bool, pub depth: usize, pub expanded: bool,
}
pub struct FileTree {
    pub root: PathBuf,
    pub offset: f64,                  // logical px scroll offset
    pub last_error: Option<String>,   // root-listing failure surface (status line)
    /* private: expanded: HashSet<PathBuf>, children: HashMap<PathBuf, DirListing> */
}
impl FileTree {
    pub fn new(root: &Path) -> Self;              // lists root eagerly; error → empty + last_error
    pub fn toggle(&mut self, path: &Path, is_dir: bool) -> bool;  // dir: expand↔collapse (collapse FREES the cached listing, returns changed); file: false
    pub fn is_expanded(&self, path: &Path) -> bool;
    pub fn expanded_dir_count(&self) -> usize;    // status-line + memory tests
    pub fn visible_rows(&self) -> Vec<RowRef<'_>>;  // depth-first over expanded set, root children first, depth 0
    pub fn visible_count(&self) -> usize;
    pub fn scroll_by(&mut self, dy: f64, viewport: f64) -> bool;   // clamps to [0, max(0, visible_count*ROW_H − viewport)]
    pub fn set_offset(&mut self, v: f64, viewport: f64) -> bool;
    pub fn select(&mut self, path: &Path);        // does NOT expand dirs (only toggle does)
    pub fn clear_selection(&mut self);
}
```

## Tasks (fresh implementer per task)

- **Task 1 — Refactor + state core:** split `src/ui.rs` → `src/ui/` (mod.rs
  re-exports; public API and all call sites unchanged; zero behavior
  change); create `src/fs.rs` + `src/tree.rs` state model with TDD tests
  (fs sorting/cap/errors; tree transitions, cache eviction, depth-first
  rows, scroll clamping, selection semantics).
- **Task 2 — Tree panel + real layout:** tree panel rendering
  (viewport-only rows, indent guides by depth, chevron/file icons, hover +
  selection backgrounds) + pointer interaction (row hit-testing →
  toggle/select, scrollbar → offset, wheel → scroll_by) + real app layout
  (Split: tree | placeholder, status line with tree stats) + partial damage
  (tree-scroll marks the panel rect; removes the render.rs allow) +
  main.rs ≤ 400 lines.

## Acceptance criteria (milestone)

- `cargo test` — all previous 48 tests green + new fs/tree/panel tests
- `cargo clippy --release --tests -- -D warnings` clean; `cargo build --release` clean
- ui.rs is gone → `src/ui/` with ≤400-line files; call sites unchanged
- Tree opens the real project dir in ARK_SMOKE + screenshot: icons, indent,
  expand/collapse state correct; status line exact
- Partial-damage path exercised (Damage::mark used; allow removed)
- Collapse frees listings (memory test); ≥50k-entry dirs truncate with flag
- RSS re-measured (≤100MB budget intact)
