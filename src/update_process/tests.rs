use super::*;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn available_event_populates_details_and_opens_panel() {
    let mut ui = UpdateUi::default();
    ui.apply_event(
        UpdateEvent::UpdateAvailable {
            version: Version::parse("0.1.1").unwrap(),
            size: 4_212_334,
            notes_url: "https://example.com/releases/0.1.1".into(),
        },
        false,
    );
    assert_eq!(ui.state, UpdateState::Available);
    assert!(ui.open);
    assert_eq!(ui.version, Some(Version::parse("0.1.1").unwrap()));
    assert_eq!(ui.size, Some(4_212_334));
}

#[test]
fn progress_is_monotonic() {
    let mut ui = UpdateUi::default();
    for percent in [0, 42, 41, 100, 99] {
        ui.apply_event(UpdateEvent::Progress(percent), false);
    }
    assert_eq!(ui.state, UpdateState::Downloading);
    assert_eq!(ui.percent, 100);
}

#[test]
fn staged_then_installed_preserves_version() {
    let mut ui = UpdateUi::default();
    let version = Version::parse("0.1.1").unwrap();
    ui.apply_event(
        UpdateEvent::Staged {
            version: version.clone(),
        },
        false,
    );
    assert_eq!(ui.state, UpdateState::Staged);
    ui.apply_event(
        UpdateEvent::Installed {
            version: version.clone(),
        },
        false,
    );
    assert_eq!(ui.state, UpdateState::Installed);
    assert_eq!(ui.version, Some(version));
}

#[test]
fn automatic_up_to_date_stays_quiet_but_manual_check_opens_panel() {
    let mut ui = UpdateUi::default();
    let event = UpdateEvent::UpToDate {
        version: Version::parse("0.1.0").unwrap(),
    };
    ui.apply_event(event.clone(), true);
    assert_eq!(ui.state, UpdateState::Idle);
    assert!(!ui.open);
    ui.apply_event(event, false);
    assert!(ui.open);
    assert_eq!(ui.status.as_deref(), Some("ark is up to date"));
}

#[test]
fn later_closes_panel_without_discarding_available_update() {
    let mut ui = UpdateUi::default();
    ui.apply_event(
        UpdateEvent::UpdateAvailable {
            version: Version::parse("0.1.1").unwrap(),
            size: 100,
            notes_url: "https://example.com".into(),
        },
        false,
    );
    ui.later();
    assert!(!ui.open);
    assert_eq!(ui.state, UpdateState::Available);
    assert!(ui.version.is_some());
}

#[test]
fn automatic_check_gate_honors_24_hours_and_clock_rollback() {
    let (temp, layout) = temp_layout("rate-limit");
    let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
    let last_check = layout.state_dir().join("last-check");
    std::fs::create_dir_all(layout.state_dir()).unwrap();

    std::fs::write(
        &last_check,
        timestamp(now - Duration::from_secs(23 * 60 * 60)),
    )
    .unwrap();
    assert!(!automatic_check_due(&layout, now));
    std::fs::write(&last_check, timestamp(now - AUTO_CHECK_INTERVAL)).unwrap();
    assert!(automatic_check_due(&layout, now));
    std::fs::write(&last_check, timestamp(now + Duration::from_secs(60))).unwrap();
    assert!(!automatic_check_due(&layout, now));
    drop(temp);
}

#[test]
fn successful_check_writes_a_bounded_timestamp_file() {
    let (temp, layout) = temp_layout("last-check");
    let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
    record_successful_check(&layout, now).unwrap();
    let contents = std::fs::read_to_string(layout.state_dir().join("last-check")).unwrap();
    assert_eq!(contents, timestamp(now));
    drop(temp);
}

#[test]
fn controller_actions_follow_update_state() {
    let (_temp, layout) = temp_layout("controller-actions");
    let helper = PathBuf::from("/helper");
    let mut controller = UpdateController::new(Some(layout), Some(helper));
    assert_eq!(controller.title_action(), Some(UpdateAction::Check));
    assert_eq!(controller.install_command(), None);

    controller.ui.state = UpdateState::Available;
    controller.ui.version = Some(Version::parse("0.1.1").unwrap());
    assert_eq!(controller.title_action(), Some(UpdateAction::TogglePanel));
    assert_eq!(controller.install_command(), Some(UpdateCommand::Stage));

    controller.ui.state = UpdateState::Staged;
    assert_eq!(controller.install_command(), Some(UpdateCommand::Apply));
    controller.ui.open = true;
    assert_eq!(controller.title_action(), Some(UpdateAction::TogglePanel));
}

