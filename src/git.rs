//! Git status engine (M7): subprocess-only (`git` CLI per the frozen stack —
//! zero in-process git). `status --porcelain=v2` for tree badges, per-file
//! `diff HEAD --unified=0` for viewer gutter marks.
//!
//! Memory: parsed entries capped (MAX_STATUS_ENTRIES), diff marks capped
//! (MAX_DIFF_MARKS) — bounded regardless of repo size. Subprocesses run on
//! background threads (the caller owns the plumbing); results are cheap
//! snapshots.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed entry cap (a 100k+ file repo degrades with a truncated flag).
pub const MAX_STATUS_ENTRIES: usize = 100_000;
/// Per-file diff mark cap.
pub const MAX_DIFF_MARKS: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
}

/// Tree badge alphabet (M/A/D/U).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    Modified,
    Added,
    Deleted,
    Untracked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStatus {
    pub path: PathBuf,
    /// Rename/copy source (staged renames report the new path in `path`).
    pub orig: Option<PathBuf>,
    pub staged: Option<StatusKind>,
    pub unstaged: Option<StatusKind>,
    pub untracked: bool,
    pub unmerged: bool,
}

impl FileStatus {
    /// The tree badge: unstaged beats staged, untracked beats all, unmerged
    /// and renames render as M.
    pub fn badge(&self) -> Badge {
        if self.untracked {
            return Badge::Untracked;
        }
        if self.unmerged {
            return Badge::Modified;
        }
        match self.unstaged.or(self.staged) {
            Some(StatusKind::Deleted) => Badge::Deleted,
            Some(StatusKind::Added) | Some(StatusKind::Copied) => Badge::Added,
            _ => Badge::Modified,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub entries: Vec<FileStatus>,
    pub truncated: bool,
    /// The repo root the entries are relative to (badge lookups join it).
    pub root: PathBuf,
    /// absolute path → badge lookup built at parse time (O(1) per tree row).
    badges: std::collections::HashMap<PathBuf, Badge>,
}

impl GitStatus {
    pub fn changed_count(&self) -> usize {
        self.entries.len()
    }

    /// Badge for an ABSOLUTE worktree path (the tree/viewer carry absolute
    /// paths; porcelain entries are repo-relative).
    pub fn badge_for(&self, path: &Path) -> Option<Badge> {
        self.badges.get(path).copied()
    }

    fn build_index(&mut self) {
        self.badges = self
            .entries
            .iter()
            .map(|e| (self.root.join(&e.path), e.badge()))
            .collect();
    }

    /// Set the repo root and rebuild the absolute badge index (used by the
    /// pure-parse tests; `status()` sets it directly).
    #[allow(dead_code)] // pure-parser test surface; production uses status()
    pub fn rooted(mut self, root: &Path) -> GitStatus {
        self.root = root.to_path_buf();
        self.build_index();
        self
    }
}

/// Gutter mark kinds for the viewer's git strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterMark {
    Added,
    Modified,
}

/// Per-file diff: (1-based worktree line, kind) pairs, ascending.
#[derive(Clone, Debug, Default)]
pub struct FileDiff {
    pub marks: Vec<(usize, GutterMark)>,
    pub truncated: bool,
}

#[derive(Debug)]
pub enum GitError {
    NotARepo,
    Io(std::io::Error),
    Git(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotARepo => write!(f, "not a git repository"),
            GitError::Io(e) => write!(f, "{e}"),
            GitError::Git(e) => write!(f, "git failed: {e}"),
        }
    }
}

impl std::error::Error for GitError {}

/// `git status --porcelain=v2` for `root`.
pub fn status(root: &Path) -> Result<GitStatus, GitError> {
    ensure_repo(root)?;
    let out = run(root, &["status", "--porcelain=v2"])?;
    let mut st = parse_status(&out);
    st.root = root.to_path_buf();
    st.build_index();
    Ok(st)
}

/// `git diff HEAD --unified=0 -- <path>`: staged + unstaged changes vs HEAD
/// as (worktree line, mark) pairs. Untracked files yield no marks; unmerged
/// files error out (the caller treats that as "no marks").
pub fn diff_file(root: &Path, path: &Path) -> Result<FileDiff, GitError> {
    ensure_repo(root)?;
    let out = run(
        root,
        &["diff", "HEAD", "--unified=0", "--", &path.to_string_lossy()],
    )?;
    Ok(parse_diff(&out))
}

