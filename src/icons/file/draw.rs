use crate::render::ColorRgb;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::super::ICON_STROKE_WIDTH;
use super::FileKind;

/// Minimum stroke for symbols drawn inside the larger file outline.
pub(super) const SYMBOL_STROKE_WIDTH: f32 = 2.0;

/// Draw one glyph directly into the framebuffer. `cx`, `cy`, and `size` are
/// logical pixels; `scale` is applied exactly once by the transform. Paths are
/// short-lived draw scratch: nothing is cached between frames or retained per
/// tree row.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pixmap: &mut Pixmap,
    kind: FileKind,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    scale: f64,
    expanded: bool,
) {
    if !(size > 0.0 && scale > 0.0) {
        return;
    }
    let transform = Transform::from_scale(scale as f32, scale as f32);
    match kind {
        FileKind::Folder => draw_folder(pixmap, cx, cy, size, color, expanded, transform),
        _ => {
            if matches!(kind, FileKind::Html | FileKind::Css) {
                draw_shield(pixmap, cx, cy, size, color, transform);
            } else if matches!(kind, FileKind::Rust) {
                draw_rust(pixmap, cx, cy, size, color, transform);
            } else if matches!(kind, FileKind::JavaScript | FileKind::TypeScript) {
                draw_script_badge(pixmap, cx, cy, size, color, transform, kind);
            } else {
                stroke_path(
                    pixmap,
                    document_path(cx, cy, size),
                    color,
                    ICON_STROKE_WIDTH,
                    transform,
                );
            }
            draw_symbol(pixmap, kind, cx, cy, size, color, transform);
        }
    }
}

fn draw_folder(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    expanded: bool,
    transform: Transform,
) {
    let left = cx - 0.38 * size;
    let right = cx + 0.38 * size;
    let top = cy - 0.30 * size;
    let bottom = cy + 0.32 * size;
    let mut path = PathBuilder::new();
    if expanded {
        path.move_to(left, top);
        path.line_to(left + 0.44 * size, top);
        path.line_to(left + 0.54 * size, top + 0.16 * size);
        path.line_to(right, top + 0.16 * size);
        path.line_to(right, top + 0.30 * size);
        path.line_to(left + 0.42 * size, top + 0.30 * size);
        path.close();
        path.move_to(left, top + 0.28 * size);
        path.line_to(right, top + 0.28 * size);
        path.line_to(right - 0.08 * size, bottom);
        path.line_to(left + 0.08 * size, bottom);
        path.close();
    } else {
        path.move_to(left, bottom);
        path.line_to(left, top);
        path.line_to(left + 0.34 * size, top);
        path.line_to(left + 0.48 * size, top + 0.16 * size);
        path.line_to(right, top + 0.16 * size);
        path.line_to(right, bottom);
        path.close();
    }
    fill_path(pixmap, path, color, transform);
}

fn draw_shield(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut path = PathBuilder::new();
    path.move_to(cx, cy - 0.40 * size);
    path.line_to(cx + 0.34 * size, cy - 0.26 * size);
    path.line_to(cx + 0.29 * size, cy + 0.16 * size);
    path.quad_to(cx, cy + 0.42 * size, cx - 0.29 * size, cy + 0.16 * size);
    path.line_to(cx - 0.34 * size, cy - 0.26 * size);
    path.close();
    stroke_path(pixmap, path, color, ICON_STROKE_WIDTH, transform);
}

fn draw_rust(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut gear = PathBuilder::new();
    gear.push_circle(cx, cy, 0.24 * size);
    stroke_path(pixmap, gear, color, ICON_STROKE_WIDTH, transform);
    let mut teeth = PathBuilder::new();
    for index in 0..8 {
        let angle = std::f32::consts::TAU * index as f32 / 8.0;
        let sin = angle.sin();
        let cos = angle.cos();
        teeth.move_to(cx + cos * 0.29 * size, cy + sin * 0.29 * size);
        teeth.line_to(cx + cos * 0.39 * size, cy + sin * 0.39 * size);
    }
    stroke_path(pixmap, teeth, color, SYMBOL_STROKE_WIDTH, transform);
}

