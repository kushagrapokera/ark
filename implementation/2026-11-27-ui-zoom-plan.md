# Whole-UI zoom implementation plan

Design: `implementation/2026-11-27-ui-zoom-design.md`

## Goal and constraints

Implement the approved session-only whole-UI zoom without new dependencies, persistent settings, unbounded state, or changes to the read-only code-viewer contract. Keep the frozen rendering stack and ≤100 MB ark-process budget.

## Files and responsibilities

- `src/zoom.rs` (new): zoom constants, clamping, keyboard/wheel gesture classification.
- `src/zoom/tests.rs` (new): pure input and boundary tests.
- `src/zoom/app_tests.rs` (new): App scale/pointer/damage state tests.
- `src/main.rs`: module wiring, App scale state, global shortcut routing, layout call.
- `src/frame.rs`: zoom-aware layout entry point with nominal-size breakpoints.
- `src/frame/zoom_tests.rs` (new): fit and breakpoint tests.
- `src/frame/chrome.rs`: shared logical-coordinate icon draw helper.
- `src/frame/chrome/icon_tests.rs` (new): chrome icon HiDPI regression.
- `README.md`: shortcut documentation.

## Task 1 — Zoom math and gestures (TDD)

- [x] Add `src/zoom.rs` with `MIN_ZOOM = 0.75`, `MAX_ZOOM = 1.50`, `ZOOM_STEP = 0.10`.
- [x] Add failing tests in `src/zoom/tests.rs`:
  - `zoom_key_step_handles_plus_equal_and_minus`: Control plus/equal → `+1`; minus → `-1`. Winit maps numpad plus/minus to logical character keys on supported platforms.
  - `zoom_key_step_requires_control`: same keys without Control → `None`; unrelated Control key → `None`.
  - `zoom_wheel_step_matches_requested_direction`: `LineDelta(_, -1)` → `+1`; positive y → `-1`; zero/PixelDelta → `None`.
  - `next_zoom_is_bounded_and_quantized`: 1.00 ± one step → 1.10/0.90; repeated steps clamp at 1.50/0.75; non-finite current resets to 1.00.
- [x] Implement the pure functions and rerun the focused tests.

## Task 2 — App zoom state and routing (TDD)

- [x] Add `device_scale` and `zoom` fields to `App`; keep `scale` as effective `device_scale * zoom`.
- [x] Add failing `src/zoom/app_tests.rs` coverage:
  - Default state is device 1.0 / zoom 1.0 / effective scale 1.0.
  - `adjust_zoom(+1)` at device scale 2.0 produces zoom 1.1 and effective scale 2.2.
  - Pointer logical coordinates change inversely with scale, preserving physical position.
  - A successful zoom marks full damage; min/max no-op returns false and leaves scale unchanged.
  - Scale changes cancel stale divider/scrollbar/terminal drag state and preserve the pointer's physical position.
- [x] Implement `adjust_zoom`, effective-scale updates, pointer preservation, stale-click clearing, logging, full damage, and redraw.
- [x] Route `zoom::key_step` at the start of `KeyboardInput`, before prompts/editors/terminals.
- [x] Route Control+`LineDelta` at the start of `MouseWheel`; consume all Control wheel events.
- [x] Update window-ready and scale-factor paths to preserve zoom while changing device scale.

## Task 3 — Zoom-aware layout (TDD)

- [x] Add failing `src/frame/zoom_tests.rs`:
  - At 1024×768, zoom 1.50 keeps the same responsive `ui_scale` and Explorer visibility as zoom 1.00.
  - For 400×300, 1024×768, 1440×900, and 2048×1536 nominal windows, device scales 1/2, and zoom 0.75/1.00/1.50, every positive layout rect is finite and within the effective logical viewport.
  - Terminal grid dimensions fit the recomputed terminal rect at each zoom.
- [x] Add `app_layout_with_visibility_and_zoom(..., zoom)`; use nominal `w * zoom` / `h * zoom` for breakpoints and responsive scale, effective `w` / `h` for geometry.
- [x] Keep existing layout functions as `zoom = 1.0` wrappers.
- [x] Call the zoom-aware entry point from `App::app_layout`.

## Task 4 — Chrome icon coordinate contract (TDD)

- [x] Add `chrome::draw_icon` preserving current call behavior and a failing HiDPI test in `src/frame/chrome/icon_tests.rs`.
- [x] Assert a logical icon at scale 2 has twice the physical position/extent of scale 1 (±2 px AA slop).
- [x] Fix `draw_icon` to pass logical coordinates/size exactly once; route all three chrome icon call sites through it.

## Task 5 — Documentation and full verification

- [x] Document shortcuts and 75%–150% range in `README.md`.
- [x] Run `cargo test --locked --all-features --all-targets`.
- [x] Run `cargo clippy --locked --all-targets --all-features -- -D warnings`.
- [x] Run `cargo build --release --locked --bin ark`.
- [x] Run `tests/source-installer.sh`, install with `ARK_SKIP_BUILD=1`, and run X11/Wayland `ARK_SMOKE=1`.
- [x] Measure RSS/PSS with the preserved `/proc` + Python `resource` method; require <100 MB.
- [x] Review diff for unrelated files, then commit and push the authorized feature.

## Verification evidence

- `cargo test --locked --all-features --all-targets`: **454 main-unit + 9 library-unit + 5 CLI tests passed**, 0 failed, 5 existing ignores.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: clean.
- `cargo build --release --locked --bin ark`: succeeded.
- `tests/source-installer.sh`: passed.
- Installed binary hash matched `target/release/ark`.
- Forced-X11 live input smoke: `Ctrl++` logged 110%, `Ctrl+-` returned to 100%, Ctrl+wheel down logged 110%, and Ctrl+wheel up returned to 100%.
- Default Wayland `ARK_SMOKE=1`: first frame presented and exited 0.
- X11 first-frame memory: **25,180 KiB peak RSS / 22,122 KiB sampled PSS**, below 100 MB.
- New modules are rustfmt-clean and `git diff --check` passes. Repository-wide formatting retains its pre-existing baseline failures.

## Deviation

Winit 0.30 exposes numpad plus/minus as logical `Character("+")` / `Character("-")` rather than `NamedKey` variants, so the gesture tests use the actual logical-key contract instead of unreachable numpad enum variants.
