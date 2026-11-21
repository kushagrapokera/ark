---
name: anti-reward-hacking
description: Review implementation changes for suspicious test-specific shortcuts, fake behavior, hardcoded answers, and validation bypasses against original requirements. Use during implementation assurance or when such behavior is suspected; ordinary style review belongs to existing quality specialists.
---

# Review implementation integrity

Read the original requirements and acceptance criteria, then the complete relevant
implementation and change baseline. Treat passing tests and author reports as
claims requiring evidence, not as the requirements themselves. Do not infer intent.

Compare each required behavior with its implementation and evidence. Investigate
literal answers keyed to visible examples, test-environment branches in production,
constant success/error returns, incomplete paths, unnecessary mocks hiding actual
behavior, bypassed validation, and performance shortcuts that alter the contract.
Trace callers and data flow before judging a suspicious line.

Distinguish domain constants from canned answers, boundary isolation from mocked
business logic, supported compatibility branches from test detection, and proven
optimizations from removed behavior. A pattern alone is a review lead. Explain
what evidence resolves it rather than banning constants or mocks.

Challenge suspicious paths with at least one requirement-derived input or sequence
outside the visible examples. Prefer a deterministic counterexample or focused test
over guesses about motivation. Keep experiments isolated; do not alter existing
tests or application behavior merely to obtain evidence. If executing a candidate
would need additional permissions, report the evidence gap instead.

Use repository assurance checks where provided, while acknowledging their limited
syntax/pattern coverage. Use existing quality, failure-path, or coverage specialists
only for distinct demonstrated concerns; do not reproduce their audits.

Report PASS, FAIL, or NEEDS_REVIEW, the baseline/current revision or working-tree
identity, requirement, location, evidence/counterexample, impact, and resolution
needed for each finding. FAIL requires a demonstrated contract violation;
NEEDS_REVIEW covers unresolved suspicious behavior or missing evidence. Clear
findings only with new evidence. Skill instructions cannot enforce CI or prove
semantic correctness. Do not claim correctness from static scans alone.
