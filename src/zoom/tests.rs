use super::*;

fn modifiers(control: bool) -> ModifiersState {
    let mut modifiers = ModifiersState::empty();
    modifiers.set(ModifiersState::CONTROL, control);
    modifiers
}

#[test]
fn zoom_key_step_handles_plus_equal_and_minus() {
    for key in [Key::Character("+".into()), Key::Character("=".into())] {
        assert_eq!(key_step(&key, modifiers(true)), Some(1), "{key:?}");
    }
    assert_eq!(
        key_step(&Key::Character("-".into()), modifiers(true)),
        Some(-1)
    );
}

#[test]
fn zoom_key_step_requires_control() {
    for key in [
        Key::Character("+".into()),
        Key::Character("=".into()),
        Key::Character("-".into()),
    ] {
        assert_eq!(key_step(&key, modifiers(false)), None, "{key:?}");
    }
    assert_eq!(key_step(&Key::Character("x".into()), modifiers(true)), None);
}

#[test]
fn zoom_wheel_step_matches_requested_direction() {
    assert_eq!(
        wheel_step(&MouseScrollDelta::LineDelta(0.0, -1.0)),
        Some(1),
        "Ctrl+wheel down increases zoom"
    );
    assert_eq!(
        wheel_step(&MouseScrollDelta::LineDelta(0.0, 2.0)),
        Some(-1),
        "Ctrl+wheel up decreases zoom"
    );
    assert_eq!(wheel_step(&MouseScrollDelta::LineDelta(0.0, 0.0)), None);
    assert_eq!(
        wheel_step(&MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 40.0,)
        )),
        None
    );
}

#[test]
fn next_zoom_is_bounded_and_quantized() {
    assert_eq!(next_zoom(1.0, 1), 1.1);
    assert_eq!(next_zoom(1.0, -1), 0.9);
    assert_eq!(next_zoom(1.45, 1), MAX_ZOOM);
    assert_eq!(next_zoom(0.75, -1), MIN_ZOOM);
    assert_eq!(next_zoom(2.0, 0), MAX_ZOOM);
    assert_eq!(next_zoom(-2.0, 0), MIN_ZOOM);
    assert_eq!(next_zoom(f64::NAN, 1), 1.0);
    assert_eq!(next_zoom(f64::INFINITY, -1), 1.0);
}
