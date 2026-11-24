use std::path::{Path, PathBuf};

/// Maximum number of side-by-side editor groups.
pub const MAX_EDITOR_GROUPS: usize = 2;
/// Eight cached viewer tabs plus the single writable config tab.
pub const MAX_TABS_PER_GROUP: usize = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenResult {
    Opened {
        group: usize,
        evicted: Option<PathBuf>,
    },
    FocusedExisting(usize),
}

#[derive(Clone, Debug, Default)]
pub struct EditorGroup {
    pub tabs: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EditorGroups {
    pub groups: Vec<EditorGroup>,
    pub active: usize,
    /// Fraction of the editor-card width assigned to the left group.
    pub divider: f64,
}

impl EditorGroups {
    pub fn new() -> Self {
        Self {
            groups: vec![EditorGroup::default()],
            active: 0,
            divider: 0.5,
        }
    }

    pub fn is_split(&self) -> bool {
        self.groups.len() == MAX_EDITOR_GROUPS
    }

    pub fn active_group(&self) -> &EditorGroup {
        &self.groups[self.active]
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active_group().active.as_deref()
    }

    pub fn visible_paths(&self) -> Vec<&Path> {
        self.groups
            .iter()
            .filter_map(|group| group.active.as_deref())
            .collect()
    }

    pub fn group_owning(&self, path: &Path) -> Option<usize> {
        self.groups
            .iter()
            .position(|group| group.tabs.iter().any(|tab| tab.as_path() == path))
    }

    /// Open in the active group, or focus the group that already owns the path.
    pub fn open(&mut self, path: PathBuf) -> OpenResult {
        if let Some(group) = self.group_owning(&path) {
            self.active = group;
            self.groups[group].active = Some(path);
            self.groups[group].error = None;
            return OpenResult::FocusedExisting(group);
        }
        let group = self.active;
        let evicted = self.insert_tab(group, path.clone());
        OpenResult::Opened { group, evicted }
    }

    pub fn select(&mut self, group: usize, path: &Path) -> Option<usize> {
        if group >= self.groups.len() {
            return None;
        }
        let owner = self.group_owning(path)?;
        self.active = owner;
        self.groups[owner].active = Some(path.to_path_buf());
        self.groups[owner].error = None;
        Some(owner)
    }

