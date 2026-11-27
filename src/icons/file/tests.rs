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
        24.0,
        (255, 255, 255),
        1.0,
        false,
    );
    pixmap
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
        assert_ne!(
            rendered(kind).data(),
            generic.data(),
            "{kind:?} must not use the generic document glyph"
        );
    }
    assert_ne!(
        rendered(FileKind::Html).data(),
        rendered(FileKind::Css).data(),
        "HTML and CSS need distinct symbols"
    );
    assert_ne!(
        rendered(FileKind::JavaScript).data(),
        rendered(FileKind::TypeScript).data(),
        "JavaScript and TypeScript need distinct symbols"
    );
}
