# Syntax highlighting research (Tier 1 vs Tier 3)

Date: 2026-11-23
Budget: ark process RSS ≤ 100 MiB

## Requirement

- Match the VS Code Dark+ palette.
- Cover every language/framework and config file type, including `.env`,
  TOML, YAML, JSON, INI, Dockerfile, and Makefile.
- Add no meaningful memory cost.

## What VS Code actually does

VS Code does not ship a zero-cost universal highlighter. Its lexical coloring
comes from TextMate grammars: ordered Oniguruma regex rules with begin/end
contexts and a per-line rule stack. That stack is what allows strings, nested
comments, and embedded languages to span lines. On top of lexical tokens, VS
Code can add semantic tokens from language servers, which distinguish
parameters, properties, locals, and globals. Semantic highlighting is
optional and arrives asynchronously; it is not part of the base tokenizer.

Monaco and `vscode-textmate` tokenize line by line and carry the rule stack
across lines. This is why VS Code grammars are regex-heavy and why their
behavior is hard to reproduce with a flat per-line regex pass.

Consequence: exact VS Code behavior for every language means shipping or
loading a grammar ecosystem (or parsers), plus an Oniguruma-compatible regex
engine. That cannot be free.

## Tier 1 / 1.5: static profiles + bounded state (implemented)

`src/highlight.rs` and `src/highlight/` now provide 25 static profiles selected
by file name, extension, or shebang:

- Rust; C/C++/C#/Java/Kotlin/Scala/Swift/JS/TS/JSX/TSX/PHP/D (C-like); Go
- Python/Ruby/Perl/R/Julia/Dart/Elixir/Crystal/Nim (Python-like); shell;
  PowerShell; SQL
- HTML/XML/Vue/Svelte/Astro/Handlebars/Twig/EJS/Jinja (markup);
  CSS/SCSS/Sass/Less
- JSON/JSONL; YAML; TOML (including `Cargo.lock`/`Pipfile`); INI (including
  `.gitconfig`/`.npmrc`); `.env`; Dockerfile; Makefile/Justfile; Markdown
- Nix; Terraform/HCL; Protobuf; CMake; GraphQL; LaTeX; Lua/Haskell/Elm;
  plain-text fallback

Each profile is a static table of control/declaration keywords, types,
comment family, and file-specific capture rules. Tier 1.5 adds VS Code-style
variables, function calls, members/constants, regex/escape/decorator tokens,
CSS selectors/values, Markdown links, current-line/indent/bracket decorations,
and syntax coloring in the writable YAML/TOML config textarea. There is still
no AST, grammar bundle, whole-file token cache, or per-file state cache.

Multiline block comments, Python triple strings, Markdown fences, and HTML
`<script>`/`<style>` blocks use one caller-owned state value. The viewer scans
back at most 256 lines to reconstruct state, then carries it through only the
visible rows. The bracket stack is capped at 256 and indent guides at 64 levels.

Memory design:

- One shared compiled regex per distinct pattern (comment family, string,
  number, file-specific rule), not one copy per language.
- Keyword/type matching scans the identifier and checks static slices; it does
  not build one large alternation regex per language.
- Profiles are compiled on first use and never evicted. Unused profiles cost an
  empty cell.

Measured cost (release build, Linux `/proc` RSS, ignored probe
`highlight::tests::all_profiles_fit_memory_budget`):

- Compiling all 25 profiles: **+3.2 MiB RSS** (3,324 KiB).
- Typical first code file: **~2 MiB** (first profile also initializes the
  shared string/number/comment regexes).
- Full workbench stress (80k files, 5 large viewers, 2 shells): **59.6 MiB
  RSS / 53.4 MiB PSS**, under the 100 MiB budget. The manual run verified all
  five `big0*.txt` viewer opens and both terminal tabs before measuring.

This is "no meaningful cost" against a 100 MiB budget, not literally zero.
It is still heuristic rather than scope-exact: complex embedded grammars,
capture-dependent delimiters, semantic locals/parameters/properties, and rare
regex constructs remain approximate. Common code and config structures now
look substantially closer to VS Code Dark+ without a grammar engine.

## Tier 3: exact grammar fidelity

### 3a. Syntect + VS Code TextMate grammars

Probe: `/tmp/tier3-probe` (`syntect 5.3` with the `onig` backend, system
libonig 5, plus `syntect-tmlanguage 0.1`). Grammars read from the installed VS
Code stable extensions directory. Test corpus: 7.7 MB / 200k-line Rust file,
streamed one line at a time (no whole-file string retained).

| Scenario | RSS delta | Peak RSS delta |
| --- | ---: | ---: |
| 1 Rust grammar + 7 themes + stream 200k lines | +1.6 MiB | +1.6 MiB |
| 31 translatable VS Code grammars + 7 themes | +11.6 MiB | +13.6 MiB |

