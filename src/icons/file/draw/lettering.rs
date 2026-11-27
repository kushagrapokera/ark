use tiny_skia::{PathBuilder, Pixmap, Transform};

use crate::render::ColorRgb;

/// Draw a short vector label centered in a badge. Labels are static ASCII;
/// no text shaping, font lookup, or per-label allocation is involved.
pub(super) fn draw_label(
    pixmap: &mut Pixmap,
    label: &str,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let count = label.chars().count();
    if count == 0 {
        return;
    }
    let width_factor = match count {
        1 => 0.34,
        2 => 0.27,
        _ => 0.22,
    };
    let cell_w = width_factor * size;
    let cell_h = 0.56 * size;
    let gap = 0.06 * size;
    let total = count as f32 * cell_w + (count - 1) as f32 * gap;
    let mut x = cx - total / 2.0;
    let y = cy - cell_h / 2.0;
    let mut path = PathBuilder::new();
    for character in label.chars() {
        letter_path(&mut path, character, x, y, cell_w, cell_h);
        x += cell_w + gap;
    }
    super::stroke_path(pixmap, path, color, super::SYMBOL_STROKE_WIDTH, transform);
}

fn letter_path(path: &mut PathBuilder, character: char, x: f32, y: f32, w: f32, h: f32) {
    match character {
        'R' | 'P' => {
            path.move_to(x, y + h);
            path.line_to(x, y);
            path.line_to(x + w, y);
            path.line_to(x + w, y + h * 0.45);
            path.line_to(x, y + h * 0.45);
            if character == 'R' {
                path.move_to(x + w * 0.42, y + h * 0.45);
                path.line_to(x + w, y + h);
            }
        }
        'Y' => {
            path.move_to(x, y);
            path.line_to(x + w / 2.0, y + h / 2.0);
            path.line_to(x + w, y);
            path.move_to(x + w / 2.0, y + h / 2.0);
            path.line_to(x + w / 2.0, y + h);
        }
        'J' => {
            path.move_to(x, y);
            path.line_to(x + w, y);
            path.line_to(x + w, y + h * 0.72);
            path.line_to(x + w / 2.0, y + h);
            path.line_to(x, y + h * 0.72);
        }
        'S' => {
            path.move_to(x + w, y + h * 0.12);
            path.cubic_to(x + w, y - h * 0.08, x, y - h * 0.08, x, y + h * 0.36);
            path.cubic_to(x, y + h * 0.55, x + w, y + h * 0.45, x + w, y + h * 0.68);
            path.cubic_to(x + w, y + h * 1.02, x, y + h * 1.02, x, y + h * 0.88);
        }
        'T' => {
            path.move_to(x, y);
            path.line_to(x + w, y);
            path.move_to(x + w / 2.0, y);
            path.line_to(x + w / 2.0, y + h);
        }
        'M' => {
            path.move_to(x, y + h);
            path.line_to(x, y);
            path.line_to(x + w / 2.0, y + h * 0.45);
            path.line_to(x + w, y);
            path.line_to(x + w, y + h);
        }
        'D' => {
            path.move_to(x, y + h);
            path.line_to(x, y);
            path.line_to(x + w * 0.55, y);
            path.quad_to(x + w, y, x + w, y + h / 2.0);
            path.quad_to(x + w, y + h, x + w * 0.55, y + h);
            path.line_to(x, y + h);
        }
        'C' => {
            path.move_to(x + w, y + h * 0.12);
            path.cubic_to(x + w * 0.55, y - h * 0.12, x - w * 0.1, y, x, y + h / 2.0);
            path.cubic_to(
                x - w * 0.1,
                y + h,
                x + w * 0.55,
                y + h * 1.12,
                x + w,
                y + h * 0.88,
            );
        }
        'V' => {
            path.move_to(x, y);
            path.line_to(x + w / 2.0, y + h);
            path.line_to(x + w, y);
        }
        'X' => {
            path.move_to(x, y);
            path.line_to(x + w, y + h);
            path.move_to(x + w, y);
            path.line_to(x, y + h);
        }
        '<' | '>' => {
            if character == '<' {
                path.move_to(x + w, y);
                path.line_to(x, y + h / 2.0);
                path.line_to(x + w, y + h);
            } else {
                path.move_to(x, y);
                path.line_to(x + w, y + h / 2.0);
                path.line_to(x, y + h);
            }
        }
        '{' | '}' => {
            if character == '{' {
                path.move_to(x + w, y);
                path.line_to(x + w * 0.35, y);
                path.cubic_to(x + w * 0.05, y, x, y + h * 0.1, x, y + h * 0.3);
                path.line_to(x, y + h * 0.4);
                path.cubic_to(
                    x,
                    y + h * 0.5,
                    x + w * 0.55,
                    y + h * 0.42,
                    x + w * 0.55,
                    y + h * 0.5,
                );
                path.cubic_to(x + w * 0.55, y + h * 0.58, x, y + h * 0.5, x, y + h * 0.6);
                path.line_to(x, y + h * 0.7);
                path.cubic_to(x, y + h * 0.9, x + w * 0.05, y + h, x + w * 0.35, y + h);
                path.line_to(x + w, y + h);
            } else {
                path.move_to(x, y);
                path.line_to(x + w * 0.65, y);
                path.cubic_to(x + w * 0.95, y, x + w, y + h * 0.1, x + w, y + h * 0.3);
                path.line_to(x + w, y + h * 0.4);
                path.cubic_to(
                    x + w,
                    y + h * 0.5,
                    x + w * 0.45,
                    y + h * 0.42,
                    x + w * 0.45,
                    y + h * 0.5,
                );
                path.cubic_to(
                    x + w * 0.45,
                    y + h * 0.58,
                    x + w,
                    y + h * 0.5,
                    x + w,
                    y + h * 0.6,
                );
                path.line_to(x + w, y + h * 0.7);
                path.cubic_to(x + w, y + h * 0.9, x + w * 0.95, y + h, x + w * 0.65, y + h);
                path.line_to(x, y + h);
            }
        }
        '#' => {
            for offset in [0.3, 0.7] {
                path.move_to(x + w * offset, y);
                path.line_to(x + w * offset, y + h);
                path.move_to(x, y + h * offset);
                path.line_to(x + w, y + h * offset);
            }
        }
        '_' => {
            path.move_to(x, y + h);
            path.line_to(x + w, y + h);
        }
        _ => {}
    }
}
