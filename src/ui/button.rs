//! Button widget: elevated fill + border chrome, `Press` outcome derived
//! purely from `pointer` + containment (stateless drawing, caller-owned
//! state — see the design contract in `super`).

use cosmic_text::Weight;

use super::{renderable, stroke_rect_inset, Pointer, Rect};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};

/// Interaction outcome of one widget frame (consumed by `draw_button`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Idle,
    Hover,
    Clicked,
}

/// Draw a button: `bg_elevated` fill, 1px border (`border` idle, `fg_dim`
/// hover) or a 2px `accent` border inside the rect when clicked; label
/// centered horizontally, baseline centered vertically. Returns the press
/// outcome derived purely from `pointer` + containment.
///
/// Degenerate rects (w or h ≤ 0) or scale ≤ 0 render nothing and return
/// `Press::Idle`, regardless of pointer state.
pub fn draw_button(
    canvas: &mut Canvas,
    label: &str,
    rect: Rect,
    theme: &Theme,
    text: &mut TextSystem,
    pointer: Pointer,
    scale: f64,
) -> Press {
    if !renderable(rect, scale) {
        return Press::Idle; // degenerate: render nothing
    }
    let inside = rect.contains(pointer.x, pointer.y);
    let press = match (inside, pointer.click) {
        (true, true) => Press::Clicked,
        (true, false) => Press::Hover,
        (false, _) => Press::Idle,
    };

    // Elevated surface over the background.
    canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, theme.bg_elevated, scale);

    // Chrome: 1 logical-px border idle/hover, 2px accent inside when clicked.
    let (border_color, border_w) = match press {
        Press::Idle => (theme.border, 1.0),
        Press::Hover => (theme.fg_dim, 1.0),
        Press::Clicked => (theme.accent, 2.0),
    };
    let phys = rect.scaled(scale);
    stroke_rect_inset(canvas.pixmap_mut(), phys, border_color, (border_w * scale) as f32);

    // Label: horizontally centered, baseline vertically centered, in the
    // monospace face at the scaled UI size.
    let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
    let metrics = text.mono_metrics(font_size, (font_size * 1.35).round());
    let label_w = metrics.advance * label.chars().count() as f32;
    let label_x = phys.x + (phys.w - f64::from(label_w)) / 2.0;
    let baseline = phys.y + phys.h / 2.0 + f64::from(metrics.ascent - metrics.descent) / 2.0;
    text.draw_line(
        canvas,
        label,
        label_x as f32,
        baseline as f32,
        TextStyle {
            color: theme.fg,
            font_size,
            weight: Weight::NORMAL,
        },
    );

    press
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{count_color_in, count_not_color};

    // ---- draw_button (acceptance 2, 3) ----

    #[test]
    fn button_press_states() {
        let theme = Theme { ..Theme::DARK };
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(200, 100);
        let rect = Rect { x: 10.0, y: 10.0, w: 80.0, h: 30.0 };

        // Completed press→release processed this frame, inside the button.
        let click_inside = Pointer { x: 50.0, y: 25.0, down: false, click: true };
        assert_eq!(
            draw_button(&mut canvas, "OK", rect, &theme, &mut text, click_inside, 1.0),
            Press::Clicked
        );
        // Pointer inside without a completed click (pressed or not).
        let hover = Pointer { x: 50.0, y: 25.0, down: true, click: false };
        assert_eq!(
            draw_button(&mut canvas, "OK", rect, &theme, &mut text, hover, 1.0),
            Press::Hover
        );
        // Pointer outside the rect, even on a click frame.
        let outside = Pointer { x: 150.0, y: 80.0, down: false, click: true };
        assert_eq!(
            draw_button(&mut canvas, "OK", rect, &theme, &mut text, outside, 1.0),
            Press::Idle
        );
    }

    #[test]
    fn button_paints_pixels() {
        let theme = Theme { ..Theme::DARK };
        let mut text = TextSystem::new();
        // 2x scale also proves the widget raster path consumes scaled rects.
        let mut canvas = Canvas::new(200, 100);
        let rect = Rect { x: 10.0, y: 10.0, w: 60.0, h: 24.0 };
        let outside = Pointer { x: 150.0, y: 80.0, down: false, click: false };
        let hover = Pointer { x: 40.0, y: 22.0, down: true, click: false };
        let click = Pointer { x: 40.0, y: 22.0, down: false, click: true };

        // Physical geometry at scale 2: rect → (20,20,120,48). Border strokes
        // rasterize inside the outer 4 physical px, so the interior sample
        // (inset 4) excludes stroke ink and its AA bleed, and a ring-color
        // count over the full rect region can only match the stroke itself
        // (fill and label colors differ from every ring color).
        let (rx0, ry0, rx1, ry1) = (20u32, 20, 140, 68);
        let (ix0, iy0, ix1, iy1) = (24u32, 24, 136, 64);

        // Hover: bg_elevated fill, fg_dim ring, fg label ink.
        canvas.fill(theme.bg);
        draw_button(&mut canvas, "OK", rect, &theme, &mut text, hover, 2.0);
        assert!(
            count_color_in(&canvas, theme.bg_elevated, ix0, iy0, ix1, iy1) >= 1000,
            "button fill must be bg_elevated in the interior"
        );
        let hover_ring = count_color_in(&canvas, theme.fg_dim, rx0, ry0, rx1, ry1);
        assert!(hover_ring > 0, "hover must paint an fg_dim border ring");
        // Label ink is fg-colored, counted over the whole rect region. The
        // pin is deliberately position-agnostic: it asserts the identity and
        // color of the label ink (fg-colored pixels must exist somewhere in
        // the button's rect region), not its placement — so a layout change
        // that keeps a fg-colored label still satisfies it, while a missing
        // or wrong-colored label fails it.
        assert!(
            count_color_in(&canvas, theme.fg, rx0, ry0, rx1, ry1) >= 20,
            "label ink must be fg-colored and intersect its button"
        );

        // Clicked: accent ring, wider than the 1px hover ring.
        canvas.fill(theme.bg);
        draw_button(&mut canvas, "OK", rect, &theme, &mut text, click, 2.0);
        let accent_ring = count_color_in(&canvas, theme.accent, rx0, ry0, rx1, ry1);
        assert!(accent_ring > 0, "clicked must paint an accent border ring");
        assert!(
            accent_ring > hover_ring,
            "clicked accent ring ({accent_ring}) must be wider than the 1px hover ring ({hover_ring})"
        );

        // Idle: border-colored ring; and the whole draw beats the ≥50px
        // change criterion from the acceptance list.
        canvas.fill(theme.bg);
        draw_button(&mut canvas, "OK", rect, &theme, &mut text, outside, 2.0);
        assert!(
            count_color_in(&canvas, theme.border, rx0, ry0, rx1, ry1) > 0,
            "idle must paint a border-colored ring"
        );
        assert!(
            count_not_color(&canvas, theme.bg) >= 50,
            "button raster must change ≥50 px vs the background"
        );
    }

    // ---- Degenerate inputs (robustness contract) ----

    #[test]
    fn button_degenerate_rect_or_scale_renders_nothing() {
        let theme = Theme { ..Theme::DARK };
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(80, 60);
        canvas.fill(theme.bg);
        let click = Pointer { x: 15.0, y: 15.0, down: false, click: true };
        let zero_w = Rect { x: 10.0, y: 10.0, w: 0.0, h: 12.0 };
        let negative_h = Rect { x: 10.0, y: 10.0, w: 30.0, h: -5.0 };
        let valid_rect = Rect { x: 10.0, y: 10.0, w: 30.0, h: 12.0 };

        // A zero-size rect contains no point at all.
        assert!(!zero_w.contains(11.0, 11.0), "interior-ish point");
        assert!(!zero_w.contains(10.0, 16.0), "left edge point");
        assert!(!zero_w.contains(9.0, 9.0), "outside point");

        // No panic, Idle, and nothing rasterizes for any degenerate input.
        assert_eq!(
            draw_button(&mut canvas, "OK", zero_w, &theme, &mut text, click, 1.0),
            Press::Idle,
            "zero-width rect renders nothing and reports Idle"
        );
        assert_eq!(
            draw_button(&mut canvas, "OK", negative_h, &theme, &mut text, click, 1.0),
            Press::Idle,
            "negative extent renders nothing and reports Idle"
        );
        assert_eq!(
            draw_button(&mut canvas, "OK", valid_rect, &theme, &mut text, click, 0.0),
            Press::Idle,
            "zero scale renders nothing and reports Idle"
        );
        assert_eq!(
            count_not_color(&canvas, theme.bg),
            0,
            "degenerate draws must rasterize nothing"
        );
    }
}
