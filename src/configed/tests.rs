//! Config editor integration tests: open/kind detection, save/reload,
//! external changes, env form interactions and textarea raster.

use super::*;
use crate::config::Config;
use crate::fs::testutil::temp_dir;
use crate::text::TextSystem;
use crate::ui::Pointer;

fn patterns() -> Vec<String> {
    Config::default().editable
}

#[test]
fn open_detects_env_and_text_and_rejects_code() {
    let d = temp_dir("configed-open");
    let env = d.path().join(".env");
    std::fs::write(&env, "A=1\n").expect("write env");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "a: 1\n").expect("write yaml");
    let code = d.path().join("main.rs");
    std::fs::write(&code, "fn main() {}\n").expect("write code");

    let e = ConfigEditor::open(d.path(), &env, &patterns()).expect("env opens");
    assert_eq!(e.kind(), ConfigKind::Env);
    assert!(!e.is_dirty());
    assert!(matches!(&e.body, Body::Env(ed) if ed.doc.entries().len() == 1));

    let t = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("yaml opens");
    assert_eq!(t.kind(), ConfigKind::Text);
    assert!(matches!(&t.body, Body::Text(ed) if ed.text() == "a: 1\n"));

    let err = ConfigEditor::open(d.path(), &code, &patterns()).expect_err("code rejected");
    assert!(matches!(err, EditError::NotWhitelisted(_)));
}

#[test]
fn save_is_atomic_and_clears_dirty() {
    let d = temp_dir("configed-save");
    let env = d.path().join(".env");
    std::fs::write(&env, "# keep\nA=1\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &env, &patterns()).expect("open");
    let Body::Env(ed) = &mut e.body else { panic!("env") };
    let line = ed.doc.entries()[0].0;
    ed.select(line);
    ed.value.text = "2".into();
    ed.value.cursor = 1;
    assert!(ed.apply_form().expect("apply"));
    e.save().expect("save");
    assert_eq!(std::fs::read_to_string(&env).expect("read"), "# keep\nA=2\n");
    assert!(!e.is_dirty());

    // Text mode: edit, save, reload from disk.
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "a: 1\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    e.handle_key(
        &winit::keyboard::Key::Named(winit::keyboard::NamedKey::End),
        None,
        &winit::keyboard::ModifiersState::empty(),
    );
    assert!(e.on_ime_commit("b: 2\n"));
    assert!(e.is_dirty());
    e.save().expect("save text");
    assert_eq!(std::fs::read_to_string(&yaml).expect("read"), "a: 1\nb: 2\n");

    // The enforced boundary still applies even if the caller tries to save
    // an editor that was opened before the whitelist changed.
    let mut stale = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    stale.patterns = vec!["*.env".into()];
    assert!(stale.save().is_err());
}

#[test]
fn external_change_reloads_clean_and_warns_dirty() {
    let d = temp_dir("configed-external");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "a: 1\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&yaml, "a: 2\n").expect("external edit");
    assert!(e.changed_on_disk());
    assert!(e.refresh_if_clean(), "clean editor reloads");
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.text(), "a: 2\n");
    assert!(!e.is_dirty());

    // Dirty editors keep their text and flag the conflict (first notice
    // reports a state change so the header repaints; repeats are no-ops).
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    assert!(e.on_ime_commit("mine\n"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&yaml, "theirs\n").expect("external edit");
    assert!(e.refresh_if_clean(), "first conflict notice repaints");
    assert!(!e.refresh_if_clean(), "already flagged: no further change");
    assert!(e.external_change());
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.text(), "a: 2\nmine\n", "dirty text preserved");
    e.reload().expect("explicit reload");
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.text(), "theirs\n");
    assert!(!e.external_change());
}

#[test]
fn ime_preedit_and_commit_hit_the_textarea() {
    let d = temp_dir("configed-ime");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "a: 1\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    assert!(e.on_ime_preedit("かな"));
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.preedit(), Some("かな"));
    assert_eq!(t.text(), "a: 1\n", "preedit does not mutate the document");
    assert!(e.on_ime_commit("かな"));
    assert!(e.is_dirty());
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.text(), "a: 1\nかな");
    assert_eq!(t.preedit(), None);
}

#[test]
fn textarea_drag_keeps_its_selection_on_release() {
    let d = temp_dir("configed-text-drag");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "abcdef\nghij\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = crate::render::Canvas::new(720, 632);
    let advance = f64::from(text.mono_metrics(14.0, 18.9).advance);
    let y = 52.0 + 5.0;
    let x1 = 44.0 + advance + 1.0;
    let x4 = 44.0 + advance * 4.0 + 1.0;

    let _ = e.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        pane(),
        Pointer { x: x1, y, down: true, click: false },
    );
    let _ = e.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        pane(),
        Pointer { x: x4, y, down: true, click: false },
    );
    let _ = e.draw(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        pane(),
        Pointer { x: x4, y, down: false, click: true },
    );
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.selected_text(), Some("bcd"), "drag selection survives release");
    assert!(!e.dragging);
    assert!(!e.is_dirty(), "selection is not an edit");
}

fn panel_theme() -> crate::render::Theme {
    crate::render::Theme {
        bg: (0, 0, 0),
        bg_elevated: (10, 20, 30),
        fg: (255, 255, 255),
        fg_dim: (90, 90, 90),
        accent: (200, 220, 255),
        border: (60, 60, 70),
        selection: (40, 50, 60),
        syntax_keyword: (203, 166, 247),
        syntax_string: (166, 227, 161),
        syntax_number: (250, 179, 135),
        syntax_comment: (108, 112, 134),
        git_modified: (250, 179, 135),
        git_added: (166, 227, 161),
        git_deleted: (243, 139, 168),
        git_untracked: (148, 226, 213),
    }
}

