//! Root-contained, no-overwrite creation for Explorer actions. These functions
//! create empty files/directories only; code-content editing remains limited to
//! the whitelisted config editor in `fs.rs`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum CreateError {
    InvalidName,
    AlreadyExists,
    Io(io::Error),
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CreateError::InvalidName => write!(f, "enter a single file or folder name"),
            CreateError::AlreadyExists => write!(f, "an entry with that name already exists"),
            CreateError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CreateError {}

fn target(root: &Path, name: &str) -> Result<PathBuf, CreateError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
        || Path::new(name).components().count() != 1
    {
        return Err(CreateError::InvalidName);
    }
    let root = fs::canonicalize(root).map_err(CreateError::Io)?;
    let target = root.join(name);
    match fs::symlink_metadata(&target) {
        Ok(_) => Err(CreateError::AlreadyExists),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(target),
        Err(e) => Err(CreateError::Io(e)),
    }
}

pub fn create_file(root: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let target = target(root, name)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(CreateError::Io)?;
    Ok(target)
}

pub fn create_dir(root: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let target = target(root, name)?;
    fs::create_dir(&target).map_err(CreateError::Io)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::testutil::temp_dir;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn creates_a_file_and_directory_inside_the_root() {
        let d = temp_dir("create-entries");
        let file = create_file(d.path(), "notes.txt").expect("create file");
        let dir = create_dir(d.path(), "new-folder").expect("create dir");
        assert_eq!(file, d.path().join("notes.txt"));
        assert!(file.is_file());
        assert!(dir.is_dir());
    }

    #[test]
    fn refuses_to_overwrite_existing_entries() {
        let d = temp_dir("create-no-overwrite");
        fs::write(d.path().join("exists.txt"), "keep").expect("write");
        assert!(matches!(
            create_file(d.path(), "exists.txt"),
            Err(CreateError::AlreadyExists)
        ));
        fs::create_dir(d.path().join("exists-dir")).expect("dir");
        assert!(matches!(
            create_dir(d.path(), "exists-dir"),
            Err(CreateError::AlreadyExists)
        ));
        assert_eq!(
            fs::read_to_string(d.path().join("exists.txt")).expect("read"),
            "keep"
        );
    }

    #[test]
    fn rejects_traversal_and_non_component_names() {
        let d = temp_dir("create-invalid");
        for name in ["", ".", "..", "../escape", "a/b", "a\\b", "bad\0name"] {
            assert!(matches!(
                create_file(d.path(), name),
                Err(CreateError::InvalidName)
            ));
        }
    }

    #[test]
    fn existing_symlinks_are_never_followed() {
        let d = temp_dir("create-symlink");
        let outside = temp_dir("create-symlink-outside");
        symlink(outside.path(), d.path().join("link")).expect("symlink");
        assert!(matches!(
            create_file(d.path(), "link"),
            Err(CreateError::AlreadyExists)
        ));
        assert_eq!(
            fs::read_dir(outside.path())
                .expect("outside listing")
                .count(),
            0
        );
    }
}
