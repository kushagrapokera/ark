//! Tier-1 highlighter tests: token precedence, language detection, and
//! file-specific fixtures. Heuristic, not a grammar engine.

use super::*;
use std::path::Path;

fn kinds_for(line: &str, lang: Lang) -> Vec<(usize, usize, Token)> {
    Highlighter::new()
        .line_spans_for(line, lang)
        .into_iter()
        .map(|s| (s.start, s.end, s.token))
        .collect()
}

fn kinds_path(line: &str, path: &str) -> Vec<(usize, usize, Token)> {
    Highlighter::new()
        .line_spans_for(line, detect::lang_for(Path::new(path)))
        .into_iter()
        .map(|s| (s.start, s.end, s.token))
        .collect()
}

fn kinds_state(
    highlighter: &Highlighter,
    line: &str,
    lang: Lang,
    state: &mut HighlightState,
) -> Vec<(usize, usize, Token)> {
    highlighter
        .line_spans_with_state(line, lang, state)
        .into_iter()
        .map(|s| (s.start, s.end, s.token))
        .collect()
}

#[test]
fn rust_keywords_numbers_types_and_functions_span_correctly() {
    assert_eq!(
        kinds_for("let x: i32 = 0x1F + 1_000;", Lang::Rust),
        vec![
            (0, 3, Token::Keyword),
            (4, 5, Token::Variable),
            (7, 10, Token::Type),
            (13, 17, Token::Number),
            (20, 25, Token::Number),
        ]
    );
    assert_eq!(
        kinds_for("fn main() { let MAX_SIZE = 4; }", Lang::Rust),
        vec![
            (0, 2, Token::Keyword),
            (3, 7, Token::Function),
            (12, 15, Token::Keyword),
            (16, 24, Token::Constant),
            (27, 28, Token::Number),
        ]
    );
}

#[test]
fn control_keywords_variables_calls_and_members_are_distinct() {
    assert_eq!(
        kinds_for("if ready { return call(item); }", Lang::Rust),
        vec![
            (0, 2, Token::ControlKeyword),
            (3, 8, Token::Variable),
            (11, 17, Token::ControlKeyword),
            (18, 22, Token::Function),
            (23, 27, Token::Variable),
        ]
    );
    assert_eq!(
        kinds_for("let user: User = Default::default();", Lang::Rust),
        vec![
            (0, 3, Token::Keyword),
            (4, 8, Token::Variable),
            (10, 14, Token::Type),
            (17, 24, Token::Type),
            (26, 33, Token::Function),
        ]
    );
    assert_eq!(
        kinds_for("obj.field = value;", Lang::CLike),
        vec![
            (0, 3, Token::Variable),
            (4, 9, Token::Property),
            (12, 17, Token::Variable),
        ]
    );
}

