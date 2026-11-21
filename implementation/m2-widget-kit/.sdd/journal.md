# Journal — m2-widget-kit

(SDD workspace opened 2026-11-21. Entries appended by the orchestrator at
gates and completion; subagent evidence lives in report-*.md files.)

## Baseline at M2 open

- M1 closed with: 18 tests passing, clippy --tests -D warnings clean,
  peak RSS 17.3 MB, binary 7.0 MB.
- Workflow per user directive: AGENTS.md + subagent-driven-development +
  required skills + testing as acceptance gates.

## 2026-11-21 — M2 complete (subagent-driven development)

### Workflow executed

Per user directive: AGENTS.md + SDD kit + skills, tests as acceptance gates.
- Task 1 (UI foundation): fresh implementer → 5 TDD tests → test-review ISSUES
  (mutation-proven visual blind spots) → fix round 1 (mutation-proof pins +
  degenerate-input contract) → fix round 2 (text baseline bug: draw_line
  omitted +run.line_y; RED→GREEN) → scoped re-review PASS. Task closed.
- Task 2 (interactive widgets): fresh implementer → 4 frozen widgets +
  routing + demo scene (12+ tests) → test-review PASS (9 mutations: 8 caught,
  1 gap proven) + integrity-review PASS (9 audit items) → fix round 1
  (guard tests caught TWO real bugs: caret byte/char-count confusion, NaN
  offset poisoning) → independent audit PASS w/ 1 new MEDIUM (logical
  baselines in TabBar/TextInput) → fix round 2 (fix + full call-site sweep
  caught one more in main.rs content box). Closed.
- Skills followed: executing-plans, subagent-driven-development,
  test-driven-development (RED→GREEN every behavioral item),
  verification-before-completion (Iron Law: orchestrator ran every gate
  itself before claiming done).
- Fix-round accounting: Task 1 used 2 of 3; Task 2 used 2 of 3. No
  failure-loop-controller escalation needed.

### What M2 delivered

- `src/ui.rs`: Rect (strict-interior contains), Pointer (one-shot click),
  draw_button (mutation-proof visual contract), draw_checkbox, Scrollbar
  (+thumb_geometry, drag state machine, NaN/Inf guards), TabBar (accent
  underline active state), Split (clamped drag), TextInput/TextInputKey
  (UTF-8-safe cursor model, named-keys-first routing w/ control-char filter).
- `src/main.rs`: DemoState, pure demo_layout/demo_status/keys helpers, M2
  scene (tab bar, split panes, button, checkbox, input, clipped content box
  w/ scrollbar demo, live status line), pointer/keyboard/wheel routing,
  ARK_SMOKE hook (exits after first present — CI-safe).
- `src/icons.rs`: HiDPI fix — logical-coordinate contract, constant stroke
  width, single transform.
- `src/text.rs`: baseline-composition fix (+run.line_y — was ~16px high).

### Verification evidence (orchestrator, fresh)

- cargo test: **43 unit + 5 integration = 48 passed, 0 failed**
- cargo clippy --release --tests -- -D warnings: **clean**
- cargo build --release: clean; binary **7.0 MB**
- ARK_SMOKE=1: **exit 0** after first present
- Live RSS: **VmHWM 18,376 kB (17.9 MB)** / PSS 15,037 kB → **18% of budget**
- Screenshot: all widgets at spec positions; status line exact
- Review trail: 5 independent subagent reviews (2 test, 2 integrity, 1
  audit) + scoped re-review; 5 mutations-class bugs + 3 production bugs
  caught by gates (baseline composition, caret byte/char, NaN offset) before
  they could reach M3.

### RSS budget status

17.9 MB peak / **100 MB budget → 18% used** with the full widget kit. M3
(file tree, virtualized lazy loading) re-measures.

### Known risks / notes for M3

- ui.rs (1519 lines) + main.rs (1080) exceed the ~400-line guidance — M3
  handoff MUST split ui.rs (widgets/ + panels/) and extract scene glue.
- render.rs Damage #[allow(dead_code)] removal lands when M3 panels emit
  partial damage.
- Fractional-scale displays untested by hand (scale-event path exercised at
  1.00x; scale-proportional tests now pin the math at 2.0x).
- Repo not yet committed (user's call).

### Stable commit hash

(none — awaiting user git init/commit)
