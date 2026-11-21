# ark project skills

Skills are installed project-only. This kit was adapted for **ark** — an
ultra-lightweight agent-first desktop code editor in Rust (see `plan.md` and
`AGENTS.md` at the repo root).

- **Testing language is Rust**: unit and integration tests run via
  `cargo test`; lint gate `cargo clippy -- -D warnings`. The former
  `javascript-testing-expert` skill (Vitest/fast-check) was **removed** as
  inapplicable on 2026-11-21.
- **All UI/motion/web-design skills were removed** in the same pass
  (`ui-*`, `animate*`, `apple-design`, `mobile-native`, `design-system`,
  `emil-design-eng`, `impeccable`, `break-ui`, `prototype`,
  `pick-ui-library`): ark's UI is hand-drawn with tiny-skia; there is no
  `frontend/` directory and no web stack.
- Clean Code chapter skills and the planning/debugging/verification/review
  skills are language-agnostic and retained. The local
  `bug-lifecycle-manager` skill manages GitHub Issues directly via `gh`
  (the original npm wrapper script was replaced).

Three Superpowers planning skills are installed at a pinned revision:

| Skill | When it applies |
| --- | --- |
| `brainstorming` | Explore requirements, architecture, alternatives and trade-offs for a new project or complex feature |
| `writing-plans` | Convert clear requirements/design into an actionable implementation plan |
| `executing-plans` | Implement an approved or otherwise authorized plan task by task |

Their portable entrypoints use `implementation/` for design/plan artifacts,
honor existing authorization and use the installed testing skills. They do not
require uninstalled Superpowers worktree/SDD/branch-finishing skills or
mandatory subagents.

## Bug investigation and testing

| Skill | When it applies | Source |
| --- | --- | --- |
| `systematic-debugging` | Investigating a bug, failing test or unexpected behavior before proposing a fix | obra/superpowers |
| `test-driven-development` | Implementing a behavioral change or bug fix with a demonstrated failing regression | obra/superpowers |
| `verification-before-completion` | Claiming a change is fixed, complete or passing | obra/superpowers |
| `pr-test-analyzer` | Reviewing behavioral coverage, missing edge cases and assertion quality in a PR or local change | anthropics/claude-code PR Review Toolkit |
| `silent-failure-hunter` | Auditing catches, retries, cleanup and fallback paths | anthropics/claude-code PR Review Toolkit |

The two Anthropic agent definitions are adapted into portable skills with no
Claude-specific Task tool or mandatory subagents. Superpowers skills retain
supporting files, with local adjustments to preserve user work, use installed
companion names and avoid unnecessary approval checkpoints. Source and
installed-file hashes for the retained skills are in `bug-quality-sources.json`
and `assurance-sources.json` alongside this README.

Imported skills remain local under the existing ignore policy. The
project-owned `bug-lifecycle-manager` directory is explicitly included so it
travels with the tracked implementation, tests and workflow.