    pub fn close_tab(&mut self, group: usize, path: &Path) -> Option<PathBuf> {
        let state = self.groups.get_mut(group)?;
        let index = state.tabs.iter().position(|tab| tab == path)?;
        let removed = state.tabs.remove(index);
        if state.active.as_deref() == Some(path) {
            state.active = state
                .tabs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|i| state.tabs.get(i)))
                .cloned();
            state.error = None;
        }
        Some(removed)
    }

    /// Add a second empty group, or collapse the secondary group and return its
    /// tabs for cache eviction.
    pub fn toggle_split(&mut self) -> Vec<PathBuf> {
        if self.is_split() {
            self.close_secondary_group()
        } else {
            self.groups.push(EditorGroup::default());
            self.active = 1;
            Vec::new()
        }
    }

    pub fn close_secondary_group(&mut self) -> Vec<PathBuf> {
        if !self.is_split() {
            return Vec::new();
        }
        let closed = self.groups.pop().expect("split group exists");
        self.active = 0;
        closed.tabs
    }

    #[cfg(test)]
    pub fn set_divider(&mut self, value: f64, min: f64, max: f64) {
        if value.is_finite() {
            self.divider = value.clamp(min, max);
        }
    }

    fn insert_tab(&mut self, group: usize, path: PathBuf) -> Option<PathBuf> {
        let state = &mut self.groups[group];
        let mut evicted = None;
        if state.tabs.len() >= MAX_TABS_PER_GROUP {
            let index = state
                .tabs
                .iter()
                .position(|tab| Some(tab.as_path()) != state.active.as_deref())
                .unwrap_or(0);
            evicted = Some(state.tabs.remove(index));
        }
        state.tabs.push(path.clone());
        state.active = Some(path);
        state.error = None;
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/repo/{name}"))
    }

    #[test]
    fn opening_files_builds_tabs_in_the_active_group() {
        let mut groups = EditorGroups::new();
        assert_eq!(groups.groups.len(), 1);
        assert_eq!(groups.active, 0);
        assert_eq!(groups.divider, 0.5);

        assert_eq!(
            groups.open(path("a.txt")),
            OpenResult::Opened {
                group: 0,
                evicted: None
            }
        );
        assert_eq!(
            groups.open(path("b.txt")),
            OpenResult::Opened {
                group: 0,
                evicted: None
            }
        );
        assert_eq!(groups.groups[0].tabs, vec![path("a.txt"), path("b.txt")]);
        assert_eq!(groups.active_path(), Some(path("b.txt").as_path()));
    }

    #[test]
    fn opening_a_visible_path_focuses_its_existing_group() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        groups.toggle_split();
        groups.open(path("b.txt"));
        assert_eq!(groups.active, 1);

        assert_eq!(groups.open(path("a.txt")), OpenResult::FocusedExisting(0));
        assert_eq!(groups.active, 0);
        assert_eq!(groups.active_path(), Some(path("a.txt").as_path()));
        assert_eq!(groups.groups[0].tabs, vec![path("a.txt")]);
        assert_eq!(groups.groups[1].tabs, vec![path("b.txt")]);
    }

    #[test]
    fn selecting_a_path_focuses_the_group_that_owns_it() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        groups.toggle_split();
        groups.open(path("b.txt"));

        assert_eq!(groups.select(1, &path("a.txt")), Some(0));
        assert_eq!(groups.active, 0);
        assert_eq!(groups.active_path(), Some(path("a.txt").as_path()));
        assert_eq!(groups.select(0, &path("b.txt")), Some(1));
        assert_eq!(groups.active, 1);
    }

    #[test]
    fn closing_the_active_tab_selects_the_next_neighbor() {
        let mut groups = EditorGroups::new();
        for name in ["a.txt", "b.txt", "c.txt"] {
            groups.open(path(name));
        }
        groups.select(0, &path("b.txt"));

        assert_eq!(groups.close_tab(0, &path("b.txt")), Some(path("b.txt")));
        assert_eq!(groups.groups[0].tabs, vec![path("a.txt"), path("c.txt")]);
        assert_eq!(groups.active_path(), Some(path("c.txt").as_path()));

        groups.select(0, &path("a.txt"));
        assert_eq!(groups.close_tab(0, &path("a.txt")), Some(path("a.txt")));
        assert_eq!(groups.active_path(), Some(path("c.txt").as_path()));
    }

    #[test]
    fn closing_the_last_tab_leaves_an_empty_group() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        assert_eq!(groups.close_tab(0, &path("a.txt")), Some(path("a.txt")));
        assert!(groups.groups[0].tabs.is_empty());
        assert_eq!(groups.active_path(), None);
    }

    #[test]
    fn tab_lists_are_capped_and_drop_the_oldest_non_active_tab() {
        let mut groups = EditorGroups::new();
        for index in 0..MAX_TABS_PER_GROUP {
            groups.open(path(&format!("file-{index}.txt")));
        }
        let active = path(&format!("file-{}.txt", MAX_TABS_PER_GROUP - 1));
        assert_eq!(groups.active_path(), Some(active.as_path()));

        let result = groups.open(path("new.txt"));
        assert_eq!(
            result,
            OpenResult::Opened {
                group: 0,
                evicted: Some(path("file-0.txt"))
            }
        );
        assert_eq!(groups.groups[0].tabs.len(), MAX_TABS_PER_GROUP);
        assert!(!groups.groups[0].tabs.contains(&path("file-0.txt")));
        assert!(groups.groups[0].tabs.contains(&active));
        assert_eq!(groups.active_path(), Some(path("new.txt").as_path()));
    }

    #[test]
    fn toggling_split_adds_an_empty_active_secondary_group() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        assert_eq!(groups.toggle_split(), Vec::<PathBuf>::new());
        assert!(groups.is_split());
        assert_eq!(groups.active, 1);
        assert_eq!(groups.active_path(), None);

        groups.open(path("b.txt"));
        assert_eq!(groups.active_path(), Some(path("b.txt").as_path()));
        assert_eq!(groups.visible_paths(), vec![path("a.txt"), path("b.txt")]);
    }

    #[test]
    fn collapsing_split_returns_secondary_tabs_for_eviction() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        groups.toggle_split();
        groups.open(path("b.txt"));
        groups.open(path("c.txt"));

        assert_eq!(groups.toggle_split(), vec![path("b.txt"), path("c.txt")]);
        assert!(!groups.is_split());
        assert_eq!(groups.active, 0);
        assert_eq!(groups.visible_paths(), vec![path("a.txt")]);
    }

    #[test]
    fn closing_the_secondary_group_collapses_to_one() {
        let mut groups = EditorGroups::new();
        groups.open(path("a.txt"));
        groups.toggle_split();
        groups.open(path("b.txt"));

        assert_eq!(groups.close_secondary_group(), vec![path("b.txt")]);
        assert_eq!(groups.groups.len(), 1);
        assert_eq!(groups.active, 0);
    }

    #[test]
    fn divider_clamps_to_the_supplied_ratio_bounds() {
        let mut groups = EditorGroups::new();
        groups.set_divider(0.1, 0.25, 0.75);
        assert_eq!(groups.divider, 0.25);
        groups.set_divider(0.9, 0.25, 0.75);
        assert_eq!(groups.divider, 0.75);
        groups.set_divider(f64::NAN, 0.25, 0.75);
        assert_eq!(
            groups.divider, 0.75,
            "non-finite input keeps the prior ratio"
        );
    }
}
