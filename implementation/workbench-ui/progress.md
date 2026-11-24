# Workbench UI — implementation record

Source plan: `/home/kushagra/.opencode/plan/ui_plan.md` (user-owned plan area).
Implemented inline; no new dependencies.

## Delivered

- **Workbench layout** (`frame.rs`): activity bar, sidebar, file/config tab
  strip, breadcrumb, editor body, bottom terminal dock, accent status bar.
- **Activity bar**: Explorer / Source Control / Terminal / Config icons with
  git and terminal-tab count badges; clicking focuses the existing surface
  (no new panels/caches).
- **File tabs**: derived from the existing LRU cache order plus the one open
  config editor; click selects, × evicts; max 8 file buffers unchanged.
- **Breadcrumbs**: root-relative path rendered under the tab strip; no state.
- **READ-ONLY badge** in the tab strip, hidden on the writable config tab.
- **Bottom terminal dock**: shells, git command tabs and agent tabs render
  below the editor instead of replacing it; same PTY/grid/scrollback state.
- **Config editor as a tab**: selecting the config tab enters the editor;
  closing it returns to the viewer; the editor stays visible while terminal
  tabs are open.
- **Accent status bar** with on-accent text, same status data.
- **All-language highlighter**: language profiles by extension
  (Rust/C-like/Python-like/Shell/YAML/TOML/SQL/Markup/Plain), universal
  fallback, `Token::Type` + `syntax_type` theme token. Regexes compiled once;
  visible lines only; no syntax tree/cache.
- **Neutral black default palette**: the shared mock's VS Code Dark+ colors
  (`#1E1E1E` background, `#252526` elevated, `#3C3C3C` borders, `#007ACC`
  accent, VS Code syntax/git colors) are now `Theme::DARK_PLUS` and the app
  default. The original Catppuccin Mocha palette remains available as
  `Theme::DARK` for widget tests/future switching.
- **Icons**: source-control, terminal, config path glyphs; Explorer uses the
  file glyph.

## Tests

- `cargo test`: **210 passed, 0 failed**, 2 ignored (live-Wayland clipboard,
  50k-file stress), 5/5 CLI.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release`: 0 warnings.
- New tests:
  - LRU tab order + exact evict.
  - Workbench layout pins (activity bar, tabs, breadcrumb, dock, status).
  - Activity-bar click, file-tab select/close, breadcrumb/READ-ONLY pixels.
  - App tab routing: viewer/config select + close/evict.
  - Language profiles: Rust, Python, YAML, SQL, unknown-file fallback.
  - Viewer syntax uses the file path (`lines.rs` fixture).

## Live verification

- Opened `src/main.rs`: file tab, breadcrumb `src > main.rs`, READ-ONLY
  badge, syntax colors; status accent bar.
- Opened/config tab: config editor in the editor area; READ-ONLY badge
  hidden; Save/Reload visible.
- Opened notes.txt: tabs MRU order `notes.txt · main.rs · config.yaml`.
- Ctrl+T: terminal dock appeared below the config editor and did not replace
  it; active terminal icon + tab.
- Opened `app.py`: Python highlighting (`def`, `str`, `None`, number, comment)
  and untracked badge in tree/tab.
- Screenshots: `/tmp/ark-ui-default.png`, `/tmp/ark-ui-viewer.png`,
  `/tmp/ark-ui-tabs.png`, `/tmp/ark-ui-config-tab.png`,
  `/tmp/ark-ui-dock2.png`, `/tmp/ark-ui-python.png`,
  `/tmp/ark-ui-black.png` (neutral black default).

## Memory

Re-ran M11 stress with the new workbench (80k files + five 200k-line files,
five open viewers, two shells + fake agent tab):

- VmHWM/VmRSS **58,640 KiB = 57.3 MiB**, Pss **52,802 KiB = 51.6 MiB**
- `PASS: RSS and PSS under the 100 MiB budget`
- idle trim still runs: `trimmed=true`.

## Tier-1 all-language highlighting

- Expanded the static registry to 25 profiles: Rust, C-like (C/C++/C#/Java/
  Kotlin/Scala/Swift/JS/TS/PHP/D), Go, Python-like (Python/Ruby/Perl/R/Julia/
  Dart/Elixir/Crystal/Nim), shell, PowerShell, SQL, markup (HTML/XML/Vue/
  Svelte/Astro/Handlebars/Twig/EJS/Jinja), CSS, JSON, YAML, TOML, INI,
  `.env`, Dockerfile, Makefile/Justfile, Markdown, Nix, HCL, Protobuf, CMake,
  GraphQL, LaTeX, Lua/Haskell/Elm, and plain fallback.
- Dark+ now distinguishes keyword/string/number/comment/type/function/
  constant/property tokens.
- Shared regexes and direct keyword/type table lookup replaced per-language
  alternation regexes.
- Memory probe: all 25 profiles +3,324 KiB release RSS; full workbench stress
  59.6 MiB RSS / 53.4 MiB PSS.
- Research record and Tier-3 measurements: `docs/research/syntax-highlighting.md`.

## Tier-1.5 VS Code-style polish

- Split control-flow keywords (`#C586C0`) from declarations/storage (`#569CD6`).
- Added variables, function calls, members/constants, regex/escape/decorator,
  CSS selector/value, and Markdown link categories; config profiles keep
  variable/CamelCase heuristics disabled.
