//! Card-workbench chrome from the supplied design. All placeholder controls
//! are drawn from static labels/fixed arrays; no chrome state or cache is
//! retained between frames.

use cosmic_text::Weight;

use crate::icons::{self, Icon};
use crate::render::ColorRgb;
use crate::text::TextStyle;
use crate::ui::{draw_button, Press, Rect};

use super::{ActivityPanel, AppLayout, Frame, CARD_RADIUS};

fn text(f: &mut Frame, label: &str, x: f64, y: f64, size: f64, color: ColorRgb, bold: bool) {
    let font_size = (size * f.scale) as f32;
    let m = f.text.mono_metrics(font_size, font_size * 1.35);
    let baseline = (y + size / 2.0) * f.scale + f64::from(m.ascent - m.descent) / 2.0;
    f.text.draw_line(
        f.canvas,
        label,
        (x * f.scale) as f32,
        baseline as f32,
        TextStyle {
            color,
            font_size,
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        },
    );
}

fn icon(f: &mut Frame, icon: Icon, cx: f64, cy: f64, size: f64, color: ColorRgb) {
    icons::draw(
        f.canvas.pixmap_mut(),
        icon,
        (cx * f.scale) as f32,
        (cy * f.scale) as f32,
        (size * f.scale) as f32,
        color,
        f.scale,
    );
}

fn card(f: &mut Frame, rect: Rect) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    f.canvas.fill_round_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        CARD_RADIUS,
        f.theme.border_subtle,
        f.scale,
    );
    f.canvas.fill_round_rect(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        (CARD_RADIUS - 1.0).max(0.0),
        f.theme.surface,
        f.scale,
    );
}

