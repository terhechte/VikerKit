use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::pane::PaneRenderData;
use viker_core::input::mode::Mode;

/// A status line that can render for a specific pane (active or inactive).
pub struct StatusLine<'a> {
    data: PaneRenderData<'a>,
}

impl<'a> StatusLine<'a> {
    pub fn new(data: PaneRenderData<'a>) -> Self {
        Self { data }
    }
}

impl Widget for StatusLine<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.data.is_active {
            self.render_active(area, buf);
        } else {
            self.render_inactive(area, buf);
        }
    }
}

impl StatusLine<'_> {
    fn render_active(self, area: Rect, buf: &mut Buffer) {
        let (mode_str, mode_style) = match self.data.mode {
            Mode::Normal => (
                " NORMAL ",
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::Insert => (
                " INSERT ",
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::Replace => (
                " REPLACE ",
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::Visual => (
                " VISUAL ",
                Style::default()
                    .bg(Color::Magenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::VisualLine => (
                " V-LINE ",
                Style::default()
                    .bg(Color::Magenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::VisualBlock => (
                " V-BLOCK ",
                Style::default()
                    .bg(Color::Magenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::Command => (
                " COMMAND ",
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Mode::Search => (
                " SEARCH ",
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        let file_name = self.data.document.file_name();
        let modified = if self.data.document.modified {
            " [+]"
        } else {
            ""
        };
        let file_info = format!(" {file_name}{modified}");

        // Diagnostic counts
        let errors = self
            .data
            .diagnostics
            .iter()
            .filter(|d| d.severity == 1)
            .count();
        let warnings = self
            .data
            .diagnostics
            .iter()
            .filter(|d| d.severity == 2)
            .count();
        let diag_str = if errors > 0 || warnings > 0 {
            format!(" E:{errors} W:{warnings}")
        } else {
            String::new()
        };

        let position = format!(
            "{} {}:{} ",
            diag_str,
            self.data.cursor.row + 1,
            self.data.cursor.col + 1,
        );

        let bar_style = Style::default().bg(Color::DarkGray).fg(Color::White);

        // Fill background
        for x in 0..area.width {
            buf[(area.x + x, area.y)].set_char(' ').set_style(bar_style);
        }

        // Mode indicator
        let mut x = area.x;
        for ch in mode_str.chars() {
            if x < area.right() {
                buf[(x, area.y)].set_char(ch).set_style(mode_style);
                x += 1;
            }
        }

        // File info
        for ch in file_info.chars() {
            if x < area.right() {
                buf[(x, area.y)].set_char(ch).set_style(bar_style);
                x += 1;
            }
        }

        // Position (right-aligned)
        let pos_start = area.right().saturating_sub(position.len() as u16);
        let mut px = pos_start;
        for ch in position.chars() {
            if px < area.right() {
                buf[(px, area.y)].set_char(ch).set_style(bar_style);
                px += 1;
            }
        }
    }

    fn render_inactive(self, area: Rect, buf: &mut Buffer) {
        let file_name = self.data.document.file_name();
        let modified = if self.data.document.modified {
            " [+]"
        } else {
            ""
        };
        let file_info = format!(" {file_name}{modified}");

        let position = format!(
            " {}:{} ",
            self.data.cursor.row + 1,
            self.data.cursor.col + 1,
        );

        // Darker background for inactive pane
        let bar_style = Style::default()
            .bg(Color::Rgb(40, 40, 40))
            .fg(Color::Rgb(140, 140, 140));

        // Fill background
        for x in 0..area.width {
            buf[(area.x + x, area.y)].set_char(' ').set_style(bar_style);
        }

        // File info
        let mut x = area.x;
        for ch in file_info.chars() {
            if x < area.right() {
                buf[(x, area.y)].set_char(ch).set_style(bar_style);
                x += 1;
            }
        }

        // Position (right-aligned)
        let pos_start = area.right().saturating_sub(position.len() as u16);
        let mut px = pos_start;
        for ch in position.chars() {
            if px < area.right() {
                buf[(px, area.y)].set_char(ch).set_style(bar_style);
                px += 1;
            }
        }
    }
}
