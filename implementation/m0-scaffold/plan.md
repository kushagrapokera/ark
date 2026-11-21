# M0 — Scaffold: CLI + window bootstrap

Master plan: `plan.md` (root), milestone M0.

## Tasks

1. Install Rust toolchain (rustup, stable, clippy, rustfmt)
2. Cargo project scaffold (`Cargo.toml`, `src/main.rs`, deps: clap, winit, softbuffer, anyhow, log/env_logger)
3. CLI parsing: `ark [PATH]` (default `.`), `ark --version`; reject non-directory paths
4. Window bootstrap: winit window titled with project dir, softbuffer surface, solid theme fill, resize + close handling
5. `install.sh` (release build → /usr/local/bin/ark + `ark.desktop`)
6. `.gitignore` — ignore `/target`
7. Verification: release build, clippy `-D warnings`, smoke run (`ark .` opens empty window), baseline RSS measurement

## Acceptance criteria

- `ark .` opens an empty ark window rooted at the current directory
- `ark --version` prints the version
- Closing the window exits cleanly; resizing repaints correctly
- Baseline RSS measured with `/usr/bin/time -v` and `smem -P ark`, recorded in `.sdd/journal.md`
- `cargo clippy -- -D warnings` passes
