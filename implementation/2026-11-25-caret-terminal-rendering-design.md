# Caret blinking and terminal rendering fixes

## Problems

1. Fish's caret appeared one cell ahead of typed text at larger window scales.
2. OpenCode's terminal retained old colored row backgrounds after redraws.
3. Carets were static in terminals and every text-input surface.

## Root causes and fixes

- Terminal glyph runs were shaped at `UI_FONT_SIZE * scale`, while cell advances
  and the cursor used `UI_FONT_SIZE * scale * ui_scale`. Text therefore occupied
  less width than its grid cells. `flush_run` now receives and uses `ui_scale`.
- `Terminal::draw` painted only non-default cell backgrounds over the persistent
  canvas. Rows that returned to the default background kept stale ink. Every
  terminal repaint now clears the full terminal rectangle to `theme.bg` first.
- Caret visibility was hard-coded on. A shared `CaretBlink` clock now toggles a
  bounded visibility bit every 530 ms, resets to visible on keyboard/mouse/PTY
  activity, and stops when no editable surface is focused.

## Scope

The blink bit is applied to terminal carets, config textarea carets, `.env`
key/value fields, search, commit messages, and New File/New Folder prompts.
Unfocused surfaces never paint a caret. Blink redraws use full damage so hidden
carets cannot leave stale ink; the shared follow-up scheduler preserves the
earliest blink/autosave/terminal deadline.

## Verification

- Terminal background-clear regression.
- Terminal text/cursor UI-scale regression.
- Terminal and single-line caret blink-off pixel tests.
- Config textarea blink-off test.
- App blink reset/toggle/reschedule test.
- Full suite, clippy, release build, and live terminal/search blink smoke.
