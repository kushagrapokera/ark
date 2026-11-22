# M9 — Config editors (.env form; yaml/toml textarea) + Rust whitelist enforcement

Master plan: `plan.md` (root), milestone M9. Feature 3: the ONLY
human-writable surfaces are whitelisted config files (`.env*`, `*.yaml`,
`*.yml`, `*.toml` by default, overridable in `ark.toml`). `.env*` gets a
key/value form (comments/order preserved, secret masking, add/remove/rename);
other configs get a minimal textarea (cursor, selection, undo, IME via winit
`Ime`). Saves are atomic temp+rename. **`fs.rs` rejects non-whitelisted
writes at the Rust layer** — the UI is not the enforcement point.

## Carry-forwards

1. `arboard` X11 clipboard fallback — still deferred (Wayland copy works).
2. Oversized cohesive files (term.rs/main.rs) — accepted per user directive.
3. Full IME candidate-window positioning (`set_ime_cursor_area`) — events
   (`Ime::Preedit`/`Ime::Commit`) and `set_ime_allowed` are wired; candidate
   popup placement uses the compositor default (KDE text-input-v3).

## New dependencies

NONE. `ark.toml` is parsed by a tiny, bounded reader for the `editable`
array only (`config.rs`); glob matching is ~40 lines; there is no TOML glob
crate, no editor crate.

## Constraints (AGENTS.md — binding)

- The whitelist can only NARROW the built-in config surface: `.env*`,
  `*.yaml`, `*.yml`, `*.toml` are the only human-writable file kinds even if
  `ark.toml` says `*`. Code files stay read-only at the Rust boundary
  (`config::is_config_surface` + `fs::write_editable_atomic`).

- Memory: config text capped at **256 KiB**, env documents at 4000 lines,
  text undo at 200 ops / 512 KiB of deltas, one editor open at a time. No
  per-file editor cache.
- Owner files only: every save goes through `fs::write_editable_atomic`,
  which canonicalizes the target, verifies it is inside the project root and
  matches the whitelist, then writes a same-directory temp file and renames.
  A rejected write is an error, never a silent no-op.
- The read-only viewer stays read-only: non-whitelisted files open in the
  M4 viewer; whitelisted files open in the config editor.
- YAML/TOML editing is intentionally minimal (no syntax highlighting, no
  wrapping, no line-ending conversion promises beyond LF normalization).
  IME is wired through winit events, not a hidden text stack.

## Frozen interfaces (implement verbatim)

```rust
// src/config.rs
pub const DEFAULT_EDITABLE: &[&str] = &[".env*", "*.yaml", "*.yml", "*.toml"];
pub struct Config { pub editable: Vec<String> }
impl Default for Config;
pub enum ConfigError { Io(std::io::Error), Parse(String) }
pub fn load(root: &Path) -> Result<Config, ConfigError>;   // ark.toml optional
pub fn parse_editable(text: &str) -> Result<Vec<String>, ConfigError>;
pub fn is_editable(root: &Path, path: &Path, patterns: &[String]) -> bool;
pub fn matches(pattern: &str, file_name: &str, rel_path: &str) -> bool;

// src/fs.rs — the write boundary
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;
pub enum EditError { OutsideRoot, NotWhitelisted(PathBuf), TooLarge { len: usize },
                     Io(std::io::Error) }
impl Display + Error
pub fn read_bounded(path: &Path, cap: usize) -> Result<String, EditError>;
pub fn write_editable_atomic(root: &Path, path: &Path, contents: &str,
                             patterns: &[String]) -> Result<(), EditError>;

// src/configed.rs — one open editor
pub struct ConfigEditor { path, dirty, error, /* private body+vendor state */ }
impl ConfigEditor {
    pub fn open(root: &Path, path: &Path, patterns: &[String]) -> Result<Self, EditError>;
    pub fn path(&self) -> &Path;
    pub fn kind(&self) -> ConfigKind;             // Env | Text
    pub fn is_dirty(&self) -> bool;
    pub fn save(&mut self) -> Result<(), EditError>;   // fs boundary again
    pub fn reload(&mut self) -> Result<(), EditError>;
    pub fn changed_on_disk(&self) -> bool;
    pub fn refresh_if_clean(&mut self) -> bool;
    pub fn scroll_by(&mut self, dy: f64) -> bool;
    pub fn handle_key(&mut self, key: &Key, text: Option<&str>,
                      mods: &ModifiersState) -> bool;
    pub fn on_ime_commit(&mut self, text: &str) -> bool;
    pub fn on_ime_preedit(&mut self, text: &str) -> bool;
    pub fn draw(&mut self, canvas, text, theme, scale, rect, pointer)
        -> ConfigEffects;
}
pub struct ConfigEffects { pub save: bool, pub reload: bool, pub changed: bool }
```

