//! Lazy filesystem listing for the file tree: one directory level at a
//! time (`std::fs::read_dir` — zero new dependencies, no watcher, no
//! recursion; the tree may go stale until M5's watcher lands).
//!
//! Memory discipline: callers cache at most one listing per expanded
//! directory (`tree.rs`); nothing here keeps state between calls.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One listed entry. `path` is the full entry path as `read_dir` reported
/// it (== `dir.join(name)` for every UTF-8 name); `name` is its lossy
/// display form.
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// One directory level's listing. `truncated` is set when the sorted
/// entries were cut at the caller's cap — never silently: the tree feeds
/// the flag to the status line.
#[derive(Clone, Debug)]
pub struct DirListing {
    pub entries: Vec<DirEntry>,
    #[allow(dead_code)] // frozen-interface flag; M4's viewer renders the truncation notice
    pub truncated: bool,
}

/// Guard against pathological directories; truncation sets `truncated`.
pub const MAX_ENTRIES_PER_DIR: usize = 50_000;

/// Sorted dirs-first, then files, each case-insensitive alphabetical.
/// Includes hidden files. Broken symlinks: is_dir=false, never followed
/// as dirs (no cycle risk: listings are per-expansion, never recursive).
pub fn read_dir_sorted_with_cap(dir: &Path, cap: usize) -> io::Result<DirListing> {
    let read = fs::read_dir(dir)?; // unlistable (missing, file, denied) → Err surfaces
    let mut dirs: Vec<DirEntry> = Vec::new();
    let mut files: Vec<DirEntry> = Vec::new();
    let mut skipped = 0usize;
    for entry in read {
        // A transiently vanished/unreadable entry must not fail the whole
        // listing — skip it, but never silently (warn + count).
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                skipped += 1;
                log::warn!("skipping unreadable entry in {}: {e}", dir.display());
                continue;
            }
        };
        // Git internals are never a workspace view (M7): skip the .git
        // directory specifically (other dotfiles stay listed honestly).
        if entry.file_name() == ".git" {
            skipped += 1;
            continue;
        }
        // file_type() does NOT follow symlinks: a symlink (even one pointing
        // at a directory) classifies as a non-dir, so the tree can never
        // follow it into a cycle; broken symlinks classify without erroring.
        let is_dir = match entry.file_type() {
            Ok(t) => t.is_dir(),
            Err(e) => {
                skipped += 1;
                log::warn!("skipping unclassifiable entry in {}: {e}", dir.display());
                continue;
            }
        };
        let entry = DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
            is_dir,
        };
        if is_dir {
            dirs.push(entry);
        } else {
            files.push(entry);
        }
    }
    // Case-insensitive alphabetical within each group; the sort is stable,
    // so names colliding case-insensitively keep their OS listing order.
    dirs.sort_by_cached_key(|e| e.name.to_lowercase());
    files.sort_by_cached_key(|e| e.name.to_lowercase());
    let mut entries = dirs;
    entries.append(&mut files);
    let mut truncated = false;
    if entries.len() > cap {
        entries.truncate(cap);
        truncated = true;
    }
    log::debug!(
        "listed {}: {} entries{}{}",
        dir.display(),
        entries.len(),
        if truncated { " (truncated)" } else { "" },
        if skipped > 0 { format!(", {skipped} skipped") } else { String::new() }
    );
    Ok(DirListing { entries, truncated })
}

/// [`read_dir_sorted_with_cap`] with the default [`MAX_ENTRIES_PER_DIR`] cap.
pub fn read_dir_sorted(dir: &Path) -> io::Result<DirListing> {
    read_dir_sorted_with_cap(dir, MAX_ENTRIES_PER_DIR)
}

/// Shared scratch-directory helper for the fs/tree unit tests: unique per
/// call, removed (best effort) on drop so failed assertions don't leak
/// trees into the OS temp dir.
#[cfg(test)]
pub(crate) mod testutil {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT_ID: AtomicU32 = AtomicU32::new(0);

