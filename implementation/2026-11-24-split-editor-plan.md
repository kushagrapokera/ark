# Split editor implementation plan

Date: 2026-11-24

Design: `implementation/2026-11-24-split-editor-design.md`

## Goal and constraints

Implement two bounded side-by-side editor groups with per-group tabs and close
controls, one cached viewer per file, pinned visible viewers, responsive split
geometry, and complete App/input routing. Preserve read-only code viewing,
whitelisted config editing, the frozen dependency stack, and the <=100 MiB RSS
budget. No subagents or new dependencies.

## Task 1 (complete) — Editor-group state

Files:

- Create `src/editor_groups.rs`
- Modify `src/main.rs` to declare `mod editor_groups;`

Interfaces:

- `MAX_EDITOR_GROUPS = 2`, `MAX_TABS_PER_GROUP = 9`
- `EditorGroup { tabs, active, error }`
- `EditorGroups { groups, active, divider }`
- Methods listed in the design document.

TDD sequence:

1. Add `editor_groups::tests` covering:
   - `opening_a_second_file_creates_a_tab_in_the_active_group`
   - `opening_a_visible_path_focuses_its_existing_group`
   - `closing_the_active_tab_selects_a_neighbor`
   - `closing_a_tab_returns_it_for_eviction_only_when_no_group_owns_it`
   - `tab_lists_are_capped_and_drop_the_oldest_non_active_tab`
   - `toggling_split_adds_an_empty_secondary_group`
   - `collapsing_split_returns_secondary_tabs_for_eviction`
   - `divider_clamps_to_the_supplied_ratio_bounds`
2. Run `cargo test editor_groups::tests` and confirm expected failures.
3. Implement the smallest state model that passes.
4. Run the full suite and clippy.

Success: all state tests pass; no path appears in both groups; tab storage is
bounded; collapse/close return exact eviction candidates.

## Task 2 (complete) — Pinned cache and simultaneous viewer borrows

Files:

- Modify `src/state.rs`
- Modify `src/state.rs` inline tests (or existing test module)

Interfaces:

- `BufferCache::get_or_open_pinned(path, pinned)`
- `BufferCache::viewers_mut(paths)`
- Private `evict_oldest_except(pinned)` and
  `enforce_index_budget_except(pinned)`

TDD sequence:

1. Add tests:
   - `pinned_open_evicts_only_unpinned_viewers`
   - `visible_pinners_survive_the_index_budget`
   - `viewers_mut_returns_distinct_borrows_in_requested_order`
2. Confirm red under `cargo test state::tests`.
3. Implement pinned eviction and post-open mutable collection.
4. Verify existing LRU, cap, refresh, and budget tests still pass.

Success: opening a third file never evicts either visible split viewer; two
distinct paths can be borrowed mutably at once; existing cache bounds remain.

## Task 3 (complete) — Responsive split layout

Files:

- Modify `src/frame.rs`
- Modify `src/frame/tests.rs`

Interfaces:

- `EditorGroupLayout { tabs, breadcrumb, body, minimap }`
- `AppLayout::{editor_groups, editor_group_count, editor_group_split}`
- `app_layout_with_commit(w, h, split_x, _show_commit, show_term, group_divider,
  group_count)`
- Keep the existing five-argument wrapper at `0.5`/one group for compatibility.

TDD sequence:

1. Add layout tests:
   - `unsplit_layout_keeps_existing_editor_geometry`
   - `split_layout_divides_the_editor_card_with_a_gap`
   - `split_layout_hides_minimaps_in_narrow_groups`
   - `split_layout_clamps_both_groups_to_minimum_widths`
   - `split_layout_recomputes_after_window_resize`
2. Confirm red, then implement fixed-array group rects.
3. Keep legacy editor rects equal to group 0 when unsplit.

Success: no negative rects; divider drag geometry stays within the editor card;
normal 1024x768 unsplit geometry remains unchanged.

## Task 4 (complete) — Per-group frame chrome and interactions

Files:

- Modify `src/frame.rs`
- Modify `src/frame/chrome.rs`
- Modify `src/frame/tests.rs`

Interfaces:

- Replace `Frame::{open_files, active_path}` with `groups`, `active_group`,
  `group_scrollbars`, `viewers`, `group_split`, and `group_split_drag`.
- Extend `FrameEffects` with group-aware tab/group/split effects.
- Add `draw_editor_group(f, layout, index)` in `frame/chrome.rs`.

TDD sequence:

