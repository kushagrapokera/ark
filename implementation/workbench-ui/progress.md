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

- New File and New Folder open a capped inline tree-row field (128 chars,
  160-char error cap) inside the target folder, matching the supplied reference:
  drag handle, rounded accent border, no card/title/buttons/placeholder. Enter
  creates; Escape cancels; validation errors appear below the field.
- The prompt targets the selected folder (or a selected file's parent; project
  root when nothing is selected), expands its ancestors, and inserts a virtual
  child row so existing entries shift down. Symlink rows are treated as files;
  the filesystem boundary also rejects symlinked or non-directory parents.
- New code files open read-only; newly created whitelisted config files open in
  the enforced config editor. Existing symlinks are never followed or replaced.
- Refresh re-lists the tree. Collapse All uses in-place `retain` to free every
  expanded listing without allocating.
- The prompt is the only new retained state and exists only while open. Stress
  with five viewers, two shells, and a contextual 128-char inline field measured
  **0 KiB RSS change** (65,588 KiB before/during/after; PSS
  57,591/57,590/57,591 KiB). `smem` and `/usr/bin/time` are unavailable in this
  environment, so measurements use `/proc` status and `smaps_rollup`.
- Verification: `cargo test` **300 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live release smoke created `tests/notes.txt`
  and `tests/nested/child` from inline fields while preserving visible sibling
  rows, collapsed expanded directories, and refreshed an external file.

## Terminal dock cleanup and resizing

- Added a centered drag divider between editor and terminal. The default
  responsive height is unchanged until the user drags; selected heights are
  clamped to keep at least 160 px of editor card space (120 px minimum, smaller
  proportionally in tiny windows).
- Terminal PTY grids resize with the dock, including shells created after a
  height change. The divider highlights on hover and retains only one height
  plus one drag latch—no layout cache.
- Removed Problems, Output, Ports, and Debug Console; the toolbar now contains
  only the functional Terminal tab and existing terminal actions.
- Removed Outline and Timeline and reclaimed their 56 px footer for the file
  tree (1024×768 tree viewport: 564 → 620 px).
- Verification: `cargo test` **304 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke grew the terminal 224 → 302 px,
  shrank it 302 → 162 px, and confirmed shell-grid resizing.
- Five-viewer/two-shell stress at 1440×900 measured **65,232 KiB RSS /
  57,183 KiB PSS** at the default height and **68,568 KiB RSS / 60,519 KiB PSS**
  with the terminal dragged to its 656 px maximum—below the 100 MiB budget.
  `smem` and `/usr/bin/time` are unavailable, so `/proc` status and
  `smaps_rollup` supplied the measurements.

## Reference-style terminal tabs

- Replaced the vertical session icons with a labeled terminal tab strip. Tabs
  select individually, show a close button, and share the existing eight-PTY
  cap; the active PTY keeps its bounded grid and scrollback.
- `+` clones the active terminal's executable, arguments, and label (bash → bash,
  fish → fish, Node → Node). With no open terminal it starts the default shell.
- The dropdown contains New Terminal, New Terminal Window, bash, fish, and
  JavaScript Debug Terminal. Split Terminal, GitHub CLI, and the three-dot menu
  are intentionally absent. The menu paints above the live terminal grid.
- New Terminal Window launches an available external terminal emulator in the
  project root; JavaScript Debug Terminal runs Node's interactive console.
  New Terminal and New Terminal Window also honor the displayed Ctrl+Shift+`
  and Ctrl+Shift+Alt+` shortcuts.
- Removed the right-side session strip, returning its width to the terminal grid.
- Verification: `cargo test` **308 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke verified bash/fish/Node creation,
  bash/fish/Node cloning, tab switching, per-tab close, and a Konsole window in
  the project root.
- Worst-case stress with five large viewers and eight open terminal tabs at
  1440×900 measured **77,500 KiB RSS / 69,448 KiB PSS**, below the 100 MiB
  budget. `smem` and `/usr/bin/time` are unavailable, so `/proc` status and
  `smaps_rollup` supplied the measurements.

