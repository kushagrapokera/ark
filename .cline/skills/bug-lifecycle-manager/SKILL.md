---
name: bug-lifecycle-manager
description: Create, deduplicate, update and verify bug records in GitHub Issues using gh during authorized bug tracking, investigation and fix workflows. Track lifecycle evidence and history without a local bug ledger.
---

# GitHub Issues bug lifecycle manager

Use GitHub Issues as the single source of truth. The project implementation is
in `src/bug-lifecycle/`; run it using the existing Node/tsx toolchain:

```bash
gh issue list --repo OWNER/REPO --label ark-bug
gh issue create --repo OWNER/REPO --title "..." --body "$(cat /tmp/bug-request.json)"
```

Read [request formats and lifecycle rules](../../docs/process/bug-lifecycle-manager.md)
before the first write. The JSON file is transient command input, not a second
ledger. Read current issue metadata/history from GitHub for every operation.
Requires `gh` on PATH and `gh auth login` or `GH_TOKEN`. Never read `.env`, print
tokens or fabricate remote success when GitHub is unavailable.

## Scope and identity

For an authorized bug tracking/fix task, create/update the issue as part of the
work without repeated confirmations. This skill's installation is not permission
to publish unrelated review results, migrate old reports, merge code or contact
other people. The user/repository chosen for the task remains the scope.

1. Read the relevant existing issue and latest fix-verification notes. Historical
   Markdown failure counts do not establish that a bug is still open.
2. Search open and closed GitHub issues for the root cause, public contract,
   reproduction and any existing NB identifier. Use `gh issue list --repo
   OWNER/REPO --state all --search QUERY` for investigation; the helper itself
   paginates all issues without search-index lag before creating a record.
3. Reuse a stable `bug.key` (for example `NB-91`) across observations of the same
   defect. A changed title, file line or test run is not a new identity. Different
   root causes need different keys. Do not deduplicate merely by similar titles.
4. Reuse a managed issue through `report`; use explicit `adopt` for an existing
   open unmanaged issue after matching its identity. Closed legacy issues require
   historical reconciliation rather than silent reopening. Ambiguous matches
   must be resolved in GitHub before creating more issues.

## Lifecycle and evidence

- **Detected:** a new observation; CI failure alone belongs here.
- **Confirmed:** reproducible contract violation, using `systematic-debugging`.
- **In Progress:** diagnosis is confirmed and the fix is being implemented.
- **Awaiting Review:** root cause, fixing revision/link and passing results for
  every declared required check exist.
- **Verified:** the required tests and a passing code review refer to the same
  fixing revision. The helper closes the issue as completed.
- **Reopened:** a verified bug reproduces again; the helper reopens the issue
  and clears old fix/test/review evidence before a new verification cycle.
- **Blocked:** record a concrete blocker; resume the recorded prior state when
  it is removed. Do not mark a bug Verified to hide an environmental failure.

The transition table and evidence guards are enforced by code, not prose alone.
Declare meaningful `requiredChecks` at detection; include native Playwright
checks for browser-sensitive defects. Use `test-driven-development` and
`javascript-testing-expert` for regressions, `pr-test-analyzer` for coverage,
`silent-failure-hunter` for failure paths, relevant Clean Code references and
`verification-before-completion` before closing. Review can be performed by an
agent; do not require human review or extra subagents. Record an actual review
artifact/link, not just an assertion that a review occurred.

## Writing and failure recovery

Use the structured JSON command. Never interpolate issue text into shell code;
the helper sends API payloads through `gh api --input -`. Include a unique stable
`change.eventId`, reason and optional `expectedVersion` for updates. Redelivery
of the same event is idempotent; reuse of an event ID for different data fails.

The helper maintains one managed body section with readable status/evidence and
append-only event metadata, preserving human text outside that section. Do not
edit the encoded metadata, erase history, or maintain a parallel status in docs.
A generated summary may be used only as a disposable view of current Issues.

Prefer the serialized `bug-lifecycle` Actions workflow for multiple writers.
Direct CLI writes must not overlap Actions or another agent. Pre-write rereads
catch intervening edits but GitHub Issues has no atomic compare-and-swap.
If authentication, permission, rate-limit, timeout, JSON or conflict errors occur,
report the failure and inspect GitHub before retrying the same key/event. Never
blindly retry a create; the remote write may already have succeeded. No local
fallback ledger or silent issue creation is allowed. Never omit required tests
or invent results to clear an evidence gate.

The workflow records same-repository default-branch CI failures as Detected and
subsequent successful runs as observations. A green CI run never automatically
verifies a fix. Manual dispatches can report/adopt/update/list records with the
same validation. Tests run with fake GitHub boundaries and never create live
issues: dry-run mode (validate the request JSON and print the would-be `gh`
commands without executing them).
