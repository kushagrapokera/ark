//! Static keyword, type and control-flow tables.

// ---- shared source-code keyword sets ----

pub(super) const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

pub(super) const RUST_TYPES: &[&str] = &[
    "bool", "char", "str", "String", "Vec", "Option", "Result", "Box", "Path", "PathBuf",
    "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize",
    "f32", "f64",
];

pub(super) const C_LIKE_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "as", "asm", "assert", "async", "await", "auto", "break", "case",
    "catch", "class", "const", "constexpr", "continue", "debugger", "default", "defer", "delete",
    "do", "else", "enum", "explicit", "export", "extends", "extern", "false", "final", "finally",
    "for", "friend", "from", "function", "goto", "if", "implements", "import", "in", "inline",
    "instanceof", "interface", "internal", "is", "let", "namespace", "new", "null", "of",
    "operator", "override", "package", "private", "protected", "public", "readonly", "return",
    "sealed", "sizeof", "static", "struct", "super", "switch", "template", "this", "throw",
    "throws", "transient", "true", "try", "type", "typedef", "typeof", "undefined", "using",
    "var", "virtual", "void", "volatile", "while", "with", "yield",
];

pub(super) const C_LIKE_TYPES: &[&str] = &[
    "bool", "byte", "char", "double", "float", "int", "long", "short", "signed", "unsigned",
    "void", "size_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
    "uint32_t", "uint64_t", "string", "String", "Object", "Exception", "number", "boolean",
    "any", "unknown", "never",
];

pub(super) const GO_KEYWORDS: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range",
    "return", "select", "struct", "switch", "type", "var", "true", "false", "nil",
];

pub(super) const GO_TYPES: &[&str] = &[
    "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8",
    "int16", "int32", "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64",
    "uintptr",
];

pub(super) const PYTHON_LIKE_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "case", "class", "continue", "def",
    "del", "do", "elif", "else", "elsif", "end", "ensure", "except", "false", "False",
    "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "module",
    "nonlocal", "None", "not", "or", "pass", "raise", "redo", "require", "rescue", "retry",
    "return", "self", "then", "True", "try", "undef", "unless", "until", "when", "while",
    "with", "yield",
];

pub(super) const PYTHON_LIKE_TYPES: &[&str] = &[
    "int", "float", "complex", "str", "bool", "list", "tuple", "dict", "set", "frozenset",
    "bytes", "bytearray", "object", "type", "NoneType", "Array", "Hash", "Symbol",
];

pub(super) const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
    "function", "return", "local", "export", "source", "in", "select", "until", "time",
    "coproc", "begin", "end", "switch",
];

pub(super) const POWERSHELL_KEYWORDS: &[&str] = &[
    "begin", "break", "catch", "class", "continue", "data", "define", "do", "dynamicparam",
    "else", "elseif", "end", "enum", "exit", "filter", "finally", "for", "foreach", "from",
    "function", "hidden", "if", "in", "param", "process", "return", "static", "switch",
    "throw", "trap", "try", "until", "using", "var", "while", "workflow",
];

pub(super) const SQL_KEYWORDS: &[&str] = &[
    "add", "all", "alter", "and", "any", "as", "asc", "begin", "between", "by", "case", "check",
    "column", "commit", "constraint", "create", "cross", "database", "default", "delete", "desc",
    "distinct", "drop", "else", "end", "exists", "false", "foreign", "from", "full", "group",
    "having", "if", "in", "index", "inner", "insert", "into", "is", "join", "key", "left",
    "like", "limit", "not", "null", "offset", "on", "or", "order", "outer", "primary",
    "references", "right", "rollback", "select", "set", "table", "then", "true", "union",
    "unique", "update", "values", "view", "when", "where", "with",
];

pub(super) const SQL_TYPES: &[&str] = &[
    "bigint", "binary", "bit", "blob", "boolean", "char", "date", "datetime", "decimal",
    "double", "float", "int", "integer", "json", "numeric", "real", "smallint", "text", "time",
    "timestamp", "varbinary", "varchar",
];

pub(super) const YAML_KEYWORDS: &[&str] = &["true", "false", "null", "yes", "no", "on", "off"];

pub(super) const DOCKER_KEYWORDS: &[&str] = &[
    "FROM", "RUN", "CMD", "LABEL", "MAINTAINER", "EXPOSE", "ENV", "ADD", "COPY", "ENTRYPOINT",
    "VOLUME", "USER", "WORKDIR", "ARG", "ONBUILD", "STOPSIGNAL", "HEALTHCHECK", "SHELL",
];

