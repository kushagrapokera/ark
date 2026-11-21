# ark — agent working rules

These rules apply to every prompt and every file in this project.

## What ark is (read plan.md before any planning turn)

- ark is an ultra-lightweight, agent-first desktop code editor: humans **view**
  code (read-only), AI agents edit it; humans edit only whitelisted config
  files (.env form editor, yaml/toml text editor).
- The plan of record is **`plan.md`** at the repo root (milestones M0–M11).
  Each milestone maps to a workspace under `implementation/<slug>/`.
- Hard constraint: **≤100MB RSS for the ark process** (agents run as child
  processes; their RAM is their own). Every dependency and design decision is
  judged against this budget. Projected: 30–70MB.

## Hard guardrails (override any other guidance)

1. **The stack is frozen** (see plan.md): `winit` + `softbuffer` + `tiny-skia`
   + `cosmic-text` (bundled fonts only), `portable-pty` + `alacritty_terminal`,
   `memmap2` + `notify`, git via subprocess, `clap`, `wl-clipboard-rs`/`arboard`.
   **No** wgpu/Vulkan, no webviews, no widget frameworks, no LSPs, no SVG/icon
   libs, no system font scanning. Do not add any dependency without explicit
   user approval plus a RAM justification.
2. **The code viewer is read-only for humans.** Never add editing features for
   code files. The only human-writable surfaces are the config editors
   (.env form, yaml/toml textarea), enforced at the Rust layer (`fs.rs`
   whitelist) — never just in the UI.
3. **Memory discipline is a feature, not an optimization.** Viewport-only
   rendering, lazy per-directory tree, LRU buffer cache, capped terminal
   scrollback, bounded glyph cache, bundled fonts only, `malloc_trim` on idle.
   Any data structure that can grow without bound is a bug.
4. **Milestone order is deliberate** (usable viewer by M4, terminal M6, config
   editor M9). Do not jump ahead or re-order without explicit user direction.

## Code organization

1. Create a separate module for each distinct concern, matching the layout in
   plan.md — `main.rs`, `render.rs`, `text.rs`, `ui.rs`, `tree.rs`,
   `viewer.rs`, `highlight.rs`, `term.rs`, `git.rs`, `configed.rs`,
   `agents.rs`, `fs.rs`, `watch.rs`, `state.rs`, `config.rs`. Never pile
   everything into one file.
2. If the same logic is needed twice, extract it once and reuse it.
3. Write proper error handling for each and every possible error — dedicated
   error definitions and clear messages (`thiserror` in library modules,
   `anyhow` at the edges). No silent failures. In particular: swallowed PTY
   bytes, ignored fs events, and dropped git output are all bugs.
4. Log significant operations with the `log` crate at appropriate levels
   (trace/debug for render paths — never log per-pixel/per-cell).
5. Perform unit and integration tests (`cargo test`) after each phase before
   it is considered done, plus a manual smoke run on a real repository.
6. Follow clear naming conventions — modules snake_case, types PascalCase,
   functions snake_case; names must be easy to read and understand.
7. Use a good, easy-to-understand file naming convention (one module per file
   pair `foo.rs` where it fits; split with `foo/` submodules when it grows).

## Project structure (Rust / cargo)

- One binary crate at the repo root: `Cargo.toml`, `src/` (all product code),
  `tests/` (integration tests), `implementation/` (step plans and SDD
  workspaces), `docs/` (design docs and review records). Entry point is
  `src/main.rs` — it must stay thin: CLI parsing + window bootstrap only.
- Every new concern gets its own module under `src/` — never a growing dump
  file. When a module passes ~400 lines, split it into `src/<mod>/` with
  submodules.
- `assets/` for bundled fonts and icons (fonts loaded via `include_bytes!`,
  never scanned from the system).
- `plan.md` at the root is the frozen plan of record; do not edit it without
  the user's explicit request.

## Coding guidelines (always-on)

Derived from
[andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills) (MIT).
These apply to every code-writing turn and complement the skill router below;
where they conflict with the numbered rules above, the numbered rules win.

1. **Think before coding** — state assumptions and tradeoffs before implementing;
   ask instead of silently choosing an interpretation. For complex design this is
   the `brainstorming` skill's job; for routine edits, state the assumption in one
   sentence and proceed.
2. **Simplicity first** — minimum code that solves the task. No unrequested
   features, single-use abstractions, or speculative configurability. This does
   not relax rule 3: every real error path still gets dedicated error definitions
   and no silent failures; only handling for genuinely impossible states is omitted.
3. **Surgical changes** — touch only what the task requires, match existing
   structure and naming, and remove only what your own change made unused; mention
   unrelated dead code instead of deleting it.
4. **Goal-driven execution** — convert the task into verifiable success criteria
   (failing test first where rule 5 applies) and loop until verified, per
   `verification-before-completion`.
5. **Prefer deterministic code** — never wrap a model call or heuristic guess
   around a question a pure function can answer (validation, routing, retries,
   type checks).

<!-- skill-router:start -->
## Skill selection workflow

