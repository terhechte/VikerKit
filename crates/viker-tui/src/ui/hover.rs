use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;

pub struct HoverPopup<'a> {
    editor: &'a Editor,
}

impl<'a> HoverPopup<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }

    fn gutter_width(&self) -> u16 {
        let lines = self.editor.document.line_count();
        let digits = if lines == 0 {
            1
        } else {
            (lines as f64).log10().floor() as u16 + 1
        };
        digits + 2
    }
}

impl Widget for HoverPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_hover {
            return;
        }
        let text = match &self.editor.hover_text {
            Some(t) if !t.is_empty() => t,
            _ => return,
        };

        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return;
        }

        // Calculate popup dimensions
        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let popup_width = (max_line_len as u16 + 4).min(area.width.saturating_sub(4));
        let popup_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));

        // Position above cursor
        let gutter_w = self.gutter_width();
        let cursor_screen_x = area.x
            + gutter_w
            + (self
                .editor
                .cursor
                .col
                .saturating_sub(self.editor.view.offset_col)) as u16;
        let cursor_screen_y = area.y
            + (self
                .editor
                .cursor
                .row
                .saturating_sub(self.editor.view.offset_row)) as u16;

        // Try above cursor, fall back to below
        let popup_y = if cursor_screen_y >= popup_height + area.y {
            cursor_screen_y - popup_height
        } else {
            (cursor_screen_y + 1).min(area.bottom().saturating_sub(popup_height))
        };

        let popup_x = cursor_screen_x.min(area.right().saturating_sub(popup_width));

        let border_style = Style::default()
            .fg(Color::Rgb(80, 90, 110))
            .bg(Color::Rgb(30, 33, 40));
        let text_style = Style::default()
            .fg(Color::Rgb(200, 200, 200))
            .bg(Color::Rgb(30, 33, 40));

        // Draw border and background
        for dy in 0..popup_height {
            let y = popup_y + dy;
            if y >= area.bottom() {
                break;
            }
            for dx in 0..popup_width {
                let x = popup_x + dx;
                if x >= area.right() {
                    break;
                }
                let is_border =
                    dy == 0 || dy == popup_height - 1 || dx == 0 || dx == popup_width - 1;
                let ch = if is_border {
                    if dy == 0 && dx == 0 {
                        '┌'
                    } else if dy == 0 && dx == popup_width - 1 {
                        '┐'
                    } else if dy == popup_height - 1 && dx == 0 {
                        '└'
                    } else if dy == popup_height - 1 && dx == popup_width - 1 {
                        '┘'
                    } else if dy == 0 || dy == popup_height - 1 {
                        '─'
                    } else {
                        '│'
                    }
                } else {
                    ' '
                };
                let style = if is_border { border_style } else { text_style };
                buf[(x, y)].set_char(ch).set_style(style);
            }
        }

        // Draw text content
        let content_width = popup_width.saturating_sub(4) as usize;
        let content_height = popup_height.saturating_sub(2) as usize;
        for (i, line) in lines.iter().take(content_height).enumerate() {
            let y = popup_y + 1 + i as u16;
            if y >= area.bottom() {
                break;
            }
            for (j, ch) in line.chars().take(content_width).enumerate() {
                let x = popup_x + 2 + j as u16;
                if x >= area.right() - 1 {
                    break;
                }
                buf[(x, y)].set_char(ch).set_style(text_style);
            }
        }
    }
}
