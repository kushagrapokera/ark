# Terminal background seams and text selection

## Problems

- OpenCode-style terminal UIs showed thin lines between rows because fractional
  logical cell bounds were truncated independently during rasterization.
- Terminal text could not be selected or copied.

## Fixes

- Added `Canvas::fill_rect_cover`, which expands logical rectangles outward to
  whole device pixels. Terminal base clears, cell backgrounds, selection
  backgrounds, and the block cursor use it so adjacent cells cannot leave seams.
- Reused alacritty's bounded grid selection model. Terminal selection stores only
  the active range inside the existing grid/scrollback; no text copy or index is
  retained until copy is requested.
- A plain left-button drag selects locally whether mouse reporting is on or off.
  The initial press is deferred until release: if the pointer stays still, the
  click is delivered to mouse-reporting apps such as OpenCode; if it moves past
  the drag threshold, no mouse reports are sent and the text is selected.
- **Ctrl+Shift+C** copies the selected terminal text through the existing
  `wl-clipboard-rs` path. `Ctrl+C` remains SIGINT.
- Pointer-to-grid mapping now includes device scale and UI scale, matching the
  rendered font metrics.

## Verification

- Pixel regression for fractional row seams.
- Selection extraction/rendering regression, including backward drags.
- Plain-drag routing regression with mouse reporting both on and off; verifies
  no PTY mouse bytes leak during selection.
- Plain-click regression; verifies mouse-reporting apps still receive press and
  release.
- Live Wayland selection-to-clipboard round trips.
- Live OpenCode plain-drag selection smoke.
- Full suite, clippy, release build.
