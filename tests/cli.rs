//! CLI integration tests: these spawn the real `ark` binary (no display
//! needed — every exercised path exits before window creation).

use std::io::Write as _;
use std::process::Command;

fn ark() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ark"))
}

#[test]
fn version_flag_prints_version() {
    let out = ark().arg("--version").output().expect("spawn ark --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "ark 0.1.0");
}

#[test]
fn help_documents_the_path_argument() {
    let out = ark().arg("--help").output().expect("spawn ark --help");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("[PATH]"), "--help must document the PATH arg");
    assert!(text.contains("Usage"), "--help must show usage");
}

#[test]
fn valid_directory_passes_validation_and_presents_first_frame() {
    // A valid directory (the workspace itself) must get past path validation
    // and complete the full open-window → raster → present pipeline. With a
    // display available, ARK_SMOKE=1 exits cleanly after the first present;
    // headless (CI) environments may fail at window creation instead — both
    // are acceptable, but a path-validation error never is.
    let out = Command::new(env!("CARGO_BIN_EXE_ark"))
        .arg(env!("CARGO_MANIFEST_DIR"))
        .env("ARK_SMOKE", "1")
        .output()
        .expect("spawn ark with project dir");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("is not a directory"),
        "a valid directory must pass path validation: {stderr}"
    );
    if out.status.success() {
        assert!(
            stderr.contains("smoke present ok"),
            "smoke run must present a frame before exiting: {stderr}"
        );
    }
}

#[test]
fn rejects_a_regular_file_as_project_path() {
    let path = std::env::temp_dir().join(format!("ark-not-a-dir-{}.txt", std::process::id()));
    {
        let mut f = std::fs::File::create(&path).expect("create temp file");
        f.write_all(b"not a directory").expect("write temp file");
    }
    let out = ark().arg(&path).output().expect("spawn ark with a file path");
    let _ = std::fs::remove_file(&path);

    assert!(!out.status.success(), "a file path must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("is not a directory"),
        "error must explain the rejection: {stderr}"
    );
}

#[test]
fn rejects_a_nonexistent_path() {
    let path = std::env::temp_dir().join("ark-no-such-dir-should-not-exist");
    let _ = std::fs::remove_dir_all(&path);
    let out = ark().arg(&path).output().expect("spawn ark with missing path");
    assert!(!out.status.success(), "a nonexistent path must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot open project path"),
        "error must explain the rejection: {stderr}"
    );
}
