//! Hand-drawn icons as tiny-skia paths. No SVG/icon library dependency —
//! each glyph is a handful of path commands in logical space, scaled at
//! raster time.
//!
//! Coordinate contract: callers pass LOGICAL `cx`/`cy` and a LOGICAL `size`;
//! `draw` applies exactly one `Transform::from_scale(scale)` to place and
//! size the glyph. The stroke width is a constant bold 2.25 in logical space —
//! the transform scales it exactly once. Callers must never pre-scale
//! coordinates or widths by `scale` (doing so double-scales position and
//! stroke thickness on HiDPI).

use crate::render::ColorRgb;
use crate::ui::{Pointer, Rect};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

mod file;
pub use file::{draw as draw_file, FileKind};

/// Bold logical stroke width used by every icon.
pub const ICON_STROKE_WIDTH: f32 = 2.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    #[allow(dead_code)] // frozen M2 demo/test surface; production folders use file icons
    ChevronRight,
    ChevronDown,
    File,
    #[allow(dead_code)] // frozen M2 test/demo surface; production icons arrive with the workbench
    Folder,
    #[allow(dead_code)] // frozen M2 test/demo surface; production icons arrive with the workbench
    Dot,
    SourceControl,
    Terminal,
    Code,
    Markdown,
    Search,
    Split,
    Plus,
    Close,
    Refresh,
    Update,
    Collapse,
    NewFile,
    NewFolder,
    DragHandle,
    ResizeGrip,
    Trash,
    PanelLeft,
    PanelBottom,
}

/// One hovered icon's accessible name and logical hit area. The frame keeps at
/// most one of these and paints it after the scene so later chrome cannot cover it.
#[derive(Clone, Copy, Debug)]
pub struct IconTooltip {
    pub label: &'static str,
    pub anchor: Rect,
}

impl IconTooltip {
    /// Return the tooltip when `pointer` is inside the icon's logical hit area.
    pub fn hovered(
        label: &'static str,
        pointer: Pointer,
        cx: f64,
        cy: f64,
        size: f64,
    ) -> Option<Self> {
        let hit = size.max(18.0);
        let anchor = Rect {
            x: cx - hit / 2.0,
            y: cy - hit / 2.0,
            w: hit,
            h: hit,
        };
        anchor
            .contains(pointer.x, pointer.y)
            .then_some(Self { label, anchor })
    }
}

impl Icon {
    /// Human-readable hover name. Names describe the action in ark's workbench,
    /// not just the glyph shape.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ChevronRight => "Expand Folder",
            Self::ChevronDown => "Collapse Folder",
            Self::File => "File",
            Self::Folder => "Folder",
            Self::Dot => "Status",
            Self::SourceControl => "Source Control",
            Self::Terminal => "Terminal",
            Self::Code => "ark",
            Self::Markdown => "Toggle Markdown Preview",
            Self::Search => "Search",
            Self::Split => "Split Editor",
            Self::Plus => "New Terminal",
            Self::Close => "Close",
            Self::Refresh => "Refresh Explorer",
            Self::Update => "Check for updates",
            Self::Collapse => "Collapse Folders",
            Self::NewFile => "New File",
            Self::NewFolder => "New Folder",
            Self::DragHandle => "Drag Handle",
            Self::ResizeGrip => "Resize Terminal",
            Self::Trash => "Close Terminal",
            Self::PanelLeft => "Toggle Explorer",
            Self::PanelBottom => "Toggle Terminal",
        }
    }

    pub fn hovered_tooltip(
        self,
        pointer: Pointer,
        cx: f64,
        cy: f64,
        size: f64,
    ) -> Option<IconTooltip> {
        IconTooltip::hovered(self.name(), pointer, cx, cy, size)
    }
}

