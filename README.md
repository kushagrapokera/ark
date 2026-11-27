# ark

ark is an ultra-lightweight, agent-first desktop code editor for Linux. Humans
view code while AI agents edit it. The only human-writable project surfaces are
whitelisted config files such as `.env`, YAML, and TOML.

ark is rendered entirely on the CPU with Rust-native windowing and software
rasterization. It does not embed a browser, webview, GPU renderer, or LSP.

## Features

- Read-only code viewer with syntax highlighting and text selection
- Real terminal tabs powered by PTYs
- Terminal text selection and clipboard copy
- Agent launcher tabs for tools such as OpenCode
- Live filesystem refresh
- Git status badges and per-file diff marks
- Commit and push panel
- Whitelisted `.env`, YAML, and TOML config editors
- Bundled fonts only
- Bounded caches and terminal scrollback

## Requirements

- Linux x86_64
- Wayland or X11
- `git` for repository features
- A supported shell such as `bash` or `fish`

Rust is only required for source/developer installs.

## Install

### Prebuilt release

Download the latest Linux release from:

<https://github.com/kushagrapokera/ark/releases>

The `0.1.0-beta` release adds a one-line installer:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/kushagrapokera/ark/releases/latest/download/ark-installer.sh | sh
```

### Developer install

```bash
cargo install --path . --locked
ark .
```

Or run directly:

```bash
cargo run --release -- .
```

The existing source installer is also available:

```bash
./install.sh --user
```

> KDE's archiver may be installed as `/usr/bin/ark`. A user-local install in
> `~/.local/bin` or `~/.cargo/bin` shadows it when that directory appears first
> on `PATH`.

## Usage

```bash
ark .
ark /path/to/project
ark --version
ark --help
```

### Terminal shortcuts

- `Ctrl+T` — open a new terminal tab
- Drag — select terminal text
- `Ctrl+Shift+C` — copy terminal selection
- `Ctrl+Shift+V` — paste into the terminal
- `Ctrl+C` — send SIGINT to the terminal application

## Updates

The initial beta supports downloading the latest release. The planned `0.1.0`
release adds an in-app **Update** button with verified download, atomic install,
and restart-to-finish behavior.

## Known limitations

- Linux x86_64 is the first supported platform.
- Code files are read-only for humans by design.
- No LSP integration.
- No GPU renderer or webview.
- Project configuration lives in `ark.toml` at the repository root.

## License

MIT. See [LICENSE](LICENSE).
