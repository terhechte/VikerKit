use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;
use viker_core::input::mode::Mode;

/// Draw the command/search line at the bottom of the screen.
pub fn draw_command_line(editor: &Editor, ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    draw_command_line_in_rect(editor, ui, rect);
}

pub fn draw_command_line_in_rect(editor: &Editor, ui: &mut Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let font_size = editor.config.font_size;
    let font = FontId::new(font_size, FontFamily::Monospace);

    // Background
    painter.rect_filled(rect, 0.0, Color32::from_rgb(40, 44, 52));

    let text = match editor.mode {
        Mode::Command => format!(":{}", editor.command_buffer),
        Mode::Search => format!("/{}", editor.search_query),
        _ => {
            if let Some(display) = editor.register_display() {
                display
            } else if let Some(ref msg) = editor.status_message {
                msg.clone()
            } else {
                String::new()
            }
        }
    };

    if !text.is_empty() {
        let color = match editor.mode {
            Mode::Command | Mode::Search => Color32::WHITE,
            _ => {
                if editor.register_display().is_some() {
                    Color32::from_rgb(229, 192, 64)
                } else {
                    Color32::from_rgb(171, 178, 191)
                }
            }
        };
        painter.text(
            Pos2::new(rect.min.x + 4.0, rect.min.y),
            egui::Align2::LEFT_TOP,
            &text,
            font,
            color,
        );
    }

    // Reserve the space
    ui.allocate_rect(rect, egui::Sense::hover());
}