pub(super) fn draw_title_bar(f: &mut Frame, layout: &AppLayout) {
    let bar = layout.title_bar;
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.surface_chrome, f.scale);
    icon(f, Icon::Code, 20.0, 18.0, 16.0, f.theme.accent);

    let full = bar.w >= 1024.0;
    let medium = bar.w >= 720.0;
    if full {
        let menus = ["File", "Edit", "Selection", "View", "Go"];
        let mut x = 44.0;
        for label in menus {
            text(f, label, x, 9.0, 11.0, f.theme.text_muted, false);
            x += match label {
                "Selection" => 66.0,
                "File" | "Edit" | "View" => 42.0,
                _ => 30.0,
            };
        }
        icon(f, Icon::More, x + 4.0, 18.0, 14.0, f.theme.text_muted);
        icon(
            f,
            Icon::ChevronLeft,
            x + 42.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::ChevronRight,
            x + 64.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
    }

    let (reserve, left, show_update) = if full {
        (190.0, 340.0, true)
    } else if medium {
        (190.0, 40.0, true)
    } else if bar.w >= 520.0 {
        (100.0, 40.0, true)
    } else if bar.w >= 300.0 {
        (72.0, 40.0, true)
    } else {
        (12.0, 40.0, false)
    };
    let command_w = (bar.w - left - reserve).clamp(0.0, 320.0);
    if command_w > 0.0 {
        let command_x = left + (bar.w - left - reserve - command_w) / 2.0;
        let command = Rect {
            x: command_x,
            y: 6.0,
            w: command_w,
            h: 24.0,
        };
        f.canvas.fill_round_rect(
            command.x,
            command.y,
            command.w,
            command.h,
            4.0,
            f.theme.surface,
            f.scale,
        );
        f.canvas.stroke_rect_strips(
            command.x,
            command.y,
            command.w,
            command.h,
            f.theme.border,
            f.scale,
        );
        icon(
            f,
            Icon::Search,
            command.x + 14.0,
            18.0,
            13.0,
            f.theme.text_muted,
        );
        let max_chars = ((command.w - 38.0) / 6.6).floor().max(1.0) as usize;
        let title = super::truncate_chars(f.title, max_chars);
        text(f, &title, command.x + 28.0, 9.0, 11.0, f.theme.fg, false);
    }

    if full {
        let command_right = left + (bar.w - left - reserve) / 2.0 + 160.0;
        icon(
            f,
            Icon::Forum,
            command_right + 18.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Deploy,
            command_right + 42.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
    }

    if show_update {
        let update = Rect {
            x: bar.w - reserve + 8.0,
            y: 7.0,
            w: 58.0,
            h: 22.0,
        };
        f.canvas.fill_round_rect(
            update.x,
            update.y,
            update.w,
            update.h,
            3.0,
            f.theme.accent,
            f.scale,
        );
        text(
            f,
            "Update",
            update.x + 10.0,
            11.0,
            10.0,
            f.theme.on_accent,
            true,
        );
    }

    if full {
        icon(
            f,
            Icon::PanelLeft,
            bar.w - 108.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::PanelBottom,
            bar.w - 84.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::PanelRight,
            bar.w - 60.0,
            18.0,
            14.0,
            f.theme.text_muted,
        );
        icon(f, Icon::More, bar.w - 32.0, 18.0, 14.0, f.theme.text_muted);
    } else if bar.w >= 520.0 {
        icon(f, Icon::More, bar.w - 20.0, 18.0, 14.0, f.theme.text_muted);
    }
}

pub(super) fn draw_cards(f: &mut Frame, layout: &AppLayout) {
    f.canvas.fill(f.theme.surface_chrome);
    card(f, layout.activity_bar);
    card(f, layout.explorer_card);
    card(f, layout.editor_card);
    if layout.term_dock.h > 0.0 {
        card(f, layout.term_dock);
    }
}

pub(super) fn draw_activity_bar(f: &mut Frame, layout: &AppLayout) -> Option<ActivityPanel> {
    let bar = layout.activity_bar;
    if bar.w <= 0.0 || bar.h <= 0.0 {
        return None;
    }
    let items: [(Icon, Option<&str>); 9] = [
        (Icon::File, None),
        (Icon::Search, None),
        (Icon::SourceControl, None),
        (Icon::Bug, None),
        (Icon::Extension, None),
        (Icon::Asterisk, None),
        (Icon::Token, Some("co")),
        (Icon::Token, Some("K")),
        (Icon::More, None),
    ];
    let mut clicked = None;
    let step = if bar.h >= 360.0 {
        40.0
    } else if bar.h >= 220.0 {
        32.0
    } else {
        28.0
    };
    // Functional icons come first; the account/settings footer is optional.
    let footer_reserve = if bar.h >= 5.0 * step + 88.0 {
        72.0
    } else {
        0.0
    };
    let visible = ((bar.h - 8.0 - footer_reserve) / step).floor().max(0.0) as usize;
    for (index, (glyph, label)) in items.into_iter().take(visible).enumerate() {
        let rect = Rect {
            x: bar.x + 6.0,
            y: bar.y + 8.0 + index as f64 * step,
            w: bar.w - 12.0,
            h: step - 4.0,
        };
        let active = matches!(
            (index, f.activity),
            (0, ActivityPanel::Explorer)
                | (2, ActivityPanel::SourceControl)
                | (3, ActivityPanel::Terminal)
                | (4, ActivityPanel::Config)
        );
        if active {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                3.0,
                f.theme.surface_hover,
                f.scale,
            );
            f.canvas
                .fill_rect(rect.x, rect.y, 2.0, rect.h, f.theme.accent, f.scale);
        } else if rect.contains(f.pointer.x, f.pointer.y) {
            f.canvas.fill_round_rect(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                3.0,
                f.theme.surface_hover,
                f.scale,
            );
        }
        let color = if active {
            f.theme.fg
        } else {
            f.theme.text_muted
        };
        if let Some(label) = label {
            text(f, label, rect.x + 13.0, rect.y + 9.0, 11.0, color, true);
        } else {
            icon(
                f,
                glyph,
                rect.x + rect.w / 2.0,
                rect.y + rect.h / 2.0,
                18.0,
                color,
            );
        }
        if index == 2 {
            let count = f.git.map(|git| git.changed_count()).unwrap_or(0);
            if count > 0 {
                let badge = Rect {
                    x: rect.x + rect.w - 16.0,
                    y: rect.y + rect.h - 12.0,
                    w: 14.0,
                    h: 12.0,
                };
                f.canvas.fill_round_rect(
                    badge.x,
                    badge.y,
                    badge.w,
                    badge.h,
                    6.0,
                    f.theme.accent,
                    f.scale,
                );
                text(
                    f,
                    &count.to_string(),
                    badge.x + 4.0,
                    badge.y + 1.0,
                    8.0,
                    f.theme.on_accent,
                    true,
                );
            }
        }
        if f.pointer.click && rect.contains(f.pointer.x, f.pointer.y) {
            clicked = match index {
                0 => Some(ActivityPanel::Explorer),
                2 => Some(ActivityPanel::SourceControl),
                3 => Some(ActivityPanel::Terminal),
                4 => Some(ActivityPanel::Config),
                _ => None,
            };
        }
    }

    if footer_reserve > 0.0 {
        let account = Rect {
            x: bar.x + 12.0,
            y: bar.y + bar.h - 72.0,
            w: 24.0,
            h: 24.0,
        };
        f.canvas.fill_round_rect(
            account.x,
            account.y,
            account.w,
            account.h,
            12.0,
            f.theme.git_modified,
            f.scale,
        );
        text(
            f,
            "K",
            account.x + 8.0,
            account.y + 5.0,
            10.0,
            f.theme.surface_chrome,
            true,
        );
        icon(
            f,
            Icon::Config,
            bar.x + bar.w / 2.0,
            bar.y + bar.h - 24.0,
            18.0,
            f.theme.text_muted,
        );
    }
    clicked
}

