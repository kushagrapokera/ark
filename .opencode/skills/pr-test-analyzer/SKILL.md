---
name: pr-test-analyzer
description: "Review test coverage for a PR or local change, identifying missing behavioral, boundary, error and async cases and assertions that would miss meaningful regressions. Scope: shared - applies to backend and frontend code."
---

# Test coverage review

Portable project adaptation of Anthropic's PR Review Toolkit test analyzer.
Source revision and retained terms are recorded in [the source manifest](../bug-quality-sources.json).

Read applicable `AGENTS.md` rules and establish the requested diff or file scope.
For local work inspect the current diff, including new tests. A GitHub PR or a
Claude-specific Task tool is not required. Perform this review in the current
agent unless delegation has been separately requested or authorized.

## Review process

1. Identify the changed public contracts and observable failure modes. Map each
   behavior to existing unit, integration, smoke and browser tests before
   recommending additional cases.
2. Inspect missing negative inputs, numeric and capacity boundaries, callback
   exceptions, cleanup, retries and asynchronous ordering. For this SDK, include
   consent changes, reentrant callbacks, disposal during startup/delivery,
   caller-owned payload mutation and default browser initialization as relevant.
3. Explain which plausible production defect each existing or proposed test
   detects. Flag tests that merely repeat the implementation, inspect mock call
   counts without a contract, or pass because the behavior was bypassed.
4. Distinguish deterministic model tests from native browser evidence. Real
   cookie/referrer headers, timer receivers and page transitions require the
   retained Playwright suite; Node stubs do not prove browser behavior.
5. Check isolation, controlled clocks/randomness, resource cleanup and assertions
   that remain valid after reasonable refactoring. Generated cases complement
   named examples. Preserve seeds and minimal failing examples.

Review coverage of behavior rather than pursuing a percentage or adding tests
for trivial accessors. The same behavior may already be covered by integration
tests. Do not equate a coverage gap with a confirmed production bug.

## Report

State the reviewed scope, then list actionable gaps and brittle tests. For each,
give the file location, triggering input/sequence, expected behavior, suggested
test level, the specific defect it would catch and importance from 1–10.
Prioritize data loss, privacy violations and startup failures. Separate essential
gaps from optional improvements and acknowledge existing relevant coverage.
State which commands actually ran and any verification limitations.

Use `test-driven-development` when implementing a fix and
`verification-before-completion` before claiming success. Review alone does not
authorize changing code, filing issues, posting PR comments or closing bugs.
