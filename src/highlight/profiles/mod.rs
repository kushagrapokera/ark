//! Static language/file-type profiles for the Tier-1.5 highlighter.
//!
//! No grammar engine, AST or per-file state: each entry is a fixed table of
//! comments, keywords, types and file-specific capture rules. Profiles are
//! compiled lazily when a file of that type is rendered.

use std::sync::OnceLock;

use regex::Regex;

use super::Token;

mod keywords;
mod rules;

use keywords::*;
use rules::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct RuleDef {
    pub pattern: &'static str,
    pub token: Token,
    /// Capture group to color; 0 means the whole match.
    pub group: usize,
    /// Shared compiled form; identical rule constants reuse one `Regex`.
    pub cell: &'static OnceLock<Regex>,
}

/// Comment syntax family. Profiles with the same family share one compiled
/// regex; only the families actually rendered are compiled.
#[derive(Clone, Copy, Debug)]
pub(super) enum CommentStyle {
    /// `//`, `/* ... */`
    Slash,
    /// `#`
    Hash,
    /// `#` or `;`
    HashSemi,
    /// `--`, `/* ... */`
    Sql,
    /// `<!-- ... -->`
    Html,
    /// `/* ... */`
    Css,
    /// `//`, `#`, `/* ... */`
    Nix,
    /// `//`, `#`, `--`
    Plain,
    /// `%`
    Percent,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LangDef {
    pub comment: CommentStyle,
    pub keywords: &'static [&'static str],
    pub control_keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub case_insensitive: bool,
    pub variables: bool,
    pub camel_case_types: bool,
    pub extras: &'static [RuleDef],
}