#[test]
fn regex_escape_decorator_and_markdown_links_use_dark_plus_categories() {
    assert_eq!(
        kinds_for("@cache\ndef cached(): pass", Lang::PythonLike),
        vec![
            (1, 6, Token::Decorator),
            (7, 10, Token::Keyword),
            (11, 17, Token::Function),
            (21, 25, Token::Keyword),
        ]
    );
    assert_eq!(
        kinds_for(r#"const re = /a+\d/g;"#, Lang::CLike),
        vec![
            (0, 5, Token::Keyword),
            (6, 8, Token::Variable),
            (11, 18, Token::Regex),
        ]
    );
    assert_eq!(
        kinds_for(r#"let s = "a\nb";"#, Lang::Rust),
        vec![
            (0, 3, Token::Keyword),
            (4, 5, Token::Variable),
            (8, 10, Token::String),
            (10, 12, Token::Escape),
            (12, 14, Token::String),
        ]
    );
    assert_eq!(
        kinds_for(r#"Regex::new(r"a+b")"#, Lang::Rust),
        vec![
            (0, 5, Token::Type),
            (7, 10, Token::Function),
            (11, 17, Token::Regex),
        ]
    );
    assert_eq!(
        kinds_for(r#"re.compile(r"a+b")"#, Lang::PythonLike),
        vec![
            (0, 2, Token::Variable),
            (3, 10, Token::Function),
            (11, 17, Token::Regex),
        ]
    );
    assert_eq!(
        kinds_path("[guide](https://ark.dev)", "README.md"),
        vec![
            (1, 6, Token::Constant),
            (8, 23, Token::String),
        ]
    );
}

#[test]
fn css_selectors_properties_values_and_units_are_distinct() {
    assert_eq!(
        kinds_path(".hero:hover { color: red; padding: 2rem; }", "site.css"),
        vec![
            (0, 5, Token::Selector),
            (5, 11, Token::Selector),
            (14, 19, Token::Property),
            (21, 24, Token::String),
            (26, 33, Token::Property),
            (35, 39, Token::Number),
        ]
    );
}

#[test]
fn block_comments_carry_across_lines_and_close_mid_line() {
    let highlighter = Highlighter::new();
    let mut state = HighlightState::default();
    assert_eq!(
        kinds_state(&highlighter, "/* open", Lang::Rust, &mut state),
        vec![(0, 7, Token::Comment)]
    );
    assert_eq!(
        kinds_state(&highlighter, "still comment", Lang::Rust, &mut state),
        vec![(0, 13, Token::Comment)]
    );
    assert_eq!(
        kinds_state(&highlighter, "end */ let x = 1;", Lang::Rust, &mut state),
        vec![
            (0, 6, Token::Comment),
            (7, 10, Token::Keyword),
            (11, 12, Token::Variable),
            (15, 16, Token::Number),
        ]
    );
}

#[test]
fn triple_quoted_strings_carry_across_lines() {
    let highlighter = Highlighter::new();
    let mut state = HighlightState::default();
    assert_eq!(
        kinds_state(&highlighter, "text = \"\"\"hello", Lang::PythonLike, &mut state),
        vec![(0, 4, Token::Variable), (7, 15, Token::String)]
    );
    assert_eq!(
        kinds_state(&highlighter, "world", Lang::PythonLike, &mut state),
        vec![(0, 5, Token::String)]
    );
    assert_eq!(
        kinds_state(&highlighter, "end \"\"\" + 1", Lang::PythonLike, &mut state),
        vec![(0, 7, Token::String), (10, 11, Token::Number)]
    );
}

#[test]
fn markdown_fences_switch_to_the_fenced_language() {
    let highlighter = Highlighter::new();
    let mut state = HighlightState::default();
    kinds_state(&highlighter, "```rust", Lang::Markdown, &mut state);
    assert_eq!(
        kinds_state(&highlighter, "fn main() { return 1; }", Lang::Markdown, &mut state),
        vec![
            (0, 2, Token::Keyword),
            (3, 7, Token::Function),
            (12, 18, Token::ControlKeyword),
            (19, 20, Token::Number),
        ]
    );
    kinds_state(&highlighter, "```", Lang::Markdown, &mut state);
    assert_eq!(
        kinds_state(&highlighter, "# Heading", Lang::Markdown, &mut state),
        vec![(0, 9, Token::Keyword)]
    );
}

#[test]
fn html_script_and_style_blocks_switch_languages() {
    let highlighter = Highlighter::new();
    let mut state = HighlightState::default();
    assert_eq!(
        kinds_state(&highlighter, "<script>", Lang::Markup, &mut state),
        vec![(1, 7, Token::Keyword)]
    );
    assert_eq!(
        kinds_state(&highlighter, "const x = 1;", Lang::Markup, &mut state),
        vec![
            (0, 5, Token::Keyword),
            (6, 7, Token::Variable),
            (10, 11, Token::Number),
        ]
    );
    assert_eq!(
        kinds_state(&highlighter, "</script>", Lang::Markup, &mut state),
        vec![(2, 8, Token::Keyword)]
    );
    kinds_state(&highlighter, "<style>", Lang::Markup, &mut state);
    assert_eq!(
        kinds_state(&highlighter, "color: red;", Lang::Markup, &mut state),
        vec![(0, 5, Token::Property), (7, 10, Token::String)]
    );
    assert_eq!(
        kinds_state(&highlighter, "</style>", Lang::Markup, &mut state),
        vec![(2, 7, Token::Keyword)]
    );
}

#[test]
fn html_tag_detection_requires_a_real_script_or_style_tag() {
    let highlighter = Highlighter::new();
    let mut state = HighlightState::default();
    highlighter.line_spans_with_state("<scripture>", Lang::Markup, &mut state);
    assert_eq!(
        kinds_state(&highlighter, "const value = 123;", Lang::Markup, &mut state),
        vec![(6, 11, Token::Property), (14, 17, Token::Number)],
        "a tag merely starting with `script` must not embed JavaScript"
    );
}

#[test]
fn lookback_state_is_bounded_to_256_lines() {
    let highlighter = Highlighter::new();
    let mut lines = vec!["/* open"];
    lines.extend(std::iter::repeat_n("plain", 299));

    let mut far = highlighter.state_before(Lang::Rust, 300, |i| lines.get(i).copied());
    assert_eq!(
        kinds_state(&highlighter, "still", Lang::Rust, &mut far),
        vec![(0, 5, Token::Variable)],
        "a comment opener outside the bounded window is not retained"
    );

    let mut near = highlighter.state_before(Lang::Rust, 100, |i| lines.get(i).copied());
    assert_eq!(
        kinds_state(&highlighter, "still", Lang::Rust, &mut near),
        vec![(0, 5, Token::Comment)],
        "an opener inside the bounded window is retained"
    );
}

#[test]
fn strings_win_over_comments_inside_them() {
    assert_eq!(
        kinds_for(r#"let url = "http://x";"#, Lang::Rust),
        vec![(0, 3, Token::Keyword), (4, 7, Token::Variable), (10, 20, Token::String)],
    );
}

#[test]
fn comments_win_over_strings_inside_them() {
    assert_eq!(kinds_for("// it's \"quoted\" // x", Lang::Rust), vec![(0, 21, Token::Comment)]);
}

#[test]
fn rust_attributes_are_decorators_not_comments() {
    assert_eq!(
        kinds_for("#[derive(Debug)]", Lang::Rust),
        vec![(2, 8, Token::Decorator), (9, 14, Token::Type)]
    );
    assert_eq!(
        kinds_for("#![allow(dead_code)]", Lang::Rust),
        vec![(3, 8, Token::Decorator), (9, 18, Token::Variable)]
    );
    assert_eq!(kinds_for("# TODO fix", Lang::Yaml), vec![(0, 10, Token::Comment)]);
}

#[test]
fn char_literals_match_but_prose_apostrophes_do_not() {
    assert_eq!(
        kinds_for("let c = '\\n';", Lang::Rust),
        vec![
            (0, 3, Token::Keyword),
            (4, 5, Token::Variable),
            (8, 9, Token::String),
            (9, 11, Token::Escape),
            (11, 12, Token::String),
        ]
    );
    assert_eq!(kinds_for("it's fine", Lang::Plain), Vec::new());
}

#[test]
fn spans_use_char_indices_not_bytes() {
    assert_eq!(
        kinds_for("let café = \"ü\";", Lang::Rust),
        vec![(0, 3, Token::Keyword), (4, 7, Token::Variable), (11, 14, Token::String)]
    );
}

#[test]
fn language_detection_covers_languages_and_config_files() {
    for (path, lang) in [
        ("main.rs", Lang::Rust),
        ("main.c", Lang::CLike),
        ("App.tsx", Lang::CLike),
        ("main.go", Lang::Go),
        ("script.py", Lang::PythonLike),
        ("Gemfile.rb", Lang::PythonLike),
        ("run.sh", Lang::Shell),
        ("run.ps1", Lang::PowerShell),
        ("query.SQL", Lang::Sql),
        ("page.html", Lang::Markup),
        ("App.vue", Lang::Markup),
        ("site.scss", Lang::Css),
        ("data.json", Lang::Json),
        ("data.jsonl", Lang::Json),
        ("Cargo.lock", Lang::Toml),
        (".env", Lang::Dotenv),
        (".env.local", Lang::Dotenv),
        (".envrc", Lang::Shell),
        (".bashrc", Lang::Shell),
        ("Gemfile", Lang::PythonLike),
        ("config.yaml", Lang::Yaml),
        ("ark.toml", Lang::Toml),
        ("config.ini", Lang::Ini),
        (".editorconfig", Lang::Ini),
        (".gitconfig", Lang::Ini),
        ("Dockerfile", Lang::Dockerfile),
        ("Makefile", Lang::Makefile),
        ("justfile", Lang::Makefile),
        ("CMakeLists.txt", Lang::Cmake),
        ("main.cmake", Lang::Cmake),
        ("notes.md", Lang::Markdown),
        ("flake.nix", Lang::Nix),
        ("main.tf", Lang::Hcl),
        ("schema.proto", Lang::Protobuf),
        ("schema.graphql", Lang::Graphql),
        ("paper.tex", Lang::Latex),
        ("index.php", Lang::CLike),
        ("init.lua", Lang::DashLike),
        ("README", Lang::Plain),
        ("data.unknown", Lang::Plain),
    ] {
        assert_eq!(detect::lang_for(Path::new(path)), lang, "{path}");
    }
}

#[test]
fn shebang_and_framework_filenames_refine_detection() {
    for (path, first_line, lang) in [
        ("script", "#!/usr/bin/env python3", Lang::PythonLike),
        ("tool", "#!/usr/bin/env -S node --trace-warnings", Lang::CLike),
        ("run", "#!/bin/bash", Lang::Shell),
        ("main.rs", "#!/usr/bin/python", Lang::Rust),
        ("meson.build", "", Lang::PythonLike),
        ("BUILD", "", Lang::PythonLike),
        ("WORKSPACE.bazel", "", Lang::PythonLike),
        ("go.mod", "", Lang::Go),
        ("Pipfile.lock", "", Lang::Json),
        ("prod.tfstate", "", Lang::Json),
        ("nginx.conf", "", Lang::Ini),
    ] {
        assert_eq!(
            lang_for_source(Path::new(path), first_line),
            lang,
            "{path}"
        );
    }
}

#[test]
fn python_yaml_toml_and_dotenv_highlight_keys_and_types() {
    assert_eq!(
        kinds_path("def greet(name: str) -> None:  # hi", "app.py"),
        vec![
            (0, 3, Token::Keyword),
            (4, 9, Token::Function),
            (10, 14, Token::Variable),
            (16, 19, Token::Type),
            (24, 28, Token::Keyword),
            (31, 35, Token::Comment),
        ]
    );
    assert_eq!(
        kinds_path("enabled: true  # feature", "config.yaml"),
        vec![
            (0, 7, Token::Property),
            (9, 13, Token::Keyword),
            (15, 24, Token::Comment),
        ]
    );
    assert_eq!(
        kinds_path("[server]", "config.toml"),
        vec![(1, 7, Token::Type)]
    );
    assert_eq!(
        kinds_path("name = \"ark\"", "config.toml"),
        vec![(0, 4, Token::Property), (7, 12, Token::String)]
    );
    assert_eq!(
        kinds_path("export DB_PASSWORD=\"hunter2\" # secret", ".env"),
        vec![
            (7, 18, Token::Property),
            (19, 28, Token::String),
            (29, 37, Token::Comment),
        ]
    );
}

#[test]
fn json_html_css_and_markdown_highlight_structures() {
    assert_eq!(
        kinds_path("\"name\": \"ark\",", "data.json"),
        vec![(0, 7, Token::Property), (8, 13, Token::String)]
    );
    assert_eq!(
        kinds_path("<div class=\"hero\">", "page.html"),
        vec![
            (1, 4, Token::Keyword),
            (5, 10, Token::Property),
            (11, 17, Token::String),
        ]
    );
    assert_eq!(
        kinds_path("  color: red;", "site.css"),
        vec![(2, 7, Token::Property), (9, 12, Token::String)]
    );
    assert_eq!(
        kinds_path("# Heading", "notes.md"),
        vec![(0, 9, Token::Keyword)]
    );
    assert_eq!(
        kinds_path("Use `cargo test` now", "notes.md"),
        vec![(4, 16, Token::String)]
    );
}

#[test]
fn docker_make_and_sql_highlight_instructions_and_keys() {
    assert_eq!(
        kinds_path("FROM rust:1.75 AS build", "Dockerfile"),
        vec![(0, 4, Token::Keyword), (10, 14, Token::Number)]
    );
    assert_eq!(
        kinds_path("build: main.o", "Makefile"),
        vec![(0, 5, Token::Function)]
    );
    assert_eq!(
        kinds_path("cc $(CFLAGS)", "Makefile"),
        vec![(5, 11, Token::Property)]
    );
    assert_eq!(
        kinds_path("SELECT id FROM users; -- fetch", "query.sql"),
        vec![
            (0, 6, Token::Keyword),
            (7, 9, Token::Variable),
            (10, 14, Token::Keyword),
            (15, 20, Token::Variable),
            (22, 30, Token::Comment),
        ]
    );
    assert_eq!(
        kinds_path("CREATE TABLE t (id INT);", "query.sql"),
        vec![
            (0, 6, Token::Keyword),
            (7, 12, Token::Keyword),
            (13, 14, Token::Variable),
            (16, 18, Token::Variable),
            (19, 22, Token::Type),
        ]
    );
}

#[test]
fn cmake_graphql_and_latex_highlight_structures() {
    assert_eq!(
        kinds_path("set(CMAKE_BUILD_TYPE \"Release\")", "CMakeLists.txt"),
        vec![
            (0, 3, Token::Keyword),
            (4, 20, Token::Constant),
            (21, 30, Token::String),
        ]
    );
    assert_eq!(
        kinds_path("    my_function(ark)", "CMakeLists.txt"),
        vec![(4, 15, Token::Function), (16, 19, Token::Variable)]
    );
    assert_eq!(
        kinds_path("type User implements Node {", "schema.graphql"),
        vec![
            (0, 4, Token::Keyword),
            (5, 9, Token::Type),
            (10, 20, Token::Keyword),
            (21, 25, Token::Type),
        ]
    );
    assert_eq!(
        kinds_path("  name: String!", "schema.graphql"),
        vec![(2, 6, Token::Property), (8, 14, Token::Type)]
    );
    assert_eq!(
        kinds_path(r"\section{Introduction} % intro", "paper.tex"),
        vec![(0, 8, Token::Keyword), (23, 30, Token::Comment)]
    );
    assert_eq!(
        kinds_path(r"$\alpha + 2$", "paper.tex"),
        vec![(0, 12, Token::String)]
    );
}

#[test]
fn unknown_extension_still_highlights_universal_tokens() {
    assert_eq!(
        kinds_path("x = \"text\" // 42", "file.xyz"),
        vec![(4, 10, Token::String), (11, 16, Token::Comment)]
    );
    assert_eq!(
        kinds_path("x = 42 # note", "file.xyz"),
        vec![(4, 6, Token::Number), (7, 13, Token::Comment)]
    );
    assert_eq!(
        kinds_path("API_KEY = value", "file.xyz"),
        vec![(0, 7, Token::Constant)]
    );
}

/// Memory evidence for the Tier-1 claim: compiling every static profile is
/// bounded. Run explicitly with
/// `cargo test highlight::tests::all_profiles_fit_memory_budget -- --ignored --nocapture`.
#[test]
#[ignore = "memory probe: run explicitly with --ignored --nocapture"]
fn all_profiles_fit_memory_budget() {
    let before = crate::memory::report().expect("linux /proc report").rss_kib;
    let highlighter = Highlighter::new();
    let mut last = before;
    for lang in Lang::ALL {
        std::hint::black_box(highlighter.line_spans_for(
            "fn main() { let x = 42; } // note",
            lang,
        ));
        let now = crate::memory::report().expect("linux /proc report").rss_kib;
        println!("{lang:?}: RSS +{} KiB", now.saturating_sub(last));
        last = now;
    }
    let after = crate::memory::report().expect("linux /proc report").rss_kib;
    let delta_kib = after.saturating_sub(before);
    println!("compiled all {} profiles: RSS +{delta_kib} KiB", Lang::ALL.len());
    assert!(
        delta_kib < 5 * 1024,
        "all profiles must stay under 5 MiB, grew {delta_kib} KiB"
    );
}

#[test]
fn empty_lines_have_no_spans() {
    for lang in Lang::ALL {
        assert_eq!(kinds_for("", lang), Vec::new(), "{lang:?}");
    }
}

#[test]
fn plain_profile_leaves_identifiers_uncolored() {
    assert_eq!(
        kinds_for("plain_identifier + call()", Lang::Plain),
        Vec::new()
    );
}
