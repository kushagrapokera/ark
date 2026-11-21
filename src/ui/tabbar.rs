//! Horizontal tab strip widget: caller-owned fixed-size tab list with one
//! active tab (state lives with the scene, never inside the widget).

use cosmic_text::Weight;

use super::{renderable, Pointer, Rect};
use crate::render::{Canvas, Theme};
use crate::text::{TextSystem, TextStyle, UI_FONT_SIZE};

/// Fixed tab metrics (LOGICAL px). Tabs are laid out left-to-right starting
/// at the bar's left edge; the space beyond the last tab is blank track.
pub const TAB_WIDTH: f64 = 160.0;
pub const TAB_HEIGHT: f64 = 30.0;

/// Horizontal tab strip with one active tab. State is caller-owned and
/// fixed-size (`tabs` is set once by the scene, `active` is an index).
///
/// - `on_pointer` processes a completed click into an activation change.
/// - `draw` paints the bar track in `selection`, the active tab with an
///   `bg_elevated` fill, a 2px accent underline and an `fg` label; inactive
///   tabs paint `fg_dim` labels only.
#[derive(Clone, Debug, Default)]
pub struct TabBar {
    /// Tab labels in display order. Set once by the scene; never grows.
    pub tabs: Vec<String>,
    /// Index of the active tab (0 unless set).
    pub active: usize,
}

impl TabBar {
    /// Process one frame's pointer: a completed click inside tab `i`
    /// activates it and returns `true` (only when the active index changed).
    /// Clicks on the blank track, outside the bar, without a completed
    /// press→release, or into an empty tab list change nothing.
    pub fn on_pointer(&mut self, bar: Rect, pointer: Pointer) -> bool {
        // contains() already rejects degenerate bars (they contain no point).
        if !pointer.click || !bar.contains(pointer.x, pointer.y) {
            return false;
        }
        let idx = ((pointer.x - bar.x) / TAB_WIDTH).floor();
        if !idx.is_finite() {
            return false;
        }
        let idx = idx as usize;
        if idx < self.tabs.len() && idx != self.active {
            self.active = idx;
            return true;
        }
        false
    }

