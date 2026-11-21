---
name: independent-solution-auditor
description: Independently audit a completed implementation against original requirements, edge cases, security expectations, and verification evidence. Use before assurance completion or for a requested independent audit; ordinary test execution alone is not this audit.
---

# Audit the delivered solution

Use a separate authorized reviewer context where available. Give it the original
requirements, relevant source/fixtures, exact baseline and candidate or working-tree
diff, and commands needed to verify. Do not pass the author's desired verdict,
suspected bug, proposed fix, or reasoning history. A reviewer may read an evidence
report as a claim, but should form its own conclusions. No nested delegation.

The reviewer is read-only on the shared checkout. Use isolated temporary artifacts
for experiments. Do not commit, reset, switch branches, alter tests, access secrets,
or take external actions. Review scope does not grant production permissions.

Map each requirement to observable behavior and evidence. Inspect beyond visible
examples: missing paths, boundary sequences, errors, relevant security regressions,
hardcoded shortcuts, test manipulation, and mismatches between claims and the actual
candidate. Run meaningful independent focused tests for the reviewed behavior;
record exact commands and results. CI can supply independent execution only when
its evidence belongs to the reviewed revision and trusted workflow.

Reuse requesting-code-review for dispatch conventions and focused quality/coverage
specialists only where needed. Findings need locations, requirements, impact, and
reproductions or clear reasoning. Resolve disagreements using evidence; neither an
author's approval nor the number of passing tests dismisses a valid finding.

Report one verdict:
- PASS: reviewed requirements have supporting evidence and no unresolved material findings.
- FAIL: demonstrated incorrect behavior, missing required functionality, or security regression.
- NEEDS_REVIEW: missing requirements/evidence, unavailable independent context or execution,
  or unresolved findings prevent a supported conclusion.

Include coverage of requirements, findings, checks actually run, candidate identity,
and independence/execution limits. Never label self-review independent. A separate
AI context reduces anchoring; it does not guarantee correctness or replace a trusted
human approval when repository policy requires one.
