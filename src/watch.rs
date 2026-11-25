//! fs watcher (M5): notify's internal thread drives inotify; every event
//! coalesces into ONE pending flag, and a wake (`EventLoopProxy<FsWake>`)
//! asks the event loop to check it. No path buffers, no queues — event
//! storms cannot grow memory. Correctness comes from re-listing (tree) and
//! stat-ing (viewer), never from trusting event paths.
//!
//! The `notify::RecommendedWatcher` value is kept alive by this struct:
//! dropping it stops watching.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use notify::Watcher as _;
use winit::event_loop::EventLoopProxy;

/// UserEvent payload for app-level wake-ups: fs activity (M5) or PTY data
/// ready to drain (M6). The event loop coalesces naturally — handlers do
/// bounded work per wake.
#[derive(Clone, Debug)]
pub enum Wake {
    /// fs activity happened — check the fs pending flag.
    FsChanged,
    /// A terminal tab has PTY bytes ready to drain.
    PtyData,
    /// A git status subprocess finished (None = not a repo / git missing).
    GitStatus(Option<std::sync::Arc<crate::git::GitStatus>>),
    /// A git diff subprocess finished (stale results are dropped by path).
    GitDiff {
        path: std::path::PathBuf,
        diff: std::sync::Arc<crate::git::FileDiff>,
    },
    /// A bounded project keyword-search worker finished.
    SearchFinished(Box<crate::search::SearchReport>),
}

/// Coalescing flag shared with notify's event thread: `signal` sets it,
/// `take` consumes it. Wakes fire only on the false→true transition, so a
/// storm produces at most one wake per processed batch.
pub struct FsPending(AtomicBool);

impl Default for FsPending {
    fn default() -> Self {
        Self::new()
    }
}

impl FsPending {
    pub fn new() -> Self {
        FsPending(AtomicBool::new(false))
    }

    /// Mark fs activity (idempotent).
    pub fn signal(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Consume the flag: `true` when activity is pending.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

pub struct Watcher {
    /// Kept alive for the process lifetime; dropping it stops watching.
    _watcher: notify::RecommendedWatcher,
    pending: Arc<FsPending>,
}

impl Watcher {
    /// Watch `root` recursively. Errors (missing root, watch limits) are
    /// logged by the caller — ark then runs WITHOUT live refresh, never
    /// crashes.
    pub fn new(root: &Path, proxy: EventLoopProxy<Wake>) -> Result<Self, notify::Error> {
        let pending = Arc::new(FsPending::new());
        let flag = pending.clone();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = &res {
                // Watch-limit churn and transient errors still count as
                // "something changed" — the app re-lists and re-stats anyway.
                log::warn!("fs watch error (treated as a change): {e}");
            }
            // Wake only on the false→true transition: the flag coalesces a
            // whole burst into one event-loop iteration. (Single notify
            // thread, so the take→signal gap cannot lose a signal.)
            if !flag.take() {
                flag.signal();
                let _ = proxy.send_event(Wake::FsChanged);
            }
        })?;
        watcher.watch(root, notify::RecursiveMode::Recursive)?;
        Ok(Watcher { _watcher: watcher, pending })
    }

    /// Consume the pending flag: `false` → nothing changed, skip processing.
    pub fn take_pending(&self) -> bool {
        self.pending.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_flag_coalesces_and_consumes() {
        let flag = FsPending::new();
        assert!(!flag.take(), "fresh flag: nothing pending");
        flag.signal();
        assert!(flag.take(), "signal → pending");
        assert!(!flag.take(), "take consumes");
        // Idempotent signaling stays a single pending unit.
        flag.signal();
        flag.signal();
        flag.signal();
        assert!(flag.take());
        assert!(!flag.take());
    }
}
