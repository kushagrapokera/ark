use crate::render::ColorRgb;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::FileKind;
use super::FilePalette;

mod lettering;

/// Bold logical stroke used for badge letters and pictograms.
pub(super) const SYMBOL_STROKE_WIDTH: f32 = 2.0;

/// Draw one high-contrast Explorer badge directly into the framebuffer.
/// Coordinates and size are logical; the effective scale is applied once.
#[allow(clippy::too_many_arguments)]
pub fn draw_with_palette(
    pixmap: &mut Pixmap,
    kind: FileKind,
    cx: f32,
    cy: f32,
    size: f32,
    palette: FilePalette,
    scale: f64,
    expanded: bool,
) {
    if !(size > 0.0 && scale.is_finite() && scale > 0.0) {
        return;
    }
    let transform = Transform::from_scale(scale as f32, scale as f32);
    if kind == FileKind::Folder {
        draw_folder(pixmap, cx, cy, size, palette.main, expanded, transform);
        return;
    }
    draw_badge(pixmap, kind, cx, cy, size, palette.main, transform);
    draw_mark(pixmap, kind, cx, cy, size, palette.glyph, transform);
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

fn draw_badge(
    pixmap: &mut Pixmap,
    kind: FileKind,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut path = PathBuilder::new();
    if matches!(kind, FileKind::Html | FileKind::Css) {
        path.move_to(cx, cy - 0.42 * size);
        path.line_to(cx + 0.37 * size, cy - 0.27 * size);
        path.line_to(cx + 0.31 * size, cy + 0.17 * size);
        path.quad_to(cx, cy + 0.44 * size, cx - 0.31 * size, cy + 0.17 * size);
        path.line_to(cx - 0.37 * size, cy - 0.27 * size);
    } else {
        let left = cx - 0.42 * size;
        let right = cx + 0.42 * size;
        let top = cy - 0.42 * size;
        let bottom = cy + 0.42 * size;
        let radius = 0.12 * size;
        path.move_to(left + radius, top);
        path.line_to(right - radius, top);
        path.quad_to(right, top, right, top + radius);
        path.line_to(right, bottom - radius);
        path.quad_to(right, bottom, right - radius, bottom);
        path.line_to(left + radius, bottom);
        path.quad_to(left, bottom, left, bottom - radius);
        path.line_to(left, top + radius);
        path.quad_to(left, top, left + radius, top);
    }
    path.close();
    fill_path(pixmap, path, color, transform);
}

fn draw_mark(
    pixmap: &mut Pixmap,
    kind: FileKind,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let label = match kind {
        FileKind::Rust => Some("R"),
        FileKind::Python => Some("PY"),
        FileKind::Html => Some("<>"),
        FileKind::Css => Some("#"),
        FileKind::JavaScript => Some("JS"),
        FileKind::TypeScript => Some("TS"),
        FileKind::Json => Some("{}"),
        FileKind::Markdown => Some("MD"),
        FileKind::Env => Some(">_"),
        FileKind::Toml => Some("T"),
        FileKind::Yaml => Some("Y"),
        FileKind::Text => Some("TXT"),
        FileKind::Csv => Some("CSV"),
        FileKind::Image => {
            draw_image(pixmap, cx, cy, size, color, transform);
            return;
        }
        FileKind::Archive => {
            draw_archive(pixmap, cx, cy, size, color, transform);
            return;
        }
        FileKind::Generic => {
            draw_generic(pixmap, cx, cy, size, color, transform);
            return;
        }
        FileKind::Folder => return,
    };
    if let Some(label) = label {
        lettering::draw_label(pixmap, label, cx, cy, size, color, transform);
    }
    if kind == FileKind::Json {
        let mut dot = PathBuilder::new();
        dot.push_circle(cx, cy, 0.045 * size);
        fill_path(pixmap, dot, color, transform);
    }
}

fn draw_generic(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut path = PathBuilder::new();
    for offset in [-0.18, 0.0, 0.18] {
        path.move_to(cx - 0.23 * size, cy + offset * size);
        path.line_to(cx + 0.23 * size, cy + offset * size);
    }
    stroke_path(pixmap, path, color, SYMBOL_STROKE_WIDTH, transform);
}

fn draw_image(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut sun = PathBuilder::new();
    sun.push_circle(cx + 0.17 * size, cy - 0.17 * size, 0.065 * size);
    fill_path(pixmap, sun, color, transform);

    let mut mountain = PathBuilder::new();
    mountain.move_to(cx - 0.27 * size, cy + 0.22 * size);
    mountain.line_to(cx - 0.07 * size, cy - 0.03 * size);
    mountain.line_to(cx + 0.04 * size, cy + 0.12 * size);
    mountain.line_to(cx + 0.15 * size, cy - 0.01 * size);
    mountain.line_to(cx + 0.28 * size, cy + 0.22 * size);
    mountain.close();
    stroke_path(pixmap, mountain, color, SYMBOL_STROKE_WIDTH, transform);
}

fn draw_archive(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    transform: Transform,
) {
    let mut path = PathBuilder::new();
    path.move_to(cx - 0.25 * size, cy - 0.12 * size);
    path.line_to(cx + 0.25 * size, cy - 0.12 * size);
    path.line_to(cx + 0.25 * size, cy + 0.27 * size);
    path.line_to(cx - 0.25 * size, cy + 0.27 * size);
    path.close();
    path.move_to(cx - 0.25 * size, cy - 0.12 * size);
    path.line_to(cx, cy - 0.02 * size);
    path.line_to(cx + 0.25 * size, cy - 0.12 * size);
    path.move_to(cx, cy - 0.02 * size);
    path.line_to(cx, cy + 0.27 * size);
    path.move_to(cx - 0.08 * size, cy - 0.02 * size);
    path.line_to(cx + 0.08 * size, cy - 0.02 * size);
    stroke_path(pixmap, path, color, SYMBOL_STROKE_WIDTH, transform);
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
