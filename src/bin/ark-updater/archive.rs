use std::fs;
use std::path::Path;
use std::process::Command;

use ark::update::{UpdateError, Version};

use super::download::{run_output, run_output_bounded};

const MAX_ARCHIVE_LISTING_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn validate_archive(archive: &Path) -> Result<(), UpdateError> {
    let files = run_output_bounded(
        "tar",
        &["-tzf", archive.to_string_lossy().as_ref()],
        MAX_ARCHIVE_LISTING_BYTES,
    )?;
    let verbose = run_output_bounded(
        "tar",
        &["-tvzf", archive.to_string_lossy().as_ref()],
        MAX_ARCHIVE_LISTING_BYTES,
    )?;
    ark::update::validate_archive_listing(
        &String::from_utf8_lossy(&files.stdout),
        &String::from_utf8_lossy(&verbose.stdout),
    )
}

pub(super) fn extract_archive(archive: &Path, destination: &Path) -> Result<(), UpdateError> {
    run_output(
        "tar",
        &[
            "-xzf",
            archive.to_string_lossy().as_ref(),
            "-C",
            destination.to_string_lossy().as_ref(),
        ],
    )?;
    Ok(())
}

pub(super) fn single_release_dir(parent: &Path) -> Result<std::path::PathBuf, UpdateError> {
    let mut entries = fs::read_dir(parent)?;
    let first = entries
        .next()
        .ok_or_else(|| UpdateError::UnsafeArchive("release directory is empty".into()))??;
    if entries.next().is_some() {
        return Err(UpdateError::UnsafeArchive(
            "archive must contain exactly one top-level entry".into(),
        ));
    }
    if !first.file_type()?.is_dir() {
        return Err(UpdateError::UnsafeArchive(
            "top-level archive entry is not a directory".into(),
        ));
    }
    Ok(first.path())
}

pub(super) fn smoke_test(binary: &Path, expected: &Version) -> Result<(), UpdateError> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|error| {
            UpdateError::StagedRelease(format!("could not run {}: {error}", binary.display()))
        })?;
    if !output.status.success() {
        return Err(UpdateError::StagedRelease(
            "staged ark --version failed".into(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let actual = Version::parse(text.trim().strip_prefix("ark ").unwrap_or(text.trim()))?;
    if &actual != expected {
        return Err(UpdateError::StagedRelease(format!(
            "staged ark reports {actual}, expected {expected}"
        )));
    }
    Ok(())
}