pub(super) fn lang_def(lang: super::Lang) -> LangDef {
    use super::Lang::*;
    match lang {
        Rust => LangDef {
            comment: CommentStyle::Slash,
            keywords: RUST_KEYWORDS,
            control_keywords: RUST_CONTROL,
            types: RUST_TYPES,
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: RUST_EXTRAS,
        },
        CLike => LangDef {
            comment: CommentStyle::Slash,
            keywords: C_LIKE_KEYWORDS,
            control_keywords: C_LIKE_CONTROL,
            types: C_LIKE_TYPES,
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: C_LIKE_EXTRAS,
        },
        Go => LangDef {
            comment: CommentStyle::Slash,
            keywords: GO_KEYWORDS,
            control_keywords: GO_CONTROL,
            types: GO_TYPES,
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: GO_EXTRAS,
        },
        PythonLike => LangDef {
            comment: CommentStyle::Hash,
            keywords: PYTHON_LIKE_KEYWORDS,
            control_keywords: PYTHON_LIKE_CONTROL,
            types: PYTHON_LIKE_TYPES,
            case_insensitive: true,
            variables: true,
            camel_case_types: true,
            extras: PY_EXTRAS,
        },
        Shell => LangDef {
            comment: CommentStyle::Hash,
            keywords: SHELL_KEYWORDS,
            control_keywords: SHELL_CONTROL,
            types: &[],
            case_insensitive: false,
            variables: true,
            camel_case_types: false,
            extras: SHELL_EXTRAS,
        },
        PowerShell => LangDef {
            comment: CommentStyle::Hash,
            keywords: POWERSHELL_KEYWORDS,
            control_keywords: POWERSHELL_CONTROL,
            types: &[],
            case_insensitive: true,
            variables: true,
            camel_case_types: true,
            extras: PS_EXTRAS,
        },
        Sql => LangDef {
            comment: CommentStyle::Sql,
            keywords: SQL_KEYWORDS,
            control_keywords: SQL_CONTROL,
            types: SQL_TYPES,
            case_insensitive: true,
            variables: true,
            camel_case_types: false,
            extras: SQL_EXTRAS,
        },
        Markup => LangDef {
            comment: CommentStyle::Html,
            keywords: &[],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: MARKUP_EXTRAS,
        },
        Css => LangDef {
            comment: CommentStyle::Css,
            keywords: &["important", "inherit", "initial", "unset", "none", "auto"],
            control_keywords: &[],
            types: &[],
            case_insensitive: true,
            variables: false,
            camel_case_types: false,
            extras: CSS_EXTRAS,
        },
        Json => LangDef {
            comment: CommentStyle::Slash,
            keywords: &["true", "false", "null"],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: JSON_EXTRAS,
        },
        Yaml => LangDef {
            comment: CommentStyle::Hash,
            keywords: YAML_KEYWORDS,
            control_keywords: &[],
            types: &[],
            case_insensitive: true,
            variables: false,
            camel_case_types: false,
            extras: YAML_EXTRAS,
        },
        Toml => LangDef {
            comment: CommentStyle::Hash,
            keywords: YAML_KEYWORDS,
            control_keywords: &[],
            types: &[],
            case_insensitive: true,
            variables: false,
            camel_case_types: false,
            extras: TOML_EXTRAS,
        },
        Ini => LangDef {
            comment: CommentStyle::HashSemi,
            keywords: &["true", "false", "yes", "no", "on", "off"],
            control_keywords: &[],
            types: &[],
            case_insensitive: true,
            variables: false,
            camel_case_types: false,
            extras: INI_EXTRAS,
        },
        Dotenv => LangDef {
            comment: CommentStyle::Hash,
            keywords: &["true", "false"],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: DOTENV_EXTRAS,
        },
        Dockerfile => LangDef {
            comment: CommentStyle::Hash,
            keywords: DOCKER_KEYWORDS,
            control_keywords: &[],
            types: &[],
            case_insensitive: true,
            variables: false,
            camel_case_types: false,
            extras: DOCKER_EXTRAS,
        },
        Makefile => LangDef {
            comment: CommentStyle::Hash,
            keywords: &[
                "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "include", "define", "endef",
                "export", "override",
            ],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: MAKE_EXTRAS,
        },
        Markdown => LangDef {
            comment: CommentStyle::Html,
            keywords: &[],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: MD_EXTRAS,
        },
        Nix => LangDef {
            comment: CommentStyle::Nix,
            keywords: NIX_KEYWORDS,
            control_keywords: NIX_CONTROL,
            types: &["bool", "float", "int", "path", "string"],
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: NIX_EXTRAS,
        },
        Hcl => LangDef {
            comment: CommentStyle::Nix,
            keywords: HCL_KEYWORDS,
            control_keywords: NIX_CONTROL,
            types: &["bool", "number", "string", "list", "map", "object", "tuple"],
            case_insensitive: true,
            variables: true,
            camel_case_types: true,
            extras: HCL_EXTRAS,
        },
        Protobuf => LangDef {
            comment: CommentStyle::Slash,
            keywords: PROTO_KEYWORDS,
            control_keywords: &[],
            types: PROTO_TYPES,
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: PROTO_EXTRAS,
        },
        DashLike => LangDef {
            comment: CommentStyle::Sql,
            keywords: DASH_LIKE_KEYWORDS,
            control_keywords: DASH_CONTROL,
            types: &[
                "Bool", "Char", "Double", "Float", "Int", "Integer", "IO", "Maybe", "String", "Text",
            ],
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: DASH_EXTRAS,
        },
        Cmake => LangDef {
            comment: CommentStyle::Hash,
            keywords: CMAKE_KEYWORDS,
            control_keywords: CMAKE_CONTROL,
            types: &[],
            case_insensitive: true,
            variables: true,
            camel_case_types: false,
            extras: CMAKE_EXTRAS,
        },
        Graphql => LangDef {
            comment: CommentStyle::Hash,
            keywords: GRAPHQL_KEYWORDS,
            control_keywords: GRAPHQL_CONTROL,
            types: GRAPHQL_TYPES,
            case_insensitive: false,
            variables: true,
            camel_case_types: true,
            extras: GRAPHQL_EXTRAS,
        },
        Latex => LangDef {
            comment: CommentStyle::Percent,
            keywords: &[],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: LATEX_EXTRAS,
        },
        Plain => LangDef {
            comment: CommentStyle::Plain,
            keywords: &[],
            control_keywords: &[],
            types: &[],
            case_insensitive: false,
            variables: false,
            camel_case_types: false,
            extras: PLAIN_EXTRAS,
        },
    }
}
