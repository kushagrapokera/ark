# ark — Agent-First Code Editor, Fully Software-Rendered (winit + softbuffer + tiny-skia)

## Context

Build **ark**: a desktop code editor where humans *view* code and AI agents (Claude Code, Codex, OpenCode, ZCode) edit. Humans edit only config files (.env form, yaml/toml text). Built-in terminals (bash/fish), live git indicators, click-to-commit/push per file. Invoked like `code .` → **`ark .`** opens a window rooted at that directory.

**Hard constraint: ≤100MB RAM** (agents are child processes; their RAM is theirs). This stack guarantees it — projected **30–70MB complete**.

## Stack (research-validated)

| Layer | Choice | Why |
|---|---|---|
| Windowing | `winit` | Cross-platform, IME events, native decorations, DnD |
| Present | `softbuffer` 0.4 | Zero-copy Wayland shm/X11/Windows GDI/macOS — no GPU context ever |
| 2D raster | `tiny-skia` | Pure-Rust Skia subset; fast CPU rasterization; no GPU libs loaded |
| Text | `cosmic-text` (+swash) | shaping/bidi/fallback; **bundled fonts only** via `FontSystem::new_with_fonts` (avoids system-wide fontdb scan) |
| Clipboard | `wl-clipboard-rs` + `arboard` | native Wayland + X11/XWayland fallback |
| Terminal | `portable-pty` + `alacritty_terminal` | battle-tested VT parser/grid model; we render the grid ourselves |
| Files | `memmap2`, `notify` (inotify) | mmap big files, debounced watching |
| Git | subprocess `git` CLI | porcelain v2 parsing; zero in-process git memory; fsmonitor/untrackedCache for giant repos |
| CLI | `clap` | `ark [PATH]`, `ark --version` |

Excluded by design: wgpu/Vulkan, webviews, Electron, widget frameworks, LSPs, SVG/icon libs (icons drawn as tiny-skia paths), system font scanning.

### Why this stack (research summary)