## Title-bar cleanup and layout controls

- Removed the Update button and the top-center search/command field.
- Made the remaining right-side layout controls functional: Explorer and
  Terminal toggle independently, with accent state and responsive auto-hide.
- Removed the Minimap toggle and More/Layout Options menu, including their
  retained state. The minimap itself keeps its existing responsive behavior;
  Split remains available in the editor tab strip.
- Opening a terminal while its dock is hidden reveals the dock; hiding Explorer
  cancels an active inline creation prompt. No layout cache is retained—only
  two visibility booleans.
- Verification: `cargo test` **311 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke confirmed only the Explorer and
  Terminal controls remain at the top right and both toggle their panels.
- Five-viewer/two-terminal stress at 1440×900 measured **65,608 KiB RSS /
  57,503 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Viewer chrome and Explorer click behavior

- Removed the minimap gutter and rendering completely; viewer panes now use
  the full editor width in both split groups.
- Removed the inner grey placeholder/message border while keeping centered
  guidance and error text.
- File rows open on the first click and become selected. Directory rows select
  and toggle expansion from anywhere in the row, including the chevron.
- Verification: `cargo test` **311 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke confirmed no minimap/inner box,
  one-click file opening, and whole-row folder expand/collapse.
- Five-viewer/two-terminal stress at 1440×900 measured **65,460 KiB RSS /
  57,345 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Larger responsive typography and icons

- Increased UI/code/terminal/config text from 14 px to 16 px and viewer line
  height from 18.9 px to 21.6 px. Tree text remains 14 px and monospace.
- Times New Roman is not bundled, and ark forbids system-font scanning, so the
  existing bundled JetBrains Mono family is retained rather than introducing an
  unapproved proprietary font.
- Added a window-area UI scale clamped to 0.9–1.25 (1.0 at 1024×768). UI chrome,
  code viewer, terminal, config text, and chrome icons scale with it; the tree
  stays fixed. Title-menu spacing and terminal placeholder text reflow to avoid
  overlap at large sizes.
- Increased tree icons to 16 px and bolded all icon strokes from 1.75 px to
  2.25 px. Chrome icons also receive the window-size scale.
- Bounded the measured-metrics cache at 64 entries so continuous window resizing
  cannot grow it without limit.
- Verification: `cargo test` **316 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live 1024×768 → 1440×900 smoke confirmed
  larger code/UI text and icons, unchanged tree text, and no title/placeholder
  overlap.
- Five-viewer/two-terminal stress at 1440×900 measured **64,044 KiB RSS /
  55,962 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Explorer status, layout cleanup, and terminal defaults

- Restored Git status badges and colored Explorer file/folder names with the
  same theme color as their M/A/D/U badge.
- Clamped long repository titles before the Explorer action icons and increased
  action spacing so neither titles nor icons overlap at large UI scales.
- Kept Explorer row labels clipped before the badge/scrollbar area.
- Made fish the preferred default shell when installed.
- Terminal dock now starts hidden. Its top-right button opens fish when no
  shell exists, reveals existing shells when hidden, and hides the dock only
  when shells are already open. Ctrl+T, terminal actions, git command tabs,
  and profile creation also reveal it. New default terminals are fish.
- Removed the three-dot More action from editor tab strips, including split
  groups; Split and per-tab/group close controls remain.
- Verification: `cargo test` **321 passed, 0 failed, 3 ignored**, 5/5 CLI;
  clippy and release build clean. Live smoke used a long repository name and
  confirmed colored M/U names, non-overlapping root actions, hidden startup
  terminal, fish on Ctrl+T and the top-right button, and split strips without
  More.
- Five-viewer/two-fish-terminal stress at 1440×900 measured **64,356 KiB RSS /
  55,286 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Terminal repaint and divider responsiveness

- Added a terminal-only render wave for PTY output, cursor updates, and terminal
  scrolling. It clears and repaints only the active grid, skips viewer preparation
  and chrome composition, and presents only the grid damage rectangle.
