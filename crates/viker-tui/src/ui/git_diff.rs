use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use viker_core::editor::Editor;
use viker_core::git::{GitDiffLine, GitLineKind, GitPatchHighlight};
use viker_core::highlight::style::SyntaxStyle;

pub struct GitDiffPopup<'a> {
    editor: &'a Editor,
}

impl<'a> GitDiffPopup<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }
}

impl Widget for GitDiffPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.editor.showing_git_diff {
            return;
        }
        let Some(diff) = &self.editor.git_diff else {
            return;
        };

        let popup_width = (area.width.saturating_mul(9) / 10).max(30);
        let popup_height = (area.height.saturating_mul(4) / 5).max(8);
        let popup_x = area.x + area.width.saturating_sub(popup_width) / 2;
        let popup_y = area.y + area.height.saturating_sub(popup_height) / 2;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_width.min(area.width),
            popup_height.min(area.height),
        );

        let border_style = Style::default()
            .fg(Color::Rgb(80, 90, 110))
            .bg(Color::Rgb(30, 33, 40));
        let bg_style = Style::default()
            .fg(Color::Rgb(200, 200, 200))
            .bg(Color::Rgb(30, 33, 40));
        draw_box(buf, popup, border_style, bg_style);

        let title = format!(" Git Diff {:?} ({}) ", diff.mode, diff.files.len());
        draw_text(
            buf,
            popup.x + 2,
            popup.y,
            popup.right() - 1,
            &title,
            border_style,
        );

        let mut row = popup.y + 1;
        let content_right = popup.right().saturating_sub(2);
        for file in &diff.files {
            if row >= popup.bottom().saturating_sub(1) {
                break;
            }
            let path = file
                .new_path
                .as_deref()
                .or(file.old_path.as_deref())
                .unwrap_or("<unknown>");
            draw_text(
                buf,
                popup.x + 2,
                row,
                content_right,
                &format!("{:?} {path}", file.change),
                Style::default()
                    .fg(Color::Rgb(97, 175, 239))
                    .bg(Color::Rgb(30, 33, 40))
                    .add_modifier(Modifier::BOLD),
            );
            row += 1;
            for hunk in &file.hunks {
                if row >= popup.bottom().saturating_sub(1) {
                    break;
                }
                draw_text(
                    buf,
                    popup.x + 2,
                    row,
                    content_right,
                    &hunk.header,
                    Style::default()
                        .fg(Color::Rgb(198, 120, 221))
                        .bg(Color::Rgb(30, 33, 40)),
                );
                row += 1;
                for line in &hunk.lines {
                    if row >= popup.bottom().saturating_sub(1) {
                        break;
                    }
                    draw_diff_line(buf, popup.x + 2, row, content_right, line);
                    row += 1;
                }
            }
        }
    }
}

fn draw_box(buf: &mut Buffer, rect: Rect, border_style: Style, bg_style: Style) {
    for dy in 0..rect.height {
        let y = rect.y + dy;
        for dx in 0..rect.width {
            let x = rect.x + dx;
            let is_border = dy == 0 || dy == rect.height - 1 || dx == 0 || dx == rect.width - 1;
            let ch = if is_border {
                if dy == 0 && dx == 0 {
                    '┌'
                } else if dy == 0 && dx == rect.width - 1 {
                    '┐'
                } else if dy == rect.height - 1 && dx == 0 {
                    '└'
                } else if dy == rect.height - 1 && dx == rect.width - 1 {
                    '┘'
                } else if dy == 0 || dy == rect.height - 1 {
                    '─'
                } else {
                    '│'
                }
            } else {
                ' '
            };
            buf[(x, y)]
                .set_char(ch)
                .set_style(if is_border { border_style } else { bg_style });
        }
    }
}

fn draw_diff_line(buf: &mut Buffer, x: u16, y: u16, right: u16, line: &GitDiffLine) {
    let bg = match line.kind {
        GitLineKind::Addition => Color::Rgb(25, 55, 35),
        GitLineKind::Deletion => Color::Rgb(65, 30, 35),
        GitLineKind::Context | GitLineKind::Other => Color::Rgb(30, 33, 40),
    };
    let prefix_style = match line.kind {
        GitLineKind::Addition => Style::default().fg(Color::Rgb(152, 195, 121)).bg(bg),
        GitLineKind::Deletion => Style::default().fg(Color::Rgb(224, 108, 117)).bg(bg),
        GitLineKind::Context | GitLineKind::Other => Style::default().fg(Color::Gray).bg(bg),
    };
    let mut col = x;
    if col < right {
        buf[(col, y)]
            .set_char(line.prefix.chars().next().unwrap_or(' '))
            .set_style(prefix_style);
        col += 1;
    }
    if col < right {
        buf[(col, y)].set_char(' ').set_style(prefix_style);
        col += 1;
    }
    for (idx, ch) in line.content.chars().enumerate() {
        if col >= right {
            break;
        }
        let style = highlight_style(&line.highlights, idx)
            .map(to_ratatui_style)
            .unwrap_or_else(|| Style::default().fg(Color::Rgb(200, 200, 200)))
            .bg(bg);
        buf[(col, y)].set_char(ch).set_style(style);
        col += 1;
    }
}

fn highlight_style(highlights: &[GitPatchHighlight], column: usize) -> Option<SyntaxStyle> {
    highlights
        .iter()
        .find(|highlight| column >= highlight.start_column && column < highlight.end_column)
        .map(|highlight| highlight.style)
}

fn to_ratatui_style(style: SyntaxStyle) -> Style {
    let mut converted = Style::default();
    if let Some(color) = style.fg {
        converted = converted.fg(Color::Rgb(color.0, color.1, color.2));
    }
    if style.italic {
        converted = converted.add_modifier(Modifier::ITALIC);
    }
    converted
}

fn draw_text(buf: &mut Buffer, x: u16, y: u16, right: u16, text: &str, style: Style) {
    for (idx, ch) in text.chars().enumerate() {
        let col = x + idx as u16;
        if col >= right {
            break;
        }
        buf[(col, y)].set_char(ch).set_style(style);
    }
}
