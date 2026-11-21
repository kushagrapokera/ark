---
name: failure-loop-controller
description: Track failed fixes for the same issue, prevent unsupported repeated approaches, and escalate after three unsuccessful attempts to systematic-debugging. Use during recurring implementation or verification failures; not a replacement for a bug tracker or root-cause analysis.
---

# Control repeated failed fixes

Identify the issue by stable symptom and acceptance criterion. Read the existing
attempt history before another fix, including after context compaction. Do not
reset the counter by renaming the issue, rewording an approach, switching agents,
or alternating between already unsuccessful solutions.

For each attempted fix, record hypothesis/approach, affected behavior, verification
command, observed outcome, and reason the approach failed. Count unsuccessful
implementation hypotheses, not the expected initial red phase of TDD or identical
reruns caused solely by an environment restriction. Record environment limitations
separately. Keep this concise and separate from the project's authorized bug records.

Before implementation, check whether the same approach already failed without new
evidence. If so, stop that approach. After three unsuccessful fixes for the same
issue, stop speculative edits and use systematic-debugging for root-cause analysis
and architectural reassessment. Do not reproduce its diagnostic procedure here.

Resume only after recording genuinely new diagnosis evidence, how it explains the
observed failure, and the new discriminating hypothesis. A generic 'try again' or
counter reset is not diagnosis. Keep all previous attempts visible. Continued
failure returns to diagnosis; material scope decisions still need user input.

Use the repository's attempt-history helper if available, or a local progress
record otherwise. Editable records and helper-only checks are advisory: an agent
can omit attempts or bypass the helper. Reliable blocking needs a trusted external
execution boundary. Do not claim this skill controls arbitrary file edits.

Report the issue, attempted approaches/outcomes, current count, stop/resume decision,
new evidence if any, and next diagnostic action. Do not claim resolution until the
original acceptance criterion is independently verified.
