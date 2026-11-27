use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ark::update::InstallLayout;

pub const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

pub fn automatic_check_due(layout: &InstallLayout, now: SystemTime) -> bool {
    match read_last_check(layout) {
        None => true,
        Some(last) => now
            .duration_since(last)
            .is_ok_and(|age| age >= AUTO_CHECK_INTERVAL),
    }
}

pub fn record_successful_check(layout: &InstallLayout, now: SystemTime) -> Result<(), String> {
    let path = layout.state_dir().join("last-check");
    std::fs::create_dir_all(layout.state_dir()).map_err(|error| error.to_string())?;
    let temporary = layout.state_dir().join(format!(
        ".last-check-{}-{}",
        std::process::id(),
        now.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let contents = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    if let Err(error) = std::fs::write(&temporary, contents) {
        if let Err(cleanup) = std::fs::remove_file(&temporary) {
            log::warn!("could not remove temporary update timestamp: {cleanup}");
        }
        return Err(error.to_string());
    }
    if let Err(error) = std::fs::rename(&temporary, &path) {
        if let Err(cleanup) = std::fs::remove_file(&temporary) {
            log::warn!("could not remove temporary update timestamp: {cleanup}");
        }
        return Err(error.to_string());
    }
    Ok(())
}

fn read_last_check(layout: &InstallLayout) -> Option<SystemTime> {
    let path = layout.state_dir().join("last-check");
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!(
                "could not inspect update check timestamp {}: {error}",
                path.display()
            );
            return None;
        }
    };
    if metadata.len() > 32 {
        log::warn!("invalid update check timestamp at {}", path.display());
        return None;
    }
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            log::warn!(
                "could not read update check timestamp {}: {error}",
                path.display()
            );
            return None;
        }
    };
    let seconds = match contents.parse::<u64>() {
        Ok(seconds) => seconds,
        Err(_) => {
            log::warn!("invalid update check timestamp at {}", path.display());
            return None;
        }
    };
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}
