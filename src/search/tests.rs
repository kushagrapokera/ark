use super::*;
use crate::fs::testutil::temp_dir;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn limits() -> SearchLimits {
    SearchLimits::default()
}

#[test]
fn literal_keyword_search_finds_nested_text_files_case_insensitively() {
    let d = temp_dir("search-nested");
    fs::create_dir_all(d.path().join("sub")).expect("mkdir");
    let a = d.path().join("a.rs");
    let b = d.path().join("sub/b.txt");
    fs::write(&a, "fn Alpha() {}\nlet alpha = 1;\n").expect("write a");
    fs::write(&b, "ALPHA\n").expect("write b");
    fs::write(d.path().join("binary.bin"), b"alpha\0hidden").expect("write binary");
    fs::write(d.path().join("invalid.txt"), [b'a', b'l', b'p', b'h', b'a', 0xff]).expect("write invalid");

    let report = run_search(d.path(), "alpha", 7, &AtomicBool::new(false));

    assert_eq!(report.generation, 7);
    assert_eq!(report.query, "alpha");
    assert_eq!(report.results.len(), 2);
    let a_result = report.results.iter().find(|r| r.path == a).expect("a.rs");
    assert_eq!(a_result.matches, 2);
    assert_eq!(a_result.line, 1);
    assert!(a_result.snippet.contains("Alpha"));
    let b_result = report.results.iter().find(|r| r.path == b).expect("b.txt");
    assert_eq!(b_result.matches, 1);
    assert_eq!(b_result.line, 1);
    assert_eq!(report.scanned_files, 2);
    assert!(!report.truncated);
    assert!(!report.cancelled);
    assert!(report.error.is_none());
}

#[cfg(unix)]
#[test]
fn search_never_follows_symlinks() {
    use std::os::unix::fs::symlink;
    let outside = temp_dir("search-symlink-outside");
    fs::write(outside.path().join("secret.txt"), "needle").expect("write outside");
    let d = temp_dir("search-symlink-root");
    symlink(outside.path(), d.path().join("linked-dir")).expect("symlink dir");
    symlink(outside.path().join("secret.txt"), d.path().join("linked.txt")).expect("symlink file");

    let report = run_search(d.path(), "needle", 1, &AtomicBool::new(false));

    assert!(report.results.is_empty(), "symlink targets must not be searched");
    assert_eq!(report.scanned_files, 0);
}

#[test]
fn search_caps_results_and_reports_truncation() {
    let d = temp_dir("search-result-cap");
    for i in 0..105 {
        fs::write(d.path().join(format!("f{i:03}.txt")), "needle\n").expect("write");
    }
    let mut capped = limits();
    capped.max_results = 100;

    let report = run_search_with_limits(d.path(), "needle", 1, &AtomicBool::new(false), capped);

    assert_eq!(report.results.len(), 100);
    assert!(report.truncated);
}

#[test]
fn search_keeps_an_exact_case_source_match_beyond_the_result_cap() {
    let d = temp_dir("search-ranked-cap");
    for i in 0..100 {
        fs::write(
            d.path().join(format!("generated{i:03}.txt")),
            "camelcasefunction\n",
        )
        .expect("write generated match");
    }
    fs::create_dir_all(d.path().join("src")).expect("mkdir src");
    let source = d.path().join("src/real.rs");
    fs::write(&source, "pub fn camelCaseFunction() {}\n").expect("write source");
    let mut capped = limits();
    capped.max_results = 1;

    let report = run_search_with_limits(
        d.path(),
        "camelCaseFunction",
        1,
        &AtomicBool::new(false),
        capped,
    );

    assert_eq!(report.results.len(), 1);
    assert_eq!(
        report.results[0].path, source,
        "an exact-case source match must outrank earlier case-insensitive matches"
    );
    assert_eq!(report.results[0].line, 1);
    assert!(report.results[0].snippet.contains("camelCaseFunction"));
    assert!(report.truncated, "the report still discloses that results were capped");
}

