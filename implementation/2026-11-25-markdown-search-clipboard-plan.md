# Implementation plan: Markdown preview, search highlighting, and clipboard

Design: `implementation/2026-11-25-markdown-search-clipboard-design.md`

## Goal and constraints

Deliver the four requested behaviors without new dependencies, without making
code/Markdown source writable, and without retaining a document AST or persistent
search index. Clipboard uses `wl-clipboard-rs` only. Terminal `Ctrl+C` remains
SIGINT; terminal paste uses `Ctrl+Shift+V`.

## 1. Markdown detection and naming

Files: `src/readme/` → `src/markdown/`, `src/main.rs`, `src/frame.rs`,
`src/frame/chrome.rs`, `src/icons.rs`, tests.

- [x] Add failing detection tests for `guide.md`, `docs/API.MD`,
  `notes.markdown`, legacy README names, and rejected backups/code files.
- [x] Rename the module/state/function to `MarkdownState`, `is_markdown`, and
  `draw_markdown_preview`; update all callers and the action tooltip.
- [x] Run focused tests, then `cargo test`.

## 2. VS Code-style streamed Markdown renderer

Files: `src/markdown/parse.rs`, `src/markdown/render.rs`, new
`src/markdown/blocks.rs`, `src/markdown/table.rs`, `src/markdown/paint.rs`,
`src/markdown/tests.rs`.

- [x] Add failing parser tests for task lists, strikethrough, and bounded
  soft-line paragraph joining.
- [x] Add failing render tests for a centered maximum-width column, H1 rule and
  spacing, task checkbox, padded code block, wrapped table row height, and no ink
  outside the viewport.
- [x] Implement bounded parser additions without a document-wide AST.
- [x] Implement the centered content column and typography/spacing scale.
- [x] Split block/table/inline painting into focused submodules; keep every file
  below roughly 400 lines.
- [x] Preserve viewport-only rendering, scroll clamping, truncation notices, and
  the existing line/span/code/table caps.
- [x] Run focused Markdown tests, then `cargo test` and clippy.

## 3. Search-result highlighting

Files: `src/viewer.rs`, `src/viewer/render.rs`, `src/viewer/tests.rs`,
`src/viewer/panel_tests.rs`, `src/state.rs`, `src/main.rs`, `src/configed.rs`,
`src/configed/keys.rs`, `src/render.rs`, `src/tests.rs`.

- [x] Add failing viewer tests for first case-insensitive match char columns,
  empty/missing/over-cap clearing, and bounded query storage.
- [x] Add failing pixel tests for search-match background under syntax text and
  horizontal scrolling.
- [x] Add `SearchMatch` and `Viewer::set_search_match`; add the theme token and
  renderer background.
- [x] Preserve/reapply the highlight in `BufferCache::refresh_pinned`.
- [x] Add failing App tests proving search navigation sets the highlight and a
  normal file open clears it.
- [x] Apply the current query in `App::open_search_result`; select the matching
  range for YAML/TOML config results.
- [x] Run focused viewer/App tests, then `cargo test` and clippy.

## 4. Bounded clipboard I/O and routing

Files: `src/clipboard.rs` (new), `src/viewer.rs`, `src/main.rs`,
`src/configed.rs`, `src/configed/keys.rs`, `src/configed/text/tests.rs`,
`src/commit.rs`, `src/search/mod.rs`, `src/term.rs`, `src/term/tests.rs`,
`src/tests.rs`.

- [x] Add failing pure tests for single-line normalization and clipboard size
  rejection; retain the ignored live Wayland round-trip test.
- [x] Add `clipboard::copy_text`/`paste_text(max_bytes)` with bounded reads,
  UTF-8 validation, and explicit errors; update existing copy callers.
- [x] Add failing config tests for textarea and `.env` field paste, including
  selection replacement, CRLF normalization, NUL refusal, and growth-cap refusal.
- [x] Add `Ctrl+V` routing for config editors, search, and commit fields; surface
  failures in existing status/error areas.
- [x] Add failing terminal encoding tests for plain and bracketed paste, plus a
  64 KiB rejection boundary.
- [x] Route terminal `Ctrl+Shift+V` to the PTY and preserve `Ctrl+C` as SIGINT.
- [x] Keep viewer `Ctrl+V` a logged read-only refusal.
- [x] Run focused clipboard/config/terminal tests, then `cargo test` and clippy.

## 5. Integration and assurance

- [x] Review failure paths and bounded state with `silent-failure-hunter`.
- [x] Review test coverage/integrity and implementation integrity.
- [x] Run `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release`, and `git diff --check`.
- [x] Live smoke: preview a non-README `.md`, toggle raw/preview, search a
  mixed-case symbol, confirm highlight and jump, paste into config/search/commit,
  and paste into a terminal with `Ctrl+Shift+V`.
- [x] Measure five-viewer/two-terminal RSS/PSS with Markdown preview and search
  active; record numbers below 100 MiB.
- [x] Update `implementation/workbench-ui/progress.md`, install the matching
  release binary, verify hashes, and commit without pushing.
