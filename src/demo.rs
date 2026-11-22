//! The M2 widget-kit demo scene, kept as a test-only verification surface.
//!
//! The M3 frame replaced this scene in production, but the widgets it
//! exercises (button, checkbox, tab bar, text input) are pinned here for
//! test builds. Production consumers now include the M6 tab strip/buttons and
//! the M8 commit panel (checkbox + text input); the M9 config editor extends
//! the same kit. The demo keeps every widget and every `icons::Icon` variant
//! referenced, and its pins stay green (AGENTS.md: previous tests never
//! regress).

use cosmic_text::Weight;

use crate::icons;
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};
use crate::ui::{
    draw_button, draw_checkbox, Pointer, Press, Rect, Scrollbar, Split, TabBar, TAB_HEIGHT,
    TextInput, TextInputKey,
};

// ---- M2 demo scene constants (logical px) ----

const DEMO_MARGIN: f64 = 24.0;
const DEMO_HEADER_BASELINE: f64 = 40.0;
const DEMO_INFO_BASELINE: f64 = 64.0;
const DEMO_TABS_Y: f64 = 88.0;
const DEMO_SPLIT_TOP: f64 = 126.0;
const DEMO_PANE_TOP: f64 = 150.0;
/// Viewport height shared by the content box and the scrollbar track.
const DEMO_PANE_HEIGHT: f64 = 200.0;
const DEMO_LINE_STEP: f64 = 22.0;
/// Status line baseline sits this far above the window bottom.
const DEMO_STATUS_BASELINE_INSET: f64 = 28.0;
/// Split clamps: keep the 280px input + margins clear on the left, and at
/// least ~124px of content box on the right.
const DEMO_SPLIT_MIN: f64 = 312.0;
const DEMO_SPLIT_MAX_MARGIN: f64 = 220.0;
/// Total scrollable content height of the demo box (12 lines × 22px).
const DEMO_CONTENT_PX: f64 = DEMO_TREE.len() as f64 * DEMO_LINE_STEP;
/// Horizontal padding of content inside the box interior (logical px).
const DEMO_TEXT_PAD: f64 = 7.0;

/// One line of the demo content box: an icon + label pair previewing the M3
/// file tree (this is also what keeps every `icons::Icon` variant referenced
/// by test builds between M2 and later consumers).
struct DemoTreeLine {
    icon: icons::Icon,
    /// Indent level; each level shifts the line 14 logical px right.
    indent: u8,
    label: &'static str,
}

const DEMO_TREE: [DemoTreeLine; 12] = [
    DemoTreeLine { icon: icons::Icon::ChevronDown, indent: 0, label: "src" },
    DemoTreeLine { icon: icons::Icon::Folder, indent: 1, label: "ui" },
    DemoTreeLine { icon: icons::Icon::File, indent: 1, label: "main.rs" },
    DemoTreeLine { icon: icons::Icon::File, indent: 1, label: "render.rs" },
    DemoTreeLine { icon: icons::Icon::File, indent: 1, label: "ui.rs" },
    DemoTreeLine { icon: icons::Icon::ChevronRight, indent: 0, label: "tests" },
    DemoTreeLine { icon: icons::Icon::ChevronDown, indent: 0, label: "assets" },
    DemoTreeLine { icon: icons::Icon::Folder, indent: 1, label: "fonts" },
    DemoTreeLine { icon: icons::Icon::File, indent: 1, label: "JetBrainsMono-Regular.ttf" },
    DemoTreeLine { icon: icons::Icon::File, indent: 0, label: "Cargo.toml" },
    DemoTreeLine { icon: icons::Icon::File, indent: 0, label: "plan.md" },
    DemoTreeLine { icon: icons::Icon::Dot, indent: 0, label: "M2 widget demo — 12 lines" },
];

/// All interactive state behind the M2 demo scene. Fixed-size: every field
/// is a bounded value struct; nothing grows with interaction history.
struct DemoState {
    clicks: u32,
    checked: bool,
    tabs: TabBar,
    split: Split,
    /// Whether a divider drag is in progress (accent overlay + drag latch).
    split_dragging: bool,
    /// Grab offset (pointer.x − divider x) captured when the drag starts, so
    /// the divider doesn't jump to the pointer inside the ±4px hit band.
    split_grab_dx: f64,
    input: TextInput,
    scroll: Scrollbar,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            clicks: 0,
            checked: false,
            tabs: TabBar {
                tabs: ["main.rs", "viewer", "commit panel"].iter().map(|s| s.to_string()).collect(),
                active: 0,
            },
            split: Split { position: 380.0 },
            split_dragging: false,
            split_grab_dx: 0.0,
            input: TextInput { text: "edit me".into(), cursor: 7, focused: false },
            scroll: Scrollbar::default(),
        }
    }
}

