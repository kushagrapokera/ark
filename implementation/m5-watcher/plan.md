# M5 — Watcher → live refresh + change flash

Master plan: `plan.md` (root), milestone M5. The "agents edit, I watch" loop:
fs events refresh the tree and the open viewer live, with a visible
"changed on disk" flash.

## M4 carry-forwards

1. `arboard` (X11 clipboard fallback) deferred — wire when X11 sessions
   become a supported target (discovered live in M4's harness).
2. `main.rs` 552 / `viewer.rs` 527 lines — accepted per user directive
   (single cohesive concerns).

## New dependency (frozen stack)

`notify 9` — inotify fs events. notify runs its own internal thread per
process; events coalesce into ONE pending flag in ark (an `AtomicBool`), so
nothing per-event reaches the app and nothing grows with event storms.

## Constraints (AGENTS.md — binding)

- Fixed-size state: the fs pipeline carries a single coalesced flag — no
  path buffers, no queues (event storms cannot grow memory).
- Correctness comes from re-listing + stat (not from trusting event paths):
  tree refresh re-lists the root + every expanded dir (bounded by the
  expanded set); the open viewer is refreshed only when a stat shows its
  (len, mtime) changed.
- Viewers are READ-ONLY: refresh carries the read position (offset, clamped
  at the next draw) and drops the selection — content shifted under it.
- Debounce: tree refresh coalesces to one re-list per 400 ms of fs activity;
  the open-file stat check is leading-edge (immediate on the first wake —
  also shrinks the mmap/SIGBUS exposure window on truncation).
- The flash is a bounded two-frame animation via `ControlFlow::WaitUntil`:
  ON for ~900 ms, then one scheduled redraw clears it. No continuous loop.

## Frozen interfaces (implement verbatim)

```rust
// src/watch.rs — fs watcher (notify's internal thread; no custom thread)
pub struct FsWake;                                   // UserEvent payload
/// Coalescing flag shared with notify's thread: `signal` sets it, `take`
/// consumes it (false → nothing to do).
pub struct FsPending { /* AtomicBool */ }
impl FsPending {
    pub fn new() -> Self;
    pub fn signal(&self);                            // idempotent
    pub fn take(&self) -> bool;
}
pub struct Watcher { /* notify::RecommendedWatcher kept alive + FsPending */ }
impl Watcher {
    /// Watch `root` recursively. Errors (missing root, watch limits) are
    /// logged by the caller — ark runs WITHOUT live refresh, never crashes.
    pub fn new(root: &Path, proxy: EventLoopProxy<FsWake>) -> Result<Self, notify::Error>;
    pub fn take_pending(&self) -> bool;              // false → nothing changed
}

// src/tree.rs — live refresh
impl FileTree {
    /// Re-list the root + every user-expanded dir. Failed sub-listings
    /// collapse that dir (vanished/unreadable — normal during refresh, NOT
    /// an error surface); a failed ROOT listing keeps the last_error
    /// surface and empties the tree.
    pub fn refresh(&mut self);
}

// src/viewer.rs — change detection + read-position carry
impl Viewer {
    /// True when the file on disk differs from what this view holds
    /// (len or mtime changed, or stat fails — e.g. the file vanished).
    pub fn changed_on_disk(&self) -> bool;
    /// Raw offset restore for cache refresh (clamped at the next draw's
    /// self-heal); used to carry the read position across a disk refresh.
    pub fn restore_offset(&mut self, v: usize);
}

// src/state.rs — cache refresh (old mapping dropped BEFORE the new open)
impl BufferCache {
    /// Re-open `path` from disk, replacing the cached viewer. Carries the
    /// old read position; the selection is dropped. On open failure the OLD
    /// viewer stays cached (a vanished/truncated file never blanks the pane)
    /// and the error is returned.
    pub fn refresh(&mut self, path: &Path) -> Result<&mut Viewer, ViewerError>;
}
```

## App wiring (main.rs)

- `EventLoop::builder().with_user_event::<FsWake>()`; the proxy goes to
  `Watcher::new`; a watcher failure logs and disables live refresh.
- `ApplicationHandler::user_event(FsWake)`: consume the pending flag (gate);
  `process_fs_change()` (testable without a window):
  1. open viewer: `changed_on_disk()` → `BufferCache::refresh` → flash
     deadline = now + FLASH_MS, full damage;
  2. tree: re-list at most once per TREE_DEBOUNCE (400 ms since the last
     refresh), full damage;
  3. set `ControlFlow::WaitUntil(flash_end)` for the clear frame.
- `about_to_wait`: flash deadline reached → clear flash, one full-damage
  redraw, control flow back to `Wait`.
- Flash rendering: a 3 px accent strip across the viewer pane's top edge
  while active (pinned by pixel test).

## Tasks (inline implementer; TDD, tests first)

- **Task 1 — state cores:** `Viewer::changed_on_disk`/`restore_offset`,
  `BufferCache::refresh`, `FileTree::refresh`, `FsPending` — TDD.
- **Task 2 — wiring + flash:** watch.rs, main.rs event plumbing, flash
  strip + viewer offset self-heal in the frame, pixel pins, live
  agent-edit verification (edit files while ark runs → tree rows appear,
  viewer content + flash update).

## Acceptance criteria (milestone)

- `cargo test` — all previous green + new; clippy `-D warnings` clean;
  release build 0 warnings; ARK_SMOKE exit 0.
- Live: while ark runs, (a) touch/create files → tree rows appear after the
  debounce; (b) rewrite the OPEN file from outside → content refreshes,
  scroll position survives, flash shows and clears; (c) delete an expanded
  dir → it collapses.
- LRU/refresh failures keep the old view (no blank pane, no crash on
  vanished/truncated files); event storms cannot grow memory (flag only).
- RSS re-measured; ≤100MB budget intact.