fn ensure_repo(root: &Path) -> Result<(), GitError> {
    match run(root, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(s) if s.trim() == "true" => Ok(()),
        Ok(_) => Err(GitError::NotARepo),
        Err(GitError::Git(e)) if e.contains("not a git repository") => Err(GitError::NotARepo),
        Err(e) => Err(e),
    }
}

fn run(root: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Git(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---- porcelain=v2 parsing ----

pub fn parse_status(out: &str) -> GitStatus {
    parse_status_with_cap(out, MAX_STATUS_ENTRIES)
}

pub fn parse_status_with_cap(out: &str, cap: usize) -> GitStatus {
    let mut st = GitStatus::default();
    for line in out.lines() {
        if st.entries.len() >= cap {
            st.truncated = true;
            break;
        }
        if let Some(e) = parse_status_line(line) {
            st.entries.push(e);
        }
    }
    st.build_index();
    st
}

fn parse_status_line(line: &str) -> Option<FileStatus> {
    if let Some(rest) = line.strip_prefix("1 ") {
        // "1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>" — 8 fields; splitn
        // keeps the path (which may contain spaces) in the last field.
        let parts: Vec<&str> = rest.splitn(8, ' ').collect();
        if parts.len() < 8 {
            return None;
        }
        let xy = parts[0].as_bytes();
        if xy.len() < 2 {
            return None;
        }
        return Some(FileStatus {
            path: unquote_path(parts[7]),
            orig: None,
            staged: xy_code(xy[0]),
            unstaged: xy_code(xy[1]),
            untracked: false,
            unmerged: false,
        });
    }
    if let Some(rest) = line.strip_prefix("2 ") {
        // "2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <Xscore> <path>\t<orig>"
        let parts: Vec<&str> = rest.splitn(9, ' ').collect();
        if parts.len() < 9 {
            return None;
        }
        let xy = parts[0].as_bytes();
        if xy.len() < 2 {
            return None;
        }
        // Field 8 = the rename score; field 9 = "path\torig".
        let _score = parts[7];
        let (path, orig) = parts[8].split_once('\t')?;
        return Some(FileStatus {
            path: unquote_path(path),
            orig: Some(unquote_path(orig)),
            staged: xy_code(xy[0]),
            unstaged: xy_code(xy[1]),
            untracked: false,
            unmerged: false,
        });
    }
    if let Some(rest) = line.strip_prefix("u ") {
        // "u <m1> <m2> <m3> <head> <index> <worktree> <path>" — 7 fields.
        let parts: Vec<&str> = rest.splitn(7, ' ').collect();
        if parts.len() < 7 {
            return None;
        }
        return Some(FileStatus {
            path: unquote_path(parts[6]),
            orig: None,
            staged: None,
            unstaged: None,
            untracked: false,
            unmerged: true,
        });
    }
    if let Some(rest) = line.strip_prefix("? ") {
        return Some(FileStatus {
            path: unquote_path(rest),
            orig: None,
            staged: None,
            unstaged: None,
            untracked: true,
            unmerged: false,
        });
    }
    None // '!' ignored lines, '# ' headers, blanks
}

fn xy_code(c: u8) -> Option<StatusKind> {
    match c {
        b'M' => Some(StatusKind::Modified),
        b'A' => Some(StatusKind::Added),
        b'D' => Some(StatusKind::Deleted),
        b'T' => Some(StatusKind::TypeChanged),
        b'R' => Some(StatusKind::Renamed),
        b'C' => Some(StatusKind::Copied),
        b'U' => Some(StatusKind::Unmerged),
        _ => None, // '.' unmodified
    }
}

/// git quotes paths containing special chars: `"wei\xc3\x9f.txt"` with
/// backslash escapes (octal bytes, `\\`, `\t`, `\n`, `\"`). Unescape into
/// bytes, then lossy UTF-8.
fn unquote_path(raw: &str) -> PathBuf {
    if !raw.starts_with('"') {
        return PathBuf::from(raw);
    }
    let inner = &raw[1..raw.len().saturating_sub(1)];
    let mut bytes = Vec::new();
    let mut it = inner.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('n') => bytes.push(b'\n'),
                Some('t') => bytes.push(b'\t'),
                Some('r') => bytes.push(b'\r'),
                Some('\\') => bytes.push(b'\\'),
                Some('"') => bytes.push(b'"'),
                Some(o) if o.is_ascii_digit() => {
                    // Exactly three octal digits (\nnn).
                    let mut val = o.to_digit(8).unwrap_or(0);
                    for _ in 0..2 {
                        if let Some(d) = it.peek().and_then(|n| n.to_digit(8)) {
                            val = val * 8 + d;
                            it.next();
                        }
                    }
                    bytes.push(val as u8);
                }
                Some(other) => bytes.extend(other.to_string().as_bytes()),
                None => {}
            },
            other => bytes.extend(other.to_string().as_bytes()),
        }
    }
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