#[test]
fn unsupported_controller_disables_update_actions() {
    let controller = UpdateController::new(None, None);
    assert!(!controller.supported());
    assert_eq!(controller.title_action(), None);
    assert_eq!(controller.install_command(), None);
    assert!(controller.restart_target().is_err());
}

#[test]
fn restart_target_uses_the_current_symlink() {
    let (temp, layout) = temp_layout("restart-target");
    let current = layout.ark_data_dir().join("current");
    std::fs::create_dir_all(&current).unwrap();
    let binary = current.join("ark");
    std::fs::write(&binary, b"fixture").unwrap();
    set_executable(&binary);
    let controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    assert_eq!(controller.restart_target().unwrap(), binary);
    drop(temp);
}

#[test]
fn stale_generation_events_and_completions_are_ignored() {
    let (_temp, layout) = temp_layout("generation-guard");
    let mut controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    controller.inflight = Some(UpdateJob {
        generation: 7,
        command: UpdateCommand::Check,
        automatic: false,
        saw_error: false,
    });
    assert!(!controller.handle_event(6, UpdateEvent::Progress(50)));
    assert_eq!(controller.ui.state, UpdateState::Idle);
    assert!(!controller.finish(6, UpdateCommand::Check, true, None));
    assert!(controller.is_busy());
    assert!(controller.handle_event(7, UpdateEvent::CheckStarted));
    assert_eq!(controller.ui.state, UpdateState::Checking);
}

#[test]
fn failed_completion_synthesizes_one_actionable_error() {
    let (_temp, layout) = temp_layout("completion-error");
    let mut controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    controller.inflight = Some(UpdateJob {
        generation: 1,
        command: UpdateCommand::Check,
        automatic: false,
        saw_error: false,
    });
    assert!(controller.finish(
        1,
        UpdateCommand::Check,
        false,
        Some("helper exited with status 1".into())
    ));
    assert_eq!(controller.ui.state, UpdateState::Failed);
    assert_eq!(controller.ui.error_code.as_deref(), Some("helper-failed"));
    assert!(!controller.is_busy());
}

#[test]
fn successful_check_completion_records_timestamp() {
    let (temp, layout) = temp_layout("completion-success");
    let mut controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    controller.inflight = Some(UpdateJob {
        generation: 1,
        command: UpdateCommand::Check,
        automatic: true,
        saw_error: false,
    });
    assert!(controller.finish(1, UpdateCommand::Check, true, None));
    assert!(controller
        .layout
        .as_ref()
        .unwrap()
        .state_dir()
        .join("last-check")
        .is_file());
    drop(temp);
}

#[test]
fn error_event_on_zero_exit_does_not_record_successful_check() {
    let (temp, layout) = temp_layout("error-no-timestamp");
    let mut controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    controller.inflight = Some(UpdateJob {
        generation: 1,
        command: UpdateCommand::Check,
        automatic: true,
        saw_error: false,
    });
    assert!(controller.handle_event(
        1,
        UpdateEvent::Error {
            code: "offline".into(),
            message: "network unavailable".into(),
        }
    ));
    assert!(controller.finish(1, UpdateCommand::Check, true, None));
    assert!(!controller
        .layout
        .as_ref()
        .unwrap()
        .state_dir()
        .join("last-check")
        .exists());
    drop(temp);
}

#[test]
fn primary_actions_match_install_and_restart_states() {
    let (_temp, layout) = temp_layout("primary-actions");
    let mut controller = UpdateController::new(Some(layout), Some(PathBuf::from("/helper")));
    controller.ui.state = UpdateState::Idle;
    assert_eq!(controller.primary_action(), Some(UpdateAction::Check));
    controller.ui.state = UpdateState::Available;
    assert_eq!(controller.primary_action(), Some(UpdateAction::Install));
    controller.ui.state = UpdateState::Staged;
    assert_eq!(controller.primary_action(), Some(UpdateAction::Install));
    controller.ui.state = UpdateState::Installed;
    assert_eq!(controller.primary_action(), Some(UpdateAction::Restart));
    controller.ui.state = UpdateState::Downloading;
    assert_eq!(controller.primary_action(), None);
}

fn set_executable(path: &PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn temp_layout(name: &str) -> (PathBuf, InstallLayout) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "ark-update-ui-{name}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let layout = InstallLayout::new(
        path.join("data"),
        path.join("bin"),
        path.join("state"),
        path.join("cache"),
        path.join("config"),
    );
    (path, layout)
}

fn timestamp(time: SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