    pub(crate) struct TempDir(PathBuf);

    impl TempDir {
        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn temp_dir(label: &str) -> TempDir {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ark-m3-{label}-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::testutil::{temp_dir, TempDir};
    use std::path::PathBuf;

    /// Fixture: mixed entries for the sort tests, including two
    /// byte-order-reversing pairs (`alpha`/`Zebra` and `café.txt`/`README.md`:
    /// byte order puts uppercase/ASCII first, case-insensitive order does
    /// not) and a non-ASCII name. Returns the guard and the exact expected
    /// dirs-first, case-insensitive order.
    fn mixed_dir(label: &str) -> (TempDir, Vec<&'static str>) {
        let d = temp_dir(label);
        fs::create_dir(d.path().join("beta")).expect("create beta dir");
        fs::File::create(d.path().join("zebra.txt")).expect("create zebra.txt");
        fs::File::create(d.path().join("README.md")).expect("create README.md");
        fs::File::create(d.path().join("café.txt")).expect("create café.txt");
        fs::File::create(d.path().join("apple.txt")).expect("create apple.txt");
        fs::File::create(d.path().join(".hidden")).expect("create .hidden");
        fs::create_dir(d.path().join("Zebra")).expect("create Zebra dir");
        fs::create_dir(d.path().join("alpha")).expect("create alpha dir");
        (
            d,
            vec![
                // Dirs, case-insensitive: byte order would be Zebra, alpha, beta.
                "alpha", "beta", "Zebra",
                // Files, case-insensitive: byte order would be README before café.
                ".hidden", "apple.txt", "café.txt", "README.md", "zebra.txt",
            ],
        )
    }

    fn names(listing: &DirListing) -> Vec<&str> {
        listing.entries.iter().map(|e| e.name.as_str()).collect()
    }

    // ---- 1. dirs first, case-insensitive within each group ----

    #[test]
    fn sorted_dirs_first_case_insensitive() {
        let (d, expected) = mixed_dir("sort");
        let listing = read_dir_sorted(d.path()).expect("listing succeeds");
        // Pins CASE-INSENSITIVE order, not byte order: the `alpha`/`Zebra`
        // and `café.txt`/`README.md` pairs REVERSE between the two orders
        // (byte order ranks uppercase ASCII first), so a ci→byte sort
        // mutation fails this test.
        assert_eq!(names(&listing), expected, "dirs first, then files, each case-insensitive");
        assert_eq!(
            listing.entries.iter().map(|e| e.is_dir).collect::<Vec<bool>>(),
            vec![true, true, true, false, false, false, false, false]
        );
        assert!(!listing.truncated, "under the cap nothing truncates");
    }

    // ---- 2. hidden files are listed honestly (no filtering in M3) ----

    #[test]
    fn hidden_files_included() {
        let d = temp_dir("hidden");
        fs::File::create(d.path().join(".gitignore")).expect("create dotfile");
        fs::File::create(d.path().join(".env")).expect("create dotfile 2");
        fs::File::create(d.path().join("README.md")).expect("create visible file");
        let listing = read_dir_sorted(d.path()).expect("listing succeeds");
        let names = names(&listing);
        assert_eq!(listing.entries.len(), 3, "nothing filtered out");
        assert!(names.contains(&".gitignore"), "dotfiles present");
        assert!(names.contains(&".env"));
        assert!(names.contains(&"README.md"));
    }

    // ---- 3. unlistable directories are errors, not panics ----

    #[test]
    fn nonexistent_dir_is_an_error() {
        let d = temp_dir("missing");
        let missing = d.path().join("no-such-subdir");
        let err = read_dir_sorted(&missing).expect_err("nonexistent dir must Err");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        let err = read_dir_sorted_with_cap(&missing, 10).expect_err("capped variant must Err too");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    // ---- 4. cap truncates the SORTED list, with the flag set ----

    #[test]
    fn cap_truncates_with_flag() {
        let d = temp_dir("cap");
        fs::create_dir(d.path().join("D_dir")).expect("create dir");
        for name in ["z.txt", "a.txt", "m.txt"] {
            fs::File::create(d.path().join(name)).expect("create file");
        }
        // Sorted order (dirs first, case-insensitive): D_dir, a.txt, m.txt, z.txt.
        let sorted = ["D_dir", "a.txt", "m.txt", "z.txt"];

        let listing = read_dir_sorted_with_cap(d.path(), 2).expect("listing succeeds");
        assert_eq!(listing.entries.len(), 2, "cap cuts the listing");
        assert!(listing.truncated, "truncation must be flagged");
        assert_eq!(
            names(&listing),
            vec![sorted[0], sorted[1]],
            "truncate the SORTED list: the prefix must be the sorted order's prefix"
        );

        let listing = read_dir_sorted_with_cap(d.path(), 4).expect("listing succeeds");
        assert_eq!(listing.entries.len(), 4);
        assert!(!listing.truncated, "cap == contents: no truncation");

        let listing = read_dir_sorted_with_cap(d.path(), 10_000).expect("listing succeeds");
        assert_eq!(names(&listing), sorted);
        assert!(!listing.truncated, "cap larger than contents: no truncation");
    }

    // ---- 5. full paths + kinds; broken symlinks never escape as errors ----

    #[test]
    fn entries_carry_full_paths_and_kinds() {
        let d = temp_dir("kinds");
        fs::create_dir(d.path().join("sub")).expect("create subdir");
        fs::File::create(d.path().join("f.txt")).expect("create file");
        #[cfg(unix)]
        std::os::unix::fs::symlink("no-such-target", d.path().join("dangling"))
            .expect("create broken symlink");

        let listing = read_dir_sorted(d.path()).expect("listing succeeds (no error escape)");

        // Every entry carries its full path.
        let expect_full: Vec<PathBuf> = listing.entries.iter().map(|e| d.path().join(&e.name)).collect();
        let actual: Vec<PathBuf> = listing.entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(actual, expect_full, "path == dir.join(name)");

        let sub = listing.entries.iter().find(|e| e.name == "sub").expect("dir listed");
        assert!(sub.is_dir, "directories classify as dirs");
        let f = listing.entries.iter().find(|e| e.name == "f.txt").expect("file listed");
        assert!(!f.is_dir, "files classify as non-dirs");

        #[cfg(unix)]
        {
            let dangling = listing
                .entries
                .iter()
                .find(|e| e.name == "dangling")
                .expect("broken symlink is listed honestly");
            assert!(!dangling.is_dir, "broken symlink is never followed");
        }
    }

    // ---- 6. (milestone acceptance) the frozen 50k cap truncates with flag ----

    /// Slow: materializes MAX_ENTRIES_PER_DIR + 1 scratch files. Run
    /// explicitly via `cargo test -- --ignored` (excluded from normal runs).
    #[test]
    #[ignore = "slow: creates 50k+1 scratch files in the OS temp dir"]
    fn max_cap_truncates_a_50k_dir() {
        let d = temp_dir("50k");
        for i in 0..=MAX_ENTRIES_PER_DIR {
            fs::File::create(d.path().join(format!("f{i:06}.txt"))).expect("create scratch file");
        }
        let listing = read_dir_sorted(d.path()).expect("listing succeeds");
        assert_eq!(listing.entries.len(), MAX_ENTRIES_PER_DIR, "cut at the frozen cap");
        assert!(listing.truncated, "a dir over the cap must be flagged truncated");
        // The truncation cuts the SORTED list: the prefix is the sorted order.
        let small = read_dir_sorted_with_cap(d.path(), 7).expect("listing succeeds");
        assert_eq!(small.entries.len(), 7);
        assert!(small.truncated);
        assert_eq!(small.entries[0].name, "f000000.txt", "sorted prefix preserved");
    }
}
