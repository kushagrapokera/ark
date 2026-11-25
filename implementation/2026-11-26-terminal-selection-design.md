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
- Drag selects locally when mouse reporting is off. When an app such as OpenCode
  enables mouse reporting, **Shift+drag** selects locally while ordinary mouse
  input continues to reach the app.
- **Ctrl+Shift+C** copies the selected terminal text through the existing
  `wl-clipboard-rs` path. `Ctrl+C` remains SIGINT.
- Pointer-to-grid mapping now includes device scale and UI scale, matching the
  rendered font metrics.

## Verification

- Pixel regression for fractional row seams.
- Selection extraction/rendering regression.
- Live Wayland selection-to-clipboard round trip.
- Live fish drag selection and colored-row rendering smoke.
- Full suite, clippy, release build.
