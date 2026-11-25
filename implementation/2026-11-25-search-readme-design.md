# Project keyword search and formatted README preview

## Goal

Make the activity-bar Search icon useful for finding code keywords across the
project, remove the two activity icons below Source Control, and add an editor
action that toggles a GitHub-style formatted view when the active tab is a README.

## Confirmed behavior

- Search is a case-insensitive literal keyword search. A query such as a function
  or variable name returns the files containing it, match counts, and the first
  matching line. Clicking a result opens that file at the match.
- The README editor action is available only when the active tab itself is a
  README (`README`, `README.md`, `README.markdown`, or `README.txt`,
  case-insensitive). It toggles formatted/raw views of that same mmap-backed tab.

## Constraints

- Frozen stack; no new dependencies, webview, browser engine, system fonts, or
  persistent search index.
- Search workers are bounded by file count, bytes scanned, per-file size, result
  count, and snippet size. The UI keeps only the latest completed report.
- README preview reuses the existing `Viewer` mmap and line index. It builds no
  full-document AST or duplicate file buffer. Layout is streamed from source lines
  each frame and only visible blocks are painted.
- All code remains read-only. Config writes continue through the enforced config
  editor.

## Selected approach

### Search

`src/search.rs` owns:

- `SearchResult { path, matches, line, snippet }`
- `SearchReport { generation, query, results, scanned_files, truncated, error }`
- `SearchState { input, results, status, scroll, running }`
- `run_search(root, query, generation, cancel) -> SearchReport`

The worker walks directories without following symlinks and skips Git internals
plus conventional generated/dependency directories (`target`, `node_modules`,
virtual environments, caches, build output, and coverage). It refuses
binary/invalid UTF-8 files, reads at most 1 MiB per file, scans at most 50,000
files or 128 MiB total, and returns at most 100 file results. Reaching the result
cap does not end the scan: the worker keeps the best bounded set, ranking
exact-case matches ahead of case-insensitive-only matches, then match count and
path. A cancelled or stale report is discarded by generation/query identity.

The App debounces typing by 250 ms, runs one worker at a time, cancels stale work,
and receives `Wake::SearchFinished(Box<SearchReport>)`. Search UI lives in the
sidebar when `ActivityPanel::Search` is active, with a focused `TextInput`, bounded
result rows, match snippets, and a scrollbar. Results open through the existing
viewer cache and jump to the first matching line.

### Activity bar

The bar contains only Explorer, Search, and Source Control. Terminal and Config
remain reachable through their existing title-bar/file workflows but lose their
activity-bar icons.

### README preview

`src/readme.rs` owns:

- `is_readme(path) -> bool`
- `ReadmeState { preview, scroll }`
- a streamed Markdown block/inline renderer over `Viewer::line_text`

Supported CommonMark/GitHub-like subset:

- ATX headings (`#` through `######`)
- paragraphs and soft line breaks
- bold, inline code, links, and image alt text
- unordered/ordered lists
- blockquotes
- fenced code blocks
- horizontal rules
- simple pipe tables

Non-goals: raw HTML execution, remote image loading, footnotes, nested block
composition, and full CommonMark conformance. Unsupported markup is displayed as
readable text rather than hidden.

The editor action strip gains a book/README icon immediately left of Split when
the group's active path is a README. Clicking it toggles `ReadmeState.preview`.
The formatted view uses the same tab, mmap, and line index; switching tabs resets
preview/scroll. Wheel and scrollbar input adjust a pixel scroll offset. A
20,000-line and 2,000-character-per-line preview guards prevent pathological CPU
work and show a clear truncation notice.

## Alternatives rejected

- **System `grep` subprocess:** platform-specific output parsing and process/RAM
  behavior; a bounded Rust worker is deterministic and portable.
- **Persistent search index:** violates ark's memory discipline and adds stale
  invalidation complexity.
- **Webview/Markdown crate:** forbidden stack and far above the RSS budget.
- **Full Markdown AST:** unnecessary retained memory; streamed blocks are enough
  for read-only preview.

## Verification

- Unit tests for bounded search, generated-directory refusal, relevance ranking
  beyond the result cap, cancellation identity, binary refusal, result
  caps, README detection, Markdown block/inline layout, wrapping, and scrolling.
- Frame tests for the three-item activity bar, functional Search routing, result
  click effects, conditional README action, and formatted/raw toggle.
- App tests for debounce/start/apply, stale result rejection, opening a result at
  its line, README toggle reset, and wheel routing.
- Live smoke: search a known function name in ark, open a result, open `plan.md`
  or a README fixture, toggle formatting, resize, and scroll.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, release build, and a
  five-viewer/two-terminal RSS/PSS measurement below 100 MiB.
