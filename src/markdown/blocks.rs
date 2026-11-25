//! VS Code-style block spacing and painting for the streamed Markdown preview.

use cosmic_text::Weight;

use crate::render::{Canvas, Theme};
use crate::text::{MonoMetrics, TextSystem};
use crate::ui::Rect;

use super::code;
use super::paint::{draw_text, draw_wrapped};
use super::parse::{wrap_spans, BlockKind, InlineSpan};
use super::table;

const BLOCK_GAP: f64 = 12.0;
const LIST_GAP: f64 = 4.0;

pub(super) struct BlockPainter<'a> {
    pub canvas: &'a mut Canvas,
    pub text: &'a mut TextSystem,
    pub theme: &'a Theme,
    pub scale: f64,
    pub content: Rect,
    pub view_top: f64,
    pub view_bottom: f64,
    pub base_font: f32,
    pub base_metrics: MonoMetrics,
    pub base_line_h: f64,
    pub advance: f64,
}

impl BlockPainter<'_> {
    pub fn draw(&mut self, block: &BlockKind, top: f64) -> f64 {
        match block {
            BlockKind::Heading { level, spans } => self.heading(*level, spans, top),
            BlockKind::Paragraph(spans) => self.paragraph(spans, top),
            BlockKind::List {
                ordered,
                task,
                spans,
            } => self.list(*ordered, *task, spans, top),
            BlockKind::Quote(spans) => self.quote(spans, top),
            BlockKind::Code(lines) => code::draw(self, lines, top) + BLOCK_GAP,
            BlockKind::Rule => self.rule(top),
            BlockKind::Table {
                headers,
                rows,
                truncated,
            } => self.table(headers, rows, *truncated, top),
        }
    }

    fn heading(&mut self, level: usize, spans: &[InlineSpan], top: f64) -> f64 {
        let multiplier = match level {
            1 => 2.0,
            2 => 1.5,
            3 => 1.25,
            4 => 1.1,
            _ => 1.0,
        };
        let font_size = self.base_font * multiplier as f32;
        let metrics = self.text.mono_metrics(font_size, font_size * 1.35);
        let line_h = (f64::from(metrics.line_height) / self.scale).max(1.0);
        let lines = wrap_spans(
            spans.to_vec(),
            self.content.w,
            f64::from(metrics.advance) / self.scale,
        );
        let margin_top = if level <= 2 { 16.0 } else { 12.0 };
        let margin_bottom = if level <= 2 { 10.0 } else { 8.0 };
        let rule_h = if level <= 2 { 9.0 } else { 0.0 };
        let height = margin_top + lines.len() as f64 * line_h + margin_bottom + rule_h;
        if top + height >= self.view_top && top <= self.view_bottom {
            let text_top = top + margin_top;
            draw_wrapped(
                self.canvas,
                self.text,
                self.theme,
                self.scale,
                &lines,
                self.content.x,
                text_top,
                line_h,
                font_size,
                metrics,
                self.view_top,
                self.view_bottom,
                self.theme.fg,
                Weight::BOLD,
                false,
            );
            if rule_h > 0.0 {
                let rule_y = text_top + lines.len() as f64 * line_h + 4.0;
                let y0 = rule_y.max(self.view_top);
                let y1 = (rule_y + 1.0).min(self.view_bottom);
                if y1 > y0 {
                    self.canvas.fill_rect(
                        self.content.x,
                        y0,
                        self.content.w,
                        y1 - y0,
                        self.theme.border,
                        self.scale,
                    );
                }
            }
        }
        height
    }

    fn paragraph(&mut self, spans: &[InlineSpan], top: f64) -> f64 {
        let lines = wrap_spans(spans.to_vec(), self.content.w, self.advance);
        let height = lines.len() as f64 * self.base_line_h + BLOCK_GAP;
        if top + height >= self.view_top && top <= self.view_bottom {
            draw_wrapped(
                self.canvas,
                self.text,
                self.theme,
                self.scale,
                &lines,
                self.content.x,
                top,
                self.base_line_h,
                self.base_font,
                self.base_metrics,
                self.view_top,
                self.view_bottom,
                self.theme.fg,
                Weight::NORMAL,
                false,
            );
        }
        height
    }

    fn list(
        &mut self,
        ordered: Option<usize>,
        task: Option<bool>,
        spans: &[InlineSpan],
        top: f64,
    ) -> f64 {
        let indent = if task.is_some() { 28.0 } else { 24.0 };
        let lines = wrap_spans(
            spans.to_vec(),
            (self.content.w - indent).max(1.0),
            self.advance,
        );
        let height = lines.len() as f64 * self.base_line_h + LIST_GAP;
        if top + height < self.view_top || top > self.view_bottom {
            return height;
        }
        let marker = if let Some(checked) = task {
            let box_y = top + (self.base_line_h - 13.0) / 2.0;
            let box_x = self.content.x + 2.0;
            if box_y + 13.0 >= self.view_top && box_y <= self.view_bottom {
                self.canvas.fill_round_rect(
                    box_x,
                    box_y,
                    13.0,
                    13.0,
                    2.0,
                    self.theme.bg_elevated,
                    self.scale,
                );
                self.canvas
                    .fill_rect(box_x, box_y, 13.0, 1.0, self.theme.border, self.scale);
                self.canvas.fill_rect(
                    box_x,
                    box_y + 12.0,
                    13.0,
                    1.0,
                    self.theme.border,
                    self.scale,
                );
                self.canvas
                    .fill_rect(box_x, box_y, 1.0, 13.0, self.theme.border, self.scale);
                self.canvas.fill_rect(
                    box_x + 12.0,
                    box_y,
                    1.0,
                    13.0,
                    self.theme.border,
                    self.scale,
                );
                if checked {
                    for (dx, dy) in [(3.0, 6.0), (5.0, 8.0), (7.0, 6.0), (9.0, 4.0)] {
                        self.canvas.fill_rect(
                            box_x + dx,
                            box_y + dy,
                            2.0,
                            2.0,
                            self.theme.accent,
                            self.scale,
                        );
                    }
                }
            }
            None
        } else if let Some(number) = ordered {
            Some(format!("{number}."))
        } else {
            let dot_y = top + self.base_line_h / 2.0 - 2.0;
            if dot_y + 4.0 >= self.view_top && dot_y <= self.view_bottom {
                self.canvas.fill_round_rect(
                    self.content.x + 7.0,
                    dot_y,
                    4.0,
                    4.0,
                    2.0,
                    self.theme.accent,
                    self.scale,
                );
            }
            None
        };
        if let Some(marker) = marker {
            if top >= self.view_top && top + self.base_line_h <= self.view_bottom {
                draw_text(
                    self.canvas,
                    self.text,
                    &marker,
                    self.content.x + 2.0,
                    top,
                    self.base_line_h,
                    self.base_font,
                    self.base_metrics,
                    self.scale,
                    self.theme.text_dim,
                    Weight::NORMAL,
                );
            }
        }
        let color = if task == Some(true) {
            self.theme.text_muted
        } else {
            self.theme.fg
        };
        draw_wrapped(
            self.canvas,
            self.text,
            self.theme,
            self.scale,
            &lines,
            self.content.x + indent,
            top,
            self.base_line_h,
            self.base_font,
            self.base_metrics,
            self.view_top,
            self.view_bottom,
            color,
            Weight::NORMAL,
            task == Some(true),
        );
        height
    }

    fn quote(&mut self, spans: &[InlineSpan], top: f64) -> f64 {
        let indent = 16.0;
        let lines = wrap_spans(
            spans.to_vec(),
            (self.content.w - indent).max(1.0),
            self.advance,
        );
        let height = lines.len() as f64 * self.base_line_h + BLOCK_GAP;
        if top + height >= self.view_top && top <= self.view_bottom {
            let y0 = top.max(self.view_top);
            let y1 = (top + height - 4.0).min(self.view_bottom);
            if y1 > y0 {
                self.canvas.fill_rect(
                    self.content.x,
                    y0,
                    3.0,
                    y1 - y0,
                    self.theme.accent,
                    self.scale,
                );
            }
            draw_wrapped(
                self.canvas,
                self.text,
                self.theme,
                self.scale,
                &lines,
                self.content.x + indent,
                top + 4.0,
                self.base_line_h,
                self.base_font,
                self.base_metrics,
                self.view_top,
                self.view_bottom,
                self.theme.text_muted,
                Weight::NORMAL,
                false,
            );
        }
        height
    }

    fn rule(&mut self, top: f64) -> f64 {
        let height = 24.0;
        let y = top + height / 2.0;
        if y >= self.view_top && y + 1.0 <= self.view_bottom {
            self.canvas.fill_rect(
                self.content.x,
                y,
                self.content.w,
                1.0,
                self.theme.border,
                self.scale,
            );
        }
        height
    }

    #[allow(clippy::too_many_arguments)]
    fn table(
        &mut self,
        headers: &[Vec<InlineSpan>],
        rows: &[Vec<Vec<InlineSpan>>],
        truncated: bool,
        top: f64,
    ) -> f64 {
        let height = table::draw_table(
            self.canvas,
            self.text,
            self.theme,
            self.scale,
            self.content,
            top,
            self.view_top,
            self.view_bottom,
            headers,
            rows,
            truncated,
            self.base_line_h,
            self.base_font,
            self.base_metrics,
        );
        height + BLOCK_GAP
    }
}
