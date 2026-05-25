use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;

pub struct CompletionPopup<'a> {
    editor: &'a Editor,
}

impl<'a> CompletionPopup<'a> {
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

impl Widget for CompletionPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_completion || self.editor.completions.is_empty() {
            return;
        }

        let items = &self.editor.completions;
        let selected = self.editor.completion_index;

        // Position popup below cursor
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

        // Popup dimensions
        let max_items = 10.min(items.len());
        let popup_height = max_items as u16;
        let popup_width = items
            .iter()
            .take(max_items)
            .map(|i| i.label.len() as u16 + 2)
            .max()
            .unwrap_or(20)
            .max(15)
            .min(area.width.saturating_sub(cursor_screen_x));

        // Place popup below cursor, or above if not enough room
        let popup_y = if cursor_screen_y + 1 + popup_height <= area.bottom() {
            cursor_screen_y + 1
        } else {
            cursor_screen_y.saturating_sub(popup_height)
        };

        let popup_x = cursor_screen_x.min(area.right().saturating_sub(popup_width));

        // Scroll window into completions list
        let scroll = if selected >= max_items {
            selected - max_items + 1
        } else {
            0
        };

        let normal_style = Style::default().bg(Color::Rgb(40, 44, 52)).fg(Color::White);
        let selected_style = Style::default()
            .bg(Color::Rgb(80, 90, 110))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        for i in 0..max_items {
            let item_idx = scroll + i;
            if item_idx >= items.len() {
                break;
            }
            let item = &items[item_idx];
            let y = popup_y + i as u16;
            if y >= area.bottom() {
                break;
            }

            let style = if item_idx == selected {
                selected_style
            } else {
                normal_style
            };

            // Fill background
            for x in popup_x..popup_x + popup_width {
                if x < area.right() {
                    buf[(x, y)].set_char(' ').set_style(style);
                }
            }

            // Draw label
            let label = &item.label;
            let mut x = popup_x + 1;
            for ch in label.chars() {
                if x >= popup_x + popup_width - 1 || x >= area.right() {
                    break;
                }
                buf[(x, y)].set_char(ch).set_style(style);
                x += 1;
            }
        }
    }
}
