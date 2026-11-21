---
name: silent-failure-hunter
description: "Audit error handling, retries, cleanup and fallbacks for hidden failures in a PR or local change, distinguishing documented SDK rejection from suppressed unexpected errors. Scope: shared - applies to backend and frontend code."
---

# Failure-path review

Portable project adaptation of Anthropic's PR Review Toolkit error-handling
reviewer. Source revision and retained terms are recorded in
[the source manifest](../bug-quality-sources.json).

Read applicable `AGENTS.md` rules and establish the requested review scope.
Review in the current agent; a Claude-specific Task tool, external PR or
additional agent is not required.

## Trace each failure path

- Locate catches, error callbacks, rejected promises, retries, default-return
  paths and fallback chains. Follow failures through callers and cleanup.
- Determine the documented contract: throw, tagged result, refused event,
  bounded retry or best-effort cleanup. Identify exactly what can fail and
  whether the result falsely reports success or loses important context.
- Check that retry exhaustion, cancellation and disposal remain observable,
  release sibling resources and preserve event-accounting invariants.
- Inspect diagnostics through `src/logging_setup/logger.ts`, tagged errors in
  `src/errors/`, and `src/shared/health-counters.ts` where appropriate. Do not
  introduce assumptions about Sentry, Statsig or another project's helper APIs.
- Check that logging/error hooks cannot recursively report their own failures
  or break the host page. Keep diagnostic context bounded; never expose cookies,
  URL tokens, credentials or raw customer payloads in logs.

A documented refusal or unavailable browser capability is not automatically a
defect. Optional chaining and default values require contract analysis. A broad
catch may be intentional at a host callback boundary; require safe observability
and correct recovery rather than changing a non-throwing SDK contract to throw.
Do not add user-facing UI to surface an SDK-internal failure.

Flag unexpected errors that disappear, misleading success results, unsafe
fallback transport choices, infinite/unbounded retries and cleanup that stops
after the first failure. Support each finding with a concrete input or sequence,
the affected contract and its demonstrated consequence. Treat unverified
suspicions separately from reproduced bugs.

## Report

For each finding provide location, severity based on impact, hidden error,
trigger, observable consequence, existing diagnostic gap, recommended correction
and a focused regression test. Distinguish confirmed defects from investigation
items. State executed checks and missing evidence; do not declare a bug fixed
from inspection alone.

Use `systematic-debugging` to investigate a confirmed failure and
`verification-before-completion` for a completion claim. This review does not
authorize external issue/PR updates or bug lifecycle automation.