// ---- commit/push command builders (M8) ----
//
// The commands run in a PTY shell tab so their output streams into ark. The
// only inputs are repo-relative paths (porcelain output) and the typed
// commit message; both are single-quoted so the shell can never interpret
// them as syntax.

/// Quote one argument for a POSIX shell: wrap in single quotes; an embedded
/// `'` is emitted as `'\''` (close, escaped quote, reopen). Empty strings
/// become `''`.
pub fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for (i, part) in s.split('\'').enumerate() {
        if i > 0 {
            out.push_str("'\\''");
        }
        out.push_str(part);
    }
    out.push('\'');
    out
}

/// The Commit action's shell command: stage exactly `paths` (repo-relative,
/// per-file granularity) and commit them with `message`.
pub fn stage_and_commit_command(paths: &[PathBuf], message: &str) -> String {
    debug_assert!(!paths.is_empty(), "the panel validates an empty selection");
    let mut cmd = String::from("git add --");
    for p in paths {
        cmd.push(' ');
        cmd.push_str(&shell_quote(&p.to_string_lossy()));
    }
    cmd.push_str(" && git commit -m ");
    cmd.push_str(&shell_quote(message));
    cmd
}

/// The Push action's shell command.
pub const PUSH_COMMAND: &str = "git push";

// ---- unified=0 diff parsing ----

pub fn parse_diff(out: &str) -> FileDiff {
    parse_diff_with_cap(out, MAX_DIFF_MARKS)
}

pub fn parse_diff_with_cap(out: &str, cap: usize) -> FileDiff {
    let mut d = FileDiff::default();
    for line in out.lines() {
        if d.marks.len() >= cap {
            d.truncated = true;
            break;
        }
        let Some(rest) = line.strip_prefix("@@ ") else {
            continue;
        };
        // "@@ -a[,b] +c[,d] @@ context"
        let mut tokens = rest.split_whitespace();
        let (Some(old_tok), Some(new_tok)) = (tokens.next(), tokens.next()) else {
            continue;
        };
        if !old_tok.starts_with('-') || !new_tok.starts_with('+') {
            continue;
        }
        let (_old_start, old_count) = parse_range(old_tok);
        let (new_start, new_count) = parse_range(new_tok);
        // + side lines: MODIFIED when the hunk also removed lines (a
        // rewrite), ADDED when the lines are pure insertions.
        if new_count > 0 {
            let kind = if old_count > 0 { GutterMark::Modified } else { GutterMark::Added };
            for l in new_start..new_start + new_count {
                if d.marks.len() >= cap {
                    d.truncated = true;
                    return d;
                }
                d.marks.push((l, kind));
            }
        }
        // Pure deletion: no new lines — mark the deletion point.
        if old_count > 0 && new_count == 0 {
            if d.marks.len() >= cap {
                d.truncated = true;
                return d;
            }
            d.marks.push((new_start.max(1), GutterMark::Modified));
        }
    }
    d
}

/// "-12,3" → (12, 3); "-5" → (5, 1); "-0,0" → (0, 0).
fn parse_range(tok: &str) -> (usize, usize) {
    let digits = &tok[1..];
    match digits.split_once(',') {
        Some((s, c)) => (s.parse().unwrap_or(1), c.parse().unwrap_or(1)),
        None => (digits.parse().unwrap_or(1), 1),
    }
}

#[cfg(test)]
mod tests;
