use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ark::update::{InstallInfo, InstallLayout, UpdateError};

use super::warn;

pub(super) fn read_install(layout: &InstallLayout) -> Result<InstallInfo, UpdateError> {
    InstallInfo::parse(&read_bounded(
        &layout.install_file(),
        ark::update::MAX_MANIFEST_BYTES,
    )?)
}

pub(super) fn read_bounded(path: &Path, max: u64) -> Result<String, UpdateError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > max {
        return Err(UpdateError::DownloadTooLarge {
            size: metadata.len(),
            max,
        });
    }
    fs::read_to_string(path).map_err(UpdateError::from)
}

pub(super) fn write_json_atomic(path: &Path, contents: &str) -> Result<(), UpdateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = unique_path(parent_or(path), &format!(".{}.tmp", file_name(path)));
    if let Err(error) = fs::write(&temporary, contents) {
        if let Err(cleanup_error) = fs::remove_file(&temporary) {
            warn(format!(
                "could not remove temporary metadata {temporary:?}: {cleanup_error}"
            ));
        }
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if let Err(cleanup_error) = fs::remove_file(&temporary) {
            warn(format!(
                "could not remove temporary metadata {temporary:?}: {cleanup_error}"
            ));
        }
        return Err(error.into());
    }
    Ok(())
}

fn parent_or(path: &Path) -> &Path {
    path.parent().unwrap_or_else(|| Path::new("."))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ark-update".into())
}

pub(super) fn unique_path(parent: &Path, prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!("{prefix}-{}-{nonce}", std::process::id()))
}

pub(super) struct WorkDir(PathBuf);

impl WorkDir {
    pub(super) fn new(parent: &Path) -> Result<Self, UpdateError> {
        fs::create_dir_all(parent)?;
        let path = unique_path(parent, ".ark-update");
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            warn(format!(
                "could not remove update workspace {:?}: {error}",
                self.0
            ));
        }
    }
}

pub(super) fn utc_now() -> String {
    match Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => {
            warn(format!(
                "date failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            "unknown".into()
        }
        Err(error) => {
            warn(format!("could not run date: {error}"));
            "unknown".into()
        }
    }
}
