# M2 — Mini widget kit

Master plan: `plan.md` (root), milestone M2.

Scope: the retained-state, event-driven widget kit that M3 (file tree), M4
(viewer), M8 (commit panel) and M9 (config editors) stand on.

## Constraints (from AGENTS.md — binding)

- **ZERO new dependencies.** Everything drawn with existing tiny-skia + our
  text system. No bitmap fonts, no widget crates.
- Fixed-size state only — no structure may grow with interaction history.
- IME/complex text editing is deferred to M9 (noted, not forgotten).

## Frozen interfaces (implementer may not rename or reshape these)

```rust
// src/ui.rs — all coordinates are LOGICAL px; scaling happens at raster time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
impl Rect { contains(x,y)->bool; scaled(scale)->Rect }

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pointer { pub x: f64, pub y: f64, pub down: bool, pub click: bool }
// click == true ONLY on the frame processing a completed press→release here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press { Idle, Hover, Clicked }
pub fn draw_button(canvas, label: &str, rect: Rect, theme, text: &mut TextSystem,
                   pointer: Pointer, scale: f64) -> Press

pub fn draw_checkbox(canvas, rect: Rect, checked: bool, theme, pointer,
                     scale: f64) -> bool  // returns the NEW checked value

pub struct Scrollbar { pub offset: f64, /* private drag: Option<f64> */ }
pub fn thumb_geometry(track_h: f64, content: f64, viewport: f64, offset: f64)
                     -> (thumb_y: f64, thumb_h: f64)
impl Scrollbar { clamp(content, viewport); scroll_by(dy, content, viewport)->bool;
                 on_pointer(track: Rect, pointer, content, viewport)->bool;
                 draw(canvas, track: Rect, content, viewport, theme, scale) }

pub struct TabBar { pub tabs: Vec<String>, pub active: usize }
// TAB_WIDTH = 160.0, TAB_HEIGHT = 30.0 (logical)
impl TabBar { on_pointer(bar: Rect, pointer)->bool /* active changed */;
              draw(canvas, bar: Rect, theme, text: &mut TextSystem, scale) }

pub struct Split { pub position: f64 }  // logical px, clamped [min,max] by drag_to
impl Split { hit_divider(area: Rect, pointer, scale)->bool;  // ±4 logical px of divider
             drag_to(x: f64, min: f64, max: f64);
             draw(canvas, area: Rect, theme, scale) }  // divider strip only

pub enum TextInputKey { Char(char), Backspace, Left, Right, Home, End }
pub struct TextInput { pub text: String, pub cursor: usize /*byte idx*/, pub focused: bool }
impl TextInput { insert(&str)->bool; backspace()->bool; handle_key(TextInputKey)->bool;
                 draw(canvas, rect, theme, text: &mut TextSystem, scale) }
```

## Tasks (fresh implementer per task, handoff-driven)

- **Task 1 — UI foundation:** `src/ui.rs` with Rect, Pointer, draw_button,
  draw_checkbox. TDD: failing tests first (see handoff-1).
- **Task 2 — Interactive widgets + demo scene:** Scrollbar (+thumb_geometry),
  TabBar, Split, TextInput; main.rs routes pointer/keyboard events, M2 demo
  scene with live status line; ARK_SMOKE screenshot verifies.

## Acceptance criteria (milestone)

- `cargo test` — new ui tests pass alongside the existing 18
- `cargo clippy --release --tests -- -D warnings` clean
- `cargo build --release` clean
- Interactive M2 scene: every widget responds to pointer/keyboard (status
  line reflects state), verified by screenshot
- ARK_SMOKE first-frame exit still works
- RSS re-measured and recorded (≤100MB budget intact)
