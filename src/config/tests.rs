//! Config/whitelist tests: defaults, `ark.toml` parsing, glob matching.

use super::*;
use crate::fs::testutil::temp_dir;
use std::path::Path;

// ---- defaults ----

#[test]
fn default_whitelist_is_the_plan_set() {
    let c = Config::default();
    assert_eq!(c.editable, vec![".env*", "*.yaml", "*.yml", "*.toml"]);
    assert_eq!(parse_editable("").expect("empty ok"), c.editable, "absent array → defaults");
}

// ---- ark.toml parsing ----

#[test]
fn parses_editable_single_and_multiline() {
    assert_eq!(
        parse_editable(r#"editable = [".env*", "*.yaml"]"#).expect("parse"),
        vec![".env*", "*.yaml"]
    );
    assert_eq!(
        parse_editable(
            "# config\n\
             editable = [\n\
               \".env*\",\n\
               '*.yml', # trailing comment\n\
               \"docs/*.toml\",\n\
             ]\n\
             other = 1\n"
        )
        .expect("parse multiline"),
        vec![".env*", "*.yml", "docs/*.toml"]
    );
    // Explicit empty list disables human writes entirely.
    assert!(parse_editable("editable = []\n").expect("empty array").is_empty());
    // Escapes in basic strings.
    assert_eq!(
        parse_editable(r#"editable = ["a\tb", "q\"q"]"#).expect("escapes"),
        vec!["a\tb", "q\"q"]
    );
}

#[test]
fn malformed_editable_is_an_error_not_a_widened_default() {
    assert!(matches!(parse_editable("editable = \"*.env\""), Err(ConfigError::Parse(_))));
    assert!(matches!(parse_editable("editable = [\"*.env\""), Err(ConfigError::Parse(_))));
    assert!(matches!(parse_editable("editable = [*.env]"), Err(ConfigError::Parse(_))));
    assert!(matches!(parse_editable("editable = [\"a\\q\"]"), Err(ConfigError::Parse(_))));
    let mut too_many = String::from("editable = [");
    for i in 0..=MAX_EDITABLE_PATTERNS {
        too_many.push_str(&format!("\"f{i}\","));
    }
    too_many.push(']');
    assert!(matches!(parse_editable(&too_many), Err(ConfigError::Parse(_))));
}

#[test]
fn load_reads_ark_toml_and_defaults_when_missing() {
    let d = temp_dir("config-load");
    assert_eq!(load(d.path()).expect("missing file → defaults"), Config::default());
    std::fs::write(d.path().join("ark.toml"), "editable = [\"*.conf\"]\n").expect("write");
    assert_eq!(
        load(d.path()).expect("load"),
        Config { editable: vec!["*.conf".into()] }
    );
    std::fs::write(d.path().join("ark.toml"), "editable = [broken\n").expect("write");
    assert!(matches!(load(d.path()), Err(ConfigError::Parse(_))));
}

// ---- glob matching / whitelist ----

#[test]
fn glob_matches_star_question_and_file_names() {
    assert!(matches("*.yaml", "config.yaml", "config.yaml"));
    assert!(matches("*.yaml", "config.yaml", "nested/config.yaml"));
    assert!(!matches("*.yaml", "config.yml", "config.yml"));
    assert!(matches(".env*", ".env", ".env"));
    assert!(matches(".env*", ".env.local", "sub/.env.local"));
    assert!(!matches(".env*", "env", "env"));
    assert!(matches("config?.toml", "config1.toml", "config1.toml"));
    assert!(!matches("config?.toml", "config12.toml", "config12.toml"));
    assert!(matches("docs/*.toml", "a.toml", "docs/a.toml"));
    assert!(!matches("docs/*.toml", "a.toml", "other/a.toml"));
    assert!(!matches("", "a", "a"));
    assert!(matches("*", "anything", "nested/anything"));
}

#[test]
fn is_editable_checks_relative_paths_and_patterns() {
    let root = Path::new("/repo");
    let p = |s: &str| root.join(s);
    let patterns = Config::default().editable;
    assert!(is_editable(root, &p(".env"), &patterns));
    assert!(is_editable(root, &p(".env.production"), &patterns));
    assert!(is_editable(root, &p("config.yaml"), &patterns));
    assert!(is_editable(root, &p("deploy/app.yml"), &patterns));
    assert!(is_editable(root, &p("ark.toml"), &patterns));
    assert!(!is_editable(root, &p("src/main.rs"), &patterns));
    assert!(!is_editable(root, &p("README.md"), &patterns));
    // Outside the root is never editable.
    assert!(!is_editable(root, Path::new("/etc/passwd"), &patterns));
    // Relative paths are accepted for UI callers.
    assert!(is_editable(root, Path::new("config.yaml"), &patterns));
    // A custom path pattern matches the relative path only.
    let custom = vec!["docs/*.toml".to_string()];
    assert!(is_editable(root, &p("docs/a.toml"), &custom));
    assert!(!is_editable(root, &p("other/a.toml"), &custom));
    // The whitelist can only NARROW the built-in config surface: no pattern
    // makes code (or any non-config file) human-writable.
    for patterns in [vec!["*".to_string()], vec!["*.rs".to_string()], vec!["src/*".to_string()]] {
        assert!(!is_editable(root, &p("src/main.rs"), &patterns), "{patterns:?}");
        assert!(!is_editable(root, &p("README.md"), &patterns), "{patterns:?}");
    }
    assert!(is_editable(root, &p(".ENV.LOCAL"), &["*".to_string()]), "case-insensitive env");
    assert!(is_editable(root, &p("config.YAML"), &["*".to_string()]), "case-insensitive ext");
    // An explicit empty whitelist disables everything.
    assert!(!is_editable(root, &p(".env"), &[]));
}
