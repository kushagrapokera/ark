//! App state (M4): the LRU buffer cache. Open viewers are kept warm —
//! re-selecting a recently viewed file is instant — with a hard cap so the
//! cache can never grow with browsing history. Evicting drops the mmap and
//! the line index (memory rule: any structure that grows without bound is a
//! bug). The git status map joins here in M7.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::viewer::{Viewer, ViewerError};

/// Aggregate heap budget for all cached viewer line indexes (M11). The file
/// count cap alone is not enough: several large indexed files could add up
/// without bound. When exceeded, the LRU viewer is dropped.
pub const INDEX_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// LRU cache of open viewers. `order` holds paths oldest-first; the most
/// recently used (hit or opened) is promoted to the back, and insertion over
/// the cap evicts from the front.
pub struct BufferCache {
    cap: usize,
    index_budget: usize,
    entries: HashMap<PathBuf, Viewer>,
    order: VecDeque<PathBuf>,
}

impl BufferCache {
    /// Cache holding at most `max(1, cap)` viewers and the default aggregate
    /// index budget.
    pub fn new(cap: usize) -> Self {
        Self::with_budget(cap, INDEX_BUDGET_BYTES)
    }

    /// Cache with an explicit aggregate line-index budget (tests use small
    /// values; the open file is always retained even if it alone exceeds it).
    pub fn with_budget(cap: usize, index_budget: usize) -> Self {
        BufferCache {
            cap: cap.max(1),
            index_budget: index_budget.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// The cached viewer for `path`, opening it on a miss (I/O happens only
    /// here). A hit promotes the entry to most-recently-used; a miss that
    /// pushes the cache over the cap evicts the least-recently-used viewer,
    /// dropping its mmap and line index.
    pub fn get_or_open(&mut self, path: &Path) -> Result<&mut Viewer, ViewerError> {
        if self.entries.contains_key(path) {
            self.promote(path);
        } else {
            let viewer = Viewer::open(path)?;
            if self.entries.len() >= self.cap {
                self.evict_oldest();
            }
            self.entries.insert(path.to_path_buf(), viewer);
            self.order.push_back(path.to_path_buf());
            self.enforce_index_budget();
        }
        Ok(self
            .entries
            .get_mut(path)
            .expect("just inserted or promoted"))
    }

    /// Move `path` to the most-recently-used position (O(n) over ≤ cap
    /// entries, so bounded).
    fn promote(&mut self, path: &Path) {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            let p = self.order.remove(pos).expect("position from len");
            self.order.push_back(p);
        }
    }

    /// Drop the least-recently-used viewer (its mmap and index free with it).
    /// Returns whether an entry was removed.
    fn evict_oldest(&mut self) -> bool {
        if let Some(oldest) = self.order.pop_front() {
            if let Some(evicted) = self.entries.remove(&oldest) {
                log::debug!("evicted viewer for {} (cache at cap)", evicted.path().display());
                return true;
            }
        }
        false
    }

    /// Sum the cached line-index heap bytes.
    fn total_index_bytes(&self) -> usize {
        self.entries.values().map(Viewer::index_bytes).sum()
    }

    /// MEMORY RULE (M11): evict LRU viewers until the summed line indices fit
    /// `index_budget`. The most-recently-opened entry is always kept, so the
    /// pane can never lose its open file to the budget.
    fn enforce_index_budget(&mut self) {
        while self.entries.len() > 1 && self.total_index_bytes() > self.index_budget {
            if !self.evict_oldest() {
                break;
            }
        }
    }

    /// Re-open `path` from disk, replacing the cached viewer (M5 live
    /// refresh). The old mapping is dropped when the new open succeeds; the
    /// read position carries over (clamped at the next draw's self-heal) and
    /// the selection drops with the old view. On open failure the OLD viewer
    /// stays cached — a vanished/binary file never blanks the pane — and the
    /// error is returned.
    pub fn refresh(&mut self, path: &Path) -> Result<&mut Viewer, ViewerError> {
        let old_offset = self.entries.get(path).map(|v| v.offset_lines());
        let mut viewer = Viewer::open(path)?;
        if let Some(off) = old_offset {
            viewer.restore_offset(off);
        }
        self.entries.insert(path.to_path_buf(), viewer);
        self.enforce_index_budget();
        Ok(self
            .entries
            .get_mut(path)
            .expect("just replaced"))
    }

    /// Whether `path` is currently cached.
    #[allow(dead_code)] // cache-assertion surface (LRU tests); M7 diagnostics consume it
    pub fn contains(&self, path: &Path) -> bool {
        self.entries.contains_key(path)
    }

    /// Cached paths in MOST-RECENTLY-FIRST order (the file-tab strip order).
    pub fn lru_paths(&self) -> Vec<PathBuf> {
        self.order.iter().rev().cloned().collect()
    }

    /// Evict one exact path (file-tab close). Returns whether it was cached.
    pub fn evict(&mut self, path: &Path) -> bool {
        let before = self.entries.len();
        self.entries.remove(path);
        self.order.retain(|p| p != path);
        before != self.entries.len()
    }

    #[allow(dead_code)] // cache-assertion surface (LRU tests); M7 diagnostics consume it
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)] // cache-assertion surface (LRU tests); M7 diagnostics consume it
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::testutil::temp_dir;
    use crate::viewer::ViewerError;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    fn make_file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, format!("content of {name}\n")).expect("write");
        p
    }

    /// MEMORY RULE: the cache never grows past the cap; the oldest viewer is
    /// dropped (mmap + index) and reopening it takes its place.
    #[test]
    fn lru_evicts_oldest_and_reopens() {
        let d = temp_dir("lru");
        let a = make_file(d.path(), "a.txt");
        let b = make_file(d.path(), "b.txt");
        let c = make_file(d.path(), "c.txt");
        let mut cache = BufferCache::new(2);

        cache.get_or_open(&a).expect("open a");
        cache.get_or_open(&b).expect("open b");
        assert_eq!(cache.len(), 2);
        assert!(cache.contains(&a) && cache.contains(&b));

        // Opening c evicts a (the least recently used).
        cache.get_or_open(&c).expect("open c");
        assert_eq!(cache.len(), 2, "MEMORY RULE: cache is capped");
        assert!(!cache.contains(&a), "oldest viewer evicted");
        assert!(cache.contains(&b) && cache.contains(&c));

        // A hit promotes: b becomes the LRU.
        cache.get_or_open(&b).expect("hit b");
        assert_eq!(cache.len(), 2);

        // Opening a again evicts the new LRU (c), not the promoted b.
        cache.get_or_open(&a).expect("reopen a");
        assert_eq!(cache.len(), 2);
        assert!(!cache.contains(&c), "unpromoted c evicted before b");
        assert!(cache.contains(&a) && cache.contains(&b));

        // The evicted path opens fresh (I/O again) with correct content.
        let v = cache.get_or_open(&c).expect("reopen c");
        assert_eq!(v.line_count(), 1);
        assert!(v.is_open_for(&c), "the reopened viewer tracks its path");
    }

    #[test]
    fn open_failures_are_not_cached() {
        let d = temp_dir("lru-err");
        let binary = d.path().join("blob.bin");
        fs::write(&binary, [b'a', 0, b'b']).expect("write");
        let mut cache = BufferCache::new(4);
        assert!(matches!(
            cache.get_or_open(&binary),
            Err(crate::viewer::ViewerError::Binary { .. })
        ));
        assert!(cache.is_empty(), "failed opens leave no entry");
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cap_zero_is_clamped_to_one() {
        let d = temp_dir("lru-cap");
        let a = make_file(d.path(), "a.txt");
        let b = make_file(d.path(), "b.txt");
        let mut cache = BufferCache::new(0);
        cache.get_or_open(&a).expect("open a");
        cache.get_or_open(&b).expect("open b");
        assert_eq!(cache.len(), 1, "cap 0 → holds exactly one viewer");
        assert!(!cache.contains(&a), "a was evicted by b");
    }

    #[test]
    fn lru_paths_are_mru_first_and_evict_removes_one() {
        let d = temp_dir("lru-paths");
        let a = make_file(d.path(), "a.txt");
        let b = make_file(d.path(), "b.txt");
        let c = make_file(d.path(), "c.txt");
        let mut cache = BufferCache::new(8);
        cache.get_or_open(&a).expect("a");
        cache.get_or_open(&b).expect("b");
        cache.get_or_open(&c).expect("c");
        assert_eq!(cache.lru_paths(), vec![c.clone(), b.clone(), a.clone()]);
        assert!(cache.evict(&b));
        assert_eq!(cache.lru_paths(), vec![c.clone(), a.clone()]);
        assert!(!cache.evict(&b), "second evict is a no-op");
    }

    /// M11: the aggregate line-index heap is capped, not just the file count.
    #[test]
    fn index_budget_evicts_lru_but_keeps_the_open_file() {
        let d = temp_dir("lru-index-budget");
        let lines: String = (0..2000).map(|i| format!("line {i}\n")).collect();
        let a = d.path().join("a.txt");
        let b = d.path().join("b.txt");
        fs::write(&a, &lines).expect("write a");
        fs::write(&b, &lines).expect("write b");

        let index_bytes = {
            let mut probe = BufferCache::new(8);
            let v = probe.get_or_open(&a).expect("probe");
            assert!(v.index_bytes() > 0);
            v.index_bytes()
        };
        // A budget that fits one index but not two.
        let mut cache = BufferCache::with_budget(8, index_bytes + index_bytes / 2);
        cache.get_or_open(&a).expect("open a");
        cache.get_or_open(&b).expect("open b");
        assert_eq!(cache.len(), 1, "aggregate index budget evicts the LRU viewer");
        assert!(cache.contains(&b), "the just-opened file is retained");
        assert!(!cache.contains(&a), "older viewer evicted");

        // A single over-budget viewer is still retained (never blank the pane).
        let mut tiny = BufferCache::with_budget(8, 1);
        tiny.get_or_open(&a).expect("open sole file");
        tiny.get_or_open(&b).expect("open second");
        assert_eq!(tiny.len(), 1, "keeps exactly the open file when over budget");
        assert!(tiny.contains(&b));
    }

    // ---- M5: refresh ----

    /// MEMORY/correctness rule: a refresh re-opens from disk (dropping the
    /// old mapping), carries the raw read position, and on open failure the
    /// OLD viewer stays cached — a vanished/binary file never blanks the pane.
    #[test]
    fn refresh_replaces_content_and_keeps_offset() {
        let d = temp_dir("lru-refresh");
        let p = d.path().join("watched.txt");
        fs::write(&p, "one\ntwo\nthree\n").expect("write");
        let mut cache = BufferCache::new(4);
        {
            let v = cache.get_or_open(&p).expect("open");
            assert!(v.set_offset_lines(2, 1), "scroll into range (3 lines, 1-row viewport)");
            assert_eq!(v.offset_lines(), 2);
        }
        thread::sleep(Duration::from_millis(30));
        fs::write(&p, "one\ntwo\nthree\nfour\n").expect("rewrite");

        let v = cache.refresh(&p).expect("refresh");
        assert_eq!(v.line_count(), 4, "content re-read from disk");
        assert_eq!(v.offset_lines(), 2, "read position carried across the refresh");
        assert!(!v.changed_on_disk(), "the refreshed view matches disk");
    }

    #[test]
    fn refresh_failure_keeps_the_old_viewer() {
        let d = temp_dir("lru-refresh-err");
        let p = d.path().join("watched.txt");
        fs::write(&p, "good content\n").expect("write");
        let mut cache = BufferCache::new(4);
        cache.get_or_open(&p).expect("open");

        // Rewrite as a binary → the fresh open is refused.
        fs::write(&p, [b'a', 0, b'b']).expect("rewrite");
        assert!(matches!(cache.refresh(&p), Err(ViewerError::Binary { .. })));
        assert!(cache.contains(&p), "the old entry survives the failure");
        let v = cache.get_or_open(&p).expect("old viewer still cached");
        assert_eq!(v.line_count(), 1, "old content still served");
    }
}
