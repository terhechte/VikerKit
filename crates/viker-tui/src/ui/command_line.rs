use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;
use viker_core::input::mode::Mode;

pub struct CommandLine<'a> {
    editor: &'a Editor,
}

impl<'a> CommandLine<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for CommandLine<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (text, style, show_cursor) = match self.editor.mode {
            Mode::Command => (
                format!(":{}", self.editor.command_buffer),
                Style::default().fg(Color::White),
                true,
            ),
            Mode::Search => (
                format!("/{}", self.editor.search_query),
                Style::default().fg(Color::White),
                true,
            ),
            _ => {
                if let Some(display) = self.editor.register_display() {
                    let cursor =
                        self.editor.pending_keys.len() == 1 && self.editor.pending_keys[0] == '"';
                    (display, Style::default().fg(Color::Yellow), cursor)
                } else if let Some(msg) = &self.editor.status_message {
                    (msg.clone(), Style::default().fg(Color::DarkGray), false)
                } else {
                    (String::new(), Style::default().fg(Color::DarkGray), false)
                }
            }
        };

        for (i, ch) in text.chars().enumerate() {
            let x = area.x + i as u16;
            if x < area.right() {
                buf[(x, area.y)].set_char(ch).set_style(style);
            }
        }

        if show_cursor {
            let cursor_x = area.x + text.len() as u16;
            if cursor_x < area.right() {
                buf[(cursor_x, area.y)]
                    .set_char(' ')
                    .set_style(Style::default().bg(Color::White));
            }
        }
    }
}
