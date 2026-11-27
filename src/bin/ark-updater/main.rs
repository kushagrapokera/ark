mod archive;
mod download;
mod install;
mod state;

use std::fs;

use ark::update::{InstallLayout, ReleaseManifest, StagedRelease, Target, UpdateError, Version};
use clap::{Parser, Subcommand};

use archive::{extract_archive, single_release_dir, smoke_test, validate_archive};
use download::{download, insecure_http_allowed, verify_checksum, verify_signature};
use install::{
    cleanup_stale_cache, ensure_disk_space, previous_version, prune_versions, switch_current,
};
use state::{read_bounded, read_install, unique_path, utc_now, write_json_atomic, WorkDir};

#[derive(Debug, Parser)]
#[command(name = "ark-updater", version)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Check whether a newer release is available.
    Check {
        #[arg(long)]
        manifest_url: Option<String>,
        #[arg(long, default_value = "stable")]
        channel: String,
    },
    /// Download, verify, and stage a newer release without installing it.
    Stage {
        #[arg(long)]
        manifest_url: Option<String>,
        #[arg(long, default_value = "stable")]
        channel: String,
    },
    /// Atomically switch to a staged release.
    Apply {
        #[arg(long)]
        version: String,
    },
    /// Switch back to the newest older installed release.
    Rollback,
}

fn main() {
    if let Err(error) = run(Cli::parse().command) {
        emit_error(&error);
        std::process::exit(1);
    }
}

pub(crate) fn warn(message: impl std::fmt::Display) {
    eprintln!("ark-updater: warning: {message}");
}

fn run(command: CommandKind) -> Result<(), UpdateError> {
    let layout = InstallLayout::detect()?;
    match command {
        CommandKind::Check {
            manifest_url,
            channel,
        } => check(&layout, manifest_url, channel),
        CommandKind::Stage {
            manifest_url,
            channel,
        } => stage(&layout, manifest_url, channel),
        CommandKind::Apply { version } => apply(&layout, &version),
        CommandKind::Rollback => rollback(&layout),
    }
}

fn check(
    layout: &InstallLayout,
    manifest_url: Option<String>,
    channel: String,
) -> Result<(), UpdateError> {
    println!("event: check-start");
    let install = read_install(layout)?;
    let (manifest, _) = load_manifest(layout, &install, manifest_url, &channel)?;
    let target = Target::detect()?;
    let asset = manifest.asset(target)?;
    if !manifest.is_newer_than(&install.version) {
        println!("event: up-to-date");
        println!("version: {}", install.version);
        return Ok(());
    }
    println!("event: update-available");
    println!("version: {}", manifest.version);
    println!("size: {}", asset.size);
    println!("notes-url: {}", manifest.notes_url);
    Ok(())
}

