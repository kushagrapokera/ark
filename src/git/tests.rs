//! Git engine tests: pure porcelain-v2 / unified-0 parsers (fixtures) and
//! LIVE subprocess tests against real temp repos.

use super::*;
use crate::fs::testutil::temp_dir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn entry(path: &str, staged: Option<StatusKind>, unstaged: Option<StatusKind>) -> FileStatus {
    FileStatus {
        path: PathBuf::from(path),
        orig: None,
        staged,
        unstaged,
        untracked: false,
        unmerged: false,
    }
}

// ---- porcelain=v2 parser ----

#[test]
fn parse_status_xy_pairs() {
    let out = "\
1 MM N... 000000 100644 100644 111 222 src/main.rs
1 M. N... 000000 100644 100644 111 222 staged-only.txt
1 .M N... 000000 100644 100644 111 222 worktree-only.txt
1 A. N... 000000 100644 100644 111 222 new.txt
1 D. N... 000000 100644 100644 111 222 staged-gone.txt
1 .D N... 000000 100644 100644 111 222 worktree-gone.txt
1 T. N... 000000 100644 100755 111 222 typechange.sh
";
    let st = parse_status(out);
    assert_eq!(st.entries.len(), 7);
    assert_eq!(st.entries[0], entry("src/main.rs", Some(StatusKind::Modified), Some(StatusKind::Modified)));
    assert_eq!(st.entries[1], entry("staged-only.txt", Some(StatusKind::Modified), None));
    assert_eq!(st.entries[2], entry("worktree-only.txt", None, Some(StatusKind::Modified)));
    assert_eq!(st.entries[3], entry("new.txt", Some(StatusKind::Added), None));
    assert_eq!(st.entries[4], entry("staged-gone.txt", Some(StatusKind::Deleted), None));
    assert_eq!(st.entries[5], entry("worktree-gone.txt", None, Some(StatusKind::Deleted)));
    assert_eq!(st.entries[6], entry("typechange.sh", Some(StatusKind::TypeChanged), None));
    assert!(!st.truncated);
}

#[test]
fn parse_status_untracked_unmerged_ignored_and_headers() {
    let out = "\
# branch.oid 1234
? untracked.txt
! ignored-should-not-appear.txt
u 100644 100644 100644 111 222 333 conflicted.txt
";
    let st = parse_status(out);
    assert_eq!(st.entries.len(), 2, "ignored skipped, headers skipped");
    let un = &st.entries[0];
    assert!(un.untracked && !un.staged.is_some() && !un.unstaged.is_some());
    assert_eq!(un.badge(), Badge::Untracked);
    let um = &st.entries[1];
    assert!(um.unmerged);
    assert_eq!(um.badge(), Badge::Modified, "unmerged → M");
}

#[test]
fn parse_status_renames_carry_orig_path() {
    let out = "2 R. N... 000000 100644 100644 111 222 R100 new-name.txt\told-name.txt\n";
    let st = parse_status(out);
    assert_eq!(st.entries.len(), 1);
    let e = &st.entries[0];
    assert_eq!(e.path, PathBuf::from("new-name.txt"));
    assert_eq!(e.orig.as_deref(), Some(Path::new("old-name.txt")));
    assert_eq!(e.staged, Some(StatusKind::Renamed));
    assert_eq!(e.badge(), Badge::Modified, "renames badge M on the new path");
}

#[test]
fn parse_status_quoted_paths_unescape() {
    // git quotes paths with special chars: leading ", backslash escapes.
    let out = "1 .M N... 000000 100644 100644 111 222 \"wei\\303\\237.txt\"\n";
    let st = parse_status(out);
    assert_eq!(st.entries[0].path, PathBuf::from("weiß.txt"), "octal UTF-8 unescaped");
}

#[test]
fn parse_status_caps_entries_and_flags_truncation() {
    let mut out = String::new();
    for i in 0..50 {
        out.push_str(&format!("? file{i}.txt\n"));
    }
    let st = parse_status_with_cap(&out, 10);
    assert_eq!(st.entries.len(), 10);
    assert!(st.truncated, "cap exceeded → truncated flag");
    let full = parse_status(&out);
    assert_eq!(full.entries.len(), 50);
    assert!(!full.truncated);
}

// ---- badge priority ----

#[test]
fn badge_priority_unstaged_beats_staged_untracked_beats_all() {
    assert_eq!(
        entry("a", Some(StatusKind::Added), Some(StatusKind::Modified)).badge(),
        Badge::Modified,
        "worktree state wins"
    );
    assert_eq!(
        entry("a", Some(StatusKind::Added), None).badge(),
        Badge::Added
    );
    let mut un = entry("a", Some(StatusKind::Added), Some(StatusKind::Modified));
    un.untracked = true;
    assert_eq!(un.badge(), Badge::Untracked, "untracked overrides XY");
    assert_eq!(entry("a", None, Some(StatusKind::Deleted)).badge(), Badge::Deleted);
    assert_eq!(
        entry("a", Some(StatusKind::Copied), None).badge(),
        Badge::Added,
        "copies introduce a new path → A"
    );
}