    /// Draw the tab strip into `bar` (one `TAB_WIDTH`-wide slot per tab).
    /// Degenerate bars rasterize nothing.
    pub fn draw(&self, canvas: &mut Canvas, bar: Rect, theme: &Theme, text: &mut TextSystem, scale: f64) {
        if !renderable(bar, scale) {
            return;
        }
        // Bar track behind the tabs (the active tab paints over it raised).
        canvas.fill_rect(bar.x, bar.y, bar.w, bar.h, theme.selection, scale);
        let font_size = (UI_FONT_SIZE as f64 * scale) as f32;
        let metrics = text.mono_metrics(font_size, (font_size * 1.35).round());
        for (i, label) in self.tabs.iter().enumerate() {
            let tab = Rect { x: bar.x + i as f64 * TAB_WIDTH, y: bar.y, w: TAB_WIDTH, h: bar.h };
            let active = i == self.active;
            if active {
                canvas.fill_rect(tab.x, tab.y, tab.w, tab.h, theme.bg_elevated, scale);
                // 2px accent underline along the tab's bottom edge.
                canvas.fill_rect(tab.x, tab.y + tab.h - 2.0, tab.w, 2.0, theme.accent, scale);
            }
            let color = if active { theme.fg } else { theme.fg_dim };
            // draw_line takes PHYSICAL px: scale the logical center; the
            // metrics term is already physical (measured at the scaled font
            // size) and must NOT be scaled again.
            let baseline = ((tab.y + tab.h / 2.0) * scale
                + f64::from(metrics.ascent - metrics.descent) / 2.0) as f32;
            text.draw_line(
                canvas,
                label,
                ((tab.x + 12.0) * scale) as f32,
                baseline,
                TextStyle { color, font_size, weight: Weight::NORMAL },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::count_color_in;

    // ---- TabBar (acceptance 3) ----

    #[test]
    fn tabbar_click_activates_tab() {
        // Custom theme (no DARK values) so hardcoded hex can't pass the
        // palette pins.
        let theme = Theme {
            bg: (10, 0, 0),
            bg_elevated: (0, 20, 0),
            fg: (0, 0, 30),
            fg_dim: (0, 40, 0),
            accent: (255, 0, 0),
            border: (5, 5, 5),
            selection: (6, 6, 6),
            syntax_keyword: (7, 0, 0),
            syntax_string: (0, 7, 0),
            syntax_number: (0, 0, 7),
            syntax_comment: (8, 8, 0),
            git_modified: (9, 0, 0),
            git_added: (0, 9, 0),
            git_deleted: (0, 0, 9),
            git_untracked: (10, 10, 0),
        };
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(600, 40);
        let bar = Rect { x: 0.0, y: 0.0, w: 600.0, h: 30.0 };
        let mut tabs = TabBar {
            tabs: vec!["main.rs".into(), "viewer".into(), "commit panel".into()],
            active: 0,
        };

        // Click inside tab 1 (160..320) → activates it.
        assert!(
            tabs.on_pointer(bar, Pointer { x: 200.0, y: 15.0, down: false, click: true }),
            "click inside tab 1 must change the active tab"
        );
        assert_eq!(tabs.active, 1);
        // Click inside tab 0 → moves back.
        assert!(tabs.on_pointer(bar, Pointer { x: 100.0, y: 15.0, down: false, click: true }));
        assert_eq!(tabs.active, 0);
        // Re-click the already-active tab → no change reported.
        assert!(!tabs.on_pointer(bar, Pointer { x: 100.0, y: 15.0, down: false, click: true }));
        assert_eq!(tabs.active, 0);
        // Click in the blank track beyond the tabs → unchanged.
        assert!(!tabs.on_pointer(bar, Pointer { x: 560.0, y: 15.0, down: false, click: true }));
        assert_eq!(tabs.active, 0);
        // Pointer present without a completed press→release → unchanged.
        assert!(!tabs.on_pointer(bar, Pointer { x: 200.0, y: 15.0, down: true, click: false }));
        assert_eq!(tabs.active, 0);
        // Click outside the bar → unchanged.
        assert!(!tabs.on_pointer(bar, Pointer { x: 200.0, y: 38.0, down: false, click: true }));
        assert_eq!(tabs.active, 0);

        // Draw with tab 0 active: accent underline + bg_elevated fill.
        canvas.fill(theme.bg);
        tabs.draw(&mut canvas, bar, &theme, &mut text, 1.0);
        let underline = count_color_in(&canvas, theme.accent, 0, 28, 160, 30);
        assert_eq!(underline, 160 * 2, "active tab underline must be accent, got {underline}");
        assert_eq!(
            count_color_in(&canvas, theme.accent, 160, 0, 600, 30),
            0,
            "inactive tabs must not paint accent"
        );
        // Measured 3844/4108 interior px exactly bg_elevated (the label's
        // ink covers the remainder); 3500 leaves generous AA margin.
        assert!(
            count_color_in(&canvas, theme.bg_elevated, 1, 1, 159, 27) >= 3500,
            "active tab fill must be bg_elevated"
        );
        assert!(
            count_color_in(&canvas, theme.fg, 0, 0, 160, 28) > 0,
            "active label ink must be fg"
        );
        assert!(
            count_color_in(&canvas, theme.fg_dim, 160, 0, 320, 28) > 0,
            "inactive label ink must be fg_dim"
        );
        assert_eq!(
            count_color_in(&canvas, theme.fg, 160, 0, 600, 28),
            0,
            "inactive labels must not use fg"
        );
        // The bar strip behind the tabs is the selection token (measured
        // ~2500/2600 exact in this window; the label ink covers the rest).
        assert!(
            count_color_in(&canvas, theme.selection, 200, 1, 300, 27) >= 2000,
            "bar track behind inactive tabs must be selection"
        );
    }

    #[test]
    fn tabbar_degenerate_empty_tabs_and_rect() {
        let theme = Theme { ..Theme::DARK };
        let mut text = TextSystem::new();
        let mut canvas = Canvas::new(200, 40);
        canvas.fill(theme.bg);
        let bar = Rect { x: 0.0, y: 0.0, w: 200.0, h: 30.0 };

        // Empty tab list: clicks never change anything; the draw paints
        // only the selection bar track (an empty strip — no labels, no
        // underline).
        let mut tabs = TabBar::default();
        assert!(!tabs.on_pointer(bar, Pointer { x: 50.0, y: 15.0, down: false, click: true }));
        tabs.draw(&mut canvas, bar, &theme, &mut text, 1.0);
        assert!(
            count_color_in(&canvas, theme.selection, 0, 0, 200, 30) >= 5000,
            "empty bar paints its selection track"
        );
        assert_eq!(count_color_in(&canvas, theme.accent, 0, 0, 200, 30), 0, "no underline");
        assert_eq!(count_color_in(&canvas, theme.fg, 0, 0, 200, 30), 0, "no label ink");

        // Zero-size bar and zero scale: no panic, nothing rasterizes.
        canvas.fill(theme.bg);
        let mut tabs2 = TabBar { tabs: vec!["a".into()], active: 0 };
        assert!(
            !tabs2.on_pointer(Rect { x: 0.0, y: 0.0, w: 0.0, h: 30.0 }, Pointer::default()),
            "degenerate bar contains no point"
        );
        tabs2.draw(&mut canvas, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, &theme, &mut text, 1.0);
        tabs2.draw(&mut canvas, bar, &theme, &mut text, 0.0);
        assert_eq!(
            crate::ui::testutil::count_not_color(&canvas, theme.bg),
            0,
            "degenerate tabbar draws rasterize nothing"
        );
    }
}
