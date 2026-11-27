use super::*;
use std::path::Path;
use tiny_skia::Pixmap;

#[test]
fn classifies_language_and_config_files_case_insensitively() {
    let cases = [
        ("main.rs", FileKind::Rust),
        ("script.PY", FileKind::Python),
        ("index.html", FileKind::Html),
        ("theme.CSS", FileKind::Css),
        ("app.js", FileKind::JavaScript),
        ("component.tsx", FileKind::TypeScript),
        ("data.json", FileKind::Json),
        ("README", FileKind::Markdown),
        ("README.md", FileKind::Markdown),
        ("readme_generator.rs", FileKind::Rust),
        (".env", FileKind::Env),
        (".ENV.local", FileKind::Env),
        ("Cargo.toml", FileKind::Toml),
        ("config.yaml", FileKind::Yaml),
        ("notes.txt", FileKind::Text),
        ("table.csv", FileKind::Csv),
        ("photo.png", FileKind::Image),
        ("bundle.tar.gz", FileKind::Archive),
        ("unknown.bin", FileKind::Generic),
    ];
    for (name, expected) in cases {
        assert_eq!(
            FileKind::from_path(Path::new(name), false),
            expected,
            "wrong icon for {name}"
        );
    }
    assert_eq!(
        FileKind::from_path(Path::new("src"), true),
        FileKind::Folder
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_file_names_fall_back_to_the_generic_icon() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let name = OsString::from_vec(b"invalid-\xff.rs".to_vec());
    assert_eq!(
        FileKind::from_path(Path::new(&name), false),
        FileKind::Generic
    );
}

fn rendered(kind: FileKind) -> Pixmap {
    let mut pixmap = Pixmap::new(32, 32).unwrap();
    draw(
        &mut pixmap,
        kind,
        16.0,
        16.0,
        18.0,
        FilePalette {
            main: (32, 96, 192),
            glyph: (255, 255, 255),
        },
        1.0,
        false,
    );
    pixmap
}

fn count_color(pixmap: &Pixmap, color: crate::render::ColorRgb) -> usize {
    let (pixels, _) = pixmap.data().as_chunks::<4>();
    pixels
        .iter()
        .filter(|pixel| pixel[0] == color.0 && pixel[1] == color.1 && pixel[2] == color.2)
        .count()
}

fn assert_distinct(left: &Pixmap, right: &Pixmap, message: &str) {
    assert!(left.data() != right.data(), "{message}");
}

#[test]
fn every_file_kind_draws_visible_procedural_ink() {
    let kinds = [
        FileKind::Folder,
        FileKind::Rust,
        FileKind::Python,
        FileKind::Html,
        FileKind::Css,
        FileKind::JavaScript,
        FileKind::TypeScript,
        FileKind::Json,
        FileKind::Markdown,
        FileKind::Env,
        FileKind::Toml,
        FileKind::Yaml,
        FileKind::Text,
        FileKind::Csv,
        FileKind::Image,
        FileKind::Archive,
        FileKind::Generic,
    ];
    for kind in kinds {
        let pixmap = rendered(kind);
        let (pixels, _) = pixmap.data().as_chunks::<4>();
        let ink = pixels.iter().filter(|pixel| pixel[3] > 0).count();
        assert!(ink > 20, "{kind:?} did not draw an icon");
    }
}

#[test]
fn symbol_strokes_are_bold_enough_for_small_tree_icons() {
    const {
        assert!(
            super::draw::SYMBOL_STROKE_WIDTH >= 2.0,
            "small language/config symbols need at least a 2px logical stroke"
        );
    }
}

#[test]
fn language_badges_render_contrasting_marks() {
    let main = (32, 96, 192);
    let glyph = (255, 255, 255);
    for kind in [
        FileKind::Rust,
        FileKind::Python,
        FileKind::Html,
        FileKind::Css,
        FileKind::JavaScript,
        FileKind::TypeScript,
        FileKind::Json,
        FileKind::Markdown,
        FileKind::Env,
        FileKind::Toml,
        FileKind::Yaml,
        FileKind::Text,
        FileKind::Csv,
    ] {
        let pixmap = rendered(kind);
        assert!(
            count_color(&pixmap, main) > 20,
            "{kind:?} needs a filled badge"
        );
        assert!(
            count_color(&pixmap, glyph) > 5,
            "{kind:?} needs a visible contrasting mark"
        );
    }
}

#[test]
fn palette_foregrounds_are_high_contrast_in_bundled_themes() {
    for theme in [crate::render::Theme::DARK, crate::render::Theme::DARK_PLUS] {
        for kind in [
            FileKind::Rust,
            FileKind::Python,
            FileKind::Html,
            FileKind::Css,
            FileKind::JavaScript,
            FileKind::TypeScript,
            FileKind::Json,
            FileKind::Markdown,
            FileKind::Env,
            FileKind::Toml,
            FileKind::Yaml,
            FileKind::Text,
            FileKind::Csv,
            FileKind::Image,
            FileKind::Archive,
            FileKind::Generic,
        ] {
            let palette = kind.palette(&theme);
            assert!(
                super::contrast_ratio(palette.main, palette.glyph) >= 4.5,
                "{kind:?} needs a readable foreground"
            );
        }
    }
}

#[test]
fn specialized_glyphs_are_distinct_from_the_generic_document() {
    let generic = rendered(FileKind::Generic);
    let specialized = [
        FileKind::Rust,
        FileKind::Python,
        FileKind::Html,
        FileKind::Css,
        FileKind::JavaScript,
        FileKind::TypeScript,
        FileKind::Json,
        FileKind::Markdown,
        FileKind::Env,
        FileKind::Toml,
        FileKind::Yaml,
        FileKind::Csv,
        FileKind::Image,
        FileKind::Archive,
    ];
    for kind in specialized {
        assert_distinct(
            &rendered(kind),
            &generic,
            &format!("{kind:?} must not use the generic document glyph"),
        );
    }
    assert_distinct(
        &rendered(FileKind::Html),
        &rendered(FileKind::Css),
        "HTML and CSS need distinct symbols",
    );
    assert_distinct(
        &rendered(FileKind::JavaScript),
        &rendered(FileKind::TypeScript),
        "JavaScript and TypeScript need distinct symbols",
    );
    assert_distinct(
        &rendered(FileKind::Toml),
        &rendered(FileKind::Yaml),
        "TOML and YAML need distinct symbols",
    );
}