pub(super) fn draw_explorer_chrome(f: &mut Frame, layout: &AppLayout) {
    if layout.explorer_card.w <= 0.0 {
        return;
    }
    let header = layout.explorer_header;
    f.canvas.fill_rect(
        header.x + 1.0,
        header.y + 1.0,
        header.w - 2.0,
        header.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    text(
        f,
        "EXPLORER",
        header.x + 12.0,
        header.y + 10.0,
        10.0,
        f.theme.fg,
        true,
    );
    icon(
        f,
        Icon::More,
        header.x + header.w - 16.0,
        header.y + header.h / 2.0,
        14.0,
        f.theme.text_muted,
    );

    let root = layout.explorer_root;
    icon(
        f,
        Icon::ChevronDown,
        root.x + 14.0,
        root.y + root.h / 2.0,
        12.0,
        f.theme.fg,
    );
    text(
        f,
        f.title,
        root.x + 28.0,
        root.y + 7.0,
        11.0,
        f.theme.fg,
        true,
    );
    let actions = [
        Icon::NewFile,
        Icon::NewFolder,
        Icon::Refresh,
        Icon::Collapse,
    ];
    for (index, glyph) in actions.into_iter().enumerate() {
        icon(
            f,
            glyph,
            root.x + root.w - 58.0 + index as f64 * 16.0,
            root.y + root.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
    }

    let footer = layout.explorer_footer;
    f.canvas.fill_rect(
        footer.x + 1.0,
        footer.y,
        footer.w - 2.0,
        footer.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    f.canvas.fill_rect(
        footer.x + 1.0,
        footer.y,
        footer.w - 2.0,
        1.0,
        f.theme.border_subtle,
        f.scale,
    );
    for (index, label) in ["OUTLINE", "TIMELINE"].into_iter().enumerate() {
        let y = footer.y + 6.0 + index as f64 * 28.0;
        icon(
            f,
            Icon::ChevronRight,
            footer.x + 14.0,
            y + 8.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            label,
            footer.x + 28.0,
            y + 2.0,
            10.0,
            f.theme.text_muted,
            true,
        );
    }
}

pub(super) fn draw_editor_actions(f: &mut Frame, layout: &AppLayout) {
    let tabs = layout.editor_tabs;
    if tabs.w <= 0.0 || tabs.h <= 0.0 {
        return;
    }
    f.canvas.fill_rect(
        tabs.x + 1.0,
        tabs.y + 1.0,
        tabs.w - 2.0,
        tabs.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    let actions = [Icon::Asterisk, Icon::Token, Icon::Split, Icon::More];
    let count = editor_action_count(tabs.w);
    let first = tabs.x + tabs.w - 18.0 - (count - 1) as f64 * 18.0;
    for (index, glyph) in actions.into_iter().take(count).enumerate() {
        icon(
            f,
            glyph,
            first + index as f64 * 18.0,
            tabs.y + tabs.h / 2.0,
            14.0,
            if index == 0 {
                f.theme.syntax_string
            } else {
                f.theme.text_muted
            },
        );
    }
}

pub(super) fn draw_minimap(f: &mut Frame, layout: &AppLayout) {
    let map = layout.minimap;
    if map.w <= 0.0 || map.h <= 0.0 {
        return;
    }
    f.canvas
        .fill_rect(map.x, map.y, 1.0, map.h, f.theme.border_subtle, f.scale);
    let colors = [
        f.theme.syntax_comment,
        f.theme.syntax_keyword,
        f.theme.syntax_type,
        f.theme.syntax_function,
        f.theme.syntax_variable,
        f.theme.syntax_string,
        f.theme.syntax_number,
        f.theme.syntax_control,
    ];
    let widths = [42.0, 26.0, 38.0, 18.0, 44.0, 34.0, 30.0, 40.0];
    for index in 0..24 {
        let y = map.y + 8.0 + index as f64 * 9.0;
        if y >= map.y + map.h - 4.0 {
            break;
        }
        let width = widths[index % widths.len()];
        f.canvas.fill_rect(
            map.x + map.w - 4.0 - width,
            y,
            width,
            2.0,
            colors[index % colors.len()],
            f.scale,
        );
    }
}

pub(super) fn draw_terminal_chrome(f: &mut Frame, layout: &AppLayout) -> (bool, bool) {
    let pane = layout.term_dock;
    if pane.h <= 0.0 {
        return (false, false);
    }
    let toolbar = layout.terminal_toolbar;
    f.canvas.fill_rect(
        toolbar.x + 1.0,
        toolbar.y + 1.0,
        toolbar.w - 2.0,
        toolbar.h - 1.0,
        f.theme.bg_elevated,
        f.scale,
    );
    let tabs: &[&str] = if pane.w >= 560.0 {
        &["PROBLEMS", "OUTPUT", "TERMINAL", "PORTS", "DEBUG CONSOLE"]
    } else if pane.w >= 360.0 {
        &["PROBLEMS", "OUTPUT", "TERMINAL"]
    } else {
        &["TERMINAL"]
    };
    let mut x = toolbar.x + 12.0;
    for label in tabs {
        let w = match *label {
            "DEBUG CONSOLE" => 116.0,
            "PROBLEMS" | "OUTPUT" => 70.0,
            _ => 62.0,
        };
        text(
            f,
            label,
            x,
            toolbar.y + (toolbar.h - 9.0) / 2.0,
            9.0,
            if *label == "TERMINAL" {
                f.theme.fg
            } else {
                f.theme.text_muted
            },
            true,
        );
        if *label == "TERMINAL" {
            f.canvas.fill_rect(
                x,
                toolbar.y + toolbar.h - 2.0,
                w - 12.0,
                2.0,
                f.theme.accent,
                f.scale,
            );
        }
        x += w;
    }

    if pane.w >= 700.0 {
        text(
            f,
            "bash",
            toolbar.x + toolbar.w - 210.0,
            toolbar.y + (toolbar.h - 10.0) / 2.0,
            10.0,
            f.theme.fg,
            false,
        );
    }
    let (plus, close) = terminal_actions(layout);
    let open_clicked = plus.is_some_and(|rect| {
        draw_button(f.canvas, "", rect, f.theme, f.text, f.pointer, f.scale) == Press::Clicked
    });
    let close_clicked = close.is_some_and(|rect| {
        draw_button(f.canvas, "", rect, f.theme, f.text, f.pointer, f.scale) == Press::Clicked
    });
    let actions: &[Icon] = match terminal_action_count(pane.w) {
        5 => &[
            Icon::Plus,
            Icon::Split,
            Icon::Close,
            Icon::More,
            Icon::ChevronUp,
        ],
        2 => &[Icon::Plus, Icon::Close],
        _ => &[Icon::Plus],
    };
    let first = pane.x + pane.w - 38.0 - (actions.len() - 1) as f64 * 20.0;
    for (index, glyph) in actions.iter().enumerate() {
        icon(
            f,
            *glyph,
            first + index as f64 * 20.0,
            toolbar.y + toolbar.h / 2.0,
            13.0,
            f.theme.text_muted,
        );
    }

    let sessions = layout.terminal_sessions;
    if sessions.w > 0.0 && sessions.h > 0.0 {
        f.canvas.fill_rect(
            sessions.x,
            sessions.y,
            1.0,
            sessions.h,
            f.theme.border_subtle,
            f.scale,
        );
        let (count, active) = f
            .terms
            .as_ref()
            .map(|terms| (terms.tabs.len().min(3), terms.active))
            .unwrap_or((3, 0));
        let visible = count.min(((sessions.h - 8.0) / 32.0).floor().max(1.0) as usize);
        for index in 0..visible {
            let y = sessions.y + 8.0 + index as f64 * 32.0;
            if index == active {
                f.canvas.fill_round_rect(
                    sessions.x + 6.0,
                    y,
                    24.0,
                    24.0,
                    3.0,
                    f.theme.surface_hover,
                    f.scale,
                );
            }
            icon(
                f,
                Icon::Terminal,
                sessions.x + 18.0,
                y + 12.0,
                14.0,
                if index == active {
                    f.theme.fg
                } else {
                    f.theme.text_muted
                },
            );
        }
    }
    (open_clicked, close_clicked)
}

fn terminal_action_count(width: f64) -> usize {
    if width >= 560.0 {
        5
    } else if width >= 360.0 {
        2
    } else {
        1
    }
}

/// Hit rects for the two functional terminal actions. Chrome and interaction
/// share this pure geometry so resizing cannot leave a click target misplaced.
pub(super) fn terminal_actions(layout: &AppLayout) -> (Option<Rect>, Option<Rect>) {
    let pane = layout.term_dock;
    let toolbar = layout.terminal_toolbar;
    if pane.w <= 0.0 || pane.h <= 0.0 || toolbar.h < 20.0 {
        return (None, None);
    }
    let count = terminal_action_count(pane.w);
    let first = pane.x + pane.w - 38.0 - (count - 1) as f64 * 20.0;
    let y = pane.y + (toolbar.h - 20.0) / 2.0;
    let plus = Some(Rect {
        x: first - 10.0,
        y,
        w: 20.0,
        h: 20.0,
    });
    let close = (count >= 2).then(|| {
        let index = if count == 5 { 2 } else { 1 };
        Rect {
            x: first + index as f64 * 20.0 - 10.0,
            y,
            w: 20.0,
            h: 20.0,
        }
    });
    (plus, close)
}

pub(super) fn draw_terminal_placeholder(f: &mut Frame, layout: &AppLayout) {
    let grid = layout.term_grid;
    if grid.w <= 0.0 || grid.h <= 0.0 {
        return;
    }
    text(
        f,
        "ark@ark",
        grid.x,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_control,
        false,
    );
    text(
        f,
        "in",
        grid.x + 58.0,
        grid.y + 8.0,
        11.0,
        f.theme.text_muted,
        false,
    );
    text(
        f,
        "repo:",
        grid.x + 76.0,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_type,
        false,
    );
    text(
        f,
        f.title,
        grid.x + 108.0,
        grid.y + 8.0,
        11.0,
        f.theme.fg,
        false,
    );
    text(
        f,
        "on",
        grid.x + 150.0,
        grid.y + 8.0,
        11.0,
        f.theme.text_muted,
        false,
    );
    text(
        f,
        "main",
        grid.x + 168.0,
        grid.y + 8.0,
        11.0,
        f.theme.syntax_function,
        false,
    );
    text(
        f,
        ">",
        grid.x,
        grid.y + 34.0,
        11.0,
        f.theme.git_untracked,
        true,
    );
    text(
        f,
        "Type a command to begin",
        grid.x + 18.0,
        grid.y + 34.0,
        11.0,
        f.theme.text_dim,
        false,
    );
}

pub(super) fn draw_status_bar(f: &mut Frame, layout: &AppLayout) {
    let bar = layout.status;
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, bar.h, f.theme.surface_chrome, f.scale);
    f.canvas
        .fill_rect(bar.x, bar.y, bar.w, 1.0, f.theme.border_subtle, f.scale);
    f.canvas
        .fill_rect(bar.x, bar.y, 32.0, bar.h, f.theme.accent, f.scale);
    icon(
        f,
        Icon::Remote,
        bar.x + 16.0,
        bar.y + bar.h / 2.0,
        13.0,
        f.theme.on_accent,
    );

    let mut left_end = bar.x + 32.0;
    if bar.w >= 180.0 {
        text(
            f,
            "main*",
            bar.x + 42.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        left_end = bar.x + 82.0;
    }
    if bar.w >= 300.0 {
        icon(
            f,
            Icon::Sync,
            bar.x + 88.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        left_end = bar.x + 104.0;
    }
    if bar.w >= 360.0 {
        icon(
            f,
            Icon::Close,
            bar.x + 112.0,
            bar.y + bar.h / 2.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            "0",
            bar.x + 122.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        icon(
            f,
            Icon::Warning,
            bar.x + 144.0,
            bar.y + bar.h / 2.0,
            11.0,
            f.theme.text_muted,
        );
        text(
            f,
            "0",
            bar.x + 154.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
        left_end = bar.x + 170.0;
    }

    // Pick the richest right-hand group that fits after the left cluster.
    let groups: [&[&str]; 5] = [
        &[],
        &["Ln 1, Col 1"],
        &["Ln 1, Col 1", "UTF-8"],
        &["Ln 1, Col 1", "Spaces: 4", "UTF-8", "LF", "{} Rust"],
        &[
            "Ln 1, Col 1",
            "Spaces: 4",
            "UTF-8",
            "LF",
            "{} Rust",
            "Prettier",
        ],
    ];
    let available = (bar.x + bar.w - left_end - 12.0).max(0.0);
    let mut labels: &[&str] = &[];
    for group in groups.iter().rev() {
        let width = group
            .iter()
            .map(|label| label.len() as f64 * 6.2 + 16.0)
            .sum::<f64>();
        if width <= available {
            labels = group;
            break;
        }
    }
    let mut x = bar.x + bar.w - 12.0;
    for label in labels.iter().rev() {
        let w = label.len() as f64 * 6.2 + 16.0;
        x -= w;
        text(
            f,
            label,
            x + 8.0,
            bar.y + 6.0,
            10.0,
            f.theme.text_muted,
            false,
        );
    }
    if x - 106.0 >= left_end {
        icon(
            f,
            Icon::Zoom,
            x - 18.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Asterisk,
            x - 42.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Check,
            x - 66.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
        icon(
            f,
            Icon::Bell,
            x - 94.0,
            bar.y + bar.h / 2.0,
            12.0,
            f.theme.text_muted,
        );
    }
}

/// Width reserved for the editor actions that are visible at this width.
pub(super) fn tab_strip(layout: &AppLayout) -> Rect {
    let reserve = match editor_action_count(layout.editor_tabs.w) {
        4 => 116.0,
        2 => 58.0,
        _ => 38.0,
    };
    Rect {
        x: layout.editor_tabs.x,
        y: layout.editor_tabs.y,
        w: (layout.editor_tabs.w - reserve).max(0.0),
        h: layout.editor_tabs.h,
    }
}

fn editor_action_count(width: f64) -> usize {
    if width >= 560.0 {
        4
    } else if width >= 320.0 {
        2
    } else {
        1
    }
}