/// Geometry of the M2 demo scene in LOGICAL px, derived from the window size
/// and the current split position. Pure — tests pin it (spec §M2 demo scene).
fn demo_layout(w: f64, h: f64, split_x: f64) -> DemoLayout {
    DemoLayout {
        tabs: Rect {
            x: DEMO_MARGIN,
            y: DEMO_TABS_Y,
            w: (w - 2.0 * DEMO_MARGIN).max(0.0),
            h: TAB_HEIGHT,
        },
        split_area: Rect {
            x: 0.0,
            y: DEMO_SPLIT_TOP,
            w: w.max(0.0),
            h: (h - 48.0 - DEMO_SPLIT_TOP).max(0.0),
        },
        button: Rect { x: DEMO_MARGIN, y: DEMO_PANE_TOP, w: 140.0, h: 34.0 },
        checkbox: Rect { x: DEMO_MARGIN, y: 210.0, w: 22.0, h: 22.0 },
        input: Rect { x: DEMO_MARGIN, y: 250.0, w: 280.0, h: 34.0 },
        content_box: Rect {
            x: split_x + DEMO_MARGIN,
            y: DEMO_PANE_TOP,
            w: (w - split_x - 96.0).max(0.0),
            h: DEMO_PANE_HEIGHT,
        },
        scroll_track: Rect { x: w - 72.0, y: DEMO_PANE_TOP, w: 8.0, h: DEMO_PANE_HEIGHT },
    }
}

struct DemoLayout {
    tabs: Rect,
    split_area: Rect,
    button: Rect,
    checkbox: Rect,
    input: Rect,
    content_box: Rect,
    scroll_track: Rect,
}

/// The live status line: `clicks N · checked <bool> · tab "<active>" ·
/// input "<text>" · scroll <pct>% · split <pos>px`. `pct` is the scrollbar
/// offset as a rounded percentage of the overflow range (0 with no
/// overflow); the split position rounds to whole logical px.
fn demo_status(demo: &DemoState) -> String {
    let clicks = demo.clicks;
    let checked = demo.checked;
    let tab = demo.tabs.tabs.get(demo.tabs.active).map(String::as_str).unwrap_or("");
    let input_text = &demo.input.text;
    let max = DEMO_CONTENT_PX - DEMO_PANE_HEIGHT;
    let pct = if max > 0.0 {
        (demo.scroll.offset / max * 100.0).round().clamp(0.0, 100.0)
    } else {
        0.0
    };
    let split = demo.split.position;
    format!(
        "clicks {clicks} · checked {checked} · tab \"{tab}\" · input \"{input_text}\" · scroll {pct:.0}% · split {split:.0}px"
    )
}

/// Map a winit key event's logical key + text into the [`TextInputKey`]s it
/// produces. Named navigation keys are checked FIRST: winit attaches text to
/// some of them (Backspace → "\x08"), so a text-first check would swallow
/// them as bogus chars. Control characters carried by text (Enter "\r",
/// Tab "\t", Escape "\x1b") are filtered — they are never editable text.
/// An empty result means "this event doesn't edit text".
fn keys_from_logical_and_text(
    logical_key: &winit::keyboard::Key,
    text: Option<&str>,
) -> Vec<TextInputKey> {
    use winit::keyboard::{Key, NamedKey};
    if let Key::Named(named) = logical_key {
        match named {
            NamedKey::Backspace => return vec![TextInputKey::Backspace],
            NamedKey::ArrowLeft => return vec![TextInputKey::Left],
            NamedKey::ArrowRight => return vec![TextInputKey::Right],
            NamedKey::Home => return vec![TextInputKey::Home],
            NamedKey::End => return vec![TextInputKey::End],
            _ => {}
        }
    }
    match text {
        Some(s) => s.chars().filter(|c| !c.is_control()).map(TextInputKey::Char).collect(),
        None => Vec::new(),
    }
}

