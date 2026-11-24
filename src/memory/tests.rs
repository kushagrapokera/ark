//! M11 memory tests: trim gating, procfs parsing, and the caps audit.

use super::*;

#[test]
fn trim_gating_waits_for_idle_and_the_min_interval() {
    let base = Instant::now();
    let idle = TRIM_IDLE;
    assert!(!should_trim(base + idle / 2, base, None), "not idle yet");
    assert!(should_trim(base + idle, base, None), "idle deadline reached");
    assert!(
        !should_trim(base + idle + Duration::from_secs(1), base, Some(base)),
        "min interval not elapsed"
    );
    assert!(
        should_trim(base + idle + TRIM_MIN_INTERVAL, base, Some(base)),
        "idle + interval"
    );
    // Robust to non-monotonic test timestamps.
    assert!(!should_trim(base, base + idle, None));
}

#[test]
fn parse_kib_reads_status_and_smaps_values() {
    let status = "Name:\tark\nVmHWM:\t  5678 kB\nVmRSS:\t 1234 kB\n";
    assert_eq!(parse_kib(status, "VmRSS:"), Some(1234));
    assert_eq!(parse_kib(status, "VmHWM:"), Some(5678));
    assert_eq!(parse_kib(status, "Pss:"), None);
    let smaps = "Rss:  2222 kB\nPss:   901 kB\n";
    assert_eq!(parse_kib(smaps, "Pss:"), Some(901));
    assert_eq!(parse_kib("garbage", "VmRSS:"), None);
}

#[cfg(target_os = "linux")]
#[test]
fn report_reads_this_process() {
    let r = report().expect("/proc exists on Linux");
    assert!(r.rss_kib > 0);
    assert!(r.hwm_kib >= r.rss_kib);
    assert!(r.pss_kib <= r.rss_kib, "PSS never exceeds RSS");
}

#[test]
fn trim_is_safe_to_call() {
    let _ = trim_heap();
    let _ = trim_heap();
}

/// M11 caps audit: these are the documented fixed limits; this test fails if
/// an accidental change lets any of them grow without bound.
#[test]
#[allow(clippy::assertions_on_constants)] // the point IS asserting frozen constants
fn caps_stay_bounded() {
    assert!(crate::state::INDEX_BUDGET_BYTES <= 32 * 1024 * 1024);
    assert!(crate::viewer::VIEW_CAP_BYTES <= 64 * 1024 * 1024);
    assert!(crate::term::SCROLLBACK_LINES <= 2000);
    assert!(crate::term::MAX_TERM_TABS <= 8);
    assert!(crate::git::MAX_STATUS_ENTRIES <= 100_000);
    assert!(crate::git::MAX_DIFF_MARKS <= 4000);
    assert!(crate::fs::MAX_ENTRIES_PER_DIR <= 50_000);
    assert!(crate::fs::MAX_CONFIG_BYTES <= 256 * 1024);
    assert!(crate::config::MAX_CONFIG_FILE_BYTES <= 64 * 1024);
    assert!(crate::config::MAX_EDITABLE_PATTERNS <= 256);
    assert!(crate::configed::env::MAX_ENV_LINES <= 4000);
    assert!(crate::configed::text::MAX_UNDO_BYTES <= 512 * 1024);
}
