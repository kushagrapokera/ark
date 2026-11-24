# Split editor design

Date: 2026-11-24

## Goal

Add two side-by-side, independently scrollable read-only editor groups with
per-group tab strips and close controls. Reuse the existing bounded viewer cache
and preserve ark's read-only/whitelist boundaries.

## Decisions

- Maximum **two** editor groups. The Split action toggles between one and two.
- Each group owns an ordered, bounded tab list and an active tab.
- A file has **one** cached `Viewer` and appears in at most one group. Opening a
  file already visible in the other group focuses that group instead of making
  a duplicate mmap/index.
- Different files in the two groups retain independent scroll and selection
  state because they are different cached `Viewer` values.
- The global `BufferCache` remains the only owner of file mappings/indexes.
  Visible group paths are pinned during cache eviction.
- The config editor remains a single instance. Its path is a tab in one group;
  the active group determines keyboard/IME routing.
- Split is available only when the editor card is at least 640 logical px wide.
  The divider ratio is clamped so neither group becomes unusably narrow.
- No horizontal/grid layouts, tab drag-and-drop, more than two groups, duplicate
  same-file views, or code editing.

## Architecture

### New state module: `src/editor_groups.rs`

```rust
pub const MAX_EDITOR_GROUPS: usize = 2;
pub const MAX_TABS_PER_GROUP: usize = 9; // 8 viewer tabs + 1 config tab

pub struct EditorGroup {
    pub tabs: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    pub error: Option<String>,
}

pub struct EditorGroups {
    pub groups: Vec<EditorGroup>,
    pub active: usize,
    pub divider: f64, // 0.0..1.0, default 0.5
}
```

Pure methods:

- `new()`
- `is_split()`, `active_group()`, `active_path()`
- `visible_paths() -> Vec<&Path>`
- `open(path) -> OpenResult` where `OpenResult` is `Opened(usize)` or
  `FocusedExisting(usize)`
- `select(group, path) -> Option<usize>` (returns the group that owns the path)
- `close_tab(group, path) -> Option<PathBuf>` (returns a path eligible for cache
  eviction)
- `close_secondary_group() -> Vec<PathBuf>`
- `toggle_split() -> Vec<PathBuf>` (returns secondary tabs to evict when
  collapsing)
- `set_divider(value)`, `clamp_divider(min_ratio, max_ratio)`

Tab lists are capped per group. When a group is full, opening a new tab removes
that group's oldest non-active tab and returns it for cache eviction if no other
group owns it.

### Cache changes: `src/state.rs`

Add pinned opening and multi-borrow APIs:

```rust
pub fn get_or_open_pinned(
    &mut self,
    path: &Path,
    pinned: &[&Path],
) -> Result<&mut Viewer, ViewerError>;

pub fn viewers_mut(&mut self, paths: &[&Path]) -> Vec<&mut Viewer>;
```

Eviction skips pinned paths. The aggregate index budget remains enforced for
non-visible entries; if the two visible viewers alone exceed it, both remain
visible rather than blanking a pane.

### Layout changes: `src/frame.rs`

```rust
pub struct EditorGroupLayout {
    pub tabs: Rect,
    pub breadcrumb: Rect,
    pub body: Rect,
    pub minimap: Rect,
}

pub struct AppLayout {
    // existing fields retained for unsplit compatibility
    pub editor_groups: [EditorGroupLayout; 2],
    pub editor_group_count: usize,
    pub editor_group_split: Rect,
}
```

Unsplitted layouts mirror group 0 into the existing `editor_tabs`, `breadcrumb`,
`viewer_pane`, and `minimap` fields. Split layouts divide the editor card with an
8 px gap and a draggable hit strip. Group headers/breadcrumbs use the existing
compact/full height rules.

### Frame changes: `src/frame.rs`, `src/frame/chrome.rs`

`Frame` receives:

```rust
pub groups: &'a [EditorGroup],
pub active_group: usize,
pub group_scrollbars: &'a mut [Scrollbar],
pub viewers: [Option<&'a mut Viewer>; 2],
pub group_split: &'a mut f64,
pub group_split_drag: &'a mut SplitDrag,
```

`FrameEffects` gains:

```rust
pub select_tab: Option<(usize, PathBuf)>,
pub close_tab: Option<(usize, PathBuf)>,
pub close_group: Option<usize>,
pub focus_group: Option<usize>,
pub toggle_split: bool,
pub group_split_moved: bool,
```

Each group draws its own tabs, breadcrumb, body, optional minimap, and active
border. A split group also draws a group-close action. The Split action remains
visible when width permits. Tabs use the existing bounded/ellipsized geometry.

### App wiring: `src/main.rs`

- Replace the single `open_path`/global tab list with `EditorGroups` plus two
  fixed scrollbar states.
- Build the visible path list, open/pin missing viewers, then borrow both viewers
  simultaneously for the frame.
- Tree double-click and `open_viewer` target the active group. Duplicate paths
  focus the owning group.
- Tab select/close and group close update group state and evict only paths no
  longer visible.
- Wheel, clipboard, status, IME, config keyboard routing, and diff refresh use
  the active group.
- FS refresh checks both visible viewer paths.
- `Ctrl+W` closes the active viewer/config tab before falling back to terminal
  close.
- Source Control in a compact split layout uses the active group's body.

## Error and lifecycle behavior

- Failed viewer opens store an error on the target group and leave its previous
  active tab unchanged.
- Closing the last tab leaves an empty-group placeholder.
- Closing the secondary group collapses to one group and evicts its tabs.
- Resizing below the split threshold keeps an existing split but clamps the
  divider/minimum widths; the Split action can still collapse it.
- Config close removes its tab and selects a neighboring viewer tab.
- No swallowed cache, PTY, fs, or git errors.

## Memory

- One mmap/index per path, still capped by the existing cache and 32 MiB index
  budget (except the at-most-two pinned visible viewers).
- Two group tab lists, each capped at 9 paths; no duplicate path across groups.
- Two scrollbars, one divider ratio, and one drag latch are fixed-size state.
- No per-pane duplicate file buffers or syntax caches.

## Verification

- Pure state tests for open/select/close, duplicate focus, tab caps, split
  collapse, and divider clamping.
- Cache tests for pinned eviction and two simultaneous mutable borrows.
- Layout tests for unsplit compatibility, split geometry, compact widths, and
  divider minimums.
- Frame tests for per-group tabs, close actions, focus clicks, split toggle,
  divider drag, and independent wheel routing.
- App tests for tab lifecycle, duplicate focus, config tabs, both-viewer fs
  refresh, and active-group clipboard routing.
- Live smoke: open two files, scroll independently, drag divider, close a tab,
  close the group, resize small/large.
- Release stress with five cached viewers and two shells remains below 100 MiB.