fn pane() -> Rect {
    Rect { x: 0.0, y: 0.0, w: 720.0, h: 632.0 }
}

#[test]
fn env_form_draws_selects_adds_and_reports_header_clicks() {
    let d = temp_dir("configed-env-ui");
    let env = d.path().join(".env");
    std::fs::write(&env, "# note\nAPP_NAME=ark\nDB_PASSWORD=hunter2\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &env, &patterns()).expect("open");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = crate::render::Canvas::new(720, 632);

    e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), Pointer::default());
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg, 0, 0, 720, 34) > 0,
        "header/title ink"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg, 0, 52, 720, 200) > 0,
        "env row ink"
    );

    // Row 0 (the first ENTRY, after the comment) selects at y≈62.
    let row_click = Pointer { x: 100.0, y: 62.0, down: false, click: true };
    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), row_click);
    assert!(!fx.changed, "selection is not a document edit");
    let Body::Env(ed) = &e.body else { panic!("env") };
    assert_eq!(ed.selected, Some(ed.doc.entries()[0].0));
    assert_eq!(ed.key.text, "APP_NAME");
    assert_eq!(ed.value.text, "ark");

    // Add button (8..239 × 602..624) appends NEW_KEY and marks dirty.
    let before = ed.doc.entries().len();
    let add_click = Pointer { x: 100.0, y: 613.0, down: false, click: true };
    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), add_click);
    assert!(fx.changed);
    assert!(e.is_dirty());
    let Body::Env(ed) = &e.body else { panic!("env") };
    assert_eq!(ed.doc.entries().len(), before + 1);
    assert_eq!(ed.doc.entries().last().expect("new").1, "NEW_KEY");

    // Header Save/Reload report effects (the App does the fs boundary).
    let save_click = Pointer { x: 681.0, y: 19.0, down: false, click: true };
    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), save_click);
    assert!(fx.save && !fx.reload);
    let reload_click = Pointer { x: 613.0, y: 19.0, down: false, click: true };
    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), reload_click);
    assert!(fx.reload && !fx.save);

    // Degenerate panes rasterize nothing and never panic.
    e.draw(&mut canvas, &mut text, &theme, 1.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, Pointer::default());
    e.draw(&mut canvas, &mut text, &theme, 0.0, pane(), Pointer::default());
}

#[test]
fn env_form_field_focus_and_typing_target_the_clicked_field() {
    let d = temp_dir("configed-env-focus");
    let env = d.path().join(".env");
    std::fs::write(&env, "APP_NAME=ark\nDB_PASSWORD=hunter2\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &env, &patterns()).expect("open");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = crate::render::Canvas::new(720, 632);

    // Select the DB row inside the config pane (pane at 0,0 → row 1 y=82).
    let row_click = Pointer { x: 400.0, y: 82.0, down: false, click: true };
    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), row_click);
    assert!(!fx.changed);
    let Body::Env(ed) = &e.body else { panic!("env") };
    assert_eq!(ed.selected, Some(1));
    assert!(ed.value.focused && !ed.key.focused, "row click focuses the value field");
    assert_eq!(ed.value.text, "hunter2");

    // Click the value field (x 577..1008, y 570..594 for a 0,0 pane) and
    // type into it.
    let value_click = Pointer { x: 700.0, y: 580.0, down: false, click: true };
    e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), value_click);
    let Body::Env(ed) = &e.body else { panic!("env") };
    assert!(ed.value.focused && !ed.key.focused, "value field keeps focus");
    assert!(e.handle_key(
        &winit::keyboard::Key::Character("X".into()),
        Some("X"),
        &winit::keyboard::ModifiersState::empty()
    ));
    let Body::Env(ed) = &e.body else { panic!("env") };
    assert_eq!(ed.value.text, "hunter2X");
    assert_eq!(ed.key.text, "DB_PASSWORD");
    assert!(e.is_dirty(), "editing a form field marks pending changes");
}

#[test]
fn textarea_draws_ink_and_maps_clicks_to_the_caret() {
    let d = temp_dir("configed-text-ui");
    let yaml = d.path().join("config.yaml");
    std::fs::write(&yaml, "alpha: 1\nbeta: 2\ngamma: 3\n").expect("write");
    let mut e = ConfigEditor::open(d.path(), &yaml, &patterns()).expect("open");
    let theme = panel_theme();
    let mut text = TextSystem::new();
    let mut canvas = crate::render::Canvas::new(720, 632);

    let fx = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), Pointer::default());
    assert!(!fx.changed);
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg, 0, 52, 720, 200) > 0,
        "text ink"
    );
    assert!(
        crate::ui::testutil::count_color_in(&canvas, theme.fg_dim, 0, 52, 44, 200) > 0,
        "line-number gutter ink"
    );

    // Click line 1, column ~2 (body starts at y=52, text at x=44).
    let advance = f64::from(text.mono_metrics(14.0, 18.9).advance);
    let click = Pointer {
        x: 44.0 + advance * 2.0 + 1.0,
        y: 52.0 + 18.9 + 5.0,
        down: false,
        click: true,
    };
    let _ = e.draw(&mut canvas, &mut text, &theme, 1.0, pane(), click);
    let Body::Text(t) = &e.body else { panic!("text") };
    assert_eq!(t.cursor(), (1, 2));
    assert!(!e.is_dirty(), "caret movement is not an edit");
}