fn draw_script_badge(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
    kind: FileKind,
) {
    let mut badge = PathBuilder::new();
    let left = cx - 0.34 * size;
    let right = cx + 0.34 * size;
    let top = cy - 0.34 * size;
    let bottom = cy + 0.34 * size;
    badge.move_to(left + 0.08 * size, top);
    badge.line_to(right - 0.08 * size, top);
    badge.quad_to(right, top, right, top + 0.08 * size);
    badge.line_to(right, bottom - 0.08 * size);
    badge.quad_to(right, bottom, right - 0.08 * size, bottom);
    badge.line_to(left + 0.08 * size, bottom);
    badge.quad_to(left, bottom, left, bottom - 0.08 * size);
    badge.line_to(left, top + 0.08 * size);
    badge.quad_to(left, top, left + 0.08 * size, top);
    badge.close();
    stroke_path(pixmap, badge, color, SYMBOL_STROKE_WIDTH, transform);
    let mut letters = PathBuilder::new();
    if kind == FileKind::JavaScript {
        letters.move_to(cx - 0.15 * size, cy - 0.16 * size);
        letters.line_to(cx - 0.20 * size, cy + 0.18 * size);
        letters.move_to(cx + 0.04 * size, cy - 0.18 * size);
        letters.line_to(cx + 0.17 * size, cy - 0.12 * size);
        letters.line_to(cx + 0.17 * size, cy + 0.02 * size);
        letters.line_to(cx + 0.02 * size, cy + 0.02 * size);
        letters.line_to(cx + 0.18 * size, cy + 0.18 * size);
    } else {
        letters.move_to(cx - 0.20 * size, cy - 0.17 * size);
        letters.line_to(cx - 0.20 * size, cy + 0.18 * size);
        letters.move_to(cx - 0.20 * size, cy - 0.17 * size);
        letters.line_to(cx + 0.02 * size, cy - 0.17 * size);
        letters.line_to(cx + 0.18 * size, cy + 0.18 * size);
        letters.move_to(cx + 0.03 * size, cy - 0.02 * size);
        letters.line_to(cx + 0.15 * size, cy - 0.02 * size);
    }
    stroke_path(pixmap, letters, color, SYMBOL_STROKE_WIDTH, transform);
}