#[test]
fn status_counts_and_lookups() {
    let out = "? a.txt\n1 .M N... 000000 100644 100644 111 222 b.txt\n";
    let st = parse_status(out);
    assert_eq!(st.changed_count(), 2);
    assert_eq!(st.badge_for(Path::new("b.txt")), Some(Badge::Modified));
    assert_eq!(st.badge_for(Path::new("a.txt")), Some(Badge::Untracked));
    assert_eq!(st.badge_for(Path::new("missing")), None);
}

// ---- unified=0 diff parser ----

#[test]
fn parse_diff_added_and_deletion_marks() {
    let out = "\
diff --git a/f.txt b/f.txt
index 111..222 100644
--- a/f.txt
+++ b/f.txt
@@ -3 +3 @@ old line
@@ -10,4 +10,0 @@ removed block
@@ -20,0 +21,3 @@ appended block
";
    let d = parse_diff(out);
    assert_eq!(
        d.marks,
        vec![
            (3, GutterMark::Modified),   // changed line (old 3 → new 3)
            (10, GutterMark::Modified),  // pure deletion → mark at the point
            (21, GutterMark::Added),
            (22, GutterMark::Added),
            (23, GutterMark::Added),
        ]
    );
    assert!(!d.truncated);
}

#[test]
fn parse_diff_new_file_and_no_hunks() {
    let new_file = "\
diff --git a/f.txt b/f.txt
new file mode 100644
@@ -0,0 +1,3 @@
";
    assert_eq!(
        parse_diff(new_file).marks,
        vec![(1, GutterMark::Added), (2, GutterMark::Added), (3, GutterMark::Added)]
    );
    let empty = parse_diff("");
    assert!(empty.marks.is_empty());
    assert!(!empty.truncated);
    // No hunks (e.g. binary) → empty, not truncated.
    assert!(parse_diff("Binary files a/x and b/x differ\n").marks.is_empty());
}

#[test]
fn parse_diff_caps_marks_and_flags_truncation() {
    let mut out = String::new();
    for i in 0..50 {
        out.push_str(&format!("@@ -{},1 +{},1 @@\n", i + 1, i + 1));
    }
    let d = parse_diff_with_cap(&out, 10);
    assert_eq!(d.marks.len(), 10);
    assert!(d.truncated);
}

// ---- LIVE subprocess tests (real git binary) ----

/// Run git in `cwd`; assert success and return stdout.
fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Create a repo with one committed file (10 lines).
fn init_repo_with_file(label: &str) -> (crate::fs::testutil::TempDir, PathBuf) {
    let d = temp_dir(label);
    let p = d.path().join("f.txt");
    std::fs::write(&p, "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\n")
        .expect("write");
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.email", "test@ark"]);
    git(d.path(), &["config", "user.name", "test"]);
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-q", "-m", "init"]);
    (d, p)
}

#[test]
fn live_status_tracks_worktree_index_and_untracked() {
    let (d, f) = init_repo_with_file("live-status");
    // Clean repo: no entries.
    let st = status(d.path()).expect("repo status");
    assert_eq!(st.entries.len(), 0);

    // Worktree modification → unstaged M.
    thread::sleep(Duration::from_millis(20));
    std::fs::write(&f, "changed\n").expect("edit");
    let st = status(d.path()).expect("status");
    assert_eq!(st.badge_for(&f), Some(Badge::Modified), "badge lookup by absolute path");

    // Untracked file → U.
    std::fs::write(d.path().join("new.txt"), "new").expect("create");
    let st = status(d.path()).expect("status");
    assert_eq!(st.changed_count(), 2);
    assert_eq!(st.badge_for(&d.path().join("new.txt")), Some(Badge::Untracked));

    // Stage the new file → A.
    git(d.path(), &["add", "new.txt"]);
    let st = status(d.path()).expect("status");
    assert_eq!(st.badge_for(&d.path().join("new.txt")), Some(Badge::Added));

    // Not a repo → NotARepo.
    let plain = temp_dir("live-status-norepo");
    assert!(matches!(status(plain.path()), Err(GitError::NotARepo)));
}

// ---- M8: commit/push command builders ----

