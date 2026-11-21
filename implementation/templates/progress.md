# SDD ledger — plan: implementation/<plan-slug>/plan.md

<!-- ORCHESTRATOR-ONLY FILE. Subagents never read or write this file.
     First line above must always name this ledger's plan file. A ledger
     whose first line names a different plan belongs to another run —
     leave it alone and create your own workspace. -->

## Rules

- One block per task. A task is DONE only when its block carries a
  `Task: complete` line — completed tasks are never re-dispatched.
- Recovery rule (after compaction or restart): trust this ledger and
  `git log` over your own recollection.
- Fix rounds are recorded inside the task block, numbered
  (fix round 1, fix round 2, ...). Three failed fix rounds on the same
  task require a bug-investigator root-cause report before any further
  speculative edit.
- The orchestrator is the only writer of this file.

## Task 1: <name>

- Task: pending | in-progress | blocked
- Handoff: implementation/<plan-slug>/.sdd/handoff-1.md
- Report: implementation/<plan-slug>/.sdd/report-1.md
- Fix rounds: (none)
- Task: complete    ← move to this line only when acceptance criteria are verified
