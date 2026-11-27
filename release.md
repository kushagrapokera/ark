# ark Linux release and click-to-update plan

_Created: 2026-11-27. Baseline: `99c87bb`._

## Goal

Ship ark to Linux users with:

1. A one-line curl installation that does not require Rust.
2. A normal desktop application entry and icon.
3. An in-app **Update** button.
4. Verified downloads and atomic installation.
5. A restart flow that launches the newly installed version.
6. Rollback to the previous version if installation or startup fails.

The first release targets **Linux x86_64**, user-local installation, and GitHub Releases.

---

## Scope

### In scope

- Linux x86_64 GNU build.
- Prebuilt release tarball.
- `curl | sh` installer.
- Versioned user-local installation.
- Desktop entry and icon.
- Signed update manifest.
- Separate `ark-updater` helper process.
- Update badge/button, release notes, and progress UI.
- Stable update channel.
- Checksum/signature verification.
- Atomic version switching.
- Previous-version retention and manual rollback.
- GitHub Actions release pipeline.
- Clean-system Linux smoke tests.

### Out of scope for the first release

- Windows and macOS.
- `.deb`, Flatpak, Snap, AppImage, AUR, Homebrew, winget.
- aarch64 builds unless added after the x86_64 pipeline is proven.
- Delta updates.
- Automatic restart.
- Automatic rollback before first frame.
- Beta channels and phased rollout.
- Telemetry or user tracking.
- Root/system installation.
- Replacing the frozen rendering/windowing stack.

---

## Assumptions and decisions

- GitHub Releases hosts the public artifacts.
- The repository will be public, or release assets will be hosted on an equivalent public HTTPS server.
- Users install without sudo under their home directory.
- The main ark process must remain under the 100 MB RSS budget.
- Networking, TLS, extraction, and signature verification live in a separate helper process.
- Updates install only after the user clicks **Update**.
- ark never restarts automatically; it prompts the user to restart.
- The first public release can be `0.1.0-beta`; `0.1.0` follows after updater hardening.
- New dependencies require explicit user approval and a RAM justification.

---

## Current release blockers

Phase 0–5 implementation is complete: packaging, curl bootstrap, updater
helper, update UI, signed release automation, and the supplied 256×256 icon.
Remaining release work:

- Public publication approval and the exact public curl-command check.
- Ubuntu 24.04 and Fedora Workstation clean-system verification.

---

# Architecture

```text
Developer machine / GitHub Actions
              │
              ├── build ark
              ├── build ark-updater
              ├── package tarball
              ├── generate SHA256SUMS
              ├── sign artifacts
              ├── generate latest.json
              └── publish GitHub Release
                          │
                          ▼
                 curl installer
                          │
                          ▼
          versioned user-local installation
                          │
                          ▼
                 ark UI process
                          │
                          ├── check for updates
                          ├── show Update button
                          └── launch ark-updater
                                      │
                                      ├── download
                                      ├── verify
                                      ├── stage
                                      └── switch current
                                                  │
                                                  ▼
                                       user restarts ark
                                                  │
                                                  ▼
                                       new version launches
```

---

# Installation layout

Use XDG-compatible paths:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/ark/
├── versions/
│   ├── 0.1.0/
│   │   ├── ark
│   │   └── ark-updater
│   └── 0.1.1/
│       ├── ark
│       └── ark-updater
└── current -> versions/0.1.1

${XDG_BIN_HOME:-$HOME/.local/bin}/ark
    -> ${XDG_DATA_HOME}/ark/current/ark

${XDG_DATA_HOME}/applications/ark.desktop
${XDG_DATA_HOME}/icons/hicolor/256x256/apps/ark.png

${XDG_CONFIG_HOME:-$HOME/.config}/ark/updater.toml
${XDG_STATE_HOME:-$HOME/.local/state}/ark/install.json
${XDG_STATE_HOME:-$HOME/.local/state}/ark/updater/last-check
${XDG_CACHE_HOME:-$HOME/.cache}/ark/updater/
```

The running executable is never overwritten. Updating changes the `current` symlink atomically.

---

# Release artifact layout

```text
ark-0.1.0-linux-x86_64/
├── ark
├── ark-updater
├── ark.desktop
├── icons/
│   └── ark.png
├── install-release.sh
├── uninstall.sh
├── README.md
├── LICENSE
└── CHANGELOG.md
```

Published assets:

```text
ark-0.1.0-linux-x86_64.tar.gz
SHA256SUMS
SHA256SUMS.sig
ark-installer.sh
ark-installer.sh.sig
latest.json
latest.json.sig
```

---

# User-facing commands

## Install

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/<owner>/ark/releases/latest/download/ark-installer.sh | sh
```