#[test]
fn shell_quote_neutralizes_every_shell_metacharacter() {
    assert_eq!(shell_quote("plain"), "'plain'");
    assert_eq!(shell_quote(""), "''");
    assert_eq!(shell_quote("two words"), "'two words'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    assert_eq!(shell_quote("a;b|c&&d>e"), "'a;b|c&&d>e'");
    assert_eq!(shell_quote("weiß λ"), "'weiß λ'");
}

#[test]
fn stage_and_commit_command_quotes_paths_and_message() {
    let paths = vec![PathBuf::from("src/main.rs"), PathBuf::from("two words.txt"), PathBuf::from("it's.txt")];
    assert_eq!(
        stage_and_commit_command(&paths, "fix: it's done"),
        "git add -- 'src/main.rs' 'two words.txt' 'it'\\''s.txt' && git commit -m 'fix: it'\\''s done'"
    );
    assert_eq!(PUSH_COMMAND, "git push");
}

#[test]
fn live_commit_command_stages_only_selected_paths() {
    let (d, f) = init_repo_with_file("live-commit");
    // Edit the tracked file and add an untracked file with a space.
    thread::sleep(Duration::from_millis(20));
    std::fs::write(&f, "line 1\nCHANGED\n").expect("edit");
    let spaced = d.path().join("two words.txt");
    std::fs::write(&spaced, "new file\n").expect("create");
    let st = status(d.path()).expect("status");
    assert_eq!(st.changed_count(), 2);

    // Select only the tracked file; the untracked file must stay out.
    let selected: Vec<PathBuf> = st
        .entries
        .iter()
        .filter(|e| e.path == Path::new("f.txt"))
        .map(|e| e.path.clone())
        .collect();
    let cmd = stage_and_commit_command(&selected, "feat: it's selected");
    let out = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(d.path())
        .output()
        .expect("sh runs");
    assert!(
        out.status.success(),
        "command failed: {cmd}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(git(d.path(), &["log", "-1", "--format=%s"]).trim(), "feat: it's selected");
    assert_eq!(git(d.path(), &["show", "HEAD:f.txt"]), "line 1\nCHANGED\n");
    let after = status(d.path()).expect("status after commit");
    assert_eq!(after.changed_count(), 1, "only the untracked file remains");
    assert!(after.entries[0].untracked);
    assert_eq!(after.entries[0].path, PathBuf::from("two words.txt"));
}

#[test]
fn live_push_command_updates_the_remote() {
    let d = temp_dir("live-push");
    let remote = d.path().join("remote.git");
    let work = d.path().join("work");
    std::fs::create_dir(&work).expect("create work");
    git(d.path(), &["init", "--bare", "-q", remote.to_str().expect("utf8")]);
    git(&work, &["init", "-q"]);
    git(&work, &["config", "user.email", "test@ark"]);
    git(&work, &["config", "user.name", "test"]);
    std::fs::write(work.join("f.txt"), "one\n").expect("write");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "one"]);
    git(&work, &["remote", "add", "origin", remote.to_str().expect("utf8")]);
    git(&work, &["push", "-q", "-u", "origin", "HEAD"]);

    // A second commit goes up with the plain push command.
    std::fs::write(work.join("f.txt"), "two\n").expect("edit");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "two"]);
    let out = Command::new("sh")
        .arg("-c")
        .arg(PUSH_COMMAND)
        .current_dir(&work)
        .output()
        .expect("sh runs");
    assert!(
        out.status.success(),
        "push failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let local = git(&work, &["rev-parse", "HEAD"]);
    let remote_head = Command::new("git")
        .args(["--git-dir", remote.to_str().expect("utf8"), "rev-parse", "HEAD"])
        .output()
        .expect("git runs");
    assert!(remote_head.status.success());
    assert_eq!(String::from_utf8_lossy(&remote_head.stdout), local, "remote head is the local head");
}

#[test]
fn live_diff_marks_edits_deletions_and_appends() {
    let (d, f) = init_repo_with_file("live-diff");
    // Edit line 3, delete line 5, append two lines after line 10.
    let edited = "\
line 1
line 2
LINE 3 EDITED
line 4
line 6
line 7
line 8
line 9
line 10
APPENDED A
APPENDED B
";
    thread::sleep(Duration::from_millis(20));
    std::fs::write(&f, edited).expect("edit");
    let d_diff = diff_file(d.path(), &f).expect("diff");
    // git folds the line-3 edit + line-5 deletion into one hunk: the
    // worktree lines that differ are 3 and 4 (both MODIFIED).
    assert_eq!(
        d_diff.marks,
        vec![(3, GutterMark::Modified), (4, GutterMark::Modified), (10, GutterMark::Added), (11, GutterMark::Added)]
    );
    assert!(
        d_diff.marks.contains(&(10, GutterMark::Added))
            && d_diff.marks.contains(&(11, GutterMark::Added)),
        "appends marked: {:?}",
        d_diff.marks
    );

    // Committed state → no marks.
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-q", "-m", "edit"]);
    let d2 = diff_file(d.path(), &f).expect("diff");
    assert!(d2.marks.is_empty(), "clean file: no marks");
}