/// Bordered content box with the fixed 12-line demo tree, shifted by the
/// scrollbar offset. Text/glyph blitting has no clip support, so the whole
/// 264-logical-px content is rastered into `scratch` and only the visible
/// slice is blitted into the box interior — no ink can escape the box.
fn draw_content_box(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    box_r: Rect,
    offset: f64,
    scratch: &mut Canvas,
) {
    if scale <= 0.0 || box_r.w <= 2.0 || box_r.h <= 2.0 || !scale.is_finite() {
        return; // degenerate box or scale: nothing to raster
    }
    // Physical interior rect (inside the 1px border strips).
    let ix0 = ((box_r.x + 1.0) * scale).round() as i32;
    let iy0 = ((box_r.y + 1.0) * scale).round() as i32;
    let ix1 = ((box_r.x + box_r.w - 1.0) * scale).round() as i32;
    let iy1 = ((box_r.y + box_r.h - 1.0) * scale).round() as i32;
    let (iw, ih) = ((ix1 - ix0).max(0) as u32, (iy1 - iy0).max(0) as u32);
    if iw == 0 || ih == 0 {
        return;
    }
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    if ix0 < 0 || iy0 < 0 || ix0 + iw as i32 > cw || iy0 + ih as i32 > ch {
        return; // interior outside the canvas — nothing to blit
    }

    // Raster the full content at natural size into the scratch canvas
    // (reused across frames; resized when the box width or scale changes).
    let content_h = ((DEMO_CONTENT_PX * scale).round() as u32).max(1);
    scratch.resize(iw, content_h);
    scratch.fill(theme.bg);
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let m = text.mono_metrics(font_size, (font_size as f64 * 1.35) as f32);
    // Scratch-space coordinates are content-relative (origin = interior top
    // left); each line is a 14px icon cell + 4px gap + fg label.
    for (i, line) in DEMO_TREE.iter().enumerate() {
        let row_cy = i as f64 * DEMO_LINE_STEP + DEMO_LINE_STEP / 2.0;
        let indent = line.indent as f64 * 14.0;
        let icon_cx = DEMO_TEXT_PAD + indent + 7.0;
        icons::draw(
            scratch.pixmap_mut(),
            line.icon,
            icon_cx as f32,
            row_cy as f32,
            14.0,
            theme.fg,
            scale,
        );
        let label_x = DEMO_TEXT_PAD + indent + 18.0;
        // draw_line takes PHYSICAL px: scale the logical row position; the
        // metrics term is already physical and must NOT be scaled again
        // (scaling it sank the text (metrics/2)×(scale−1) px at scale > 1).
        let baseline = row_cy * scale + f64::from(m.ascent - m.descent) / 2.0;
        text.draw_line(
            scratch,
            line.label,
            (label_x * scale) as f32,
            baseline as f32,
            TextStyle { color: theme.fg, font_size, weight: Weight::NORMAL },
        );
    }

    // Blit the visible slice [offset, offset + viewport) into the interior.
    let sy = ((offset * scale).round() as u32).min(content_h.saturating_sub(ih));
    let canvas_w = canvas.width() as usize;
    let row_bytes = iw as usize * 4;
    let dst = canvas.pixmap_mut().data_mut();
    let src = scratch.pixmap().data();
    for row in 0..ih as usize {
        let s = (sy as usize + row) * row_bytes;
        let d = ((iy0 as usize + row) * canvas_w + ix0 as usize) * 4;
        dst[d..d + row_bytes].copy_from_slice(&src[s..s + row_bytes]);
    }

    // Border outline on top, exact pixels.
    canvas.stroke_rect_strips(box_r.x, box_r.y, box_r.w, box_r.h, theme.border, scale);
}

