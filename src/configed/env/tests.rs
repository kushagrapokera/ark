//! `.env` model tests: round-trip preservation, entries, edits, masking.

use super::*;

const SAMPLE: &str = "\
# app secrets
APP_NAME=ark

# database
export DATABASE_URL=postgres://localhost/ark
INVALID LINE WITHOUT EQUALS
DB_PASSWORD=hunter2
QUOTED=\"two words\"
";

#[test]
fn parse_render_preserves_comments_order_and_unsupported_lines() {
    let doc = EnvDoc::parse(SAMPLE).expect("parse");
    assert_eq!(doc.render(), SAMPLE, "byte-for-byte round-trip");
    assert_eq!(doc.line_count(), 8);
    assert_eq!(
        doc.entries(),
        vec![
            (1, "APP_NAME", "ark"),
            (4, "DATABASE_URL", "postgres://localhost/ark"),
            (6, "DB_PASSWORD", "hunter2"),
            (7, "QUOTED", "\"two words\""),
        ]
    );
    let export = doc.line(4).expect("line");
    assert!(matches!(export, EnvLine::Entry { export: true, .. }));
    assert!(matches!(doc.line(5), Some(EnvLine::Raw(s)) if s == "INVALID LINE WITHOUT EQUALS"));
}

#[test]
fn empty_and_newline_only_documents_round_trip() {
    let empty = EnvDoc::parse("").expect("parse");
    assert_eq!(empty.render(), "");
    assert!(empty.entries().is_empty());
    let newline = EnvDoc::parse("\n").expect("parse");
    assert_eq!(newline.render(), "\n");
    let no_trailing = EnvDoc::parse("A=1").expect("parse");
    assert_eq!(no_trailing.render(), "A=1");
}

#[test]
fn apply_renames_and_updates_in_place() {
    let mut doc = EnvDoc::parse(SAMPLE).expect("parse");
    assert!(doc.apply(1, "APP_TITLE", "ark editor"));
    assert_eq!(doc.render().lines().nth(1), Some("APP_TITLE=ark editor"));
    assert_eq!(doc.render().lines().next(), Some("# app secrets"), "comments untouched");

    // Invalid key/value edits are rejected and leave the entry unchanged.
    assert!(!doc.apply(1, "BAD KEY", "x"));
    assert!(!doc.apply(1, "A=B", "x"));
    assert!(!doc.apply(1, "", "x"));
    assert!(!doc.apply(1, "OK", &"x".repeat(MAX_ENV_VALUE_CHARS + 1)));
    assert!(!doc.apply(0, "COMMENT", "not an entry"));
    assert_eq!(doc.render().lines().nth(1), Some("APP_TITLE=ark editor"));
}

#[test]
fn append_and_remove_keep_order_and_raw_lines() {
    let mut doc = EnvDoc::parse("A=1\n# tail\n").expect("parse");
    let new_line = doc.append();
    assert_eq!(new_line, 2);
    assert!(doc.apply(new_line, "B", "2"));
    assert_eq!(doc.render(), "A=1\n# tail\nB=2\n");

    assert!(doc.remove(0));
    assert_eq!(doc.render(), "# tail\nB=2\n", "comments survive removals");
    assert!(!doc.remove(0), "raw lines cannot be removed as entries");
    assert!(doc.remove(1));
    assert_eq!(doc.render(), "# tail\n");
}

#[test]
fn secret_keys_and_masking() {
    for key in ["DB_PASSWORD", "api_key", "GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY", "PRIVATE_KEY"] {
        assert!(is_secret_key(key), "{key}");
    }
    for key in ["APP_NAME", "DATABASE_URL", "PORT", "KEY"] {
        assert!(!is_secret_key(key), "{key}");
    }
    assert_eq!(mask_value(""), "");
    assert_eq!(mask_value("abc"), "••••", "short values keep a minimum mask");
    assert_eq!(mask_value("a-very-long-secret-token").chars().count(), 12);
    assert!(mask_value("hunter2").chars().all(|c| c == '•'));
}

#[test]
fn line_cap_is_enforced() {
    let text = "A=1\n".repeat(MAX_ENV_LINES + 1);
    assert!(matches!(EnvDoc::parse(&text), Err(EditError::Io(_))));
    let ok = "A=1\n".repeat(MAX_ENV_LINES);
    assert!(EnvDoc::parse(&ok).is_ok());
}