- Forwarded terminal-model `PtyWrite` events through a shared synchronized PTY
  writer. Primary Device Attribute queries now receive `CSI ?6c`, so fish no
  longer waits 10 seconds for a capability reply; cursor/query replies use the
  same path.
- Kept scene damage authoritative: layout/input/menu/window changes still repaint
  the workbench, and hidden terminals keep parsing output without scheduling paints.
- Divider drags now resize the visual terminal model immediately but defer the PTY
  resize. Pointer movement coalesces to one redraw request and one stable
  editor/terminal damage region per presented frame; release fits the final grid
  and sends one SIGWINCH. The per-frame damage guard resets when a render wave is
  consumed, so movement after the first frame keeps repainting instead of freezing.
- Release-before-final-paint is handled explicitly, so a stale intermediate row
  count is never sent to fish. Window/layout changes still resize the PTY immediately.
- Verification: `cargo test` **329 passed, 0 failed, 3 ignored**, 5/5 CLI; clippy and
  release build clean. Live drag smoke observed **0 PTY resizes before release and
  1 after release** (`71×12`); trace logs confirmed terminal-only frames. A 20,000-line
  output storm completed without losing input or exceeding the memory budget.
- A real-PTY regression captures the five-byte Primary Device Attribute reply.
  Live fish first-command latency dropped from **12,449 ms** on the previous binary
  to **2,376 ms** with the reply path, and the terminal-compatibility warning is
  gone. The remaining time is fish greeting/fastfetch startup, not ark's query wait.
- Drag profiling (`ARK_TERM_PROFILE=1`) found and fixed a first-frame-only damage
  latch: 40 pointer movements previously produced 7 scene frames and then froze
  until release; the final binary produces 46 frames, with ~11 ms median total
  frame time. A mid-drag screenshot confirmed the panel grows before mouse release.
- The output-storm wall/CPU time was unchanged versus the previous binary (~10.1 s,
  ~7.6 s CPU), confirming terminal grid text rendering is now the dominant cost;
  streamed drawing/glyph caching remains the next optimization stage. Fish's
  10-second capability wait is removed by the reply path above; its remaining
  greeting/fastfetch startup is independent of terminal rendering.
- Five-viewer/two-fish-terminal stress at 1440×900 measured **64,808 KiB RSS /
  55,582 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Simplified title bar

- Removed the File, Edit, Selection, View, Go, More, Back, and Forward controls
  from the top bar. Only the ark logo and the Explorer/Terminal layout controls
  remain.
- Increased the logo from 16 px to 24 px; it continues to scale with the window
  UI scale and stays within the 36 px title bar.
- Removed the now-unused left-chevron icon. Explorer, Terminal, and editor-strip
  More controls are unchanged.
- Verification: `cargo test` **331 passed, 0 failed, 3 ignored**, 5/5 CLI; clippy
  and release build clean. Live 1024×768 and 1440×900 screenshots confirmed the
  clean title bar, larger logo, and retained right-side layout controls.

## Bottom status bar removed

- Removed the entire bottom status bar: branch/remote/sync/error/warning icons,
  cursor and encoding labels, formatter state, and the status-bar layout/damage
  path. Status-only icon variants and formatting helpers were removed with it.
- Reclaimed the former 24 px status strip for the workbench. At 1024×768 the
  cards now extend to y=760, leaving only the normal 8 px window padding.
- Prevented fractional bottom tree rows from drawing glyph descenders into that
  padding; a regression pins an ink-free bottom edge at compact and wide sizes.
- Verification: `cargo test` **331 passed, 0 failed, 3 ignored**, 5/5 CLI; clippy
  and release build clean. Live 1024×768 and 1440×900 screenshots confirmed no
  bottom bar with the terminal both hidden and open.
- Five-viewer/two-fish-terminal stress at 1440×900 measured **40,424 KiB RSS /
  32,176 KiB PSS**, below the 100 MiB budget. `smem` and `/usr/bin/time` are
  unavailable, so `/proc` status and `smaps_rollup` supplied the measurements.

