use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;

/// Draw the tab bar showing open buffers.
pub fn draw_tab_bar(editor: &Editor, ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    draw_tab_bar_in_rect(editor, ui, rect);
}

pub fn draw_tab_bar_in_rect(editor: &Editor, ui: &mut Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let font_size = editor.config.font_size - 1.0;
    let font = FontId::new(font_size, FontFamily::Monospace);

    // Background
    painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 33, 40));

    let char_width = font_size * 0.6;
    let mut x = rect.min.x + 4.0;

    for (i, buf) in editor.buffers.iter().enumerate() {
        let name = buf.document.file_name();
        let modified = if buf.document.modified { " +" } else { "" };
        let label = format!(" {}{} ", name, modified);
        let tab_width = label.len() as f32 * char_width;

        let is_current = i == editor.current_buffer;

        if is_current {
            let tab_rect = Rect::from_min_size(
                Pos2::new(x, rect.min.y),
                egui::vec2(tab_width, rect.height()),
            );
            painter.rect_filled(tab_rect, 0.0, Color32::from_rgb(40, 44, 52));
        }

        let color = if is_current {
            Color32::WHITE
        } else {
            Color32::from_rgb(120, 120, 120)
        };

        painter.text(
            Pos2::new(x, rect.min.y),
            egui::Align2::LEFT_TOP,
            &label,
            font.clone(),
            color,
        );

        x += tab_width + 2.0;
    }

    ui.allocate_rect(rect, egui::Sense::hover());
}
