# M1 — Render foundation

Master plan: `plan.md` (root), milestone M1.

## Tasks

1. Bundle JetBrains Mono Regular + Bold + OFL license (`assets/fonts/`)
2. Deps: `tiny-skia 0.11`, `cosmic-text 0.14.2` (default-features off — drops
   `fontconfig`, the system-font scanner; keeps `std` + `swash`), `fontdb 0.16`
3. `src/render.rs` — Theme tokens (Catppuccin Mocha-inspired), Damage
   accumulator (capped rect tracking, full short-circuit), persistent Canvas
   (premultiplied RGBA Pixmap), `blit_to_buffer` (0x00RRGGBB packing)
4. `src/text.rs` — TextSystem: FontSystem from bundled bytes only (family
   addressed by name "JetBrains Mono"; Family::Monospace would resolve to
   uninstalled "Noto Sans Mono"), SwashCache glyph raster cache, measured
   MonoMetrics per physical size (quarter-px cache key), draw_line with
   baseline-positioned swash mask/color glyph blitting (premultiplied src-over)
5. `src/icons.rs` — chevrons/file/folder/dot as tiny-skia paths in logical
   space, scaled stroke via Transform (crisp HiDPI)
6. `src/main.rs` — App gains canvas/scale/damage; ScaleFactorChanged →
   re-raster; Resized → full damage; event-driven redraw only (0% idle);
   M1 verification scene (header, metrics report, pangram + box drawing +
   unicode, icon row, theme swatches)

## Acceptance criteria

- Window renders text/icons/theme from bundled assets — visually verified via screenshot
- `cargo clippy --release -- -D warnings` passes
- No system font scanning (fontconfig feature disabled)
- Peak RSS re-measured and recorded
