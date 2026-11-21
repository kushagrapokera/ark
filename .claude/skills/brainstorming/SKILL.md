---
name: brainstorming
description: Explore requirements, architecture, alternatives and trade-offs before a new project or complex feature, resolving consequential unknowns before implementation. Simple, clearly specified changes can proceed directly.
---

# Brainstorming

Project adaptation of obra/superpowers. The pinned source and local changes are
recorded in [the source manifest](../bug-quality-sources.json).

## Understand the intended outcome

Read applicable project guidance and relevant existing code/designs. Establish
who needs the feature, the problem it solves and what observable success means.
Build on the existing TypeScript SDK, its public contracts and test fixtures.
Separate stated requirements from your assumptions.

For a clear, bounded request, proceed through the normal workflow. For a complex
feature, identify consequential unknowns: scope, non-goals, API/data ownership,
privacy, compatibility, lifecycle behavior, performance and failure recovery.
Ask focused questions only where the answer changes the design. Continue useful
independent exploration while waiting. Do not require permission for routine
read-only exploration already covered by the user's request.

## Compare approaches

Consider plausible alternatives privately, then present the useful options with
their costs, risks and trade-offs. Recommend an approach and explain why it fits
this repository. Cover component boundaries, data flow, state transitions, error
handling and how behavior will be verified. Do not silently turn an assumption
into an agreed requirement.

Use a compact diagram when it clarifies a relationship. For an optional browser
mockup companion, read [visual-companion.md](visual-companion.md); launch its
server only if the user wants that companion. Text or Mermaid is sufficient for
ordinary architecture discussion.

A small authorized spike can resolve a specific uncertainty. Label it as a
prototype and distinguish what it demonstrates from production readiness.

## Design and handoff

Save a substantial software design to
`implementation/YYYY-MM-DD-<topic>-design.md`, unless the user chooses another
location. Include goals/non-goals, requirements, selected approach, alternatives,
interfaces, important edge cases, test strategy and unresolved decisions.
Self-review using [builder-check-prompt.md](builder-check-prompt.md) as a checklist.
A separate reviewer is optional and requires authorization for delegation.

If a necessary product decision remains unresolved, present the concrete design
for review. Respect an existing approval or an instruction to proceed with your
judgment; do not repeat approval checkpoints for authorized work. Once design is
clear, use `writing-plans` for a multi-step implementation. Installing this skill
alone does not authorize starting a new feature, committing or publishing.