Version-pinned, auditable variant:

```bash
curl -fsSL https://github.com/<owner>/ark/releases/download/v0.1.0/ark-installer.sh \
  -o /tmp/ark-install.sh
less /tmp/ark-install.sh
sh /tmp/ark-install.sh
```

## Run

```bash
ark .
```

## Update

1. ark shows **Update available**.
2. User clicks **Update**.
3. User sees version, release notes, size, and progress.
4. ark downloads, verifies, stages, and switches `current`.
5. ark shows **Restart to finish**.
6. User restarts ark.
7. The new version launches.

## Rollback

```bash
ark-updater rollback
```

The previous version remains installed until rollback or cleanup.

---

# Interfaces

## `src/update.rs`

Dependency-light shared module used by ark and `ark-updater`.

Required types/responsibilities:

- `Version`: parse and compare `major.minor.patch`.
- `TargetTriple`: select supported Linux architecture.
- `InstallLayout`: resolve XDG install/state/cache paths.
- `InstallMetadata`: version, architecture, channel, manifest URL.
- `UpdateState`: idle, checking, available, downloading, verifying, staged, ready, failed.
- `UpdateError`: offline, rate-limited, unsupported platform, invalid manifest, signature invalid, checksum mismatch, oversized download, insufficient disk, permission denied, install failed, rollback failed.
- `ReleaseManifest`: validated in-memory representation.
- Pure helpers for version comparison, target selection, and path validation.

This module must not import HTTP, TLS, archive, or signature crates.

## `ark-updater` CLI

```bash
ark-updater check --manifest-url <url> --channel stable
ark-updater stage --manifest-url <url> --channel stable
ark-updater apply --version <version>
ark-updater rollback
ark-updater --version
```

The helper writes a line-oriented event protocol to stdout:

```text
event: check-start
event: update-available
version: 0.1.1
size: 4212334
notes-url: https://...
event: progress
percent: 42
event: staged
version: 0.1.1
event: ready
event: error
code: signature-invalid
message: update signature verification failed
```

ark reads stdout incrementally and updates the UI. stderr is logged. No shell interpolation is used.

## `latest.json`

```json
{
  "version": "0.1.1",
  "channel": "stable",
  "published_at": "2026-11-28T00:00:00Z",
  "minimum_version": "0.1.0",
  "critical": false,
  "notes_url": "https://github.com/<owner>/ark/releases/tag/v0.1.1",
  "assets": {
    "x86_64-unknown-linux-gnu": {
      "url": "https://github.com/<owner>/ark/releases/download/v0.1.1/ark-0.1.1-linux-x86_64.tar.gz",
      "size": 4212334,
      "sha256": "...",
      "signature": "..."
    }
  }
}
```

Validation rules:

- version is valid semver and newer than installed version;
- channel matches the selected channel;
- target asset exists;
- URL is HTTPS;
- size is positive and below the configured cap;
- SHA-256 is well-formed;
- signature is present for stable releases;
- notes URL is HTTPS.

## `install.json`

```json
{
  "version": "0.1.0",
  "arch": "x86_64",
  "channel": "stable",
  "manifest_url": "https://github.com/<owner>/ark/releases/latest/download/latest.json",
  "installed_at": "2026-11-27T00:00:00Z"
}
```

---

# Work plan

## Phase 0 — Repository and release hygiene

### Files

- `README.md`
- `LICENSE`
- `CHANGELOG.md`
- `assets/icons/ark.png`
- `Cargo.toml`

### Tasks

- [x] Add README with install, run, update, uninstall, requirements, shortcuts, and known limitations.
- [x] Add MIT license text.
- [x] Add changelog with `0.1.0-beta` section.
- [x] Add 256×256 PNG icon.
- [x] Add Cargo metadata: readme, license, repository, authors, keywords, categories.
- [x] Create GitHub repository and configure `origin`.
- [x] Keep the `ark` command name; document the KDE `/usr/bin/ark` conflict.

