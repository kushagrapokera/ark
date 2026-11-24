//! Path, filename and shebang language detection.

use std::path::Path;

use super::Lang;

/// Pick a static profile from the file name/extension.
pub fn lang_for(path: &Path) -> Lang {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") || name == "env" {
        return Lang::Dotenv;
    }
    if name == ".envrc"
        || name == ".bashrc"
        || name == ".bash_profile"
        || name == ".bash_login"
        || name == ".bash_aliases"
        || name == ".zshrc"
        || name == ".zprofile"
        || name == ".zshenv"
        || name == ".zlogin"
        || name == ".profile"
    {
        return Lang::Shell;
    }
    if name == "dockerfile" || name.starts_with("dockerfile.") || name == "containerfile" {
        return Lang::Dockerfile;
    }
    if name == "makefile"
        || name == "gnumakefile"
        || name == "justfile"
        || name.starts_with("makefile.")
    {
        return Lang::Makefile;
    }
    if name == "cmakelists.txt" {
        return Lang::Cmake;
    }
    if name == "meson.build"
        || name == "meson_options.txt"
        || name == "build"
        || name == "workspace"
        || name == "workspace.bazel"
    {
        return Lang::PythonLike;
    }
    if name == "go.mod" {
        return Lang::Go;
    }
    if name == "pipfile.lock" || name.ends_with(".tfstate") {
        return Lang::Json;
    }
    if name == "nginx.conf" || name == "httpd.conf" || name == "fstab" || name == "hosts" {
        return Lang::Ini;
    }
    if name == "cargo.lock" || name == "poetry.lock" || name == "pipfile" {
        return Lang::Toml;
    }
    if name == "gemfile"
        || name == "rakefile"
        || name == "vagrantfile"
        || name == "podfile"
        || name == "brewfile"
        || name == "sconstruct"
        || name == "sconscript"
    {
        return Lang::PythonLike;
    }
    if name == ".editorconfig"
        || name == ".gitconfig"
        || name == ".npmrc"
        || name == ".pypirc"
        || name == ".wgetrc"
        || name == ".curlrc"
        || name == ".inputrc"
    {
        return Lang::Ini;
    }
    if name == ".gitignore" || name == ".dockerignore" || name == ".npmignore" {
        return Lang::Plain;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "rs" => Lang::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "m" | "mm" | "cs" | "java"
        | "kt" | "kts" | "scala" | "swift" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx"
        | "php" | "phtml" | "groovy" | "gradle" | "sol" | "re" | "rei" | "d" => Lang::CLike,
        "go" => Lang::Go,
        "py" | "pyi" | "rb" | "rake" | "gemspec" | "pl" | "pm" | "r" | "jl" | "dart" | "ex"
        | "exs" | "cr" | "nim" | "bazel" => Lang::PythonLike,
        "sh" | "bash" | "zsh" | "fish" | "awk" => Lang::Shell,
        "ps1" | "psm1" | "psd1" => Lang::PowerShell,
        "sql" | "ddl" | "psql" => Lang::Sql,
        "html" | "htm" | "xml" | "xhtml" | "svg" | "vue" | "svelte" | "astro" | "hbs"
        | "handlebars" | "twig" | "ejs" | "jinja" | "j2" | "njk" | "cshtml" | "razor" | "aspx"
        | "ascx" | "jsp" | "erb" => Lang::Markup,
        "css" | "scss" | "sass" | "less" => Lang::Css,
        "json" | "jsonc" | "json5" | "jsonl" | "ndjson" | "geojson" | "har" | "webmanifest"
        | "ipynb" => Lang::Json,
        "yaml" | "yml" => Lang::Yaml,
        "toml" => Lang::Toml,
        "ini" | "cfg" | "conf" | "properties" => Lang::Ini,
        "md" | "markdown" | "mdx" | "rmd" | "qmd" => Lang::Markdown,
        "nix" => Lang::Nix,
        "tf" | "tfvars" | "hcl" => Lang::Hcl,
        "proto" => Lang::Protobuf,
        "lua" | "hs" | "lhs" | "elm" => Lang::DashLike,
        "cmake" => Lang::Cmake,
        "graphql" | "gql" => Lang::Graphql,
        "tex" | "latex" | "sty" | "cls" => Lang::Latex,
        "mk" => Lang::Makefile,
        _ => Lang::Plain,
    }
}

/// Detect from path/name first, then fall back to a shebang for extensionless
/// scripts. An explicit extension always wins.
pub fn lang_for_source(path: &Path, first_line: &str) -> Lang {
    let lang = lang_for(path);
    if lang != Lang::Plain {
        return lang;
    }
    lang_for_shebang(first_line).unwrap_or(lang)
}

fn lang_for_shebang(line: &str) -> Option<Lang> {
    let rest = line.trim_start().strip_prefix("#!")?.trim();
    let mut words = rest.split_whitespace();
    let mut command = words.next()?;
    if command.rsplit('/').next()? == "env" {
        command = words
            .find(|word| !word.starts_with('-'))
            .unwrap_or(command);
    }
    let name = command.rsplit('/').next()?.to_ascii_lowercase();
    let lang = match name.as_str() {
        "python" | "python2" | "python3" | "pypy" | "pypy3" | "ruby" | "perl" | "rscript"
        | "julia" => Lang::PythonLike,
        "node" | "nodejs" | "deno" | "bun" | "php" => Lang::CLike,
        "bash" | "sh" | "zsh" | "fish" | "dash" | "ksh" | "awk" => Lang::Shell,
        "pwsh" | "powershell" => Lang::PowerShell,
        "lua" => Lang::DashLike,
        _ => return None,
    };
    Some(lang)
}

