# Config editor auto-save

## Goal

Remove the need to press `Ctrl+S` for the only human-editable surfaces in ark:
whitelisted `.env`, YAML, and TOML config files. Code and Markdown source remain
read-only.

## Behavior

- Dirty config edits are saved atomically after a 750 ms pause.
- Keyboard edits, IME commits, pastes, and applied `.env` form changes reset the
  debounce.
- The existing Rust whitelist and `write_editable_atomic` boundary remain the
  enforcement point; auto-save introduces no new write path.
- The Save button and `Ctrl+S` remain available for immediate saves.
- Pending edits are flushed before switching/closing config tabs, structural
  editor changes, or closing the window.
- If the file changed externally while dirty, auto-save pauses and keeps the
  user's buffer. Further edits do not hide the conflict; Reload or an explicit
  Save resolves it.
- Save failures remain visible in the config header and are logged. A failed
  flush keeps the editor/tab/window open rather than discarding edits.

## State and scheduling

`App` owns one `Option<Instant>` deadline. `schedule_config_autosave` arms it
only for a dirty editor without an external-change conflict. The winit
`WaitUntil` follow-up scheduler wakes at the deadline, and
`autosave_config_if_due` flushes through the existing atomic save routine.
Manual save/reload/open/close clear or flush the deadline.

## Verification

- Debounce timing and atomic disk result.
- External-change conflict preservation and no overwrite.
- Further edits keep the conflict visible.
- Closing a config tab flushes pending edits.
- Full unit/integration suite, clippy, release build, and live YAML/TOML plus
  `.env` auto-save smoke.