pub(super) const NIX_KEYWORDS: &[&str] = &[
    "assert", "else", "if", "in", "inherit", "let", "or", "rec", "then", "with",
];

pub(super) const HCL_KEYWORDS: &[&str] = &["true", "false", "null"];

pub(super) const PROTO_KEYWORDS: &[&str] = &[
    "syntax", "package", "import", "option", "message", "enum", "service", "rpc", "returns",
    "repeated", "optional", "required", "oneof", "map", "reserved", "extend", "stream",
];

pub(super) const PROTO_TYPES: &[&str] = &[
    "double", "float", "int32", "int64", "uint32", "uint64", "sint32", "sint64", "fixed32",
    "fixed64", "sfixed32", "sfixed64", "bool", "string", "bytes",
];

pub(super) const DASH_LIKE_KEYWORDS: &[&str] = &[
    "and", "case", "class", "data", "deriving", "do", "else", "if", "import", "in", "instance",
    "let", "module", "of", "or", "then", "type", "where", "function", "local", "nil", "not",
    "repeat", "return", "until", "while",
];

pub(super) const CMAKE_KEYWORDS: &[&str] = &[
    "add_custom_command", "add_custom_target", "add_executable", "add_library", "add_subdirectory",
    "break", "cmake_minimum_required", "configure_file", "continue", "else", "elseif", "endforeach",
    "endfunction", "endif", "endmacro", "endwhile", "false", "file", "find_package", "foreach",
    "function", "if", "include", "install", "list", "macro", "message", "on", "off", "option",
    "project", "return", "set", "string", "target_include_directories", "target_link_libraries",
    "true", "while",
];

pub(super) const GRAPHQL_KEYWORDS: &[&str] = &[
    "directive", "enum", "extend", "false", "fragment", "implements", "input", "interface",
    "mutation", "null", "on", "query", "scalar", "schema", "subscription", "true", "type", "union",
];

pub(super) const GRAPHQL_TYPES: &[&str] = &["Boolean", "Float", "ID", "Int", "String"];

// ---- control-flow keywords (VS Code Dark+ orchid) ----

pub(super) const RUST_CONTROL: &[&str] = &[
    "await", "break", "continue", "else", "for", "if", "in", "loop", "match", "return", "while",
    "yield",
];

pub(super) const C_LIKE_CONTROL: &[&str] = &[
    "await", "break", "case", "catch", "continue", "default", "do", "else", "finally", "for",
    "goto", "if", "return", "switch", "throw", "try", "while", "with", "yield",
];

pub(super) const GO_CONTROL: &[&str] = &[
    "break", "case", "continue", "default", "defer", "else", "fallthrough", "for", "go", "goto",
    "if", "range", "return", "select",
];

pub(super) const PYTHON_LIKE_CONTROL: &[&str] = &[
    "await", "break", "case", "continue", "do", "elif", "else", "elsif", "end", "ensure", "except",
    "finally", "for", "if", "in", "match", "raise", "redo", "rescue", "retry", "return", "then",
    "try", "unless", "until", "when", "while", "with", "yield",
];

pub(super) const SHELL_CONTROL: &[&str] = &[
    "begin", "case", "do", "done", "elif", "else", "end", "esac", "fi", "for", "if", "in",
    "return", "select", "switch", "then", "time", "until", "while",
];

pub(super) const POWERSHELL_CONTROL: &[&str] = &[
    "begin", "break", "catch", "continue", "do", "else", "elseif", "end", "exit", "filter",
    "finally", "for", "foreach", "if", "in", "process", "return", "switch", "throw", "trap", "try",
    "until", "while",
];

pub(super) const SQL_CONTROL: &[&str] = &["case", "else", "end", "then", "when"];
pub(super) const DASH_CONTROL: &[&str] = &["case", "do", "else", "if", "of", "then", "when"];
pub(super) const NIX_CONTROL: &[&str] = &["else", "if", "in", "then"];
pub(super) const CMAKE_CONTROL: &[&str] = &[
    "break", "continue", "else", "elseif", "endforeach", "endif", "endwhile", "foreach", "if",
    "return", "while",
];
pub(super) const GRAPHQL_CONTROL: &[&str] = &["on"];

