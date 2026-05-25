use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;

pub struct FileFinderPopup<'a> {
    editor: &'a Editor,
}

impl<'a> FileFinderPopup<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for FileFinderPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_file_finder {
            return;
        }

        // Popup dimensions: centered, 60% width, up to 20 items + 3 for border/input
        let popup_width = (area.width * 3 / 5)
            .max(30)
            .min(area.width.saturating_sub(4));
        let max_items = 15.min(self.editor.file_finder_filtered.len());
        let popup_height = (max_items as u16 + 3).min(area.height.saturating_sub(2));

        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 3;

        let border_style = Style::default()
            .fg(Color::Rgb(80, 90, 110))
            .bg(Color::Rgb(30, 33, 40));
        let bg_style = Style::default().fg(Color::White).bg(Color::Rgb(30, 33, 40));
        let selected_style = Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(80, 90, 110))
            .add_modifier(Modifier::BOLD);
        let input_style = Style::default().fg(Color::White).bg(Color::Rgb(40, 44, 52));

        // Draw border
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
                let style = if is_border { border_style } else { bg_style };
                buf[(x, y)].set_char(ch).set_style(style);
            }
        }

        // Title
        let title = " Open File ";
        let title_x = popup_x + (popup_width.saturating_sub(title.len() as u16)) / 2;
        for (i, ch) in title.chars().enumerate() {
            let x = title_x + i as u16;
            if x < area.right() && x > popup_x && x < popup_x + popup_width - 1 {
                buf[(x, popup_y)].set_char(ch).set_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Rgb(30, 33, 40))
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Input line (row 1)
        let input_y = popup_y + 1;
        if input_y < area.bottom() {
            let content_width = popup_width.saturating_sub(2) as usize;
            // Fill input background
            for dx in 1..popup_width - 1 {
                let x = popup_x + dx;
                if x < area.right() {
                    buf[(x, input_y)].set_char(' ').set_style(input_style);
                }
            }
            // Draw query
            let prompt = "> ";
            for (i, ch) in prompt.chars().enumerate() {
                let x = popup_x + 1 + i as u16;
                if x < area.right() {
                    buf[(x, input_y)]
                        .set_char(ch)
                        .set_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(40, 44, 52)));
                }
            }
            for (i, ch) in self.editor.file_finder_query.chars().enumerate() {
                if i >= content_width - 2 {
                    break;
                }
                let x = popup_x + 3 + i as u16;
                if x < area.right() {
                    buf[(x, input_y)].set_char(ch).set_style(input_style);
                }
            }
            // Cursor
            let cursor_x = popup_x + 3 + self.editor.file_finder_query.len() as u16;
            if cursor_x < popup_x + popup_width - 1 && cursor_x < area.right() {
                buf[(cursor_x, input_y)]
                    .set_char(' ')
                    .set_style(Style::default().bg(Color::White));
            }

            // Count
            let count_str = format!(
                " {}/{}",
                self.editor.file_finder_filtered.len(),
                self.editor.file_finder_entries.len()
            );
            let count_x = (popup_x + popup_width - 1).saturating_sub(count_str.len() as u16);
            for (i, ch) in count_str.chars().enumerate() {
                let x = count_x + i as u16;
                if x < popup_x + popup_width - 1 && x < area.right() {
                    buf[(x, input_y)].set_char(ch).set_style(
                        Style::default()
                            .fg(Color::DarkGray)
                            .bg(Color::Rgb(40, 44, 52)),
                    );
                }
            }
        }

        // File list
        let list_start_y = popup_y + 2;
        let content_height = popup_height.saturating_sub(3) as usize;
        let selected = self.editor.file_finder_index;

        let scroll = if selected >= content_height {
            selected - content_height + 1
        } else {
            0
        };

        for i in 0..content_height {
            let item_idx = scroll + i;
            let y = list_start_y + i as u16;
            if y >= popup_y + popup_height - 1 || y >= area.bottom() {
                break;
            }

            if item_idx >= self.editor.file_finder_filtered.len() {
                continue;
            }

            let style = if item_idx == selected {
                selected_style
            } else {
                bg_style
            };

            // Fill row
            for dx in 1..popup_width - 1 {
                let x = popup_x + dx;
                if x < area.right() {
                    buf[(x, y)].set_char(' ').set_style(style);
                }
            }

            // Draw filename
            let path = &self.editor.file_finder_filtered[item_idx];
            for (j, ch) in path.chars().enumerate() {
                let x = popup_x + 2 + j as u16;
                if x >= popup_x + popup_width - 1 || x >= area.right() {
                    break;
                }
                buf[(x, y)].set_char(ch).set_style(style);
            }
        }
    }
}
