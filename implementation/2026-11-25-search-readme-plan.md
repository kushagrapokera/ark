# Implementation plan: keyword search and README preview

> Follow-up: the Markdown scope and renderer were expanded in
> `implementation/2026-11-25-markdown-search-clipboard-plan.md`.

Design: `implementation/2026-11-25-search-readme-design.md`

## 1. Bounded keyword-search core

Files: `src/search.rs` (new), `src/main.rs`, `src/watch.rs`, `src/tests.rs`.

- [x] Add failing tests for:
  - case-insensitive literal matches across nested text files;
  - binary/invalid UTF-8 and symlink refusal;
  - per-file/result/scan caps with `truncated=true`;
  - cancellation stopping a walk promptly;
  - bounded, control-free snippets and correct 1-based line numbers.
- [x] Implement `SearchResult`, `SearchReport`, and `run_search` with a bounded
  directory stack, 1 MiB/file, 50,000 files, 128 MiB total, 100 results, and 160
  character snippets. Never follow symlinks; skip `.git`.
- [x] Add `Wake::SearchFinished(Box<SearchReport>)`.
- [x] Add App search fields and a 250 ms debounce. Starting a search cancels the
  previous worker, increments generation, and keeps at most one worker/queued
  query. Apply only reports whose generation and query still match.
- [x] Test stale report rejection and inline search execution without a proxy.

## 2. Search sidebar UI and routing

Files: `src/search.rs`, `src/frame.rs`, `src/frame/chrome.rs`, `src/main.rs`,
`src/frame/tests.rs`, `src/tests.rs`.

- [x] Add failing frame tests for the three-item activity bar (Explorer, Search,
  Source Control), Search activation, input focus, and result-click effects.
- [x] Add `ActivityPanel::Search`; remove Bug/Extension entries and their active
  mappings from `draw_activity_bar`.
- [x] Add `SearchState` input/results/scroll/status and a sidebar renderer with a
  bounded viewport, file-relative labels, match counts, snippets, and scrollbar.
- [x] Pass `Option<&mut SearchState>` through `Frame`; draw Search instead of the
  tree when active. Return `search_open: Option<(PathBuf, usize)>` and input-focus
  changes through effects/state.
- [x] Route Search clicks, wheel scrolling, IME, and keyboard editing before
  terminal/config routing. Enter starts immediately; Escape returns to Explorer.
- [x] Open results through the viewer cache at the reported line and test the
  jump/clamp behavior.

## 3. README detection, action, and streamed renderer

Files: `src/readme.rs` (new), `src/icons.rs`, `src/frame.rs`,
`src/frame/chrome.rs`, `src/main.rs`, `src/frame/tests.rs`, `src/tests.rs`.

- [x] Add failing tests for README name detection and Markdown block/inline
  layout: headings, bold, inline code, links, lists, blockquotes, fences, rules,
  simple tables, wrapping, and unsupported-markup fallback.
- [x] Implement `is_readme` and `ReadmeState { preview, scroll }`.
- [x] Implement a streamed renderer over `Viewer::line_text`: one temporary block
  at a time, no AST or file copy, visible-block painting only, bounded line/span
  sizes, and a clear truncation notice beyond the preview guard.
- [x] Add an `Icon::Readme` book glyph and tooltip name.
- [x] Extend editor action geometry with a README action left of Split only when
  that group's active path is a README. Add `toggle_readme` effects and reserve
  space for the extra action.
- [x] Toggle formatted/raw rendering for the same viewer tab. Reset preview and
  scroll on tab changes; clamp scroll to streamed content height and draw a
  scrollbar.
- [x] Route wheel input to README scroll before normal viewer scrolling.

## 4. Integration, assurance, and verification

- [x] Run focused tests after each task, then full `cargo test`.
- [x] Review search failure paths, cancellation, caps, and stale-result handling
  with `silent-failure-hunter`; review tests with `pr-test-analyzer` and
  `test-integrity-guardian`; review implementation integrity with
  `anti-reward-hacking`.
- [x] Run `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and
  `git diff --check`.
- [x] Live smoke: search a known ark symbol, open a result at its line, open a
  README, toggle formatted/raw, scroll, resize, and verify the activity bar has
  only three icons.
- [x] Record five-viewer/two-terminal RSS/PSS below 100 MiB and update
  `implementation/workbench-ui/progress.md`.

## 5. Search reliability follow-up

Files: `src/search/mod.rs`, `src/search/ui.rs`, `src/search/tests.rs`,
`src/frame/tests.rs`.

- [x] Add regressions for exact-case source matches hidden beyond the result cap,
  generated directories consuming the scan budget, and blank scrolled result pages.
- [x] Skip conventional generated/dependency directories and keep scanning within
  the existing file/byte caps while retaining the top 100 ranked results.
- [x] Position viewport rows from the full scroll offset so middle/bottom pages
  render and remain clickable.
- [x] Re-run unit/integration tests, clippy, release build, and live search/scroll smoke.