fn draw_symbol(
    pixmap: &mut Pixmap,
    kind: FileKind,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut path = PathBuilder::new();
    match kind {
        FileKind::Python => {
            path.push_circle(cx - 0.09 * size, cy - 0.05 * size, 0.12 * size);
            path.push_circle(cx + 0.09 * size, cy + 0.05 * size, 0.12 * size);
            stroke_path(pixmap, path, color, SYMBOL_STROKE_WIDTH, transform);
            return;
        }
        FileKind::Html => {
            path.move_to(cx - 0.14 * size, cy - 0.13 * size);
            path.line_to(cx + 0.13 * size, cy - 0.13 * size);
            path.move_to(cx - 0.15 * size, cy - 0.13 * size);
            path.line_to(cx - 0.04 * size, cy + 0.02 * size);
            path.line_to(cx - 0.15 * size, cy + 0.16 * size);
            path.move_to(cx + 0.15 * size, cy - 0.13 * size);
            path.line_to(cx + 0.04 * size, cy + 0.02 * size);
            path.line_to(cx + 0.15 * size, cy + 0.16 * size);
        }
        FileKind::Css => {
            for offset in [-0.12, 0.02, 0.16] {
                path.move_to(cx - 0.16 * size, cy + offset * size);
                path.line_to(cx + 0.16 * size, cy + offset * size);
            }
        }
        FileKind::Json => {
            path.move_to(cx - 0.12 * size, cy - 0.18 * size);
            path.cubic_to(
                cx - 0.26 * size,
                cy - 0.18 * size,
                cx - 0.22 * size,
                cy,
                cx - 0.12 * size,
                cy,
            );
            path.cubic_to(
                cx - 0.24 * size,
                cy,
                cx - 0.24 * size,
                cy + 0.20 * size,
                cx - 0.10 * size,
                cy + 0.20 * size,
            );
            path.move_to(cx + 0.12 * size, cy - 0.18 * size);
            path.cubic_to(
                cx + 0.26 * size,
                cy - 0.18 * size,
                cx + 0.22 * size,
                cy,
                cx + 0.12 * size,
                cy,
            );
            path.cubic_to(
                cx + 0.24 * size,
                cy,
                cx + 0.24 * size,
                cy + 0.20 * size,
                cx + 0.10 * size,
                cy + 0.20 * size,
            );
        }
        FileKind::Markdown => {
            path.move_to(cx - 0.17 * size, cy + 0.10 * size);
            path.line_to(cx - 0.17 * size, cy - 0.12 * size);
            path.line_to(cx - 0.06 * size, cy - 0.02 * size);
            path.line_to(cx + 0.05 * size, cy - 0.12 * size);
            path.line_to(cx + 0.05 * size, cy + 0.10 * size);
            path.move_to(cx + 0.17 * size, cy - 0.16 * size);
            path.line_to(cx + 0.17 * size, cy + 0.08 * size);
            path.line_to(cx + 0.10 * size, cy + 0.01 * size);
            path.line_to(cx + 0.17 * size, cy + 0.08 * size);
        }
        FileKind::Env => {
            path.move_to(cx - 0.20 * size, cy - 0.15 * size);
            path.line_to(cx - 0.10 * size, cy - 0.04 * size);
            path.line_to(cx - 0.20 * size, cy + 0.07 * size);
            path.move_to(cx - 0.02 * size, cy + 0.10 * size);
            path.line_to(cx + 0.18 * size, cy + 0.10 * size);
        }
        FileKind::Toml | FileKind::Yaml => {
            for (index, offset) in [-0.13, 0.02, 0.17].into_iter().enumerate() {
                path.move_to(cx - 0.18 * size, cy + offset * size);
                path.line_to(cx + 0.18 * size, cy + offset * size);
                let node = if index % 2 == 0 { -0.08 } else { 0.08 };
                path.push_circle(cx + node * size, cy + offset * size, 0.05 * size);
            }
        }
        FileKind::Text | FileKind::Generic => {
            for offset in [-0.12, 0.02, 0.16] {
                path.move_to(cx - 0.17 * size, cy + offset * size);
                path.line_to(cx + 0.17 * size, cy + offset * size);
            }
        }
        FileKind::Csv => {
            path.move_to(cx - 0.22 * size, cy - 0.20 * size);
            path.line_to(cx + 0.22 * size, cy - 0.20 * size);
            path.line_to(cx + 0.22 * size, cy + 0.20 * size);
            path.line_to(cx - 0.22 * size, cy + 0.20 * size);
            path.close();
            path.move_to(cx, cy - 0.20 * size);
            path.line_to(cx, cy + 0.20 * size);
            path.move_to(cx - 0.22 * size, cy);
            path.line_to(cx + 0.22 * size, cy);
        }
        FileKind::Image => {
            path.push_circle(cx + 0.13 * size, cy - 0.13 * size, 0.065 * size);
            path.move_to(cx - 0.23 * size, cy + 0.18 * size);
            path.line_to(cx - 0.06 * size, cy - 0.04 * size);
            path.line_to(cx + 0.04 * size, cy + 0.08 * size);
            path.line_to(cx + 0.13 * size, cy - 0.01 * size);
            path.line_to(cx + 0.24 * size, cy + 0.18 * size);
            path.close();
        }
        FileKind::Archive => {
            path.move_to(cx - 0.23 * size, cy - 0.08 * size);
            path.line_to(cx + 0.23 * size, cy - 0.08 * size);
            path.line_to(cx + 0.23 * size, cy + 0.22 * size);
            path.line_to(cx - 0.23 * size, cy + 0.22 * size);
            path.close();
            path.move_to(cx - 0.23 * size, cy - 0.08 * size);
            path.line_to(cx, cy + 0.04 * size);
            path.line_to(cx + 0.23 * size, cy - 0.08 * size);
            path.move_to(cx, cy + 0.04 * size);
            path.line_to(cx, cy + 0.22 * size);
        }
        FileKind::Folder | FileKind::Rust | FileKind::JavaScript | FileKind::TypeScript => return,
    }
    stroke_path(pixmap, path, color, SYMBOL_STROKE_WIDTH, transform);
}

fn document_path(cx: f32, cy: f32, size: f32) -> PathBuilder {
    let left = cx - 0.27 * size;
    let right = cx + 0.27 * size;
    let top = cy - 0.38 * size;
    let bottom = cy + 0.38 * size;
    let fold = 0.16 * size;
    let mut path = PathBuilder::new();
    path.move_to(left, top);
    path.line_to(right - fold, top);
    path.line_to(right, top + fold);
    path.line_to(right, bottom);
    path.line_to(left, bottom);
    path.close();
    path.move_to(right - fold, top);
    path.line_to(right - fold, top + fold);
    path.line_to(right, top + fold);
    path
}

fn stroke_path(
    pixmap: &mut Pixmap,
    path: PathBuilder,
    color: ColorRgb,
    width: f32,
    transform: Transform,
) {
    let path = path.finish().expect("valid procedural icon stroke");
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, 255);
    paint.anti_alias = true;
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, transform, None);
}

fn fill_path(pixmap: &mut Pixmap, path: PathBuilder, color: ColorRgb, transform: Transform) {
    let path = path.finish().expect("valid procedural icon fill");
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, 255);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
}
