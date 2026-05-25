use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;

pub struct TabBar<'a> {
    editor: &'a Editor,
}

impl<'a> TabBar<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let info = self.editor.buffer_info();

        // Fill background
        let bg_style = Style::default().bg(Color::Rgb(40, 44, 52));
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_style(bg_style);
            buf[(x, area.y)].set_char(' ');
        }

        let mut x = area.x;
        for (name, modified, current) in info.iter() {
            let mod_mark = if *modified { " +" } else { "" };
            let label = format!(" {}{} ", name, mod_mark);
            let style = if *current {
                Style::default()
                    .bg(Color::Rgb(50, 56, 66))
                    .fg(Color::Rgb(171, 178, 191))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(Color::Rgb(40, 44, 52))
                    .fg(Color::Rgb(92, 99, 112))
            };

            for ch in label.chars() {
                if x >= area.x + area.width {
                    break;
                }
                buf[(x, area.y)].set_char(ch);
                buf[(x, area.y)].set_style(style);
                x += 1;
            }

            // Separator
            if x < area.x + area.width {
                buf[(x, area.y)].set_char('│');
                buf[(x, area.y)].set_style(
                    Style::default()
                        .bg(Color::Rgb(40, 44, 52))
                        .fg(Color::Rgb(60, 64, 72)),
                );
                x += 1;
            }

            if x >= area.x + area.width {
                break;
            }
        }
    }
}