#[test]
fn search_skips_generated_build_and_dependency_directories() {
    let d = temp_dir("search-generated-dirs");
    let source = d.path().join("real.rs");
    fs::write(&source, "pub fn camelCaseFunction() {}\n").expect("write source");
    for generated in ["target/debug", "node_modules/pkg"] {
        let dir = d.path().join(generated);
        fs::create_dir_all(&dir).expect("mkdir generated directory");
        for i in 0..20 {
            fs::write(dir.join(format!("noise{i}.txt")), "camelCaseFunction\n")
                .expect("write generated match");
        }
    }

    let report = run_search(d.path(), "camelCaseFunction", 1, &AtomicBool::new(false));

    assert_eq!(report.scanned_files, 1, "generated directories are not project source");
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.results[0].path, source);
    assert!(!report.truncated);
}

#[test]
fn fractional_scroll_keeps_result_ink_out_of_the_search_chrome() {
    use std::path::Path;

    use crate::render::{Canvas, Theme};
    use crate::text::TextSystem;
    use crate::ui::{Pointer, Rect, Scrollbar};

    let rect = Rect { x: 0.0, y: 0.0, w: 304.0, h: 768.0 };
    let panel = panel_layout(rect);
    let mut state = SearchState::default();
    for i in 0..20 {
        state.results.push(SearchResult {
            path: PathBuf::from(format!("/repo/src/f{i:02}.rs")),
            matches: 1,
            line: 1,
            snippet: format!("fn target{i}() {{}}"),
        });
    }
    state.scroll = 32.0;
    let mut theme = Theme::DARK_PLUS;
    theme.bg = (0, 0, 0);
    theme.surface = (0, 0, 0);
    theme.bg_elevated = (0, 0, 0);
    theme.fg = (255, 255, 255);
    let mut canvas = Canvas::new(304, 768);
    canvas.fill(theme.bg);
    let mut text = TextSystem::new();
    let mut scrollbar = Scrollbar::default();

    let _ = draw_search_panel(
        &mut canvas,
        &mut text,
        &theme,
        1.0,
        1.0,
        rect,
        Path::new("/repo"),
        &mut state,
        Pointer::default(),
        &mut scrollbar,
    );

    assert_eq!(
        crate::ui::testutil::count_color_in(
            &canvas,
            theme.fg,
            0,
            (panel.results.y - 24.0) as u32,
            304,
            panel.results.y as u32,
        ),
        0,
        "a partially visible row must not paint its label into the status/input area"
    );
}

#[test]
fn search_caps_files_and_bytes() {
    let d = temp_dir("search-scan-cap");
    for i in 0..5 {
        fs::write(d.path().join(format!("f{i}.txt")), "needle\n").expect("write");
    }
    let cancel = AtomicBool::new(false);
    let mut file_cap = limits();
    file_cap.max_files = 2;
    let report = run_search_with_limits(d.path(), "needle", 1, &cancel, file_cap);
    assert_eq!(report.scanned_files, 2);
    assert!(report.truncated);

    let mut byte_cap = limits();
    byte_cap.max_total_bytes = 3;
    let report = run_search_with_limits(d.path(), "needle", 1, &cancel, byte_cap);
    assert!(report.scanned_files <= 1);
    assert!(report.truncated);
}

#[test]
fn cancellation_stops_the_worker_promptly() {
    let d = temp_dir("search-cancel");
    for i in 0..200 {
        fs::write(d.path().join(format!("f{i:03}.txt")), "needle\n").expect("write");
    }
    let cancel = AtomicBool::new(true);
    let start = Instant::now();
    let report = run_search(d.path(), "needle", 1, &cancel);

    assert!(report.cancelled);
    assert!(report.results.is_empty());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn snippets_are_bounded_and_control_free() {
    let d = temp_dir("search-snippet");
    fs::write(d.path().join("long.txt"), format!("needle{}\tend\n", "x".repeat(300))).expect("write");

    let report = run_search(d.path(), "needle", 1, &AtomicBool::new(false));

    let result = report.results.first().expect("match");
    assert!(result.snippet.chars().count() <= 160);
    assert!(!result.snippet.chars().any(char::is_control));
    assert!(result.snippet.starts_with("needle"));
}