## Icon tooltips and terminal grip

- Added hover names for every workbench icon, including activity/title controls,
  Explorer actions, tree rows, editor/terminal tabs and actions, resize handles,
  and close controls. One bounded tooltip is painted after the scene, flips above
  near the bottom edge, and stays inside the window.
- Kept terminal-only repaints from erasing tooltips over terminal chrome by using
  a scene wave while that chrome is hovered.
- Replaced the terminal divider's stacked-line grip with three horizontal dots.
- Removed the Explorer header ellipsis and its now-unused More icon variant.
- Verification: `cargo test` **338 passed, 0 failed, 3 ignored**, 5/5 CLI; clippy
  and release build clean. Live hover smoke confirmed New File, Toggle Explorer,
  New Terminal, Terminal Profiles, and Resize Terminal labels; the three-dot grip
  and clean Explorer header are visible in the final screenshots.
- Five-viewer/two-fish-terminal stress at 1440×900 measured **40,584 KiB RSS /
  30,781 KiB PSS**, below the 100 MiB budget.

## Project keyword search and README preview

- Made the Search activity icon functional: a debounced background worker performs
  case-insensitive literal keyword search across project text files and shows
  matching files, match counts, and first-line snippets. Clicking a result opens
  that file at the matching line while keeping Search open.
- Bounded search with no persistent index: 1 MiB/file, 50,000 files, 128 MiB total
  scan, 100 results, 160-character snippets, a bounded directory stack, symlink
  refusal, binary/UTF-8 refusal, cancellation, and stale-generation rejection.
- Reduced the activity bar to Explorer, Search, and Source Control; removed the
  Terminal/Config placeholder icons and their unused glyph variants.
- Added a README editor action immediately left of Split, available only when the
  active tab is a README. It toggles a streamed GitHub-style preview of the same
  mmap-backed viewer: headings, bold, inline code, links/image alt text, lists,
  blockquotes, fenced code, rules, and simple tables. No AST, duplicate file
  buffer, webview, dependency, or persistent layout index is added.
- Preview layout is viewport-only with 20,000-line and 2,000-character guards;
  wheel/scrollbar scrolling clamps to streamed content height.
- Verification: `cargo test` **360 passed, 0 failed, 3 ignored**, 5/5 CLI; clippy
  and release build clean. Live smoke confirmed three visible activity icons,
  keyword results across README/Rust files, result opening, raw README action,
  and formatted headings/lists/code/table rendering.
- Five-viewer/two-fish-terminal stress at 1440×900 plus a full-project no-match
  search measured **38,960 KiB RSS / 30,261 KiB PSS** (sampled peak RSS 39,540
  KiB), below the 100 MiB budget.

## Search reliability follow-up

- Fixed missing camel-case hits caused by generated trees consuming the scan
  budget and the worker stopping at 100 matches. Search now skips conventional
  build/dependency directories, scans through the existing file/byte limits, and
  retains the top 100 results with exact-case matches ranked first.
- Fixed result rows using only their intra-row offset instead of the full scroll
  position, which blanked middle/bottom pages. Row painting and hit testing are
  now clipped to the visible result viewport, including fractional scroll offsets.
- Verification: **364 unit tests + 5 CLI tests**, clippy/release clean. Live smoke
  found `open_search_result` in ark source while scanning 430 files (excluding
  `target/`), scrolled a 100-result fixture to its bottom, and opened the visible
  result. Updated five-viewer/two-terminal stress measured **39,192 KiB RSS /
  31,788 KiB PSS** (sampled peak RSS 39,792 KiB), below the 100 MiB budget.
- `smem` is unavailable in this environment; PSS was read from
  `/proc/<pid>/smaps_rollup` during the live stress run.

## Markdown, search highlighting, and clipboard follow-up

- Replaced the README-only preview with a streamed VS Code-style Markdown view
  for every `.md`/Markdown file: centered reading column, heading hierarchy and
  rules, paragraph spacing, task checkboxes, strikethrough, padded code blocks,
  blockquotes, and wrapped content-sized tables. The existing mmap viewer,
  viewport-only rendering, and bounded parser guards remain in place.
