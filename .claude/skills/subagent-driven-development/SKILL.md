---
name: subagent-driven-development
description: Execute an implementation plan with fresh implementer contexts and independent per-task review when delegated implementation is explicitly requested. Only runs when explicitly invoked; executing-plans remains the default inline executor.
---

# Execute with isolated agent contexts

Adapted from obra/superpowers at the revision in ../assurance-sources.json.
Retain fresh implementers, separate task review, scoped re-review, and final review.
LICENSE.upstream preserves attribution. Portable briefs replace upstream wrappers;
no unavailable companion skills or automatic commits/worktrees are required.

Read the authorized plan, original requirements, current diff, and progress record.
Use executing-plans for overall progress/verification conventions. Preserve user
changes. This skill requires explicit delegation and suitable tools; otherwise
continue inline if authorized and disclose that delegation is unavailable.

Dispatch one implementer at a time using [implementer-prompt.md](implementer-prompt.md).
Give only the task requirements, relevant interfaces, scope, and shared constraints;
use fresh contexts and the session model unless the user specifies otherwise.
Implementers may not delegate. Do not require commits or branch changes.

After each task, request an independent review using
[task-reviewer-prompt.md](task-reviewer-prompt.md). Resolve material findings, then
use [re-review-prompt.md](re-review-prompt.md) for fixes. Never adjudicate away an
unmet acceptance criterion as complete. Three unsuccessful fixes require the
failure-loop-controller and systematic-debugging before further implementation;
this overrides upstream five-round retry loops. Record attempts and progress so
compaction does not restart completed work.

After all tasks, use requesting-code-review for whole-change review and
independent-solution-auditor for requirement assurance. Verify actual artifacts
and commands before completion. Missing independent evidence means NEEDS_REVIEW.
Do not merge, commit, publish, or broaden permissions solely because this workflow
is active. Ask for consequential missing requirements while continuing independent
work already authorized.