```rust
// src/configed/env.rs — pure .env model
pub struct EnvDoc { /* private lines */ }
pub enum EnvLine { Raw(String), Entry { export: bool, key: String, value: String } }
impl EnvDoc {
    pub fn parse(text: &str) -> Result<EnvDoc, EditError>;   // MAX_ENV_LINES cap
    pub fn render(&self) -> String;
    pub fn entries(&self) -> Vec<(usize, &str, &str)>;   // (line idx, key, value)
    pub fn apply(&mut self, line: usize, key: &str, value: &str) -> bool;
    pub fn append(&mut self) -> usize;                    // NEW_KEY at the end
    pub fn remove(&mut self, line: usize) -> bool;
}
pub fn is_secret_key(key: &str) -> bool;

// src/configed/text.rs — pure textarea model
pub struct TextEditor { /* private */ }
impl TextEditor {
    pub fn new(text: String, max_bytes: usize) -> Self;
    pub fn text(&self) -> &str;
    pub fn insert(&mut self, s: &str) -> bool;
    pub fn backspace(&mut self) -> bool;
    pub fn delete(&mut self) -> bool;
    pub fn move_cursor(&mut self, motion: Motion, select: bool) -> bool;
    pub fn select_all(&mut self) -> bool;
    pub fn selected_text(&self) -> Option<&str>;
    pub fn undo(&mut self) -> bool;
    pub fn redo(&mut self) -> bool;
    pub fn line_count(&self) -> usize;
    pub fn line(&self, i: usize) -> &str;
    pub fn cursor(&self) -> (usize, usize);             // (line, char col)
    pub fn set_cursor(&mut self, line: usize, col: usize, select: bool);
    pub fn selected_range(&self) -> Option<Range<usize>>;
}
```

## App wiring

- `PaneMode { Viewer, Terminals, Config }` is explicit in `App`:
  terminal tabs keep running hidden; opening a terminal switches to
  Terminals, double-clicking a file switches to Viewer/Config, and closing
  the config editor returns to Viewer.
- Double-click: `config::is_editable` → `ConfigEditor::open` (errors surface
  in the pane, never a silent viewer fallback); otherwise the M4 viewer.
- Keyboard in Config mode: Ctrl+S save, Ctrl+W close, Ctrl+Z/Ctrl+Shift+Z
  undo/redo, Ctrl+A select all, Ctrl+C copy; everything else goes to the
  editor (env fields or textarea). IME events go to the editor; the window's
  `set_ime_allowed` follows the pane.
- `Ctrl+S` and the header Save button share `ConfigEditor::save`; failures
  show in the editor header and log.
- fs events: a clean editor reloads from disk; a dirty editor keeps its text
  and shows “changed on disk”; the watcher still drives tree/git refresh.
- Right pane precedence: Config > Viewer > placeholder when no terminal is
  active; Terminals mode is explicit.

## Tasks (inline; TDD, tests first)

1. `config.rs`: defaults, `ark.toml` `editable` parser, glob matcher; tests.
2. `fs.rs`: `read_bounded` + `write_editable_atomic` (root containment,
   symlink resolution, whitelist, cap, temp+rename, permissions); direct
   enforcement tests (`.rs` rejected even when called directly).
3. `configed/env.rs`: parse/render round-trip, comments/order/export,
   apply/append/remove, secret-key rules; tests.
4. `configed/text.rs`: UTF-8 editing, selection, undo/redo caps, line/col
   mapping, IME commit/preedit; tests.
5. `configed.rs`: open/kind detection, dirty/save/reload/external change,
   env form layout + textarea raster/hit-test/buttons; tests + pixel pins.
6. App/frame wiring: PaneMode, double-click routing, keyboard/IME/wheel,
   save/reload, status segment, fs refresh.
7. Gates + live verification: edit `.env` + a YAML in the real UI with
   xdotool, verify atomic contents and enforcement rejection; RSS.

## Acceptance criteria

- `cargo test` green (+ config/glob, enforcement, env, text, editor, App
  routing tests), clippy clean, release build 0 warnings, ARK_SMOKE exit 0.
- Direct call `fs::write_editable_atomic(root, root/"src/main.rs", …)`
  fails with `NotWhitelisted`; `../outside` fails with `OutsideRoot`;
  allowed files save atomically (no `.ark-tmp` left).
- `.env` form preserves comments/order, masks secret-looking values in the
  list, edits keys/values, adds/removes entries; save re-parses on reload.
- YAML/TOML textarea: cursor + selection + undo/redo + Ctrl+A/C + IME
  preedit/commit; save/reload; dirty marker; external clean reload.
- Non-repo/read-only files still open in the viewer; config files only via
  the enforced editor.
- RSS re-measured; ≤100MB intact; no unbounded state added.
