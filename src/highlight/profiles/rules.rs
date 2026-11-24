//! Shared file-specific regex rules and profile extras.

use std::sync::OnceLock;

use regex::Regex;

use super::RuleDef;
use crate::highlight::Token;

macro_rules! rule {
    ($cell:ident, $name:ident, $pattern:literal, $token:expr, $group:literal) => {
        static $cell: OnceLock<Regex> = OnceLock::new();
        const $name: RuleDef = RuleDef {
            pattern: $pattern,
            token: $token,
            group: $group,
            cell: &$cell,
        };
    };
}

// ---- file-specific capture rules ----

rule!(CONST_RE, CONST_RULE, r"\b[A-Z][A-Z0-9_]{2,}\b", Token::Constant, 0);
rule!(
    FUNCTION_DECL_RE,
    FUNCTION_DECL,
    r"\b(?:fn|def|defp|func|function|fun|sub|proc)\s+([A-Za-z_][A-Za-z0-9_]*)",
    Token::Function,
    1
);
rule!(
    TYPE_DECL_RE,
    TYPE_DECL,
    r"\b(?:class|struct|enum|trait|interface|type|union|record|message|service|rpc|protocol)\s+([A-Za-z_][A-Za-z0-9_]*)",
    Token::Type,
    1
);
rule!(RUST_MACRO_RE, RUST_MACRO, r"\b([a-z_][a-z0-9_]*)!", Token::Function, 1);

rule!(
    SHELL_FN_RE,
    SHELL_FN,
    r"^\s*(?:function\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\(\)",
    Token::Function,
    1
);
rule!(
    POWERSHELL_FN_RE,
    POWERSHELL_FN,
    r"\bfunction\s+([A-Za-z_][A-Za-z0-9_-]*)",
    Token::Function,
    1
);

rule!(
    MARKUP_TAG_RE,
    MARKUP_TAG,
    r"</?\s*([A-Za-z][A-Za-z0-9:_-]*)",
    Token::Keyword,
    1
);
rule!(
    MARKUP_ATTR_RE,
    MARKUP_ATTR,
    r"([A-Za-z_:][A-Za-z0-9:._-]*)\s*=",
    Token::Property,
    1
);

rule!(
    CSS_PROPERTY_RE,
    CSS_PROPERTY,
    r"(?:^|[;{])\s*([a-zA-Z-]+)\s*:",
    Token::Property,
    1
);
rule!(CSS_AT_RULE_RE, CSS_AT_RULE, r"@[a-zA-Z-]+", Token::Keyword, 0);

