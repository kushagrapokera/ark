//! Root-contained, no-overwrite creation for Explorer actions. These functions
//! create empty files/directories only; code-content editing remains limited to
//! the whitelisted config editor in `fs.rs`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum CreateError {
    InvalidName,
    OutsideRoot,
    SymlinkParent,
    InvalidParent,
    AlreadyExists,
    Io(io::Error),
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CreateError::InvalidName => write!(f, "enter a single file or folder name"),
            CreateError::OutsideRoot => write!(f, "target folder is outside the project root"),
            CreateError::SymlinkParent => write!(f, "target folder path contains a symlink"),
            CreateError::InvalidParent => write!(f, "target folder path is not a directory"),
            CreateError::AlreadyExists => write!(f, "an entry with that name already exists"),
            CreateError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CreateError {}

fn checked_parent(root: &Path, parent: &Path) -> Result<PathBuf, CreateError> {
    let root = fs::canonicalize(root).map_err(CreateError::Io)?;
    if !fs::symlink_metadata(&root)
        .map_err(CreateError::Io)?
        .is_dir()
    {
        return Err(CreateError::InvalidParent);
    }
    let parent = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        root.join(parent)
    };
    if !parent.starts_with(&root) {
        return Err(CreateError::OutsideRoot);
    }
    let relative = parent
        .strip_prefix(&root)
        .map_err(|_| CreateError::OutsideRoot)?;
    let mut checked = root;
    for component in relative.components() {
        match component {
            Component::Normal(name) => {
                checked.push(name);
                let meta = fs::symlink_metadata(&checked).map_err(CreateError::Io)?;
                if meta.file_type().is_symlink() {
                    return Err(CreateError::SymlinkParent);
                }
                if !meta.is_dir() {
                    return Err(CreateError::InvalidParent);
                }
            }
            Component::CurDir => {}
            _ => return Err(CreateError::InvalidParent),
        }
    }
    Ok(checked)
}

fn target(root: &Path, parent: &Path, name: &str) -> Result<PathBuf, CreateError> {
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
    let parent = checked_parent(root, parent)?;
    let target = parent.join(name);
    match fs::symlink_metadata(&target) {
        Ok(_) => Err(CreateError::AlreadyExists),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(target),
        Err(e) => Err(CreateError::Io(e)),
    }
}

#[cfg(test)]
pub fn create_file(root: &Path, name: &str) -> Result<PathBuf, CreateError> {
    create_file_in(root, root, name)
}

pub fn create_file_in(root: &Path, parent: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let target = target(root, parent, name)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(CreateError::Io)?;
    Ok(target)
}

#[cfg(test)]
pub fn create_dir(root: &Path, name: &str) -> Result<PathBuf, CreateError> {
    create_dir_in(root, root, name)
}

pub fn create_dir_in(root: &Path, parent: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let target = target(root, parent, name)?;
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
    fn creates_inside_a_selected_subdirectory() {
        let d = temp_dir("create-in-parent");
        let parent = d.path().join("src");
        fs::create_dir(&parent).expect("create src");
        let file = create_file_in(d.path(), &parent, "notes.txt").expect("create file");
        let dir = create_dir_in(d.path(), &parent, "nested").expect("create dir");
        assert_eq!(file, parent.join("notes.txt"));
        assert_eq!(dir, parent.join("nested"));
        assert!(!d.path().join("notes.txt").exists());
    }

    #[test]
    fn rejects_a_parent_outside_the_project_root() {
        let d = temp_dir("create-parent-root");
        let outside = temp_dir("create-parent-outside");
        assert!(matches!(
            create_file_in(d.path(), outside.path(), "escape.txt"),
            Err(CreateError::OutsideRoot)
        ));
        let link = d.path().join("link");
        symlink(outside.path(), &link).expect("symlink");
        assert!(matches!(
            create_file_in(d.path(), &link, "escape.txt"),
            Err(CreateError::SymlinkParent)
        ));
    }

    #[test]
    fn rejects_a_symlinked_parent_inside_the_project_root() {
        let d = temp_dir("create-parent-symlink");
        let target = d.path().join("src");
        fs::create_dir(&target).expect("create src");
        let link = d.path().join("link");
        symlink(&target, &link).expect("symlink");

        assert!(matches!(
            create_file_in(d.path(), &link, "escape.txt"),
            Err(CreateError::SymlinkParent)
        ));
        assert!(matches!(
            create_dir_in(d.path(), &link, "escape"),
            Err(CreateError::SymlinkParent)
        ));
        assert_eq!(fs::read_dir(&target).expect("target listing").count(), 0);
    }

    #[test]
    fn rejects_a_non_directory_parent() {
        let d = temp_dir("create-parent-file");
        let file = d.path().join("file.txt");
        fs::write(&file, "keep").expect("write");

        assert!(matches!(
            create_file_in(d.path(), &file, "notes.txt"),
            Err(CreateError::InvalidParent)
        ));
        assert_eq!(fs::read_to_string(&file).expect("read"), "keep");
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
