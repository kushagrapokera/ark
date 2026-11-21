---
name: writing-plans
description: Turn clear requirements or a design into an actionable implementation plan for a multi-step feature, with exact files, interfaces, dependencies and meaningful verification steps.
---

# Writing implementation plans

Project adaptation of obra/superpowers. Source revision and local changes are
recorded in [the source manifest](../bug-quality-sources.json).

Read the requirements/design, applicable project instructions and affected code.
If a consequential requirement is unresolved, use `brainstorming` or ask a
focused question; do not invent agreement. Keep planning proportionate to scope.

Save the plan under `implementation/YYYY-MM-DD-<feature>-plan.md` unless the user
specifies a location. Do not create an isolated worktree merely to write a plan.

## Plan contents

Start with the goal, design reference, architecture, existing stack, constraints,
assumptions and non-goals. Identify files and responsibilities before listing
implementation tasks. Follow existing modules and reusable fixtures; avoid
unrelated refactors and speculative dependencies.

Break the work into independently verifiable tasks ordered by dependency. For
each task specify:

- Exact files to create/modify and their responsibilities.
- Consumed/produced interfaces, signatures, types and required values.
- Observable behavior and relevant error, boundary and lifecycle cases.
- Regression assertions and meaningful unit/integration/smoke checks.
- Commands to run and what result establishes success.
- Dependencies on earlier tasks, using the same interface names throughout.

Use checkboxes for checkable actions. For behavioral code, plan the
`test-driven-development` fail/pass/refactor cycle. Use Vitest `.test.ts` files
and existing fixtures; use the separate Playwright `.spec.ts` suite for native
browser behavior. Treat documentation/configuration with appropriate validation
rather than invented application tests. A test must identify a plausible defect
it would detect; avoid implementation-mirroring assertions.

Give exact test names/assertions and unambiguous implementation guidance without
transcribing every function body. Replace vague steps such as "handle edge cases"
with the actual input, sequence and expected contract.

## Self-review and handoff

Map every requirement to a task. Check interface consistency and dependency
ordering. Identify important failure modes the happy-path tasks missed and add
appropriate checks to the owning task. Consider consent reentrancy, disposal,
bounded queues, privacy and default browser startup where relevant. Use
`pr-test-analyzer` for a targeted coverage review when useful.

Link the plan for review. If the user requested planning only, stop at the plan.
If implementation is already authorized and no necessary decision remains,
continue with `executing-plans`; otherwise obtain the missing decision before
its dependent work. Existing approval persists. No mandatory subagent workflow,
additional skill installation or automatic commit/push is implied by this plan.
