use super::UpdateError;

pub fn validate_archive_listing(files: &str, verbose: &str) -> Result<(), UpdateError> {
    for line in files.lines() {
        if line.is_empty()
            || line.starts_with('/')
            || line.contains('\\')
            || line.chars().any(char::is_control)
            || line.split('/').any(|part| part == "..")
        {
            return Err(UpdateError::UnsafeArchive(format!("unsafe path: {line}")));
        }
    }
    for line in verbose.lines() {
        let trimmed = line.trim_start();
        let kind = trimmed.chars().next().unwrap_or(' ');
        if !matches!(kind, '-' | 'd') {
            return Err(UpdateError::UnsafeArchive(format!(
                "unsupported entry type: {line}"
            )));
        }
    }
    Ok(())
}