### Verification

- `cargo metadata --no-deps --format-version 1` includes release metadata.
- README commands match actual behavior.
- Icon is a valid 256×256 PNG.
- `cargo test` still passes.

### Evidence — 2026-11-27

- `cargo test`: 404 passed, 0 failed, 5 ignored; 5/5 CLI tests passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release`: succeeded.
- Cargo metadata reports `0.1.0-beta`, MIT, README, and repository URL.
- `git diff --check`: clean.
- Remaining Phase 0 item: user-provided 256×256 icon.

---

## Phase 1 — Versioned release installer

### Files

- `packaging/ark.desktop`
- `packaging/install-release.sh`
- `packaging/uninstall.sh`
- `packaging/build-release.sh`
- `packaging/verify-release.sh`
- `tests/installer.sh`

### Tasks

- [x] Create desktop entry with `Exec=ark %f`, `Icon=ark`, and `StartupWMClass=ark`.
- [x] Implement POSIX `install-release.sh` using `#!/bin/sh` and `set -eu`.
- [x] Detect unsupported operating systems and architectures.
- [x] Smoke-test bundled `ark --version`.
- [x] Stage installation under the XDG data directory.
- [x] Install into `versions/<version>`.
- [x] Atomically update `current`.
- [x] Create `~/.local/bin/ark` symlink.
- [x] Install desktop entry with absolute `Exec`.
- [x] Install icon when present; real icon asset remains pending.
- [x] Write `install.json`.
- [x] Preserve the previous version.
- [x] Print PATH instructions when necessary.
- [x] Implement uninstall with optional state/cache cleanup.
- [x] Implement tarball builder.
- [x] Implement release verifier.

### Acceptance

- Installing into a temporary HOME works without Cargo.
- `ark --version` works through the symlink.
- Desktop entry validates with `desktop-file-validate` when available.
- Reinstall replaces the same version safely.
- Failed smoke test leaves `current` unchanged.
- Uninstall removes ark-created files without touching project files.

### Evidence — 2026-11-27

- `tests/installer.sh`: passed, including reinstall, upgrade retention, broken
  bundle rejection, unsupported architecture, checksum corruption, and purge.
