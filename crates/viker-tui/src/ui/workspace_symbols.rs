use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;
use viker_core::lsp;

pub struct WorkspaceSymbolsPopup<'a> {
    editor: &'a Editor,
}

impl<'a> WorkspaceSymbolsPopup<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for WorkspaceSymbolsPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_workspace_symbols {
            return;
        }

        let popup_width = (area.width * 3 / 5)
            .max(40)
            .min(area.width.saturating_sub(4));
        let max_items = 15.min(self.editor.workspace_symbol_results.len());
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
        let title = " Workspace Symbols ";
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
            for dx in 1..popup_width - 1 {
                let x = popup_x + dx;
                if x < area.right() {
                    buf[(x, input_y)].set_char(' ').set_style(input_style);
                }
            }
            let prompt = "> ";
            for (i, ch) in prompt.chars().enumerate() {
                let x = popup_x + 1 + i as u16;
                if x < area.right() {
                    buf[(x, input_y)]
                        .set_char(ch)
                        .set_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(40, 44, 52)));
                }
            }
            for (i, ch) in self.editor.workspace_symbol_query.chars().enumerate() {
                if i >= content_width - 2 {
                    break;
                }
                let x = popup_x + 3 + i as u16;
                if x < area.right() {
                    buf[(x, input_y)].set_char(ch).set_style(input_style);
                }
            }
            // Cursor
            let cursor_x = popup_x + 3 + self.editor.workspace_symbol_query.len() as u16;
            if cursor_x < popup_x + popup_width - 1 && cursor_x < area.right() {
                buf[(cursor_x, input_y)]
                    .set_char(' ')
                    .set_style(Style::default().bg(Color::White));
            }

            // Count
            let count_str = format!(" {}", self.editor.workspace_symbol_results.len());
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

        // Symbol list
        let list_start_y = popup_y + 2;
        let content_height = popup_height.saturating_sub(3) as usize;
        let selected = self.editor.workspace_symbol_index;

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

            if item_idx >= self.editor.workspace_symbol_results.len() {
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

            // Draw symbol info
            let sym = &self.editor.workspace_symbol_results[item_idx];
            let kind_label = lsp::symbol_kind_label(sym.kind);
            let file_name = sym.uri.rsplit('/').next().unwrap_or(&sym.uri);
            let display = format!(
                "[{}] {}  {}:{}",
                kind_label,
                sym.name,
                file_name,
                sym.start_line + 1
            );
            for (j, ch) in display.chars().enumerate() {
                let x = popup_x + 2 + j as u16;
                if x >= popup_x + popup_width - 1 || x >= area.right() {
                    break;
                }
                // Color the kind label
                let char_style = if j < kind_label.len() + 2 {
                    let kind_color = match sym.kind {
                        6 | 12 => Color::Rgb(97, 175, 239),   // Method/Function: blue
                        23 | 5 => Color::Rgb(229, 192, 123),  // Struct/Class: yellow
                        10 | 22 => Color::Rgb(198, 120, 221), // Enum: purple
                        11 => Color::Rgb(86, 182, 194),       // Interface/Trait: cyan
                        14 => Color::Rgb(209, 154, 102),      // Constant: orange
                        2 => Color::Rgb(152, 195, 121),       // Module: green
                        _ => Color::Rgb(171, 178, 191),
                    };
                    if item_idx == selected {
                        Style::default()
                            .fg(kind_color)
                            .bg(Color::Rgb(80, 90, 110))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(kind_color).bg(Color::Rgb(30, 33, 40))
                    }
                } else {
                    style
                };
                buf[(x, y)].set_char(ch).set_style(char_style);
            }
        }
    }
}
