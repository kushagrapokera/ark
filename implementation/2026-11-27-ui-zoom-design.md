# Whole-UI zoom design

Date: 2026-11-27

## Goal

Add session-only whole-UI zoom:

- `Ctrl++` / `Ctrl+-` increase/decrease zoom.
- `Ctrl+wheel down` increases zoom; `Ctrl+wheel up` decreases zoom.
- Text, Explorer icons/rows, editor content, terminal grids, toolbars, and panel spacing scale together without overlap or out-of-bounds layout.

## Decisions

- Zoom is a multiplier from **0.75× to 1.50×**, in **0.10** steps.
- Zoom is session-only and resets when ark restarts.
- The effective render scale is `device_scale * zoom`.
- Layout uses the effective logical viewport (`physical / effective_scale`), so geometry scales with zoom.
- Responsive breakpoints and automatic text scaling use the nominal window size (`physical / device_scale`), not the zoomed viewport. Zoom must not hide the Explorer merely because the effective viewport crossed a breakpoint.
- `Ctrl+wheel` consumes the event even at a zoom limit; it must not fall through to scrolling.
- Keyboard zoom works globally, including while a terminal, search field, commit field, or config editor has focus.
- `PixelDelta` trackpad events are not zoom gestures; ordinary wheel handling remains unchanged.

## Architecture

- New `src/zoom.rs`: bounded zoom math and pure key/wheel gesture recognition.
- `App` stores `device_scale`, `zoom`, and effective `scale`; `adjust_zoom` updates scale, preserves the pointer's physical position, cancels stale geometry drags, clears stale click state, marks full damage, and requests a redraw.
- `frame::app_layout_with_visibility_and_zoom` accepts the zoom multiplier. Existing layout entry points delegate with `zoom = 1.0`.
- Existing renderers continue receiving the effective scale; no per-widget zoom state is added.
- Terminal visual/PTY resizing, viewer/tree offset clamping, Markdown/config layout, and scrollbar geometry use the recomputed layout on the next full frame.

## Existing code issue exposed by zoom

Chrome icon call sites pre-scale coordinates and size and then pass the scale to `icons::draw`, scaling twice. A shared `chrome::draw_icon` helper will enforce the logical-coordinate contract and get a HiDPI regression test.

## Non-goals

- No persisted setting or `ark.toml` schema change.
- No `Ctrl+0` reset shortcut.
- No touchpad/pixel-delta zoom gesture.
- No dependency, asset, font, or cache additions.

## Verification

- Pure gesture tests for plus/equal/numpad-plus, minus/numpad-minus, Ctrl gating, wheel direction, limits, and invalid values.
- App tests for effective scale, pointer preservation, full damage, and bounded no-op limits.
- Layout tests across window sizes, device scales, and zoom limits for finite in-bounds geometry and stable responsive breakpoints.
- Chrome icon HiDPI test proving position/size scale exactly once.
- Full Rust suite, clippy, release build, source-installer test, and installed X11/Wayland first-frame smoke.
