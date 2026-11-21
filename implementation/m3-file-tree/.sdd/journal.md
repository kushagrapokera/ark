# Journal — m3-file-tree

(SDD workspace opened 2026-11-22. Entries appended by the orchestrator at
gates and completion; subagent evidence lives in report-*.md files.)

## Baseline at M3 open

- M2 closed with: 48 tests passing (43 unit + 5 CLI), clippy --tests
  -D warnings clean, peak RSS 17.9 MB, binary 7.0 MB.
- Carry-forwards bound into this milestone: ui.rs split (1,519 lines),
  partial damage (Damage::mark + allow removal), main.rs ≤ 400 lines.
- Workflow: AGENTS.md + subagent-driven-development + skills + tests as
  acceptance gates (user directive, continuing from M2).

## M3 close (2026-11-22)

- Workflow change (user directive): Task 2 implemented INLINE ("do it
  normally… do not implement the subagents until I ask you to do so") —
  no subagent dispatch; TDD + all acceptance gates unchanged. Task 1's
  fix-round-1 leftovers (selection getter, allow narrowing, dead-code
  attrs) were closed by the same inline pass.
- M3 closed: 69 unit + 5 CLI tests green; clippy --release --tests
  -D warnings clean; release build 0 warnings; ARK_SMOKE exit 0;
  main.rs 399 lines; screenshot verified (real repo tree, exact status
  line); RSS 20.7 MB (budget intact). Carry-forwards for M4: text.rs
  438 lines (pre-existing M1/M2) exceeds the ~400 guidance; keyboard tree
  nav (M4 scope) will consume the allow-gated frozen surfaces
  (is_expanded/clear_selection/RowHit.y).