- Search-result navigation now retains a bounded per-viewer highlight for the
  queried word, renders it beneath syntax/selection, and reapplies it after live
  file refresh. YAML/TOML search hits select the matching source range.
- Added bounded `wl-clipboard-rs` copy/paste routing. `Ctrl+V` works in config
  editors, `.env` fields, search, commit messages, and new-item prompts;
  `Ctrl+Shift+V` pastes into terminals with bracketed-paste support. Terminal
  `Ctrl+C` remains SIGINT, and code/Markdown source stays read-only.
- Verification: `cargo test` **384 passed, 0 failed, 4 ignored**, 5/5 CLI;
  clippy and release build clean. Both live Wayland clipboard tests passed.
  Visual smoke confirmed non-README Markdown formatting and clicked-search
  highlighting. Five-viewer/two-terminal stress with Markdown preview and search
  active measured **34,352 KiB RSS / 27,570 KiB PSS**, below the 100 MiB budget.

## Config editor auto-save

- Added a 750 ms debounced automatic save for whitelisted `.env`, YAML, and TOML
  editors. Code and Markdown source remain read-only.
- Saves use the existing enforced atomic write boundary; manual Save/`Ctrl+S`
  still work. Pending edits flush before config/tab/window close.
- Auto-save pauses on external-file conflicts instead of overwriting them, and
  further edits no longer hide the conflict. Reload or explicit Save resolves it.
- Verification: `cargo test` **388 passed, 0 failed, 4 ignored**, 5/5 CLI; clippy
  and release build clean. Live YAML and `.env` edits saved without keyboard
  commands, and the header transitioned from “saves automatically” to “saved”.

## Markdown overflow and inline hierarchy polish

- Fixed fenced-code overflow by hard-wrapping long lines inside the padded block.
  Wrapped rows are painted viewport-only and capped at 4,096 rows with a clear
  truncation notice.
- Inline code now renders as a raised, bordered chip, including inside tables;
  bold text, links, task lists, and table headers retain their distinct styles.
- Verification: `cargo test` **391 passed, 0 failed, 4 ignored**, 5/5 CLI; clippy
  and release build clean. Wide and narrow live previews confirmed code stays
  inside its box and bold/inline-code/table hierarchy is visually distinct.

## Editor action overlap fix

- Removed the inert close-group button from the primary split editor; only the
  secondary group now shows it.
- Markdown and Split actions now use dedicated 24 px slots with a guaranteed gap
  from the tab strip and from each other.
- Verification: `cargo test` **393 passed, 0 failed, 4 ignored**, 5/5 CLI; clippy
  and release build clean. A live three-tab split Markdown session confirmed the
  actions no longer overlap the tabs or each other.

## Caret blinking and terminal rendering fixes

- Fixed the terminal caret gap by shaping terminal text with the same UI scale
  used for cell advances and cursor geometry.
- Fixed stale OpenCode-style background bands by clearing the full terminal
  rectangle before painting each frame.
- Added a shared 530 ms blinking caret for terminals, config textareas/fields,
  search, commit messages, and new-item prompts. Input resets it to visible;
  unfocused surfaces hide it.
- Verification: `cargo test` **400 passed, 0 failed, 4 ignored**, 5/5 CLI; clippy
  and release build clean. Live fish/search smoke confirmed carets blink on and
  off, and terminal regressions cover background clearing and scaled geometry.

## Notes

- `PaneMode::Terminals` was removed: terminal tabs now only set terminal
  focus/activity; they never hide the editor. `PaneMode` is Viewer/Config.
- Tab labels are derived from the LRU + one config editor, so the strip is
  bounded by the existing cache caps (8 + 1).
- No web stack, fonts, or icon assets were introduced.
- `cargo fmt --check` remains red on pre-existing formatting drift in untouched
  files (for example `src/ui/tabbar.rs`); this change avoids unrelated reformatting.