fn stage(
    layout: &InstallLayout,
    manifest_url: Option<String>,
    channel: String,
) -> Result<(), UpdateError> {
    cleanup_stale_cache(layout)?;
    let install = read_install(layout)?;
    let (manifest, url) = load_manifest(layout, &install, manifest_url, &channel)?;
    if !manifest.is_newer_than(&install.version) {
        println!("event: up-to-date");
        println!("version: {}", install.version);
        return Ok(());
    }
    let target = Target::detect()?;
    let asset = manifest.asset(target)?;
    let work = WorkDir::new(&layout.cache_dir())?;
    let archive_path = work.path().join(format!("ark-{}.tar.gz", manifest.version));
    let required = asset
        .size
        .saturating_mul(2)
        .saturating_add(16 * 1024 * 1024);
    ensure_disk_space(&layout.versions_dir(), required)?;

    println!("event: download-start");
    println!("version: {}", manifest.version);
    download(&asset.url, &archive_path, asset.size, Some(asset.size))?;
    verify_checksum(&archive_path, &asset.sha256)?;
    if let Some(signature_url) = &asset.signature {
        let signature = work.path().join("ark.tar.gz.sig");
        download(signature_url, &signature, 64 * 1024, None)?;
        verify_signature(&archive_path, &signature)?;
    }

    validate_archive(&archive_path)?;
    fs::create_dir_all(layout.versions_dir())?;
    let staged = unique_path(
        &layout.versions_dir(),
        &format!(".stage-{}", manifest.version),
    );
    fs::create_dir_all(&staged)?;
    let result: Result<(), UpdateError> = (|| {
        extract_archive(&archive_path, &staged)?;
        let release_dir = single_release_dir(&staged)?;
        smoke_test(&release_dir.join("ark"), &manifest.version)?;
        let info = StagedRelease {
            version: manifest.version.clone(),
            path: release_dir,
            channel: manifest.channel.clone(),
            manifest_url: url,
        };
        write_json_atomic(&layout.staged_file(), &info.to_json())?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(error) = fs::remove_dir_all(&staged) {
            warn(format!(
                "could not remove failed staging directory {staged:?}: {error}"
            ));
        }
    }
    result?;

    println!("event: staged");
    println!("version: {}", manifest.version);
    println!("event: ready");
    Ok(())
}

fn apply(layout: &InstallLayout, requested_version: &str) -> Result<(), UpdateError> {
    let requested = Version::parse(requested_version)?;
    let staged = StagedRelease::parse(&read_bounded(
        &layout.staged_file(),
        ark::update::MAX_MANIFEST_BYTES,
    )?)?;
    if staged.version != requested {
        return Err(UpdateError::StagedRelease(format!(
            "staged version {} does not match requested version {}",
            staged.version, requested
        )));
    }
    let mut install = read_install(layout)?;
    if staged.version <= install.version {
        return Err(UpdateError::StagedRelease(format!(
            "refusing to install {} over newer version {}",
            staged.version, install.version
        )));
    }
    let previous_version = install.version.clone();
    let canonical_versions = fs::canonicalize(layout.versions_dir())?;
    let canonical_staged = fs::canonicalize(&staged.path)?;
    if canonical_staged == canonical_versions || !canonical_staged.starts_with(&canonical_versions)
    {
        return Err(UpdateError::StagedRelease(
            "staged path is outside the versions directory".into(),
        ));
    }
    smoke_test(&staged.path.join("ark"), &staged.version)?;
    let destination = layout.version_dir(&staged.version);
    let backup = unique_path(
        &layout.versions_dir(),
        &format!(".backup-{}", staged.version),
    );
    let had_previous = destination.exists();
    if had_previous {
        fs::rename(&destination, &backup)?;
    }
    if let Err(error) = fs::rename(&staged.path, &destination) {
        if had_previous {
            if let Err(restore_error) = fs::rename(&backup, &destination) {
                warn(format!(
                    "could not restore {destination:?}: {restore_error}"
                ));
            }
        }
        return Err(error.into());
    }
    if let Err(error) = switch_current(layout, &staged.version) {
        if let Err(cleanup_error) = fs::remove_dir_all(&destination) {
            warn(format!(
                "could not remove failed staged install {destination:?}: {cleanup_error}"
            ));
        }
        if had_previous {
            if let Err(restore_error) = fs::rename(&backup, &destination) {
                warn(format!(
                    "could not restore {destination:?}: {restore_error}"
                ));
            }
        }
        return Err(error);
    }
    install.version = staged.version;
    install.channel = staged.channel;
    install.manifest_url = staged.manifest_url;
    install.installed_at = utc_now();
    if let Err(error) = write_json_atomic(&layout.install_file(), &install.to_json()) {
        if let Err(rollback_error) = switch_current(layout, &previous_version) {
            warn(format!(
                "could not restore previous current link: {rollback_error}"
            ));
        }
        return Err(error);
    }
    if let Err(error) = prune_versions(layout, &install.version) {
        warn(format!("could not prune old ark versions: {error}"));
    }
    if had_previous {
        if let Err(error) = fs::remove_dir_all(&backup) {
            warn(format!(
                "could not remove old same-version backup {backup:?}: {error}"
            ));
        }
    }
    if let Some(parent) = staged.path.parent() {
        if parent
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".stage-"))
        {
            if let Err(error) = fs::remove_dir_all(parent) {
                warn(format!(
                    "could not remove empty staging directory {parent:?}: {error}"
                ));
            }
        }
    }
    if let Err(error) = fs::remove_file(layout.staged_file()) {
        warn(format!("could not remove staged metadata: {error}"));
    }
    println!("event: installed");
    println!("version: {}", install.version);
    Ok(())
}