The installed VS Code tree contains 84 `.tmLanguage.json` grammars (85
grammar/plist files) totaling **5.19 MiB on disk**. Only 31 of 84 translated:
53 were rejected, mostly because they use `\G` anchors that syntect's Onig
backend cannot honor. Markdown also hit unsupported `begin`/`while` rules, and
some grammars failed JSON/YAML translation. TypeScript and JavaScript are
among the rejected grammars, so this path cannot deliver the requested
VS Code parity even if its ~12-14 MiB cost were accepted.

A single grammar is cheap (~1.6 MiB), which suggests on-demand loading plus
eviction. But on-demand loading does not fix the unsupported-rule problem.
`syntect-tmlanguage` is also new (v0.1.0) and introduces an Oniguruma C
dependency (or the slower pure-Rust `fancy-regex` backend).

### 3b. Tree-sitter

Probe: `/tmp/tree-probe` (`tree-sitter 0.27`, `tree-sitter-rust 0.24`),
full-file parse of the same generated Rust source:

| Lines parsed | RSS delta | Parse time |
| ---: | ---: | ---: |
| 100 | +1.0 MiB | 0.6 ms |
| 1,000 | +2.3 MiB | 5 ms |
| 10,000 | +15.1 MiB | 55 ms |
| 200,000 | +285.1 MiB | 1.12 s |

The parse tree is the cost: it scales with node count (~16.5 nodes per line in
this sample). Parsing only a viewport is cheap, but a viewport parse cannot
reconstruct enclosing scopes reliably, and each language needs its own
compiled grammar crate. Full-file parsing is incompatible with the 100 MiB
budget on kernel-scale files.

### 3c. Why not pre-tokenize files?

Pre-tokenizing on open still needs the grammar engine and a cache proportional
to file size/token count; for a 200k-line file the token spans alone exceed the
Tier-1 total. It also goes stale on every edit. This trades memory for latency
without removing the parser dependency.

## Conclusion

- **Tier 1 is the correct default for ark.** It adds ~2 MiB typical / 3.2 MiB
  worst case, covers common languages and config files, and keeps the
  workbench well under budget.
- **Tier 3 cannot be sold as "no extra memory".** A single grammar is ~1.6 MiB;
  the translatable subset of VS Code's own grammars is ~12-14 MiB and still
  misses TypeScript/JavaScript. Full-file tree-sitter parsing is 15 MiB at
  10k lines and 285 MiB at 200k lines.
- If grammar fidelity later becomes a hard requirement, the only honest options
  are: (a) a bounded, per-grammar TextMate engine with a different Oniguruma
  binding that supports `\G`, loaded on demand with a hard cap and eviction, or
  (b) viewport-scoped tree-sitter with grammars only for selected languages.
  Both need explicit dependency and memory approval; neither is zero-cost.

## Reproduce

```sh
# Tier 1 profile budget
cargo test --release highlight::tests::all_profiles_fit_memory_budget -- --ignored --nocapture

# Tier 1 full workbench stress
cargo build --release
DIRS=80 FILES_PER_DIR=1000 BIG_LINES=200000 ./implementation/m11-memory-proof/stress.sh

# Tier 3 TextMate probe (temporary project)
cd /tmp/tier3-probe && cargo run --release -- one   # one Rust grammar
cd /tmp/tier3-probe && cargo run --release -- all   # all installed VS Code grammars

# Tier 3 tree-sitter probe
cd /tmp/tree-probe && cargo run --release -- /tmp/tier3-sample.rs
```

## Sources

- VS Code syntax highlighting guide:
  <https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide>
- VS Code semantic highlighting overview:
  <https://github.com/microsoft/vscode-wiki/blob/main/Semantic-Highlighting-Overview.md>
- Monaco tokenizer/Monarch documentation:
  <https://microsoft.github.io/monaco-editor/monarch.html>
- Line-based tokenization and rule-state notes:
  <https://mjtsai.com/blog/2017/02/13/optimizations-in-syntax-highlighting/>
- `vscode-textmate` (line-based tokenizer, rule stacks):
  <https://github.com/microsoft/vscode-textmate>
- VS Code built-in grammars:
  <https://github.com/microsoft/vscode/tree/main/extensions>
- Syntect (TextMate-style engine, Oniguruma backend):
  <https://docs.rs/syntect>
- `syntect-tmlanguage` (VS Code grammar loader):
  <https://docs.rs/syntect-tmlanguage>
- Tree-sitter multi-language memory report:
  <https://github.com/tree-sitter/tree-sitter/issues/5547>
- Tree-sitter (parse trees and incremental parsing):
  <https://tree-sitter.github.io/tree-sitter/>