- `sh -n packaging/*.sh tests/installer.sh`: passed.
- `desktop-file-validate packaging/ark.desktop`: passed.
- Built and verified a real `0.1.0-beta` x86_64 tarball.
- Installed the tarball into a temporary HOME and ran `ark --version`.
- Ran the installed binary with `ARK_SMOKE=1`; first frame presented successfully.
- Uninstalled the smoke install successfully.
- `cargo test`: 404 passed, 0 failed, 5 ignored; 5/5 CLI tests passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release`: succeeded.
- `shellcheck` was not installed on this machine.

---

## Phase 2 — Curl bootstrap installer

### Files

- `packaging/ark-installer.sh.in`
- `packaging/build-release.sh`
- `tests/curl-installer.sh`
- `.github/workflows/release.yml` _(upload wiring deferred to Phase 5)_

### Tasks

- [x] Create POSIX shell template; no Bash-only syntax.
- [x] Bake repository, version, architecture, and checksum into the generated script.
- [x] Support `curl` or `wget`.
- [x] Map `x86_64` and reject unsupported architectures.
- [x] Download tarball to a temporary directory.
- [x] Verify baked SHA-256 or signed `SHA256SUMS`.
- [x] Extract with `tar`.
- [x] Run bundled `install-release.sh`.
- [x] Clean temporary files on success and failure.
- [x] Support `ARK_BASE_URL` for local integration tests.
- [ ] Generate and upload `ark-installer.sh` per release _(generation complete;
  upload workflow deferred to Phase 5 pending the real icon and signing)_.

### Acceptance

- Local fake release installs through the generated script.
- Corrupted tarball is rejected before installation.
- Unsupported architecture fails clearly.
- Missing `tar`, `curl`/`wget`, or `sha256sum` produces actionable errors.
- Published command works on a clean Ubuntu system without Rust _(pending
  public release and real icon)_.

### Evidence — 2026-11-27

- `tests/curl-installer.sh`: passed for curl, wget, corrupt downloads,
  unsupported architecture, missing downloader, and temp cleanup.
- `tests/installer.sh`: still passes.
- `sh -n packaging/*.sh tests/*.sh`: passed.
- Built a real `0.1.0-beta` tarball and generated `ark-installer.sh`.
- Ran the exact `curl ... | sh` flow against a local HTTP release server.
- Installed binary reported `ark 0.1.0-beta`.
- Installed binary completed `ARK_SMOKE=1` first-frame smoke.
- `cargo test`: 404 passed, 0 failed, 5 ignored; 5/5 CLI tests passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release`: succeeded.
- `shellcheck` was not installed on this machine.

---

## Phase 3 — Updater shared model and helper

### Files

- `src/update.rs`
- `src/update/archive.rs`
- `src/update/error.rs`
- `src/update/install.rs`
- `src/update/json.rs`
- `src/update/layout.rs`
- `src/update/manifest.rs`
- `src/update/tests.rs`
- `src/update/version.rs`
- `src/lib.rs`
- `src/bin/ark-updater/main.rs`
- `src/bin/ark-updater/archive.rs`
- `src/bin/ark-updater/download.rs`
- `src/bin/ark-updater/install.rs`
- `src/bin/ark-updater/state.rs`
- `Cargo.toml`
- `packaging/build-release.sh`
- `packaging/verify-release.sh`
- `tests/installer.sh`
- `tests/curl-installer.sh`
- `tests/update.sh`

### Dependency gate

No new Cargo dependencies were added. The helper uses the same Linux system
tools as the installer (`curl`/`wget`, `tar`, `sha256sum`) plus OpenSSL for
Ed25519 verification. The shared model includes a bounded, dependency-free
JSON parser.

The updater target is feature-gated so normal ark builds remain unchanged. The
main ark binary does not link updater networking or archive code.

### Tasks

- [x] Add pure version parsing/comparison tests first.
- [x] Add target selection tests.
- [x] Add XDG path resolution tests.
- [x] Add manifest validation tests.
- [x] Add archive path-safety tests.
- [x] Implement `ark-updater check`.
- [x] Implement `ark-updater stage`.
- [x] Implement `ark-updater apply`.
- [x] Implement `ark-updater rollback`.
- [x] Enforce download-size and disk-space limits.
- [x] Verify signature and checksum before extraction.
- [x] Extract into a same-filesystem staging directory.
- [x] Smoke-test staged binaries.
- [x] Switch `current` atomically.
- [x] Retain previous version.
- [x] Require `ark-updater` in release tarballs and verify its `--version`.
- [x] Clean stale download/cache files with a bounded retention policy.

### Acceptance

- Unit tests cover malformed manifests, downgrade attempts, unsupported targets, oversized downloads, traversal paths, and invalid signatures.
- A local HTTP release can be checked, staged, applied, and rolled back.
- Failed verification never changes `current`.
- Interrupted downloads leave the installed version untouched.
- Release tarballs include an executable `ark-updater`.
- Main ark binary builds without updater features.

### Evidence — 2026-11-27

- Nine shared updater unit tests passed.
- `tests/installer.sh`: passed, including rejection of a release bundle without
  `ark-updater` and verification of both packaged executables.
- `tests/curl-installer.sh`: passed with the helper included in the tarball.
- `tests/update.sh` passed with both curl and wget: local HTTP check, stage,
  apply, rollback, apply-time downgrade rejection, outside-staging rejection,
  apply/rollback metadata-write recovery, repeated rollback retention, invalid
  signature, invalid checksum, size mismatch, oversized download, traversal
  archive, downgrade check, and failed download.
- `cargo test`: 413 unit tests passed, 0 failed, 5 ignored; 5/5 CLI tests
  passed. `cargo test --all-features --all-targets` also passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: clean.
- `cargo build --release --locked --bin ark`: succeeded without updater features.
- `cargo build --release --locked --bin ark-updater --features updater`:
  succeeded.
- Built and verified a real `0.1.0-beta` tarball, installed it into a temporary
  HOME, and ran both `ark --version` and `ark-updater --version`.
- First-frame `ARK_SMOKE=1` run exited successfully with 23,548 KiB maximum RSS
  (Python `resource.getrusage`); GNU `time` and `smem` were unavailable.
- No new Cargo dependencies were added.

---

## Phase 4 — Update UI

### Files

- `src/main.rs`
- `src/frame.rs`
- `src/frame/chrome.rs`
- `src/frame/update.rs`
- `src/frame/update/tests.rs`
- `src/frame/tests.rs`
- `src/update.rs`
- `src/update/state.rs`
- `src/update_process.rs`
- `src/update_process/rate.rs`
- `src/update_process/runner.rs`
- `src/update_process/tests.rs`
- `src/update_protocol.rs`
- `src/icons.rs`
- `src/watch.rs`
- `src/tests.rs`

### Tasks

- [x] Add update state to `App`.
- [x] Add title-bar update badge/button.
- [x] Check once shortly after startup.
- [x] Store last successful check in XDG state.
- [x] Rate-limit automatic checks to once per 24 hours.
- [x] Add manual **Check for updates** action.
- [x] Spawn `ark-updater` without blocking the UI thread.
- [x] Parse line-oriented helper events.
- [x] Show version, release notes URL, download size, and progress.
- [x] Add **Install Update**, **Update Later**, and **Restart to finish** states.
- [x] Show actionable errors.
- [x] Warn when terminals/agents are running.
- [x] Flush config autosave before restart.
- [x] Never restart automatically.

### Acceptance

- UI remains responsive during download.
- Progress is bounded and monotonic.
- Offline/rate-limited errors do not block normal use.
- Failed update keeps the old version selected.
- Update button is inaccessible or disabled for unsupported install layouts.
- Existing frame interaction tests still pass.

### Evidence — 2026-11-27

- Protocol/state/controller/frame/App tests cover event parsing, bounded lines,
  monotonic progress, stale generations, 24-hour rate limiting, downgrade/error
  handling, title/panel actions, unsupported layouts, and restart autosave.
- `cargo test`: 441 unit tests passed, 0 failed, 5 ignored; 5/5 CLI tests passed.
- `cargo test --all-features --all-targets`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: clean.
- Locked release builds for ark and `ark-updater` succeeded.
- Built, verified, and installed a real `0.1.0-beta` tarball.
- Installed-copy smoke with a fake helper observed the startup
  `UpdateAvailable` event, wrote `last-check`, and suppressed a second launch's
  automatic check inside 24 hours.
- First-frame `ARK_SMOKE=1` run exited successfully with 23,788 KiB maximum RSS
  (Python `resource.getrusage`); GNU `time` and `smem` were unavailable.
- No new Cargo dependencies were added.

---

## Phase 5 — Signing and release automation

### Files

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `packaging/sign-release.sh`
- `packaging/verify-release.sh`
- `src/bin/ark-updater/main.rs`
- `src/bin/ark-updater/download.rs`
- `tests/signing.sh`
- `tests/embedded-key.sh`
- `tests/update.sh`
- `.gitignore`

### Tasks

- [x] Add PR CI for tests, clippy, release build, shellcheck, and packaging checks.
- [x] Add tag-triggered release workflow.
- [x] Build ark and ark-updater independently.
- [x] Generate tarball and `SHA256SUMS`.
- [x] Generate and sign `latest.json`.
- [x] Sign installer and release archive.
- [x] Store the private signing key in the `ARK_SIGNING_PRIVATE_KEY` GitHub Secret.
- [x] Derive and embed only the public key in the release updater binary.
- [x] Upload all assets to a draft GitHub Release.
- [x] Download and verify published assets after upload.

### Acceptance

- A test tag produces a complete draft release.
- Every published asset has a checksum.
- Stable assets have valid signatures.
- Missing signature causes CI failure.
- Signing secrets never appear in logs or artifacts.

### Required GitHub configuration

- The `ARK_SIGNING_PRIVATE_KEY` repository secret is configured. The workflow
  derives the public key at build time; no private key is committed or uploaded.
- Release signing public-key SHA-256 fingerprint:
  `5b62a8f3193b075ecc1ab8f556b7549dbf350ffe3a9e2f4b6cfa5f27c2425602`.
- Optionally set the `ARK_MINIMUM_VERSION` repository variable. It defaults to
  `0.1.0-beta`.
- The real 256×256 PNG is installed at `assets/icons/ark.png`; the workflow
  rejects a missing or incorrectly sized icon.
- Push a `v<VERSION>` tag matching `Cargo.toml` only when draft publication is
  authorized.

### Evidence — 2026-11-27

- `tests/signing.sh`: passed for signed archive/installer/manifest/checksums,
  corrupt archive signatures, and missing manifest signatures.
- `tests/embedded-key.sh`: passed with no runtime public-key override.
- `tests/update.sh`: passed with signed stable manifests and rejected an invalid
  `latest.json.sig`.
- `shellcheck packaging/*.sh`: clean.
- Workflow YAML parsed successfully and `actionlint` reported no issues.
- `cargo test --all-features --all-targets`: 441 unit tests passed, 0 failed,
  5 ignored; 5/5 CLI tests passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: clean.
- Built actual `0.1.0-beta` release binaries with an ephemeral test public key,
  packaged and signed them, and passed signed verification. The test private key
  was removed and never placed in the repository or release directory.
- No new Cargo dependencies were added.
- The `v0.1.0-beta` tag produced a complete draft GitHub Release with eight
  signed/checksummed assets.
- The release workflow passed post-upload verification. An independent local
  download recovered the embedded public key, matched fingerprint
  `5b62a8f3193b075ecc1ab8f556b7549dbf350ffe3a9e2f4b6cfa5f27c2425602`, and
  passed `verify-release.sh` signed mode.
- The release remains a draft; public publication and the exact public curl
  command are pending explicit approval.

---

## Phase 6 — End-to-end verification

### Automated tests

- [x] `cargo test`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo build --release --locked --bin ark`
- [x] `cargo build --release --locked --bin ark-updater --features updater`
- [x] `tests/installer.sh`
- [x] `tests/update-smoke.sh`
- [x] `shellcheck packaging/*.sh` when available
- [x] `desktop-file-validate packaging/ark.desktop` when available
- [x] `git diff --check`

### Update smoke sequence

- [x] Install `0.1.0` through curl.
- [x] Serve fake `0.1.1`.
- [x] Click **Update**.
- [x] Confirm download and verification.
- [x] Confirm `current` points to `0.1.1`.
- [x] Restart ark.
- [x] Confirm `ark --version` reports `0.1.1`.
- [x] Confirm project/config behavior is unchanged.
- [x] Corrupt signature and confirm rejection.
- [x] Disconnect network and confirm ark remains usable.
- [x] Kill during download and confirm `0.1.0` remains active.
- [x] Run rollback and confirm `0.1.0` returns.

### Clean-system tests

- [ ] Ubuntu 24.04 without Rust.
- [ ] Fedora Workstation.
- [x] Wayland session.
- [x] X11 session.
- [x] DPI scale 1.0 and 2.0.
- [x] Desktop icon launch.
- [x] `ark .`.
- [x] Terminal/OpenCode smoke.
- [x] Clipboard smoke.
- [x] Git status smoke.
- [x] RSS/PSS below 100 MB.

### Evidence — 2026-11-27

- `tests/update-smoke.sh` passed in headless CI mode and interactive X11 mode.
  The interactive run clicked **Install Update**, confirmed `current` switched to
  `0.1.1`, clicked **Restart to finish**, and then rolled back to `0.1.0-beta`.
- The same smoke rejected a corrupt manifest signature, stayed on `0.1.0-beta`
  after an offline check, and kept `0.1.0-beta` active after killing a throttled
  download.
- The real generated installer completed through `curl ... | sh` against a
  local release server and reported `ark 0.1.0-beta`.
- Installed binary ran with `/usr/bin:/bin` as PATH, without Cargo.
- Default Wayland and forced X11 first-frame smoke passed; X11 scale factors
  1.0 and 2.0 both presented successfully.
- `gtk-launch ark` launched the installed desktop entry successfully.
- Terminal command, `opencode --version`, and git command smoke passed.
- Native clipboard round-trip and terminal-selection copy tests passed.
- Git-status UI smoke showed modified and untracked badges in a real repository.
- Live process measurement: 27,388 KiB RSS, 23,574 KiB PSS, 27,388 KiB peak RSS,
  below the 100 MB budget.
- Ubuntu 24.04 and Fedora Workstation remain unverified on this Garuda Linux host.

---

# Security requirements

- HTTPS only for manifest and artifacts.
- Stable manifest and artifacts must be signed.
- Public verification key embedded in ark/updater.
- Private signing key only in CI secrets.
- Verify before extraction and before switching versions.
- Reject absolute paths, `..`, and symlink escapes in archives.
- Enforce maximum download size.
- Stage on the same filesystem as the install.
- Never execute downloaded shell scripts.
- Never overwrite the running executable.
- Keep the previous version until the new one is accepted.
- Log significant update failures with actionable messages.

The curl bootstrap still trusts the installer host/TLS. Publish immutable versioned installer URLs and provide a manual download/inspect path.

---

# Failure behavior

| Failure | Required behavior |
|---|---|
| Offline | Keep current app fully usable; show non-blocking status |
| Rate limited | Skip this cycle; retry at next scheduled check |
| Unsupported architecture | Refuse install/update with clear message |
| Invalid manifest | Abort without downloading |
| Checksum mismatch | Abort without extracting |
| Signature invalid | Abort without extracting |
| Oversized download | Abort and delete partial file |
| Disk full | Keep current version; clean staging files |
| Permission denied | Keep current version; show install path |
| Smoke test fails | Do not switch `current` |
| Apply interrupted | Keep current version or restore previous symlink |
| New version fails after restart | Keep previous version; allow `ark-updater rollback` |

---

# Release sequence

## `0.1.0-beta`

- Prebuilt x86_64 tarball.
- Curl installer.
- SHA-256 verification.
- Versioned user-local install.
- Desktop entry/icon.
- Updater helper, update UI, automatic checks, restart, and rollback.

## `0.1.0`

- Signed manifest/artifacts.
- `ark-updater` helper.
- Update badge/button.
- Automatic update checks.
- Restart-to-finish flow.
- Rollback command.
- Clean-system validation.

---

# Release-day checklist

- [x] Update `Cargo.toml` version.
- [x] Update `CHANGELOG.md`.
- [x] Run full test/lint/build gate.
- [x] Build and verify tarball.
- [x] Test curl installer on a clean HOME.
- [x] Test update from the previous release.
- [x] Record RSS/PSS.
- [x] Commit release changes.
- [x] Tag `vX.Y.Z`.
- [x] Push tag when publication is authorized.
- [x] Verify GitHub Release assets.
- [ ] Test the exact public curl command.
- [ ] Publish release notes.
- [ ] Announce release.

---

# Estimated effort

| Work | Estimate |
|---|---:|
| Repository hygiene, docs, icon, desktop entry | 2–4 hours |
| Tarball and versioned installer | 4–8 hours |
| Curl bootstrap installer | 3–6 hours |
| GitHub release workflow | 3–6 hours |
| Updater helper and signed manifest | 1–2 days |
| Update UI | 1 day |
| Security/rollback/update tests | 1–2 days |
| Clean Linux validation | 1 day |

Expected total: **5–9 focused days**, or **1–2 calendar weeks** with debugging and interruptions.

---

# Definition of done

The Linux release is complete when a user can:

1. Run the published curl command on a clean Linux system.
2. Install ark without Rust or sudo.
3. Launch ark from the desktop or terminal.
4. Receive an update notification.
5. Click **Update** and see verified download/install progress.
6. Restart ark and run the new version.
7. Roll back to the previous version if needed.
8. Continue using ark normally when offline or when an update fails.

---

# References

- cargo-dist shell installer:
  <https://axodotdev.github.io/cargo-dist/book/installers/shell.html>
- cargo-dist checksums:
  <https://axodotdev.github.io/cargo-dist/book/artifacts/checksums.html>
- self_update crate:
  <https://docs.rs/self_update/latest/self_update/>
- self-replace crate:
  <https://docs.rs/self-replace/latest/self_replace/>
- zipsign archive signing:
  <https://docs.rs/zipsign-api/latest/zipsign_api/>
- Linux `/proc` memory accounting:
  <https://docs.kernel.org/filesystems/proc.html>