| Approach | Real-world RAM | Verdict |
|---|---|---|
| Electron | 200–350MB baseline | ❌ |
| Tauri (Linux WebKitGTK) | 30–150MB, unpredictable (tauri#5889) | ❌ can't guarantee <100MB |
| Zed / Lapce / GPUI Kit | 200MB–2GB / 260–290MB | ❌ GPU stack + indexing |
| Helix | ~50–150MB | ✅ but TUI, no GUI |
| Lite XL (custom software rendering) | 10–30MB | ✅ proof of concept |
| **winit + softbuffer + tiny-skia (chosen)** | **30–70MB projected** | ✅ chosen |

Key insights: (1) the GPU stack and silently-loaded subsystems dominate RSS — "the cheapest memory is a library you never load" (135MB→30MB by ditching the GPU); (2) WebKitGTK's floor is uncontrollable; (3) a **read-only** viewer is a fraction of a full editor, making custom rendering cheap; (4) `alacritty_terminal` + custom grid rendering is the proven terminal pattern (Zed, tessera, muxterm); (5) viewport-only rendering is what lets Lite XL sit at 10–30MB; (6) iced ships tiny-skia as its official software backend (4MB binary, 60ms startup).

## Architecture

```
ark/src/
├── main.rs        # clap CLI, single-window bootstrap (process-per-window)
├── render.rs      # softbuffer surface, tiny-skia Pixmap, damage-rect raster, HiDPI
├── text.rs        # cosmic-text integration, monospace glyph cache, UI text layout
├── ui.rs          # mini widget kit: button, checkbox, tab bar, scrollbar, panels, tooltip
├── tree.rs        # lazy virtualized file tree
├── viewer.rs      # read-only viewer: mmap, line index, viewport grid, selection+copy, gutter
├── highlight.rs   # regex highlighter, visible-lines-only
├── term.rs        # pty + alacritty_terminal model + grid renderer + input/mouse routing
├── git.rs         # status/diff/add/commit/push via subprocess
├── configed.rs    # .env form editor; yaml/toml minimal textarea (cursor/selection/undo/IME)
├── agents.rs      # detect + launch claude/codex/opencode/zcode
├── fs.rs          # lazy dir listing, atomic writes, whitelist enforcement (Rust-side)
├── watch.rs       # notify → debounced invalidation
├── state.rs       # LRU buffer cache, status map
└── config.rs      # ark.toml: shells, editable whitelist, caps
```

## Memory engineering (the guarantee)

1. **Viewport-only rendering everywhere** (tree rows, code lines, terminal grid) + damage-rect raster/present; frame budget: cap terminal redraws (~30fps) under output storms, parse PTY bytes in bounded chunks.
2. **Lazy tree** (one dir level per expand); **LRU buffer cache** (~8–12 files); mmap + chunked preview for files >64MB; refuse binaries.
3. **Terminal scrollback capped** (2000 lines default, configurable); free terminal state on tab close.
4. **Bundled fonts only** (JetBrains Mono + a UI sans subset) — no system font scan; bounded glyph cache.
5. Periodic `malloc_trim` on idle; explicit drops so RSS returns to OS.
6. `git status` out-of-process (fsmonitor + untrackedCache one-click "optimize repo" for 50M-line repos) — costs latency, not ark RAM.
7. Bundle target: single binary, no runtime deps beyond system libs (wayland/x11/glibc).

**Projection**: base ~15–30MB + 5 open files ~15MB + 2 capped terminals ~15MB + git maps ~2MB ≈ **30–70MB typical, <100MB guaranteed**.

## Features (the 5 requirements)

1. **Terminals** — tab strip + shell registry (bash/fish/zsh, default `$SHELL`); real PTYs; close kills process group; alacritty_terminal grid → our renderer; keyboard focus routing + mouse reporting (apply the patches tessera/muxterm wrote for egui_term — same model).
2. **Read-only viewer** — all code files open read-only; fs events → debounced refresh + "changed on disk" flash (watch agents edit live); text selection + copy; gutter reserved for git marks.
3. **Write-only config editor** — whitelist in `ark.toml` (`.env*`, `*.yaml`, `*.yml`, `*.toml`): .env → key/value form (preserve comments/order, secret masking, add/remove/rename); yaml/toml → minimal textarea (cursor, selection, undo, IME via winit `Ime`); atomic temp+rename saves. **Enforcement lives in Rust** (`fs.rs` rejects non-whitelisted writes) — not just UI.
4. **Git indicators** — `status --porcelain=v2` on open + debounced on fs events → tree badges (M/A/D/U); per-open-file lazy `git diff --unified=0` → added/modified gutter marks.
5. **Commit & push panel** — changed-files checkboxes → stage selected (`git add -- <paths>`, per-file granularity) → commit message → Commit → Push streams output into a git terminal tab. Clicks only, zero commands.
6. **Agent tabs** — detect `claude`/`codex`/`opencode`/`zcode` on PATH; one-click launch in project dir; their edits flow through features 2+4 live.

## Milestones

- **M0** — rustup install (missing on system). Cargo project; winit window + softbuffer surface clearing; clap CLI; `install.sh` (symlink to /usr/local/bin + .desktop entry). *Deliverable: `ark .` opens an empty window; baseline RSS measured.*
- **M1** — Render foundation: damage-rect pipeline, HiDPI handling, cosmic-text + bundled fonts + monospace glyph cache, theme tokens, hand-drawn icon set (tiny-skia paths).
- **M2** — Mini widget kit (button/checkbox/tab bar/scrollbar/panel split/single-line input).
- **M3** — Lazy virtualized file tree panel.
- **M4** — Read-only viewer (mmap, line index, viewport render, selection+copy, regex highlight, LRU cache, size caps, gutter strip). *First genuinely usable ark.*
- **M5** — Watcher → live refresh + change flash (the "agents edit, I watch" loop).
- **M6** — Terminals (pty + alacritty_terminal + grid renderer + input routing + mouse + shell registry + frame budgeting).
- **M7** — Git status engine + tree badges + per-line gutters.
- **M8** — Commit & push panel.
- **M9** — Config editors (.env form; yaml/toml textarea with undo/IME) + Rust whitelist enforcement.
- **M10** — Agent launcher tabs.
- **M11** — Memory hardening + proof: malloc_trim, caps, heaptrack pass, Linux-kernel-repo stress test, `smem` assertion <100MB.

## Windows port (later)

winit/softbuffer/tiny-skia are cross-platform (GDI on Windows); portable-pty → ConPTY (same API); notify → ReadDirectoryChangesW (no code change); add cmd/PowerShell to shell registry; `std::path` everywhere; NSIS/MSI packaging. ~1 week, no rewrite.

## Verification

- **RAM proof**: `smem -P ark` (PSS) + `/usr/bin/time -v` with kernel-sized repo, 5 files open, 2 terminals, agent running → assert <100MB (expect 30–70MB).
- **Scale**: Linux kernel clone (~40M lines, 80k files) — tree expand latency, status latency, scroll smoothness at 1080p/4K.
- **Agent loop**: run `claude` in an ark terminal → live refresh, gutter updates, changes panel, commit+push via clicks only.
- **Enforcement**: IPC write to a `.rs` file rejected at the Rust layer; `.env` form + `config.yaml` saves atomic with comments preserved.
- **Rendering**: damage correctness (no artifacts on resize/scroll), IME test with fcitx5 on KDE Wayland.

## Risks & mitigations

- **DIY text/widget bugs (artifacts, blending)** → keep the widget kit minimal, golden-path first; viewer + terminal are monospace grids (simplest possible text rendering).
- **IME variability on Wayland** → KDE (Garuda) is well-supported via text-input-v3; config editing is small-surface, so impact is bounded.
- **Effort overrun** → milestone order delivers a usable viewer by M4 before the hard parts (terminal M6, editor M9); strictly no feature creep into the widget kit.
- **Clipboard on pure Wayland** → wl-clipboard-rs primary, arboard fallback; XWayland is default-on in KDE anyway.
- **No accessibility** (known gap) → defer; document.