rule!(JSON_KEY_RE, JSON_KEY, r#""([^"]+)"\s*:"#, Token::Property, 0);

rule!(
    YAML_KEY_RE,
    YAML_KEY,
    r"^\s*([A-Za-z0-9_.-]+)\s*:",
    Token::Property,
    1
);
rule!(
    TOML_TABLE_RE,
    TOML_TABLE,
    r"^\s*\[([^\]]+)\]",
    Token::Type,
    1
);
rule!(
    TOML_KEY_RE,
    TOML_KEY,
    r"^\s*([A-Za-z0-9_.-]+)\s*=",
    Token::Property,
    1
);
rule!(
    INI_SECTION_RE,
    INI_SECTION,
    r"^\s*\[([^\]]+)\]",
    Token::Type,
    1
);
rule!(
    INI_KEY_RE,
    INI_KEY,
    r"^\s*([A-Za-z0-9_.-]+)\s*[=:]",
    Token::Property,
    1
);
rule!(
    DOTENV_KEY_RE,
    DOTENV_KEY,
    r"^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_.-]*)\s*=",
    Token::Property,
    1
);

rule!(
    DOCKER_INSTRUCTION_RE,
    DOCKER_INSTRUCTION,
    r"(?i)^\s*(from|run|cmd|label|maintainer|expose|env|add|copy|entrypoint|volume|user|workdir|arg|onbuild|stopsignal|healthcheck|shell)\b",
    Token::Keyword,
    1
);
rule!(
    MAKE_TARGET_RE,
    MAKE_TARGET,
    r"^([A-Za-z0-9_.%/-]+)\s*:",
    Token::Function,
    1
);
rule!(MAKE_VAR_RE, MAKE_VAR, r"\$\(([^)]+)\)", Token::Property, 1);

rule!(MD_HEADING_RE, MD_HEADING, r"^#{1,6}\s+.*$", Token::Keyword, 0);
rule!(MD_CODE_RE, MD_CODE, r"`[^`]+`", Token::String, 0);
rule!(MD_BOLD_RE, MD_BOLD, r"\*\*[^*]+\*\*", Token::Keyword, 0);

rule!(
    NIX_ATTR_RE,
    NIX_ATTR,
    r"\b([A-Za-z_][A-Za-z0-9_'-]*)\s*=",
    Token::Property,
    1
);
rule!(
    HCL_KEY_RE,
    HCL_KEY,
    r"^\s*([A-Za-z_][A-Za-z0-9_-]*)\s*[=:]",
    Token::Property,
    1
);
rule!(
    PROTO_FIELD_RE,
    PROTO_FIELD,
    r"^\s*(?:repeated\s+|optional\s+|required\s+)?[A-Za-z_][A-Za-z0-9_.]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*=",
    Token::Property,
    1
);
rule!(
    DASH_FN_RE,
    DASH_FN,
    r"^\s*([a-z_][A-Za-z0-9_']*)\s*::",
    Token::Function,
    1
);
rule!(
    CMAKE_CALL_RE,
    CMAKE_CALL,
    r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    Token::Function,
    1
);
rule!(CMAKE_VAR_RE, CMAKE_VAR, r"\$\{([^}]+)\}", Token::Property, 1);
rule!(
    GRAPHQL_FIELD_RE,
    GRAPHQL_FIELD,
    r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:",
    Token::Property,
    1
);
rule!(
    GRAPHQL_TYPE_DECL_RE,
    GRAPHQL_TYPE_DECL,
    r"^\s*(?:type|interface|union|enum|input|scalar|schema|directive)\s+([A-Za-z_][A-Za-z0-9_]*)",
    Token::Type,
    1
);
rule!(LATEX_COMMAND_RE, LATEX_COMMAND, r"\\[a-zA-Z@]+", Token::Keyword, 0);
rule!(
    LATEX_MATH_RE,
    LATEX_MATH,
    r"\$\$[^$]+\$\$|\$[^$]+\$",
    Token::String,
    0
);

rule!(
    FUNCTION_CALL_RE,
    FUNCTION_CALL,
    r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    Token::Function,
    1
);
rule!(
    MEMBER_RE,
    MEMBER,
    r"(?:\.|::)([A-Za-z_][A-Za-z0-9_]*)",
    Token::Property,
    1
);
rule!(
    DECORATOR_RE,
    DECORATOR,
    r"@([A-Za-z_][A-Za-z0-9_.]*)",
    Token::Decorator,
    1
);
rule!(
    RUST_ATTRIBUTE_RE,
    RUST_ATTRIBUTE,
    r"#!?\[\s*([A-Za-z_][A-Za-z0-9_]*)",
    Token::Decorator,
    1
);
rule!(
    REGEX_ASSIGN_RE,
    REGEX_ASSIGN,
    r"[=(:,!]\s*(/(?:\\.|[^/\\\n])+/[A-Za-z]*)",
    Token::Regex,
    1
);
rule!(
    REGEX_RETURN_RE,
    REGEX_RETURN,
    r"\b(?:return|=>)\s*(/(?:\\.|[^/\\\n])+/[A-Za-z]*)",
    Token::Regex,
    1
);
rule!(
    RUST_REGEX_RE,
    RUST_REGEX,
    r##"\bRegex::new\(\s*(r#*"[^"]*"#*)"##,
    Token::Regex,
    1
);
rule!(
    PY_REGEX_RE,
    PY_REGEX,
    r#"\bre\.(?:compile|match|search|fullmatch|findall|finditer|sub)\(\s*(r?"(?:\\.|[^"\\])*"|r?'(?:\\.|[^'\\])*')"#,
    Token::Regex,
    1
);

rule!(
    CSS_SELECTOR_CLASS_RE,
    CSS_SELECTOR_CLASS,
    r"[.#][A-Za-z_][A-Za-z0-9_-]*",
    Token::Selector,
    0
);
rule!(
    CSS_SELECTOR_PSEUDO_RE,
    CSS_SELECTOR_PSEUDO,
    r":{1,2}[A-Za-z-][A-Za-z0-9-]*",
    Token::Selector,
    0
);
rule!(
    CSS_SELECTOR_ELEMENT_RE,
    CSS_SELECTOR_ELEMENT,
    r"([A-Za-z][A-Za-z0-9_-]*)\s*(?:,|\{)",
    Token::Selector,
    1
);
rule!(
    CSS_UNIT_RE,
    CSS_UNIT,
    r"\b\d+(?:\.\d+)?(?:px|em|rem|vh|vw|%|s|ms|deg)\b",
    Token::Number,
    0
);
rule!(
    CSS_VALUE_RE,
    CSS_VALUE,
    r"(?:^|[;{])\s*[A-Za-z-]+\s*:\s*([^;{}]*[^;\s}])",
    Token::String,
    1
);

rule!(
    MD_LINK_TEXT_RE,
    MD_LINK_TEXT,
    r"\[([^\]]+)\]\(([^)]+)\)",
    Token::Constant,
    1
);
rule!(
    MD_LINK_URL_RE,
    MD_LINK_URL,
    r"\[([^\]]+)\]\(([^)]+)\)",
    Token::String,
    2
);
rule!(MD_ITALIC_RE, MD_ITALIC, r"\*[^*\n]+\*", Token::ControlKeyword, 0);

pub(super) const RUST_EXTRAS: &[RuleDef] = &[
    CONST_RULE, FUNCTION_DECL, TYPE_DECL, RUST_MACRO, RUST_ATTRIBUTE, RUST_REGEX, FUNCTION_CALL,
    MEMBER,
];
pub(super) const C_LIKE_EXTRAS: &[RuleDef] = &[
    CONST_RULE, FUNCTION_DECL, TYPE_DECL, DECORATOR, FUNCTION_CALL, MEMBER, REGEX_ASSIGN,
    REGEX_RETURN,
];
pub(super) const GO_EXTRAS: &[RuleDef] = &[CONST_RULE, FUNCTION_DECL, TYPE_DECL, FUNCTION_CALL, MEMBER];
pub(super) const PY_EXTRAS: &[RuleDef] = &[
    CONST_RULE, FUNCTION_DECL, TYPE_DECL, DECORATOR, PY_REGEX, FUNCTION_CALL, MEMBER,
];
pub(super) const SHELL_EXTRAS: &[RuleDef] = &[CONST_RULE, SHELL_FN, FUNCTION_CALL, MEMBER];
pub(super) const PS_EXTRAS: &[RuleDef] = &[CONST_RULE, POWERSHELL_FN, FUNCTION_CALL, MEMBER];
pub(super) const SQL_EXTRAS: &[RuleDef] = &[CONST_RULE, MEMBER];
pub(super) const MARKUP_EXTRAS: &[RuleDef] = &[MARKUP_TAG, MARKUP_ATTR];
pub(super) const CSS_EXTRAS: &[RuleDef] = &[
    CSS_SELECTOR_CLASS, CSS_SELECTOR_PSEUDO, CSS_SELECTOR_ELEMENT, CSS_PROPERTY, CSS_UNIT,
    CSS_VALUE, CSS_AT_RULE,
];
pub(super) const JSON_EXTRAS: &[RuleDef] = &[JSON_KEY];
pub(super) const YAML_EXTRAS: &[RuleDef] = &[YAML_KEY];
pub(super) const TOML_EXTRAS: &[RuleDef] = &[TOML_TABLE, TOML_KEY];
pub(super) const INI_EXTRAS: &[RuleDef] = &[INI_SECTION, INI_KEY];
pub(super) const DOTENV_EXTRAS: &[RuleDef] = &[DOTENV_KEY];
pub(super) const DOCKER_EXTRAS: &[RuleDef] = &[DOCKER_INSTRUCTION];
pub(super) const MAKE_EXTRAS: &[RuleDef] = &[MAKE_TARGET, MAKE_VAR];
pub(super) const MD_EXTRAS: &[RuleDef] = &[
    MD_HEADING, MD_CODE, MD_BOLD, MD_ITALIC, MD_LINK_TEXT, MD_LINK_URL,
];
pub(super) const NIX_EXTRAS: &[RuleDef] = &[CONST_RULE, NIX_ATTR, FUNCTION_CALL, MEMBER];
pub(super) const HCL_EXTRAS: &[RuleDef] = &[CONST_RULE, HCL_KEY, FUNCTION_CALL, MEMBER];
pub(super) const PROTO_EXTRAS: &[RuleDef] = &[CONST_RULE, TYPE_DECL, PROTO_FIELD, FUNCTION_CALL, MEMBER];
pub(super) const DASH_EXTRAS: &[RuleDef] = &[
    CONST_RULE, FUNCTION_DECL, TYPE_DECL, DASH_FN, FUNCTION_CALL, MEMBER,
];
pub(super) const CMAKE_EXTRAS: &[RuleDef] = &[CONST_RULE, CMAKE_CALL, CMAKE_VAR, MEMBER];
pub(super) const GRAPHQL_EXTRAS: &[RuleDef] = &[GRAPHQL_FIELD, GRAPHQL_TYPE_DECL, DECORATOR, FUNCTION_CALL];
pub(super) const LATEX_EXTRAS: &[RuleDef] = &[LATEX_COMMAND, LATEX_MATH];
pub(super) const PLAIN_EXTRAS: &[RuleDef] = &[CONST_RULE];