- Added bounded multiline state: block comments, Python triple strings,
  Markdown fences, and HTML `<script>`/`<style>` blocks. Lookback is capped at
  256 lines; bracket depth at 256; indent guides at 64 levels.
- Added current-line, indent-guide, selection, and three-color bracket-pair
  decorations.
- Added shebang/framework filename detection and wired the same highlighter into
  the writable YAML/TOML config textarea. The `.env` form remains unchanged.
- Split highlighting into `detect`, `state`, `theme`, `tokenize`, and profile
  keyword/rule modules; split viewer rasterization into `viewer/render.rs`.
- Verification: `cargo test` **230 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy clean; release build clean.
- Memory probe: all 25 profiles **+3,324 KiB** release RSS.
- Manual M11 workload verified five `big0*.txt` viewers + two shells:
  **61,000 KiB = 59.6 MiB RSS**, **54,673 KiB = 53.4 MiB PSS**.
- Visual smoke: Rust, TSX, Python triple strings/decorators, Markdown fenced
  Rust, HTML embedded JS/CSS, and config textarea syntax colors.

## Header and sidebar cleanup

- Removed the Claude/Codex/OpenCode/ZCode launcher buttons and the M10 agent
  registry/launch product code. Historical M10 implementation records remain.
- Removed the `scale … canvas … rows … expanded … agents …` debug line.
- Replaced the sidebar `-`/`+` buttons with a centered three-vertical-dot
  handle. The handle drags the divider and double-click resets to 280 px.
- Verification after cleanup: `cargo test` **224 passed, 0 failed, 3 ignored**,
  5/5 CLI; clippy and release build clean; visual smoke confirms the quiet
  header and three-dot handle.

## Card-workbench redesign

- Reworked the frame into the supplied card layout: 36 px title bar, padded
  activity/explorer/editor/terminal cards, rounded borders, elevated headers,
  breadcrumb/minimap strips, terminal panel tabs, session strip, and a neutral
  status bar.
- Added static placeholders for menus, command center, layout controls,
  activity panels, editor actions, minimap, terminal tabs/toolbar, and status
  items. Existing tree/viewer/config/terminal interactions remain functional.
- The terminal card is always visible; its `+` button opens the first PTY even
  when no terminal tab exists.
- Reused the Source Control/commit backend; the follow-up Source Control card
  now exposes the per-file Commit & Push workflow.
- No new persistent heap state or caches: chrome is drawn from static labels,
  fixed arrays, and stack-local geometry each frame. Removed the old retained
  terminal `TabBar` from the production frame.
- Card-layout stress with five verified large viewers and two shells:
  **60,580 KiB = 59.2 MiB RSS**, **53,891 KiB = 52.6 MiB PSS** — no retained
  memory regression.
- Verification: `cargo test` **224 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy clean. Visual smoke matches the supplied card structure and Dark+
  palette; exact glyphs/fonts remain bounded by ark's bundled JetBrains Mono
  and hand-drawn icon set.

## Responsive card layout

- Window resize recomputes every card, pane, toolbar, and hit target from the
  current logical size; no layout state or cache is retained.
- Below 760 px the Explorer and its resize handle hide to protect the editor;
  below 560 px padding/activity width shrink and the minimap disappears.
- Short windows use a compact terminal card/toolbar. Narrow editor strips
  show fewer actions, keep tabs inside the available width, and truncate long
  breadcrumbs to the file name.
- Activity spacing prioritizes functional icons; title/status toolbars drop
  optional placeholders before they can overlap. Terminal `+`/close hit rects
  share geometry with their visible glyphs, and the App now dispatches those
  effects after rendering.
- Verification: `cargo test` **234 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live resize smoke covered **400×300**,
  **640×480**, **1024×768**, and **1440×900**, including opening a shell with
  the compact `+` button and resizing its PTY grid.
- Stress with five large viewers, two shells, and a 400×300 → 1440×900 resize:
  **64,896 KiB = 63.4 MiB RSS**, **57,392 KiB = 56.0 MiB PSS** — below the
  100 MiB budget; the larger framebuffer accounts for the increase over the
  1024×768 card smoke.

## Source Control card

- Reconnected the third activity-bar icon to the existing per-file Commit &
  Push panel. Wide windows host it in the left card under a `SOURCE CONTROL`
  header; compact windows host it in the editor body so it stays reachable.
- Reused the existing bounded `CommitPanel` state, git status snapshot,
  viewport-only changed-file list, checkboxes, message field, and streaming
  PTY command tabs. No new persistent state or caches.
- Matched the current card style with surface/elevated tokens, subtle rules,
  rounded controls, and an accent Commit button. Compact panel geometry keeps
  the message and both actions usable at the minimum window size.
- Restored wheel scrolling and post-render Commit/Push dispatch through the
  active Source Control rect.
- Live scratch-repo smoke: unchecked `b.txt`, committed only `a.txt` with
  `feat: source control smoke`, verified `b.txt` remained modified, then pushed
  successfully to a local bare remote.
