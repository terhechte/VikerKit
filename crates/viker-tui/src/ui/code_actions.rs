use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;

pub struct CodeActionsPopup<'a> {
    editor: &'a Editor,
}

impl<'a> CodeActionsPopup<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for CodeActionsPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_code_actions || self.editor.code_actions.is_empty() {
            return;
        }

        let actions = &self.editor.code_actions;
        let selected = self.editor.code_action_index;

        let items: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();

        // Popup dimensions
        let max_items = 15.min(items.len());
        let popup_height = (max_items as u16 + 2).min(area.height.saturating_sub(2));
        let max_label_len = items
            .iter()
            .take(max_items)
            .map(|s| s.len())
            .max()
            .unwrap_or(20);
        let popup_width = (max_label_len as u16 + 4)
            .max(20)
            .min(area.width.saturating_sub(4));

        // Center horizontally, place near the cursor
        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;

        let border_style = Style::default()
            .fg(Color::Rgb(80, 90, 110))
            .bg(Color::Rgb(30, 33, 40));
        let normal_style = Style::default().fg(Color::White).bg(Color::Rgb(30, 33, 40));
        let selected_style = Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(80, 90, 110))
            .add_modifier(Modifier::BOLD);

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
                buf[(x, y)].set_char(ch).set_style(if is_border {
                    border_style
                } else {
                    normal_style
                });
            }
        }

        // Title
        let title = " Code Actions ";
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

        // Scroll window
        let content_height = (popup_height - 2) as usize;
        let scroll = if selected >= content_height {
            selected - content_height + 1
        } else {
            0
        };

        // Draw items
        for i in 0..content_height {
            let item_idx = scroll + i;
            if item_idx >= items.len() {
                break;
            }
            let y = popup_y + 1 + i as u16;
            if y >= popup_y + popup_height - 1 {
                break;
            }

            let style = if item_idx == selected {
                selected_style
            } else {
                normal_style
            };

            // Fill row background
            for dx in 1..popup_width - 1 {
                let x = popup_x + dx;
                if x < area.right() {
                    buf[(x, y)].set_char(' ').set_style(style);
                }
            }

            // Draw label
            let label = items[item_idx];
            for (j, ch) in label.chars().enumerate() {
                let x = popup_x + 2 + j as u16;
                if x >= popup_x + popup_width - 1 || x >= area.right() {
                    break;
                }
                buf[(x, y)].set_char(ch).set_style(style);
            }
        }
    }
}
