# Journal — m1-render-foundation

## 2026-11-21 — M1 complete

### What we did

- Bundled **JetBrains Mono Regular + Bold** (OFL license included) via
  `include_bytes!` — loaded through `fontdb::Source::Binary` into
  `FontSystem::new_with_fonts`; cosmic-text compiled **without `fontconfig`**
  so system fonts are never scanned into RAM (memory rule 4).
- `src/render.rs`: Theme tokens (Catppuccin Mocha-inspired), persistent
  tiny-skia Canvas (premultiplied RGBA), Damage accumulator (capped rect
  tracking with full short-circuit — ready for partial presents from M3),
  `blit_to_buffer` packing into softbuffer's documented `0x00RRGGBB` layout.
- `src/text.rs`: TextSystem — monospace metrics **measured from real layout**
  (probe run, cached per quarter-pixel size), `draw_line` positions glyphs by
  exact baseline and blits swash rasters (mask: 1 B/px coverage → premultiplied
  src-over; color: 4 B/px RGBA) with bounds clipping.
- `src/icons.rs`: 5 icons as tiny-skia paths in logical space, stroked/filled
  under a scale Transform (stroke width scales → crisp HiDPI).
- `src/main.rs`: HiDPI scale tracking (ScaleFactorChanged → full damage),
  event-driven redraw only (zero idle CPU), M1 verification scene.

### Key API discoveries (verified from crate source)

- cosmic-text **0.14.2 has no `no-default-fonts` feature**; the system-font
  scanner is the `fontconfig` cargo feature → disable default features.
- `LayoutGlyph` has **no** `cache_key`; it lives on
  `glyph.physical((x, baseline), scale).cache_key`.
- `SwashCache::get_image` returns `&Option<SwashImage>`; Mask content is
  **1 byte per pixel** (coverage); Color is 4 bytes non-premultiplied RGBA.
- cosmic-text maps `Family::Monospace` → "Noto Sans Mono" (not bundled!) —
  we address our font as `Family::Name("JetBrains Mono")`.
- softbuffer 0.4.8: `BufferMut::present_with_damage(&[Rect])` exists and is
  Wayland-native (zero-copy partial presents) — adopted as the present path.
- softbuffer buffer format is documented `0x00RRGGBB` on all backends.

### What we tested / found

- `cargo build --release`: clean
- `cargo clippy --release -- -D warnings`: **clean** (fixed: TextStyle struct
  for 8-arg lint, no-effect `.min(255)`, `as_chunks::<4>`, dead-code scope)
- Live Wayland smoke run: window opened, scene presented, clean exit
- **Screenshot visually verified**: bold accent header; metrics line shows
  mono advance **8.40px = 14px × 0.6 exactly** (JetBrains Mono metric);
  pangram/digits/symbols crisp; box-drawing + Unicode (ü ∑ ≈ ≠) render from
  the bundled font; 5 icons drawn; 6 theme swatches + border rule
- **M1 memory**: peak (VmHWM) **17,748 kB (17.3 MB)**; PSS 14,302 kB
  (up from 12.9 MB at M0 — text stack + larger binary as expected)
- Binary: 7.0 MB

### RSS budget status

17.3 MB peak / **100 MB budget → 17% used** with the full text foundation
active. Re-measure after M2 (widget kit).

### Known risks / notes

- Box-drawing glyph joins look slightly loose at 14px — cosmetic, revisit if
  it matters in the viewer (M4) where box drawing matters for TUI content.
- Not yet human-verified: drag-resize + fractional scaling on the user's
  display (scale event path is exercised and logged at 1.00x).
- Partial-damage path (`Damage::mark`, `present_with_damage` with sub-rects)
  is plumbed but unused until M3 panels — dead-code-allowed with intent.

### Stable commit hash

(repo not yet committed — awaiting user)

## Addendum 2026-11-21 — test suite added (AGENTS.md rule 5 compliance)

The M1 close-out relied on manual verification (smoke run, screenshot, RSS
measurement) with zero `cargo test` files — a violation of rule 5 flagged in
review. Fixed:

- `src/render.rs` tests (7): Damage accumulator lifecycle (starts full, take
  clears, marked rects returned, cap degrades to full-canvas, no-op while
  full), `blit_to_buffer` packs `0x00RRGGBB`, `Canvas::fill_rect` HiDPI
  scaling, resize.
- `src/text.rs` tests (6): measured mono advance = 0.6 × size (8.4px @ 14px,
  12.0px @ 20px) for bundled JetBrains Mono, metrics cache consistency,
  ascent+descent ≈ line_height, glyph raster paints over background, empty
  line is a no-op, blending math (50% black over white → mid-grey 127,
  100% coverage replaces channels).
- `tests/cli.rs` (5): `--version`, `--help` documents PATH, valid directory
  passes validation AND completes the full open-window → raster → present
  pipeline via the new `ARK_SMOKE=1` hook, file path rejected
  ("is not a directory"), nonexistent path rejected ("cannot open project path").

### Key discovery: hanging GUI test

The first integration test spawned the real binary with a valid directory on
a display-capable machine — ark opened a real window and blocked forever
waiting for a human. Fixed with the **`ARK_SMOKE=1` environment hook**: ark
exits cleanly after the first successful present (`App::render` sets
`pending_exit`, `about_to_wait` exits). This makes the full pipeline testable
headlessly and CI-safe; it doubles as a scripted smoke test for future
milestones. Lesson: every GUI milestone needs a headless exit hook in its
verification path.

### Results

- `cargo test`: **18 passed, 0 failed** (13 unit in 0.16s, 5 integration in 0.39s)
- `cargo clippy --release --tests -- -D warnings`: clean
