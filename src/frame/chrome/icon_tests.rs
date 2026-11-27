use super::*;
use tiny_skia::Pixmap;

const INK: ColorRgb = (200, 100, 50);

fn ink_bbox(pixmap: &Pixmap) -> Option<(u32, u32, u32, u32)> {
    let width = pixmap.width() as usize;
    let (pixels, _) = pixmap.data().as_chunks::<4>();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    let mut any = false;
    for y in 0..pixmap.height() as usize {
        for x in 0..width {
            if pixels[y * width + x][3] == 0 {
                continue;
            }
            any = true;
            x0 = x0.min(x as u32);
            y0 = y0.min(y as u32);
            x1 = x1.max(x as u32);
            y1 = y1.max(y as u32);
        }
    }
    any.then_some((x0, y0, x1, y1))
}

#[test]
fn chrome_icons_scale_position_and_size_exactly_once() {
    let mut one = Pixmap::new(120, 60).unwrap();
    let mut two = Pixmap::new(240, 120).unwrap();
    draw_icon(&mut one, Icon::ChevronRight, 50.0, 25.0, 22.0, INK, 1.0);
    draw_icon(&mut two, Icon::ChevronRight, 50.0, 25.0, 22.0, INK, 2.0);

    let b1 = ink_bbox(&one).expect("1x chrome icon");
    let b2 = ink_bbox(&two).expect("2x chrome icon");
    for (axis, a, b) in [
        (0, b1.0, b2.0),
        (1, b1.1, b2.1),
        (2, b1.2, b2.2),
        (3, b1.3, b2.3),
    ] {
        assert!(
            (f64::from(b) - 2.0 * f64::from(a)).abs() <= 2.0,
            "bbox edge {axis}: 2x {b} must be twice 1x {a} (±2px AA slop)"
        );
    }
}
