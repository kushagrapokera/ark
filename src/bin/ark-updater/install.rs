use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use ark::update::{InstallLayout, UpdateError, Version};

use super::state::unique_path;
use super::warn;

const CACHE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_INSTALLED_VERSIONS: usize = 3;

pub(super) fn switch_current(layout: &InstallLayout, version: &Version) -> Result<(), UpdateError> {
    fs::create_dir_all(layout.app_data_dir())?;
    let temporary = unique_path(&layout.app_data_dir(), ".current");
    std::os::unix::fs::symlink(format!("versions/{version}"), &temporary)?;
    if let Err(error) = fs::rename(&temporary, layout.current_link()) {
        if let Err(cleanup_error) = fs::remove_file(&temporary) {
            warn(format!(
                "could not remove temporary current link {temporary:?}: {cleanup_error}"
            ));
        }
        return Err(error.into());
    }
    Ok(())
}

fn installed_versions(layout: &InstallLayout) -> Result<Vec<Version>, UpdateError> {
    let entries = match fs::read_dir(layout.versions_dir()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if let Ok(version) = Version::parse(&entry.file_name().to_string_lossy()) {
            versions.push(version);
        }
    }
    versions.sort();
    Ok(versions)
}

pub(super) fn previous_version(
    layout: &InstallLayout,
    current: &Version,
) -> Result<Option<Version>, UpdateError> {
    Ok(installed_versions(layout)?
        .into_iter()
        .rfind(|version| version < current))
}

pub(super) fn prune_versions(layout: &InstallLayout, current: &Version) -> Result<(), UpdateError> {
    let mut versions = installed_versions(layout)?;
    versions.reverse();
    let mut older_kept = 0usize;
    for version in versions {
        if version == *current {
            continue;
        }
        if version < *current && older_kept < MAX_INSTALLED_VERSIONS - 1 {
            older_kept += 1;
            continue;
        }
        let path = layout.version_dir(&version);
        if let Err(error) = fs::remove_dir_all(&path) {
            warn(format!(
                "could not remove old ark version {path:?}: {error}"
            ));
        }
    }
    Ok(())
}

pub(super) fn ensure_disk_space(path: &Path, required: u64) -> Result<(), UpdateError> {
    let available = available_bytes(path)?;
    if available < required {
        return Err(UpdateError::InsufficientDisk {
            available,
            required,
        });
    }
    Ok(())
}

fn available_bytes(path: &Path) -> Result<u64, UpdateError> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| UpdateError::InstallLayout("install path contains a NUL byte".into()))?;
    let mut stat = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let result = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

pub(super) fn cleanup_stale_cache(layout: &InstallLayout) -> Result<(), UpdateError> {
    cleanup_stale_entries(&layout.cache_dir(), None)?;
    cleanup_stale_entries(&layout.versions_dir(), Some(&[".stage-", ".backup-"]))
}

fn cleanup_stale_entries(parent: &Path, prefixes: Option<&[&str]>) -> Result<(), UpdateError> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if prefixes.is_some_and(|prefixes| !prefixes.iter().any(|prefix| name.starts_with(prefix)))
        {
            continue;
        }
        let modified = match entry.metadata().and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(error) => {
                warn(format!(
                    "could not inspect stale update entry {:?}: {error}",
                    entry.path()
                ));
                continue;
            }
        };
        let stale = SystemTime::now()
            .duration_since(modified)
            .ok()
            .is_some_and(|age| age >= CACHE_RETENTION);
        if stale {
            let path = entry.path();
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if let Err(error) = result {
                warn(format!(
                    "could not remove stale update entry {:?}: {error}",
                    path
                ));
            }
        }
    }
    Ok(())
}
