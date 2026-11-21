---
name: executing-plans
description: Implement an approved or otherwise authorized plan step by step, tracking progress, resolving routine discrepancies and verifying each task before reporting completion.
---

# Executing implementation plans

Project adaptation of obra/superpowers. Source revision and local changes are
recorded in [the source manifest](../bug-quality-sources.json). This portable
workflow uses the installed skills and requires no external Superpowers helpers.

## Establish scope and resume state

Read the plan, its design/spec, project guidance and current working-tree diff.
Confirm execution is covered by the user's instructions; respect an existing
approval without asking for it again. A request to write/review a plan alone is
not a request to implement it.

Identify completed tasks from the actual files, test evidence and existing plan
checkboxes before starting. Preserve unrelated/user changes. Use the current
workspace unless isolation is needed; do not change branches, commit, merge or
publish solely because this skill is active.

Check task interfaces and dependencies against the design. Resolve routine
implementation discrepancies using the stated contract, and record material
choices with their reason. Ask only when a necessary decision cannot be inferred
or an action exceeds the authorized scope. Continue independent authorized work.

## Execute each task

1. Read its exact requirements and the files/interfaces it consumes.
2. Use `test-driven-development` for behavioral work. Demonstrate the regression
   fails for the intended reason, implement the change, and verify it passes.
   Preserve existing fixes; use an isolated baseline when necessary.
3. Run the task's relevant unit/integration checks and required smoke programs.
   Use `systematic-debugging` when a check fails instead of weakening assertions
   to match incorrect behavior. Native browser claims need Playwright evidence.
4. Compare actual output with the plan's expected result. If the plan is wrong,
   correct the smallest discrepancy consistent with the design and record why.
   Do not silently change scope or omit a required check.
5. Mark the task complete only after its deliverable and evidence exist. Record
   progress in the plan or a companion file under `implementation/`, including
   commands/results, deviations and remaining work. This is task progress, not
   a duplicate bug ledger.

Keep concise progress updates during execution. Continue through the authorized
plan rather than inserting routine approval gates. Resume from the recorded
state after interruptions or context compaction; do not repeat completed work.

## Final review and completion

Review the full change against the requirements, including cross-task effects.
Use relevant Clean Code skills, `pr-test-analyzer` for coverage review and
`silent-failure-hunter` for failure paths where appropriate. Self-review is
supported; delegate only when requested or otherwise authorized.

Fix demonstrated problems within scope and rerun affected checks. Use
`verification-before-completion` before claiming the plan is done. Report the
result, executed checks, unresolved requirements and material limitations.
Preserve plan progress/evidence; do not delete workspaces automatically. Follow
separate user instructions for committing, PR creation, merging or deployment.
