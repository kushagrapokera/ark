---
name: requesting-code-review
description: Request a scoped independent review after substantial implementation or when a code review is needed. Use available authorized reviewer tools; test-integrity and requirement-audit skills own their specific assurance decisions.
---

# Request code review

Adapted from obra/superpowers at the revision in ../assurance-sources.json;
LICENSE.upstream preserves the upstream license. The dispatch and evidence model
is retained with portable tools and authorization boundaries.

Choose the true task baseline and candidate, including uncommitted and untracked
changes when applicable. Do not assume HEAD~1 covers a multi-commit task. Give an
independent reviewer the original requirements and relevant artifacts using
[code-reviewer.md](code-reviewer.md). Use an available subagent with a fresh context
when authorized, otherwise report that independent review remains unavailable.
Do not supply a preferred verdict or the author's reasoning history.

Reviewers are read-only on the shared checkout and do not delegate further. They
may run focused checks in an isolated workspace. Review is not authorization to
commit, publish, alter tests, or access external systems.

Address Critical and Important findings using evidence and scoped re-review. Do
not silently park a material requirement failure as complete. Route repeated
failed fixes through failure-loop-controller/systematic-debugging after three
failures. Distinguish quality review from independent CI execution and trusted
human approval. Record unresolved findings and use NEEDS_REVIEW if evidence is
missing; final verification still owns completion claims.
