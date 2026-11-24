# M10 — Agent launcher tabs

Master plan: `plan.md` (root), milestone M10. Feature 6: detect
`claude`/`codex`/`opencode`/`zcode` on PATH; one-click launch in the project
directory as a terminal tab; the agent's edits then flow through the existing
M4 viewer / M5 watcher / M7 git pipeline.

## Carry-forwards

1. `arboard` X11 clipboard fallback — still deferred.
2. `set_ime_cursor_area` for the config editor — still deferred (M9 note).
3. Windows agent discovery (`PATHEXT`, `.exe`) — deferred to the Windows
   port; Unix PATH/executable-bit detection is the M10 target.

## New dependencies

NONE. Detection is one bounded PATH scan (`std::env::split_paths` +
metadata/executable bit); agents run in the existing `portable-pty` tabs.
Ark never interprets, proxies or captures an agent's I/O beyond the terminal
grid it already maintains.

## Constraints (AGENTS.md — binding)

- Fixed registry: exactly four known agents; each detected at most once
  (first PATH hit wins). No unbounded process/agent lists.
- Detection never spawns a subprocess; it only stats PATH candidates.
- Launch is click-only (a header button) and uses the ABSOLUTE path found at
  startup, in the project root cwd, so a later PATH change cannot redirect
  the binary. Launch failures are logged, never silent.
- The agent's RAM is its own child process (the ≤100MB budget covers ark
  only); the PTY/scrollback caps from M6 apply unchanged.
- Agent edits are visible through the existing watcher; M10 adds no second
  file-refresh path.

## Frozen interfaces (implement verbatim)

```rust
// src/agents.rs
pub const AGENT_LABELS: [&str; 4] = ["claude", "codex", "opencode", "zcode"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind { Claude, Codex, OpenCode, ZCode }
impl AgentKind {
    pub const ALL: [AgentKind; 4];
    pub fn binary(self) -> &'static str;   // PATH name + button label
}

#[derive(Clone, Debug, Default)]
pub struct AgentRegistry { /* private found: Vec<(AgentKind, PathBuf)> */ }
impl AgentRegistry {
    pub fn detect() -> Self;                       // $PATH, absolute hits
    pub fn detect_in(paths: &[PathBuf]) -> Self;   // pure/testable
    pub fn entries(&self) -> &[(AgentKind, PathBuf)];
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn find(&self, kind: AgentKind) -> Option<&Path>;
    #[cfg(test)] pub(crate) fn from_entries(...) -> Self;
}

// src/term.rs — direct program tab (agent CLIs are interactive, not `-c`)
impl Terminal {
    pub fn spawn_program(root, program, label, size, proxy)
        -> Result<Terminal, TermError>;
}
impl TerminalTabs {
    pub fn open_program(&mut self, root, program, label, size, proxy)
        -> Result<(), TermError>;
}

// src/frame.rs
// Frame gains `agents: &AgentRegistry`; FrameEffects gains
// `launch_agent: Option<AgentKind>`. Header draws one button per detected
// agent (right-aligned, fixed width, skipped when the window is too narrow).
```

## Layout / interaction

- Header top-right: detected agent buttons (`claude`, `codex`, `opencode`,
  `zcode`) as the M2 button widget; right-aligned, 82×22 logical px with a
  6px gap. Buttons that would collide with the title zone (<220px) are
  omitted on narrow windows. Undetected agents get no button.
- Clicking an agent button reports `launch_agent`; the App opens a terminal
  tab that directly executes the scanned binary with cwd = project root,
  labels the tab with the agent name, switches to Terminals and focuses it.
- The info line appends `· agents N` so detection is visible even when the
  window is too narrow for all buttons; launcher failures log an error.
- Existing terminal tabs, Ctrl+T/W, process-group teardown, scroll caps and
  hidden-terminal drain behavior are reused unchanged.

## Tasks (inline; TDD, tests first)

1. `agents.rs`: registry, PATH scan, executable check, first-hit-wins,
   absolute paths; tests with fake executable files.
2. `term.rs`: `spawn_program` (+ test-only wake variant) and
   `TerminalTabs::open_program`; live test that the program runs with the
   project as cwd and streams output under an agent label.
3. `frame.rs`: agent button layout/draw/effects + `agents` field; pixel and
   click tests; update existing frame constructors.
4. App wiring: detect at startup (log hits), dispatch `launch_agent`, open a
   programme tab, switch/focus the pane; no second watcher path.
5. Gates + live verification: fake `claude` executable in PATH that writes a
   file and echoes; click → tab opens, output streams, tree/git refresh;
   real PATH detection shows installed agents; RSS.

## Acceptance criteria

- `cargo test` green (+ registry/layout/launch tests), clippy clean, release
  build 0 warnings, ARK_SMOKE exit 0.
- On a PATH containing executable `claude`/`opencode` (and a non-executable
  `codex`), only the executable hits produce buttons; clicking one opens a
  tab labelled with that agent, running from the project root.
- The fake agent's file write appears in the tree/viewer/git through the
  existing watcher (no M10-specific refresh code).
- No agent installed → no buttons, app unchanged.
- RSS re-measured with an agent tab; ark stays well under 100MB (agent RAM is
  its own process).
