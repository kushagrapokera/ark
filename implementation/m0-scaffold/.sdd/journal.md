# Journal — m0-scaffold

## 2026-11-21 — M0 complete

### What we did

- Installed rustup stable **rustc/cargo 1.100.0** (minimal profile + clippy + rustfmt)
- Scaffolded the cargo project: deps `clap 4` (derive), `winit 0.30.13`,
  `softbuffer 0.4.8`, `anyhow 1`, `log 0.4`/`env_logger 0.11`; release profile
  thin LTO + strip
- CLI: `ark [PATH]` (defaults to `.`), `--version`; path canonicalized and
  validated as a directory
- Window bootstrap: winit 0.30 `ApplicationHandler`; `Arc<Window>` +
  `Context<Arc<Window>>` + `Surface<Arc<Window>, Arc<Window>>`; solid theme
  fill presented on `RedrawRequested`; resize → `request_redraw`; close → exit
- `install.sh` (release build → `/usr/local/bin/ark` + XDG `ark.desktop`;
  requires sudo, run manually by the user) and `/target` in `.gitignore`

### What we tested / found

- `cargo build --release`: clean, 17s incremental
- `cargo clippy --release -- -D warnings`: clean, zero warnings
- `ark --version` → `ark 0.1.0`; `--help` renders correctly
- Smoke run on the live Wayland session (KDE, `wayland-1`): window opened,
  surface resized to 1024×768, first frame presented, clean exit on SIGTERM
- **Baseline memory (empty window, 1024×768):**
  - `VmRSS`: 13,176 kB (12.9 MB)
  - `VmHWM` (peak): 13,176 kB (12.9 MB)
  - `Pss` (smaps_rollup): 10,064 kB (9.8 MB)
- Binary size: 5.0 MB (stripped release)

### Decisions & why

- softbuffer **0.4.8 changed its API** from lifetime-borrowed
  (`Surface<'ctx>`) to owned generics (`Surface<D, W>`); the `Box::leak`
  workaround from older guides is obsolete — `App` stores the owned
  `Context`/`Surface` directly.
- Pixel channel order (Rgb vs Bgr packing) deferred to M1: a uniform fill is
  order-agnostic. M1 will read the surface pixel format before drawing glyphs.
- Logging via `env_logger` (default level `info`, timestamps off).

### Known risks / notes

- Interactive resize repaint is exercised in code (Resized → request_redraw →
  render) but not yet human-verified by drag-resize; user should run
  `cargo run --release -- .` and confirm visually.
- No other issues.

### RSS budget status

13.2 MB peak / **100 MB budget → 13% used** at the empty-window baseline.
Re-measure after each milestone; M1 (text pipeline + glyph cache) is next.

### Stable commit hash

(repo not yet committed — awaiting user; commit when ready)