/// M2 demo scene: exercises every widget with a live status line (spec
/// §M2 demo scene). `pointer` is this frame's logical pointer with the
/// click flag armed by the event router; widget state transitions happen
/// here, right before the widgets draw.
#[allow(clippy::too_many_arguments)] // moved as-is from the M2 scene layer
fn draw_scene(
    canvas: &mut Canvas,
    text: &mut TextSystem,
    theme: &Theme,
    scale: f64,
    demo: &mut DemoState,
    pointer: Pointer,
    scratch: &mut Canvas,
) {
    canvas.fill(theme.bg);
    let lw = canvas.width() as f64 / scale;
    let lh = canvas.height() as f64 / scale;
    let layout = demo_layout(lw, lh, demo.split.position);

    // Header + info line (kept from M1, repositioned for the widget layout).
    let header_size = 16.0 * scale as f32;
    let info_size = (UI_FONT_SIZE as f64 * scale) as f32;
    text.draw_line(
        canvas,
        "ark — widget kit (M2)",
        (DEMO_MARGIN * scale) as f32,
        (DEMO_HEADER_BASELINE * scale) as f32,
        TextStyle { color: theme.accent, font_size: header_size, weight: Weight::BOLD },
    );
    let mm = text.mono_metrics(info_size, (info_size as f64 * 1.35) as f32);
    let (cw, ch) = (canvas.width(), canvas.height());
    let adv = mm.advance;
    text.draw_line(
        canvas,
        &format!(
            "scale {scale:.2}x · canvas {cw}x{ch}px · logical {lw:.0}x{lh:.0} · mono {info_size:.0}px → advance {adv:.2}px"
        ),
        (DEMO_MARGIN * scale) as f32,
        (DEMO_INFO_BASELINE * scale) as f32,
        TextStyle { color: theme.fg_dim, font_size: info_size, weight: Weight::NORMAL },
    );

    // TabBar: process the frame's click, then draw.
    demo.tabs.on_pointer(layout.tabs, pointer);
    demo.tabs.draw(canvas, layout.tabs, theme, text, scale);

    // Split: grab within ±4px while the pointer is down, drag until release
    // (sticky latch so the divider can be pulled beyond the grab band). The
    // grab offset captured at press keeps the divider under the pointer.
    if pointer.down && (demo.split_dragging || demo.split.hit_divider(layout.split_area, pointer, scale)) {
        if !demo.split_dragging {
            demo.split_grab_dx = pointer.x - demo.split.position;
        }
        demo.split_dragging = true;
        let max = (lw - DEMO_SPLIT_MAX_MARGIN).max(DEMO_SPLIT_MIN);
        demo.split.drag_to(pointer.x - demo.split_grab_dx, DEMO_SPLIT_MIN, max);
    } else if !pointer.down {
        demo.split_dragging = false;
    }
    demo.split.draw(canvas, layout.split_area, theme, scale);
    if demo.split_dragging {
        // Accent overlay while dragging (draw painted the resting strip).
        canvas.fill_rect(
            demo.split.position - 1.0,
            layout.split_area.y,
            2.0,
            layout.split_area.h,
            theme.accent,
            scale,
        );
    }

    // Focus: a completed click inside the input focuses it, a completed
    // click anywhere else unfocuses it. Only the focused input consumes
    // keys (the event router checks `focused` before feeding keys).
    if pointer.click {
        demo.input.focused = layout.input.contains(pointer.x, pointer.y);
    }

    // Left pane widgets (spec positions).
    if draw_button(canvas, "Click me", layout.button, theme, text, pointer, scale) == Press::Clicked {
        demo.clicks += 1;
    }
    demo.checked = draw_checkbox(canvas, layout.checkbox, demo.checked, theme, pointer, scale);
    // Checkbox label at x=54, vertically centered against the 22px box.
    let label_baseline = 221.0 * scale + f64::from(mm.ascent - mm.descent) / 2.0;
    text.draw_line(
        canvas,
        "allow agent edits",
        (54.0 * scale) as f32,
        label_baseline as f32,
        TextStyle { color: theme.fg, font_size: info_size, weight: Weight::NORMAL },
    );
    demo.input.draw(canvas, layout.input, theme, text, scale);

    // Right pane: bordered content box with the demo tree (clipped) and the
    // scrollbar next to it (pointer drag + draw share the frame's pointer).
    draw_content_box(canvas, text, theme, scale, layout.content_box, demo.scroll.offset, scratch);
    demo.scroll.on_pointer(layout.scroll_track, pointer, DEMO_CONTENT_PX, DEMO_PANE_HEIGHT);
    demo.scroll.draw(canvas, layout.scroll_track, DEMO_CONTENT_PX, DEMO_PANE_HEIGHT, theme, scale);

    // Live status line (mono 12px, dim).
    let status_size = (12.0 * scale) as f32;
    text.draw_line(
        canvas,
        &demo_status(demo),
        (DEMO_MARGIN * scale) as f32,
        ((lh - DEMO_STATUS_BASELINE_INSET) * scale) as f32,
        TextStyle { color: theme.fg_dim, font_size: status_size, weight: Weight::NORMAL },
    );
}

// The demo's own test pins moved here together with the scene code (the
// M2 main.rs tests that reference it) — impl + tests would exceed the
// ~400-line guidance in one file.
#[cfg(test)]
mod tests;
