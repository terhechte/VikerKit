use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;

/// Draw the hover information popup.
pub fn draw_hover(editor: &Editor, ui: &mut Ui, area: Rect, char_width: f32, line_height: f32) {
    if !editor.showing_hover {
        return;
    }
    let text = match &editor.hover_text {
        Some(t) => t,
        None => return,
    };

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);

    let lines: Vec<&str> = text.lines().collect();
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(20);
    let popup_width = ((max_line_len + 4) as f32 * char_width).min(area.width() * 0.9);
    let popup_height = (lines.len() as f32 * line_height + 8.0).min(area.height() * 0.5);

    // Position above cursor
    let cursor_x = area.min.x + (editor.cursor.col as f32 + 5.0) * char_width;
    let cursor_y = area.min.y + ((editor.cursor.row - editor.view.offset_row) as f32) * line_height;
    let popup_y = (cursor_y - popup_height).max(area.min.y);

    let popup_rect = Rect::from_min_size(
        Pos2::new(cursor_x, popup_y),
        egui::vec2(popup_width, popup_height),
    );

    let painter = ui.painter_at(popup_rect);
    painter.rect_filled(popup_rect, 4.0, Color32::from_rgb(30, 33, 40));
    painter.rect_stroke(
        popup_rect,
        4.0,
        egui::Stroke::new(1.0, Color32::from_rgb(60, 65, 75)),
        egui::StrokeKind::Outside,
    );

    for (i, line) in lines.iter().enumerate() {
        let y = popup_rect.min.y + 4.0 + i as f32 * line_height;
        if y + line_height > popup_rect.max.y {
            break;
        }
        painter.text(
            Pos2::new(popup_rect.min.x + 8.0, y),
            egui::Align2::LEFT_TOP,
            *line,
            font.clone(),
            Color32::from_rgb(171, 178, 191),
        );
    }
}