Choose the current intent/stage and one lead skill. Explicit user requests take
priority within task authorization and skill scope. Simple explanations and
documentation edits need no workflow skill; validate actual changes appropriately.
Routing decisions come from this file — ark has no `config/skill-routing.json`
or npm routing scripts (that machinery belonged to the template this kit was
adapted from and was removed).

| Intent / stage | Lead skill |
| --- | --- |
| complex-design / requirements | `brainstorming` |
| implementation-plan / plan | `writing-plans` |
| approved-plan / execute | `executing-plans` |
| bug-investigation / investigate | `systematic-debugging` |
| behavior-change / implement | `test-driven-development` |
| coverage-review / review | `pr-test-analyzer` |
| failure-review / review | `silent-failure-hunter` |
| completion / complete | `verification-before-completion` |
| bug-tracking / track | `bug-lifecycle-manager` |
| request-review / review | `requesting-code-review` |
| delegated-plan / execute (explicit request only) | `subagent-driven-development` |
| implementation-integrity / review | `anti-reward-hacking` |
| test-integrity / review | `test-integrity-guardian` |
| independent-audit / review | `independent-solution-auditor` |
| repeated-failure / investigate | `failure-loop-controller` |

Removed from the original kit as inapplicable to a Rust desktop app with a
hand-drawn UI: `javascript-testing-expert` (Vitest/web) and every UI/motion/
web-design skill (`ui-*`, `animate*`, `apple-design`, `mobile-native`,
`design-system`, `emil-design-eng`, `impeccable`, `break-ui`, `prototype`,
`pick-ui-library`). Do not route work to them; they are not installed.

### Implementation assurance

For applicable implementation work: requirements → plan → TDD/implementation →
anti-reward-hacking → test-integrity-guardian → testing/debugging →
independent-solution-auditor → verification-before-completion. Failures return
to diagnosis. Use failure-loop-controller for recurring failed fixes; after three
unsuccessful attempts, record root-cause evidence before another speculative edit.
executing-plans remains the default executor. Delegated implementation requires
an explicit request; bounded read-only reviewer contexts are authorized for this
assurance workflow, without nested delegation or external writes.
Change evidence: `git diff --stat --base BASE [--head HEAD]` plus the test and
RSS records described below. Protected edits need independent justification/review;
candidate approval files cannot authorize themselves. Skills and local failure
histories remain advisory. See docs/process/assurance.md when it exists.

### Clean Code specialists

Choose the matching concern; do not run every chapter for each change.

| When needed | Specialist skill |
| --- | --- |
| cleanliness audit | `clean-code-ch01-clean-code` |
| naming review | `clean-code-ch02-meaningful-names` |
| function review | `clean-code-ch03-functions` |
| comment review | `clean-code-ch04-comments` |
| formatting review | `clean-code-ch05-formatting` |
| abstraction review | `clean-code-ch06-objects-and-data-structures` |
| error structure review | `clean-code-ch07-error-handling` |
| dependency boundary review | `clean-code-ch08-boundaries` |
| test maintainability review | `clean-code-ch09-unit-tests` |
| class review | `clean-code-ch10-classes` |
| architecture review | `clean-code-ch11-systems` |
| simple design review | `clean-code-ch12-emergence` |
| async state review | `clean-code-ch13-concurrency` |
| refactoring review | `clean-code-ch14-successive-refinement` |
| smell audit | `clean-code-ch17-smells-and-heuristics` |
| execution path review | `clean-code-appa-concurrency-ii` |

### UI craft for ark (replaces the old UI/motion specialist table)

- ark's UI is hand-drawn with tiny-skia in Rust; **there is no `frontend/`
  directory and no web stack**. The old UI/motion skills were removed.
- Visual consistency comes from the theme tokens and shared primitives in
  `src/render.rs` / `src/text.rs` and the widget kit scope in plan.md. Extend
  the widget kit only when a milestone requires it; no speculative widgets.

### Testing & verification commands (ark)

- `cargo test` — unit + integration
- `cargo clippy -- -D warnings` — lint gate
- `cargo build --release` — release binary
- RSS proof: `smem -P ark` (PSS) and `/usr/bin/time -v ./target/release/ark <repo>`
  — record the numbers; the ≤100MB budget is an acceptance criterion
- Big-repo smoke: open a Linux-kernel-sized checkout, expand the tree, run git
  status, do a commit+push on a scratch branch, verify live refresh

- Load specialists only for distinct requirements or demonstrated risks. Do not
  run all skills, every Clean Code chapter, or overlapping audits.
- Use TDD for implementation order, the test analyzer for coverage review, and
  Chapter 9 for test maintainability.
- Choose the matching Clean Code chapter for a specific review concern. Broad
  cleanliness/smell catalogs apply to broad audits or unresolved concerns.
- Reuse loaded instructions in the active context. Reassess when scope/stage
  changes or a check fails. Report missing skills; do not invent or install them.
- No mandatory per-task skill report or new approval/delegation gate is required.
  Preserve existing authorization and required unit/integration/smoke checks.
- GitHub Issues is the only bug record for authorized tracking. Preserve NB
  identities; historical Markdown failures do not establish current bugs. Use
  the `bug-lifecycle-manager` skill and serialized issue updates; do not migrate
  historical reports or create unrelated issues without task authorization.
<!-- skill-router:end -->