1. Add frame tests:
   - `split_frame_draws_two_tab_strips_and_two_viewer_bodies`
   - `clicking_a_group_tab_selects_it_and_focuses_that_group`
   - `clicking_a_tab_close_reports_the_group_and_path`
   - `clicking_group_close_reports_the_secondary_group`
   - `clicking_split_toggles_the_editor_groups`
   - `dragging_the_group_divider_updates_the_ratio`
   - `wheel_routes_to_the_viewer_under_the_pointer`
   - `empty_secondary_group_draws_its_placeholder`
2. Confirm red, then implement group tab geometry, chrome, body dispatch, and
   divider drag.
3. Preserve tree/viewer/config/terminal/source-control behavior.

Success: each group has independent tabs/body/scrollbar; all click targets match
drawn controls; no tab paints over group actions.

## Task 5 (complete) — App lifecycle and input routing

Files:

- Modify `src/main.rs`
- Modify `src/tests.rs`

Interfaces:

- `App.editor_groups: EditorGroups`
- `App.group_scrollbars: [Scrollbar; 2]`
- `App.group_split_drag: SplitDrag`
- Remove `App.open_path`; use `editor_groups.active_path()`.
- Add `open_viewer`, `toggle_editor_split`, `close_editor_group`,
  `focus_editor_group`, and group-aware `select_tab`/`close_tab`.

TDD sequence:

1. Add App tests:
   - `opening_a_visible_file_focuses_its_existing_group`
   - `closing_a_tab_evicts_only_after_the_last_group_reference`
   - `closing_the_secondary_group_collapses_to_one`
   - `config_opens_as_a_tab_in_the_active_group`
   - `fs_change_refreshes_both_visible_viewers`
   - `ctrl_w_closes_the_active_viewer_tab_before_a_terminal`
2. Confirm red, then wire rendering, wheel, copy, status, diff refresh, config
   IME/keyboard, tree double-click, and shortcuts to group state.
3. Ensure Source Control's compact rect uses the active group body when split.

Success: no stale single-pane paths remain; both visible files refresh; active
group owns keyboard/copy/wheel; config remains whitelist-enforced.

## Task 6 (complete) — Integration, memory, and documentation

Files:

- Modify `implementation/workbench-ui/progress.md`
- Update design/plan checkboxes if implementation decisions change.

Checks:

1. `cargo test`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo build --release`
4. `git diff --check`
5. Live X11 smoke:
   - Open two files in separate groups.
   - Scroll each independently.
   - Drag the divider and resize 400x300 -> 1440x900.
   - Close a tab, empty a group, and collapse the split.
6. Release stress: five large viewers + two shells, with split active and a
   resize; record RSS/PSS and require <=100 MiB.
7. Install with `./install.sh --user` and verify binary hashes match.
8. Commit the feature separately; do not push unless requested.

## Execution evidence

- Task 1: `cargo test editor_groups::tests` — 10 passed.
- Task 2: `cargo test state::tests` — 11 passed, including pinned eviction,
  pinned refresh, and simultaneous mutable borrows.
- Task 3: `cargo test frame::tests::` — all layout pins passed; unsplit
  1024×768 geometry remains unchanged.
- Task 4: split-frame test renders two viewers and exercises tab select,
  Split toggle, divider drag, and secondary-group close.
- Task 5: App tests cover duplicate focus, group collapse/eviction, config
  tabs, both-viewer fs refresh, active-tab close, and per-group wheel routing.
- Full suite: `cargo test` — 266 passed, 0 failed, 3 ignored; CLI 5/5.
- `cargo clippy --all-targets -- -D warnings` — clean.
- Live X11 smoke opened `a.txt` and `b.txt` in separate groups at 1024×768;
  screenshots: `/tmp/opencode/ark-split-live6.png`.
- Release stress with five large viewers, two shells, split active, and a
  400×300 → 1440×900 resize: **65,420 KiB = 63.9 MiB RSS**,
  **57,660 KiB = 56.3 MiB PSS**.

### Implementation deviations

- Kept the legacy single-pane frame fields as a test-only adapter so existing
  viewer/config/frame regression tests continue to exercise the same raster
  path. Production always supplies `EditorGroups`.
- Split-group minimaps appear at 480 px group width; unsplit minimap behavior is
  unchanged.
- Source Control in a compact split layout occupies the active group's body
  while the inactive group remains visible.

## Requirement map

- Full per-group tab strips/close controls: Tasks 1, 4, 5.
- Independent scrolling: Tasks 2, 4, 5.
- Same-file focus instead of duplicate memory: Tasks 1, 2, 5.
- Responsive split/divider: Tasks 3, 4, 6.
- Read-only/config boundaries: Tasks 4, 5.
- Bounded memory: Tasks 1, 2, 6.
- Error/lifecycle handling: Tasks 1, 2, 5.
- Verification evidence: Task 6.
