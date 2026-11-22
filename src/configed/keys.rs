//! Config-editor keyboard routing (M9). Split from `configed.rs` to keep the
//! facade state-focused; child module access keeps the body representation
//! private to `configed`.

use winit::keyboard::{Key, ModifiersState};

use super::text::{Motion, TextEditor};
use super::{Body, ConfigEditor, EnvEditor, ENV_KEY_CAP, ENV_VALUE_CAP};
use crate::ui::{TextInput, TextInputKey};

pub(super) fn handle(
    editor: &mut ConfigEditor,
    key: &Key,
    text: Option<&str>,
    mods: &ModifiersState,
) -> bool {
    let ctrl = mods.control_key();
    let shift = mods.shift_key();
    // Ctrl chords are either App shortcuts or textarea undo/copy; env form
    // fields must never insert the chord's text (e.g. Ctrl+Z → "z").
    if ctrl && matches!(editor.body, Body::Env(_)) {
        return false;
    }
    let dirty;
    let changed = match &mut editor.body {
        Body::Env(ed) => {
            let before_key = ed.key.text.clone();
            let before_value = ed.value.text.clone();
            let (redraw, doc_changed) = env_key(ed, key, text);
            dirty =
                doc_changed || ed.key.text != before_key || ed.value.text != before_value;
            redraw
        }
        Body::Text(t) => {
            let changed = text_key(t, key, text, ctrl, shift);
            dirty = t.is_dirty();
            changed
        }
    };
    if dirty {
        editor.dirty = true;
        editor.error = None;
        editor.external_change = false;
    }
    changed
}

fn env_key(ed: &mut EnvEditor, key: &Key, text: Option<&str>) -> (bool, bool) {
    use winit::keyboard::{Key as WKey, NamedKey};
    if matches!(key, WKey::Named(NamedKey::Escape)) {
        ed.key.focused = false;
        ed.value.focused = false;
        return (true, false);
    }
    if matches!(key, WKey::Named(NamedKey::Tab))
        || matches!(key, WKey::Named(NamedKey::ArrowDown) if ed.key.focused)
    {
        if ed.key.focused {
            ed.key.focused = false;
            ed.value.focused = true;
        } else if ed.value.focused {
            ed.value.focused = false;
            ed.key.focused = true;
        }
        return (true, false);
    }
    if matches!(key, WKey::Named(NamedKey::Enter)) {
        if ed.selected.is_some() {
            return match ed.apply_form() {
                Ok(applied) => (true, applied),
                Err(e) => {
                    ed.form_error = Some(e);
                    (true, false)
                }
            };
        }
        return (false, false);
    }
    let changed = if ed.key.focused {
        field_key(&mut ed.key, key, text, ENV_KEY_CAP)
    } else if ed.value.focused {
        field_key(&mut ed.value, key, text, ENV_VALUE_CAP)
    } else {
        false
    };
    (changed, false)
}

fn field_key(input: &mut TextInput, key: &Key, text: Option<&str>, cap: usize) -> bool {
    use winit::keyboard::{Key as WKey, NamedKey};
    match key {
        WKey::Named(NamedKey::Backspace) => input.handle_key_with_cap(TextInputKey::Backspace, cap),
        WKey::Named(NamedKey::ArrowLeft) => input.handle_key_with_cap(TextInputKey::Left, cap),
        WKey::Named(NamedKey::ArrowRight) => input.handle_key_with_cap(TextInputKey::Right, cap),
        WKey::Named(NamedKey::Home) => input.handle_key_with_cap(TextInputKey::Home, cap),
        WKey::Named(NamedKey::End) => input.handle_key_with_cap(TextInputKey::End, cap),
        _ => match text {
            Some(t) if !t.is_empty() && !t.chars().any(char::is_control) => {
                input.insert_with_cap(t, cap)
            }
            _ => false,
        },
    }
}

fn text_key(
    t: &mut TextEditor,
    key: &Key,
    text: Option<&str>,
    ctrl: bool,
    shift: bool,
) -> bool {
    use winit::keyboard::{Key as WKey, NamedKey};
    if ctrl {
        if let WKey::Character(s) = key {
            match s.to_ascii_lowercase().as_str() {
                "a" => return t.select_all(),
                "z" => return if shift { t.redo() } else { t.undo() },
                "y" => return t.redo(),
                "c" => {
                    if let Some(sel) = t.selected_text() {
                        match crate::viewer::copy_to_clipboard(sel) {
                            Ok(()) => log::info!("copied {} config chars", sel.chars().count()),
                            Err(e) => log::error!("config copy failed: {e}"),
                        }
                    }
                    return true;
                }
                _ => return false,
            }
        }
        return false;
    }
    let select = shift;
    match key {
        WKey::Named(NamedKey::Backspace) => t.backspace(),
        WKey::Named(NamedKey::Delete) => t.delete(),
        WKey::Named(NamedKey::Enter) => t.insert("\n"),
        WKey::Named(NamedKey::Tab) => t.insert("\t"),
        WKey::Named(NamedKey::ArrowLeft) => t.move_cursor(Motion::Left, select),
        WKey::Named(NamedKey::ArrowRight) => t.move_cursor(Motion::Right, select),
        WKey::Named(NamedKey::ArrowUp) => t.move_cursor(Motion::Up, select),
        WKey::Named(NamedKey::ArrowDown) => t.move_cursor(Motion::Down, select),
        WKey::Named(NamedKey::Home) => t.move_cursor(Motion::Home, select),
        WKey::Named(NamedKey::End) => t.move_cursor(Motion::End, select),
        WKey::Named(NamedKey::PageUp) => t.move_cursor(Motion::PageUp, select),
        WKey::Named(NamedKey::PageDown) => t.move_cursor(Motion::PageDown, select),
        _ => match text {
            Some(s) => t.insert(s),
            None => false,
        },
    }
}