fn rollback(layout: &InstallLayout) -> Result<(), UpdateError> {
    let mut install = read_install(layout)?;
    let current_version = install.version.clone();
    let previous =
        previous_version(layout, &install.version)?.ok_or(UpdateError::RollbackUnavailable)?;
    smoke_test(&layout.version_dir(&previous).join("ark"), &previous)?;
    switch_current(layout, &previous)?;
    install.version = previous.clone();
    install.installed_at = utc_now();
    if let Err(error) = write_json_atomic(&layout.install_file(), &install.to_json()) {
        if let Err(rollback_error) = switch_current(layout, &current_version) {
            warn(format!(
                "could not restore previous current link: {rollback_error}"
            ));
        }
        return Err(error);
    }
    if let Err(error) = prune_versions(layout, &install.version) {
        warn(format!("could not prune old ark versions: {error}"));
    }
    println!("event: rolled-back");
    println!("version: {}", previous);
    Ok(())
}

fn load_manifest(
    layout: &InstallLayout,
    install: &ark::update::InstallInfo,
    manifest_url: Option<String>,
    channel: &str,
) -> Result<(ReleaseManifest, String), UpdateError> {
    let url = manifest_url_for(install, manifest_url);
    let work = WorkDir::new(&layout.cache_dir())?;
    let path = work.path().join("latest.json");
    download(&url, &path, ark::update::MAX_MANIFEST_BYTES, None)?;
    let text = read_bounded(&path, ark::update::MAX_MANIFEST_BYTES)?;
    let manifest = ReleaseManifest::parse(&text, insecure_http_allowed())?;
    if manifest.channel != channel {
        return Err(UpdateError::InvalidManifest(format!(
            "expected channel {channel}, found {}",
            manifest.channel
        )));
    }
    Ok((manifest, url))
}

fn manifest_url_for(install: &ark::update::InstallInfo, override_url: Option<String>) -> String {
    override_url.unwrap_or_else(|| install.manifest_url.clone())
}

fn emit_error(error: &UpdateError) {
    let message = error.to_string().replace(['\n', '\r'], " ");
    println!("event: error");
    println!("code: {}", error_code(error));
    println!("message: {message}");
}

fn error_code(error: &UpdateError) -> &'static str {
    match error {
        UpdateError::Io(_) => "io-error",
        UpdateError::InvalidVersion(_) => "invalid-version",
        UpdateError::UnsupportedTarget(_) => "unsupported-target",
        UpdateError::InvalidManifest(_) => "invalid-manifest",
        UpdateError::InsecureUrl(_) => "insecure-url",
        UpdateError::DownloadTooLarge { .. } => "download-too-large",
        UpdateError::SizeMismatch { .. } => "size-mismatch",
        UpdateError::ChecksumMismatch => "checksum-mismatch",
        UpdateError::SignatureInvalid(_) => "signature-invalid",
        UpdateError::UnsafeArchive(_) => "unsafe-archive",
        UpdateError::MissingTool(_) => "missing-tool",
        UpdateError::CommandFailed { .. } => "command-failed",
        UpdateError::InsufficientDisk { .. } => "insufficient-disk",
        UpdateError::InstallLayout(_) => "install-layout",
        UpdateError::StagedRelease(_) => "staged-release",
        UpdateError::RollbackUnavailable => "rollback-unavailable",
    }
}
