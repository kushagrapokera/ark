//! Memory hardening (M11): idle `malloc_trim`, a Linux memory report and the
//! caps audit gate.
//!
//! The trim is glibc/Linux-only; elsewhere it is an explicit no-op. RSS/PSS
//! reads are `/proc` text (the host has no heaptrack/smem); the parser is
//! pure and unit-tested so the report itself stays trustworthy.

use std::time::{Duration, Instant};

/// Idle time before returning free heap pages to the OS.
pub const TRIM_IDLE: Duration = Duration::from_secs(3);
/// Never trim more often than this (avoids allocator churn on idle wakeups).
pub const TRIM_MIN_INTERVAL: Duration = Duration::from_secs(30);

/// One `/proc` snapshot. KiB units, as the kernel reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemReport {
    pub rss_kib: u64,
    pub hwm_kib: u64,
    pub pss_kib: u64,
}

/// Whether idle trimming is due now. Pure: exercised by the timing tests.
pub fn should_trim(now: Instant, last_activity: Instant, last_trim: Option<Instant>) -> bool {
    let idle = now
        .checked_duration_since(last_activity)
        .is_some_and(|d| d >= TRIM_IDLE);
    let interval_ok = match last_trim {
        Some(t) => now
            .checked_duration_since(t)
            .is_some_and(|d| d >= TRIM_MIN_INTERVAL),
        None => true,
    };
    idle && interval_ok
}

/// Return free heap pages to the OS (glibc `malloc_trim(0)`). A no-op on
/// non-glibc targets; always safe to call.
pub fn trim_heap() -> bool {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // Safety: malloc_trim has no preconditions and is called on the main
        // thread while the app is idle.
        unsafe { libc::malloc_trim(0) != 0 }
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        false
    }
}

/// Read VmRSS/VmHWM/Pss from `/proc` (`None` outside Linux or if unavailable).
pub fn report() -> Option<MemReport> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let rss_kib = parse_kib(&status, "VmRSS:")?;
    let hwm_kib = parse_kib(&status, "VmHWM:").unwrap_or(rss_kib);
    let pss_kib = std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .and_then(|s| parse_kib(&s, "Pss:"))
        .unwrap_or(0);
    Some(MemReport { rss_kib, hwm_kib, pss_kib })
}

/// Parse `"Key:\t123 kB"` from procfs text. Pure (test fixtures).
pub fn parse_kib(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests;
