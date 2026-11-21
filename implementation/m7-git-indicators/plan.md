# M7 — Git status engine + tree badges + per-line gutters

Master plan: `plan.md` (root), milestone M7. Feature 4: `status --porcelain=v2`
on open + debounced on fs events → tree badges (M/A/D/U); per-open-file lazy
`git diff --unified=0` → added/modified gutter marks.

## Carry-forwards

1. `arboard` X11 clipboard fallback — still deferred.
2. Oversized cohesive files (term.rs/main.rs) — accepted per user directive.

## New dependencies

NONE — git runs as a subprocess (frozen stack: zero in-process git; status
"costs latency, not ark RAM").

## Constraints (AGENTS.md — binding)

- Fixed-size state: parsed status capped (100k entries → truncated flag),
  per-file diff marks capped (4000 → truncated), the App keeps ONE status
  snapshot + ONE open-file diff (bounded; no per-file diff map).
- Subprocess only: no git crate, no libgit2. `git` is resolved per call.
- Git runs on background threads (status of a big repo must never block the
  UI event loop); results arrive as `Wake` user events.
- Diff refresh is lazy: only the OPEN viewer file, refreshed when (a) the
  open file changes, (b) a new status snapshot lands.
- Untracked/unmerged/deleted conventions: badges M/A/D/U (M = modified
  incl. type-change + unmerged; A = staged-new incl. copy; D = deleted;
  U = untracked); unstaged beats staged for the badge. Renames badge the
  new path as M. Deleted-from-worktree files have no tree row → no badge
  (staged deletions with a surviving worktree copy do show D).

## Frozen interfaces (implement verbatim)

```rust
// src/git.rs — subprocess engine + parsers
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind { Modified, Added, Deleted, Renamed, Copied, Unmerged, TypeChanged }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStatus {
    pub path: PathBuf,
    pub orig: Option<PathBuf>,       // rename/copy source
    pub staged: Option<StatusKind>,
    pub unstaged: Option<StatusKind>,
    pub untracked: bool,
    pub unmerged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge { Modified, Added, Deleted, Untracked }

impl FileStatus { pub fn badge(&self) -> Badge; }   // priority per constraints

#[derive(Clone, Debug, Default)]
pub struct GitStatus { pub entries: Vec<FileStatus>, pub truncated: bool }
impl GitStatus {
    pub fn changed_count(&self) -> usize;            // entries with a badge
    pub fn badge_for(&self, path: &Path) -> Option<Badge>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterMark { Added, Modified }

#[derive(Clone, Debug, Default)]
pub struct FileDiff { pub marks: Vec<(usize, GutterMark)>, pub truncated: bool }  // (1-based line, kind)

#[derive(Debug)]
pub enum GitError { NotARepo, Io(std::io::Error), Git(String) }
impl Display + Error

pub fn status(root: &Path) -> Result<GitStatus, GitError>;              // --porcelain=v2
pub fn diff_file(root: &Path, path: &Path) -> Result<FileDiff, GitError>;  // diff HEAD --unified=0
pub fn parse_status(out: &str) -> GitStatus;                            // tested pure
pub fn parse_diff(out: &str) -> FileDiff;                               // tested pure
```

## App wiring (main.rs)

- `Wake::GitStatus(Arc<GitStatus>)` / `Wake::GitDiff { path, diff: Arc<FileDiff> }`
  — git subprocesses run on detached threads; results land as user events.
  One in-flight status at a time (a fs burst while in flight sets a pending
  flag, re-dispatched on completion).
- Startup: `request_git_refresh()` after App construction; `process_fs_change`
  re-dispatches (the M5 fs pipeline is the debounce trigger); open-file diff
  re-runs on (a) double-click open, (b) viewer disk-refresh, (c) new status.
- State: `git: Option<GitStatus>` (None = not a repo / git missing),
  `file_diff: Option<(PathBuf, Arc<FileDiff>)>` — one open-file diff.
- Status line: `… · git <n>` (n = changed entries) / `… · no git`.

## Rendering

- Tree badges: 1-char colored letter at the row's right edge (left of the
  scrollbar track) — M yellow, A green, D red, U teal (theme tokens).
- Viewer gutter: the reserved 4px mark strip paints per-line bars — Added
  green, Modified yellow — for the visible lines of the open file.
- Widget kit untouched; colors join `Theme` as four git tokens.

## Tasks (inline; TDD, tests first)

1. git.rs parsers (pure fixtures) + theme tokens.
2. Live-git tests (temp repo: commit → modify → add → untracked → status;
   diff marks after edits).
3. Rendering: tree badges + viewer gutter marks (pixel pins).
4. App plumbing + status segment.
5. Gates + live verification (edit files while ark runs → badges/marks).

## Acceptance criteria

- `cargo test` all green (+ parsers, live-git tests, pixel pins); clippy
  clean; release build 0 warnings; ARK_SMOKE exit 0.
- Live: modify a tracked file → M badge; create a file → U badge; stage a
  file → A badge; the OPEN file's edited lines show gutter marks; a
  non-repo dir shows `no git` and the tree stays clean.
- RSS re-measured; ≤100MB intact. Event-storm safety: bounded status/diff
  caps; one in-flight git command; results are Arc snapshots.