- Verification: `cargo test` **235 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Five-viewer/two-shell stress with the card
  active and a 400×300 → 1440×900 resize: **62,140 KiB = 60.7 MiB RSS**,
  **54,425 KiB = 53.1 MiB PSS**.

## Activity bar cleanup

- Removed the unused Asterisk/agent/more placeholders beneath Config and the
  nonfunctional account/settings footer.
- Kept Explorer, Search, Source Control, Terminal, and Config; Terminal and
  Config retain their existing behavior.
- Removed the Claude/Codex launcher placeholders beside the title-bar command
  center and their unused glyph definitions.
- Removed the Claude/Codex editor-action placeholders at the right of the tab
  strip; only Split and More remain.
- Tree row names now use char-boundary-safe ellipsis clipping before the git
  badge and scrollbar track, so long names cannot cross the sidebar boundary.

## Split editor groups

- Added two bounded side-by-side editor groups with per-group tab strips,
  active-tab underlines, tab close buttons, group close, and Split toggle.
- Each group has independent viewer scroll/selection for different files. A
  file appears in only one group; opening it again focuses the owning group.
- Reused the global LRU cache and pinned both visible viewers during open,
  refresh, and eviction. Added simultaneous mutable borrows without duplicating
  mmap/index memory.
- Config files are tabs in a group; keyboard/IME/copy/wheel/diff routing follows
  the active group. FS refresh checks both visible viewers.
- Responsive geometry clamps the divider and hides split minimaps in narrow
  groups. Split is offered only when the editor card is at least 640 px wide.
- Verification: `cargo test` **266 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy clean. Live two-file split smoke confirmed both per-group tab strips,
  viewers, and controls.
- Split stress with five large viewers, two shells, and a 400×300 → 1440×900
  resize: **65,420 KiB = 63.9 MiB RSS**, **57,660 KiB = 56.3 MiB PSS** — below
  the 100 MiB budget.

## Horizontal viewer scrolling

- Long code lines are clipped to each editor pane's visible column window;
  syntax colors, selection backgrounds, brackets, and indent guides are clipped
  with the text.
- A horizontal scrollbar appears only when the longest line exceeds the pane
  width. It supports thumb dragging, track clicks, and Shift+wheel scrolling.
- The viewer stores only the longest-line char count, a column offset, and one
  fixed-size scrollbar drag latch per cached file—no per-line width cache.
- Verification: `cargo test` **270 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live split-pane smoke confirmed clipping and
  scrollbar dragging.
- Split stress with five large viewers, two shells, and a resize:
  **65,552 KiB = 64.0 MiB RSS**, **57,806 KiB = 56.4 MiB PSS**.

## Sidebar resize handle

- Moved the three-dot Explorer/editor divider handle to the vertical center of
  the sidebar edge.
- Enlarged and rounded the dots, added a contrasting floating pill, and kept
  the handle visible at idle.
- Added an 800 ms accent/size pulse after input, with continuous emphasis while
  hovered or dragged. Redraw events do not extend the pulse, so it settles
  automatically without retained animation state.
- Verification: `cargo test` **271 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy clean. Live idle/pulse screenshots confirmed centered visibility and
  frame-to-frame animation.
- Split stress with five viewers, two shells, and a resize:
  **65,748 KiB = 64.2 MiB RSS**, **57,843 KiB = 56.5 MiB PSS**.

## Explorer root actions

- New File and New Folder open a capped in-card name prompt (128 chars,
  160-char error cap), support Enter/Create and Escape/Cancel, and create
  root-contained, no-overwrite entries through a dedicated Rust filesystem
  boundary.
- The prompt targets the selected folder (or a selected file's parent; project
  root when nothing is selected), opens directly below that row with matching
  nesting indentation, and expands the target after creation so the new entry is
  visible. Symlink rows are treated as files; the filesystem boundary also
  rejects symlinked or non-directory parents.
- New code files open read-only; newly created whitelisted config files open in
  the enforced config editor. Existing symlinks are never followed or replaced.
- Refresh re-lists the tree. Collapse All uses in-place `retain` to free every
  expanded listing without allocating.
- The prompt is the only new retained state and exists only while open. Stress
  with five viewers, two shells, and a contextual 128-char prompt measured
  **65,476 → 65,600 KiB RSS** and **57,525 → 57,571 KiB PSS** while opening the
  prompt (+124/+46 KiB); after cancel RSS stayed at 65,600 KiB and PSS at
  57,571 KiB, still far below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable in this environment, so measurements use `/proc` status and
  `smaps_rollup`.
- Verification: `cargo test` **295 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke created `src/inside.txt` and
  `src/nested/child` from their selected folder rows, collapsed expanded
  directories, and refreshed an external file.

## Notes

- `PaneMode::Terminals` was removed: terminal tabs now only set terminal
  focus/activity; they never hide the editor. `PaneMode` is Viewer/Config.
- Tab labels are derived from the LRU + one config editor, so the strip is
  bounded by the existing cache caps (8 + 1).
- No web stack, fonts, or icon assets were introduced.
