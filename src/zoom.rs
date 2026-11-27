use winit::event::MouseScrollDelta;
use winit::keyboard::{Key, ModifiersState};

pub const MIN_ZOOM: f64 = 0.75;
pub const MAX_ZOOM: f64 = 1.50;
pub const ZOOM_STEP: f64 = 0.10;

/// Zoom direction for a Control-modified logical key.
pub fn key_step(key: &Key, modifiers: ModifiersState) -> Option<i32> {
    if !modifiers.control_key() {
        return None;
    }
    match key {
        Key::Character(character) if character == "+" || character == "=" => Some(1),
        Key::Character(character) if character == "-" => Some(-1),
        _ => None,
    }
}

/// Ctrl+wheel down increases zoom; Ctrl+wheel up decreases it. Touchpad
/// pixel deltas deliberately remain outside this shortcut.
pub fn wheel_step(delta: &MouseScrollDelta) -> Option<i32> {
    let MouseScrollDelta::LineDelta(_, y) = delta else {
        return None;
    };
    if !y.is_finite() || *y == 0.0 {
        return None;
    }
    Some(if *y < 0.0 { 1 } else { -1 })
}

/// Apply one bounded, tenth-quantized zoom step.
pub fn next_zoom(current: f64, step: i32) -> f64 {
    if !current.is_finite() {
        return 1.0;
    }
    let next = (current + f64::from(step) * ZOOM_STEP) * 10.0;
    (next.round() / 10.0).clamp(MIN_ZOOM, MAX_ZOOM)
}

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod tests;
