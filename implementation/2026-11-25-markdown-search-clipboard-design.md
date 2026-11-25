# Markdown preview, search highlighting, and clipboard support

## Goal

Make Markdown preview useful for every Markdown file, make search navigation
show the matched word in the opened file, and add native clipboard paste to the
surfaces where editing is allowed.

## Confirmed requirements

1. Markdown preview should look and behave like a compact VS Code-style preview,
   not a plain monospace dump.
2. Clicking a search result opens the file at the match and highlights the searched
   word.
3. `Ctrl+C` copies viewer/config selections. `Ctrl+V` pastes into terminals,
   whitelisted config editors, search, and commit-message fields.
4. A focused terminal keeps standard semantics: `Ctrl+C` is SIGINT and
   `Ctrl+Shift+V` pastes. `Ctrl+Shift+C` is reserved for terminal copy.
5. Preview is available for all `.md` files (plus the existing Markdown extensions
   and legacy extensionless/`README.txt` names), not only `README.md`.
6. No new dependency is approved. Clipboard remains `wl-clipboard-rs` only.
7. Code and Markdown source remain read-only. Pasted text must never modify them.

## Constraints

- Frozen stack; no webview, browser engine, widget framework, system font scan,
  persistent search index, full-document Markdown AST, or duplicate file buffer.
- Preview stays streamed and viewport-only with the existing 20,000-line,
  2,000-character inline, 512-code-line, and 64-table-row guards.
- Clipboard reads and copies are bounded to 1 MiB. Config insertion keeps its
  256 KiB document cap; single-line fields keep their existing caps; terminal
  paste is bounded to 64 KiB per operation.
- Ark remains below 100 MiB RSS. The only new retained per-viewer state is one
  bounded search highlight (line, char span, and a query capped at 128 chars).

## Selected approach

### Markdown preview

Rename the `readme` module to `markdown` so the names match the expanded behavior:
`MarkdownState`, `is_markdown`, and `draw_markdown_preview`. The editor action uses
the existing book glyph but is labeled/tooltipped “Markdown Preview”.

The renderer keeps the existing mmap-backed `Viewer` and streams one block at a
time. It paints into a centered content column (maximum 860 logical px, at least
24 px side padding) over the full editor background.

Visual changes:

- typography scale and spacing for H1–H6, with rules under H1/H2;
- paragraph spacing and soft-line joining;
- bold, strikethrough, inline code, and link styling;
- raised, bordered inline-code chips that also work inside tables;
- task-list checkboxes and completed-item styling;
- padded fenced-code blocks with bounded, viewport-only hard wrapping;
- padded blockquotes and list markers;
- tables with wrapped cells, content-derived row heights, header fill, and grid
  lines;
- viewport clipping for every block and table row.

The bundled JetBrains Mono faces remain the only fonts. Exact VS Code font
metrics are not reproduced without adding a font asset; layout, hierarchy,
spacing, and colors follow the VS Code Markdown preview model.

Non-goals: opening links in a browser, loading remote/local images, raw HTML
execution, footnotes, nested block composition, and full CommonMark conformance.
Image alt text and unsupported markup remain readable text.

### Search highlighting

`Viewer` owns one optional `SearchMatch`:

```rust
SearchMatch {
    line: usize,       // zero-based
    start_col: usize,  // char columns
    end_col: usize,
    query: String,     // <= 128 chars, used to reapply after live refresh
}
```

`Viewer::set_search_match(line_1_based, query)` finds the first
case-insensitive literal occurrence on that line. It uses the existing `regex`
crate with an escaped query, so Unicode case folding and byte-to-char offset
conversion are handled by tested library behavior. Empty, over-cap, missing, or
invalid matches clear the highlight.

The viewer renderer paints a dedicated search-match background beneath syntax
text and selection. `BufferCache::refresh_pinned` reapplies the stored query to
the refreshed line, so agent edits do not silently lose the highlight.

`App::open_search_result` applies the current search query after scrolling to the
reported line. Normal file opens clear the target viewer's highlight. Config
search results select the matching range in YAML/TOML text; `.env` form results
keep their existing focus behavior.

### Clipboard

Move clipboard I/O from `viewer.rs` into `src/clipboard.rs`:

```rust
pub fn copy_text(text: &str) -> std::io::Result<()>;
pub fn paste_text(max_bytes: usize) -> std::io::Result<String>;
```

`paste_text` reads at most `max_bytes + 1`, rejects oversized payloads instead of
truncating, validates UTF-8, and returns explicit errors. It uses
`wl-clipboard-rs` only; unsupported sessions surface a logged error rather than
pretending the paste succeeded.

Routing:

- viewer selection: existing `Ctrl+C` copy; `Ctrl+V` is a logged read-only no-op;
- config textarea: `Ctrl+C` copy and `Ctrl+V` insertion through `TextEditor`;
- `.env` fields: `Ctrl+V` inserts normalized single-line text into the focused
  key/value field;
- search/commit fields: `Ctrl+V` inserts normalized single-line text at the caret;
- terminal: `Ctrl+Shift+V` reads clipboard text and writes it to the PTY, wrapped
  in bracketed-paste markers when the terminal enables `TermMode::BRACKETED_PASTE`;
  `Ctrl+C` remains SIGINT.

Clipboard errors are logged and, for editable UI surfaces, shown in that
surface's existing error/status area.

## Files and responsibilities

- `src/markdown/` (renamed from `src/readme/`): detection, state, bounded parser,
  streamed renderer, block/table painters, and tests.
- `src/viewer.rs`, `src/viewer/render.rs`, `src/state.rs`: bounded search-match
  state, rendering, and refresh preservation.
- `src/clipboard.rs` (new): bounded native clipboard I/O and pure text
  normalization helpers.
- `src/main.rs`, `src/configed/keys.rs`, `src/configed.rs`, `src/commit.rs`,
  `src/search/mod.rs`: clipboard routing and search-result highlight application.
- `src/frame.rs`, `src/frame/chrome.rs`, `src/icons.rs`, `src/render.rs`: Markdown
  action naming and search-highlight theme token.
- `src/tests.rs`, `src/viewer/tests.rs`, `src/viewer/panel_tests.rs`,
  `src/markdown/tests.rs`, `src/configed/text/tests.rs`, `src/term/tests.rs`:
  behavioral regressions.

## Test strategy

1. Detection: `guide.md`, `docs/API.MD`, `notes.markdown`, and legacy README names
   are previewable; `README.md.bak` and code files are not.
2. Parser: task lists, strikethrough, soft-line paragraphs, and existing blocks
   stay bounded.
3. Renderer: centered column, H1 rule/spacing, task checkbox, padded code block,
   wrapped multi-line table row, and viewport-only ink.
4. Search: mixed-case query highlights the correct char span; missing/empty
   queries clear it; refresh reapplies it; App navigation sets it; normal open
   clears it.
5. Clipboard: normalization, caps, UTF-8 errors, config insertion, single-line
   field insertion, terminal bracketed paste, and read-only viewer refusal.
6. Integration: live `.md` preview toggle, search-result highlight, config paste,
   terminal paste, and five-viewer/two-terminal RSS/PSS below 100 MiB.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`
- `git diff --check`
- Live X11/Wayland smoke for Markdown, search highlight, and clipboard
- `/proc/<pid>/status` and `/proc/<pid>/smaps_rollup` stress measurement (`smem`
  is not installed in the current environment)