/// Stroke-based outline icon in LOGICAL space, centered on (cx, cy) with a
/// nominal `size` (the icon box edge length). `scale` is applied exactly
/// once, by the transform below — see the module contract.
pub fn draw(
    pixmap: &mut Pixmap,
    icon: Icon,
    cx: f32,
    cy: f32,
    size: f32,
    color: ColorRgb,
    scale: f64,
) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, 255);
    paint.anti_alias = true;
    // Constant bold LOGICAL stroke width; the transform scales it once.
    let stroke = Stroke {
        width: ICON_STROKE_WIDTH,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Stroke::default()
    };
    let transform = Transform::from_scale(scale as f32, scale as f32);
    let s = size;

    let path = match icon {
        Icon::ChevronRight => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.25 * s, cy - 0.3 * s);
            pb.line_to(cx + 0.2 * s, cy);
            pb.line_to(cx - 0.25 * s, cy + 0.3 * s);
            pb.finish().expect("valid chevron path")
        }
        Icon::ChevronDown => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.3 * s, cy - 0.25 * s);
            pb.line_to(cx, cy + 0.2 * s);
            pb.line_to(cx + 0.3 * s, cy - 0.25 * s);
            pb.finish().expect("valid chevron path")
        }
        Icon::File => {
            // Sheet with a folded corner, outlined
            let x0 = cx - 0.3 * s;
            let x1 = cx + 0.3 * s;
            let y0 = cy - 0.4 * s;
            let y1 = cy + 0.4 * s;
            let f = 0.18 * s;
            let mut pb = PathBuilder::new();
            pb.move_to(x0, y0);
            pb.line_to(x1 - f, y0);
            pb.line_to(x1, y0 + f);
            pb.line_to(x1, y1);
            pb.line_to(x0, y1);
            pb.close();
            pb.finish().expect("valid file path")
        }
        Icon::Folder => {
            // Tab + body, filled silhouette
            let x0 = cx - 0.35 * s;
            let x1 = cx + 0.35 * s;
            let y0 = cy - 0.3 * s;
            let y1 = cy + 0.35 * s;
            let tab = 0.16 * s;
            let mut pb = PathBuilder::new();
            pb.move_to(x0, y1);
            pb.line_to(x0, y0 + tab);
            pb.line_to(x0 + 0.3 * s, y0 + tab);
            pb.line_to(x0 + 0.42 * s, y0);
            pb.line_to(x1, y0);
            pb.line_to(x1, y1);
            pb.close();
            pb.finish().expect("valid folder path")
        }
        Icon::Dot => {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, 0.16 * s);
            pb.finish().expect("valid dot path")
        }
        Icon::SourceControl => {
            // Git branch: stem + two node circles.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.16 * s, cy - 0.28 * s);
            pb.line_to(cx - 0.16 * s, cy + 0.28 * s);
            pb.push_circle(cx - 0.16 * s, cy - 0.30 * s, 0.10 * s);
            pb.push_circle(cx - 0.16 * s, cy + 0.30 * s, 0.10 * s);
            pb.push_circle(cx + 0.24 * s, cy, 0.10 * s);
            pb.move_to(cx - 0.16 * s, cy + 0.04 * s);
            pb.cubic_to(
                cx + 0.04 * s,
                cy + 0.04 * s,
                cx + 0.04 * s,
                cy,
                cx + 0.14 * s,
                cy,
            );
            pb.finish().expect("valid source control path")
        }
        Icon::Terminal => {
            // Screen + prompt chevron/underscore.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.34 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.34 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.34 * s, cy + 0.28 * s);
            pb.line_to(cx - 0.34 * s, cy + 0.28 * s);
            pb.close();
            pb.move_to(cx - 0.18 * s, cy - 0.08 * s);
            pb.line_to(cx - 0.04 * s, cy);
            pb.line_to(cx - 0.18 * s, cy + 0.08 * s);
            pb.move_to(cx + 0.04 * s, cy + 0.10 * s);
            pb.line_to(cx + 0.20 * s, cy + 0.10 * s);
            pb.finish().expect("valid terminal path")
        }
        Icon::Code => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.22 * s, cy - 0.30 * s);
            pb.line_to(cx - 0.42 * s, cy);
            pb.line_to(cx - 0.22 * s, cy + 0.30 * s);
            pb.move_to(cx + 0.22 * s, cy - 0.30 * s);
            pb.line_to(cx + 0.42 * s, cy);
            pb.line_to(cx + 0.22 * s, cy + 0.30 * s);
            pb.move_to(cx + 0.08 * s, cy - 0.38 * s);
            pb.line_to(cx - 0.08 * s, cy + 0.38 * s);
            pb.finish().expect("valid code path")
        }
        Icon::Markdown => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.28 * s, cy - 0.34 * s);
            pb.line_to(cx + 0.24 * s, cy - 0.34 * s);
            pb.line_to(cx + 0.32 * s, cy - 0.24 * s);
            pb.line_to(cx + 0.32 * s, cy + 0.34 * s);
            pb.line_to(cx - 0.28 * s, cy + 0.34 * s);
            pb.close();
            pb.move_to(cx - 0.12 * s, cy - 0.34 * s);
            pb.line_to(cx - 0.12 * s, cy + 0.34 * s);
            pb.finish().expect("valid markdown path")
        }
        Icon::Search => {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx - 0.08 * s, cy - 0.08 * s, 0.25 * s);
            pb.move_to(cx + 0.10 * s, cy + 0.10 * s);
            pb.line_to(cx + 0.36 * s, cy + 0.36 * s);
            pb.finish().expect("valid search path")
        }
        Icon::ResizeGrip => {
            let mut pb = PathBuilder::new();
            for dx in [-0.30, 0.0, 0.30] {
                pb.push_circle(cx + dx * s, cy, 0.09 * s);
            }
            pb.finish().expect("valid resize grip path")
        }
        Icon::Split => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.36 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.36 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.36 * s, cy + 0.28 * s);
            pb.line_to(cx - 0.36 * s, cy + 0.28 * s);
            pb.close();
            pb.move_to(cx, cy - 0.28 * s);
            pb.line_to(cx, cy + 0.28 * s);
            pb.finish().expect("valid split path")
        }
        Icon::Plus => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.30 * s, cy);
            pb.line_to(cx + 0.30 * s, cy);
            pb.move_to(cx, cy - 0.30 * s);
            pb.line_to(cx, cy + 0.30 * s);
            pb.finish().expect("valid plus path")
        }
        Icon::Close => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.28 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.28 * s, cy + 0.28 * s);
            pb.move_to(cx + 0.28 * s, cy - 0.28 * s);
            pb.line_to(cx - 0.28 * s, cy + 0.28 * s);
            pb.finish().expect("valid close path")
        }
        Icon::Refresh | Icon::Update => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx + 0.30 * s, cy - 0.24 * s);
            pb.cubic_to(
                cx + 0.10 * s,
                cy - 0.42 * s,
                cx - 0.28 * s,
                cy - 0.32 * s,
                cx - 0.32 * s,
                cy - 0.06 * s,
            );
            pb.cubic_to(
                cx - 0.36 * s,
                cy + 0.22 * s,
                cx - 0.10 * s,
                cy + 0.40 * s,
                cx + 0.18 * s,
                cy + 0.30 * s,
            );
            pb.move_to(cx + 0.30 * s, cy - 0.24 * s);
            pb.line_to(cx + 0.08 * s, cy - 0.24 * s);
            pb.line_to(cx + 0.16 * s, cy - 0.42 * s);
            pb.finish().expect("valid refresh path")
        }
        Icon::Collapse => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.30 * s, cy - 0.18 * s);
            pb.line_to(cx - 0.12 * s, cy);
            pb.line_to(cx - 0.30 * s, cy + 0.18 * s);
            pb.move_to(cx + 0.30 * s, cy - 0.18 * s);
            pb.line_to(cx + 0.12 * s, cy);
            pb.line_to(cx + 0.30 * s, cy + 0.18 * s);
            pb.finish().expect("valid collapse path")
        }
        Icon::NewFile | Icon::NewFolder => {
            let mut pb = PathBuilder::new();
            if matches!(icon, Icon::NewFile) {
                pb.move_to(cx - 0.30 * s, cy - 0.34 * s);
                pb.line_to(cx + 0.12 * s, cy - 0.34 * s);
                pb.line_to(cx + 0.30 * s, cy - 0.16 * s);
                pb.line_to(cx + 0.30 * s, cy + 0.34 * s);
                pb.line_to(cx - 0.30 * s, cy + 0.34 * s);
                pb.close();
            } else {
                pb.move_to(cx - 0.34 * s, cy + 0.30 * s);
                pb.line_to(cx - 0.34 * s, cy - 0.18 * s);
                pb.line_to(cx - 0.08 * s, cy - 0.18 * s);
                pb.line_to(cx + 0.04 * s, cy - 0.32 * s);
                pb.line_to(cx + 0.34 * s, cy - 0.32 * s);
                pb.line_to(cx + 0.34 * s, cy + 0.30 * s);
                pb.close();
            }
            pb.move_to(cx + 0.10 * s, cy + 0.02 * s);
            pb.line_to(cx + 0.10 * s, cy + 0.22 * s);
            pb.move_to(cx, cy + 0.12 * s);
            pb.line_to(cx + 0.20 * s, cy + 0.12 * s);
            pb.finish().expect("valid new path")
        }
        Icon::PanelLeft | Icon::PanelBottom => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.36 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.36 * s, cy - 0.28 * s);
            pb.line_to(cx + 0.36 * s, cy + 0.28 * s);
            pb.line_to(cx - 0.36 * s, cy + 0.28 * s);
            pb.close();
            match icon {
                Icon::PanelLeft => {
                    pb.move_to(cx - 0.36 * s, cy - 0.28 * s);
                    pb.line_to(cx - 0.12 * s, cy - 0.28 * s);
                    pb.line_to(cx - 0.12 * s, cy + 0.28 * s);
                    pb.line_to(cx - 0.36 * s, cy + 0.28 * s);
                }
                Icon::PanelBottom => {
                    pb.move_to(cx - 0.36 * s, cy + 0.28 * s);
                    pb.line_to(cx + 0.36 * s, cy + 0.28 * s);
                    pb.line_to(cx + 0.36 * s, cy + 0.08 * s);
                    pb.line_to(cx - 0.36 * s, cy + 0.08 * s);
                }
                _ => unreachable!(),
            }
            pb.close();
            pb.finish().expect("valid panel path")
        }
        Icon::DragHandle => {
            let mut pb = PathBuilder::new();
            for (dy, half) in [(-0.26, 0.32), (0.0, 0.20), (0.26, 0.32)] {
                pb.move_to(cx - half * s, cy + dy * s);
                pb.line_to(cx + half * s, cy + dy * s);
            }
            pb.finish().expect("valid drag handle path")
        }
        Icon::Trash => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 0.32 * s, cy - 0.24 * s);
            pb.line_to(cx + 0.32 * s, cy - 0.24 * s);
            pb.move_to(cx - 0.20 * s, cy - 0.34 * s);
            pb.line_to(cx + 0.20 * s, cy - 0.34 * s);
            pb.move_to(cx - 0.12 * s, cy - 0.34 * s);
            pb.line_to(cx - 0.12 * s, cy - 0.42 * s);
            pb.line_to(cx + 0.12 * s, cy - 0.42 * s);
            pb.line_to(cx + 0.12 * s, cy - 0.34 * s);
            pb.move_to(cx - 0.24 * s, cy - 0.20 * s);
            pb.line_to(cx - 0.16 * s, cy + 0.36 * s);
            pb.line_to(cx + 0.16 * s, cy + 0.36 * s);
            pb.line_to(cx + 0.24 * s, cy - 0.20 * s);
            pb.finish().expect("valid trash path")
        }
    };

    match icon {
        Icon::File
        | Icon::ChevronRight
        | Icon::ChevronDown
        | Icon::SourceControl
        | Icon::Terminal
        | Icon::Code
        | Icon::Markdown
        | Icon::Search
        | Icon::Split
        | Icon::Plus
        | Icon::Close
        | Icon::Refresh
        | Icon::Update
        | Icon::Collapse
        | Icon::NewFile
        | Icon::NewFolder
        | Icon::DragHandle
        | Icon::Trash
        | Icon::PanelLeft
        | Icon::PanelBottom => {
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
        Icon::Folder | Icon::Dot | Icon::ResizeGrip => {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Accent-like ink color for the HiDPI test (deliberately not any theme
    /// token so exact-count assertions can't be confused with a background).
    const INK: ColorRgb = (200, 100, 50);
    const BG: ColorRgb = (0, 0, 0);

    /// Pixels carrying ANY ink coverage (RGB differs from `bg`).
    fn count_ink(pixmap: &Pixmap) -> usize {
        let (px, _) = pixmap.data().as_chunks::<4>();
        px.iter().filter(|p| (p[0], p[1], p[2]) != BG).count()
    }

    /// Inclusive bounding box of all pixels differing from `bg` (any AA
    /// blend level counts as ink).
    fn ink_bbox(pixmap: &Pixmap) -> Option<(u32, u32, u32, u32)> {
        let w = pixmap.width() as usize;
        let (px, _) = pixmap.data().as_chunks::<4>();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        let mut any = false;
        for y in 0..pixmap.height() as usize {
            for x in 0..w {
                let p = &px[y * w + x];
                if (p[0], p[1], p[2]) != BG {
                    any = true;
                    x0 = x0.min(x as u32);
                    y0 = y0.min(y as u32);
                    x1 = x1.max(x as u32);
                    y1 = y1.max(y as u32);
                }
            }
        }
        any.then_some((x0, y0, x1, y1))
    }

    #[test]
    fn every_icon_has_a_hover_name_and_hit_area() {
        let icons = [
            Icon::ChevronRight,
            Icon::ChevronDown,
            Icon::File,
            Icon::Folder,
            Icon::Dot,
            Icon::SourceControl,
            Icon::Terminal,
            Icon::Code,
            Icon::Markdown,
            Icon::Search,
            Icon::Split,
            Icon::Plus,
            Icon::Close,
            Icon::Refresh,
            Icon::Update,
            Icon::Collapse,
            Icon::NewFile,
            Icon::NewFolder,
            Icon::DragHandle,
            Icon::ResizeGrip,
            Icon::Trash,
            Icon::PanelLeft,
            Icon::PanelBottom,
        ];
        let inside = Pointer { x: 50.0, y: 25.0, down: false, click: false };
        let outside = Pointer { x: 80.0, y: 25.0, down: false, click: false };
        for icon in icons {
            assert!(!icon.name().is_empty(), "{icon:?} needs a tooltip name");
            let tooltip = icon.hovered_tooltip(inside, 50.0, 25.0, 14.0).expect("hovered icon");
            assert_eq!(tooltip.label, icon.name());
            assert!(icon.hovered_tooltip(outside, 50.0, 25.0, 14.0).is_none());
        }
    }

    /// The logical-coordinate contract (see `draw`): the same logical icon
    /// under scale 2 must land at exactly 2× the physical position with ~2×
    /// the extent, and its stroke must be exactly one scale thicker. The
    /// pre-fix bug multiplied the stroke width by `scale` by hand AND let
    /// the transform scale it, inflating the stroke band ~2× beyond the
    /// contract at 2×.
    #[test]
    fn icon_strokes_are_bold() {
        assert_eq!(ICON_STROKE_WIDTH, 2.25);
    }

    #[test]
    fn icons_hidpi_proportional() {
        // The same LOGICAL icon under both scales.
        let mut pm1 = Pixmap::new(120, 60).expect("pixmap");
        pm1.fill(tiny_skia::Color::from_rgba8(BG.0, BG.1, BG.2, 255));
        draw(&mut pm1, Icon::ChevronRight, 50.0, 25.0, 22.0, INK, 1.0);

        let mut pm2 = Pixmap::new(240, 120).expect("pixmap");
        pm2.fill(tiny_skia::Color::from_rgba8(BG.0, BG.1, BG.2, 255));
        draw(&mut pm2, Icon::ChevronRight, 50.0, 25.0, 22.0, INK, 2.0);

        let b1 = ink_bbox(&pm1).expect("scale-1 icon must rasterize");
        let b2 = ink_bbox(&pm2).expect("scale-2 icon must rasterize");

        // 2× position offset: every physical edge lands at twice the 1×
        // coordinate (±2px AA slop).
        for (axis, a, b) in [
            (0, b1.0, b2.0),
            (1, b1.1, b2.1),
            (2, b1.2, b2.2),
            (3, b1.3, b2.3),
        ] {
            assert!(
                (f64::from(b) - 2.0 * f64::from(a)).abs() <= 2.0,
                "bbox edge {axis}: scale-2 {b} must be 2× scale-1 {a} (±2px)"
            );
        }

        // ~2× extent: the constant AA-slop term keeps the ratio just under
        // 2.0; the old double-scaling bug inflated it past 2.1. Bands below
        // are pinned from measurement (see the MEASURE output note).
        let ext1 = (f64::from(b1.2 - b1.0) + 1.0, f64::from(b1.3 - b1.1) + 1.0);
        let ext2 = (f64::from(b2.2 - b2.0) + 1.0, f64::from(b2.3 - b2.1) + 1.0);
        for (axis, (a, b)) in [(0, (ext1.0, ext2.0)), (1, (ext1.1, ext2.1))] {
            let ratio = b / a;
            assert!(
                (1.75..=2.15).contains(&ratio),
                "extent axis {axis}: scale-2 extent {b:.1} / scale-1 extent {a:.1} = {ratio:.2} must be ~2×"
            );
        }

        // Stroke thickness grows with scale exactly once. Full-coverage core
        // counts overstate growth for thin strokes (the AA band is ~constant
        // in physical px), so this pins the TOTAL ink area: a stroked glyph
        // is length × width, both double → ~3–4× total ink; the old bug's
        // extra width multiplication pushed it past ~5×.
        let n1 = count_ink(&pm1);
        let n2 = count_ink(&pm2);
        assert!(
            n1 >= 20,
            "1× icon must rasterize a visible stroke, got {n1}"
        );
        let ratio = n2 as f64 / n1.max(1) as f64;
        assert!(
            (2.5..=5.0).contains(&ratio),
            "stroke ink ratio 2×/1× = {ratio:.2} must reflect one scale of stroke growth \
             (measured {n1} → {n2}); a ratio near 8× means the stroke was scaled twice"
        );
    }
}
