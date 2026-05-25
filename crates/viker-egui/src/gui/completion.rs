use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;

/// Draw the completion popup.
pub fn draw_completion(
    editor: &Editor,
    ui: &mut Ui,
    area: Rect,
    char_width: f32,
    line_height: f32,
) {
    if !editor.showing_completion || editor.completions.is_empty() {
        return;
    }

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);
    let max_items = 10.min(editor.completions.len());
    let popup_height = max_items as f32 * line_height;
    let popup_width = 300.0_f32.min(area.width() * 0.8);

    // Position below cursor
    let cursor_x = area.min.x + (editor.cursor.col as f32 + 5.0) * char_width;
    let cursor_y =
        area.min.y + ((editor.cursor.row - editor.view.offset_row) as f32 + 1.0) * line_height;

    let popup_rect = Rect::from_min_size(
        Pos2::new(cursor_x, cursor_y),
        egui::vec2(popup_width, popup_height + 4.0),
    );

    let painter = ui.painter_at(popup_rect);

    // Background
    painter.rect_filled(popup_rect, 4.0, Color32::from_rgb(30, 33, 40));
    painter.rect_stroke(
        popup_rect,
        4.0,
        egui::Stroke::new(1.0, Color32::from_rgb(60, 65, 75)),
        egui::StrokeKind::Outside,
    );

    // Items
    let start_idx = if editor.completion_index >= max_items {
        editor.completion_index - max_items + 1
    } else {
        0
    };

    for (i, item) in editor
        .completions
        .iter()
        .skip(start_idx)
        .take(max_items)
        .enumerate()
    {
        let y = popup_rect.min.y + 2.0 + i as f32 * line_height;
        let is_selected = start_idx + i == editor.completion_index;

        if is_selected {
            let sel_rect = Rect::from_min_size(
                Pos2::new(popup_rect.min.x + 1.0, y),
                egui::vec2(popup_width - 2.0, line_height),
            );
            painter.rect_filled(sel_rect, 2.0, Color32::from_rgb(50, 55, 65));
        }

        let color = if is_selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(171, 178, 191)
        };

        painter.text(
            Pos2::new(popup_rect.min.x + 8.0, y),
            egui::Align2::LEFT_TOP,
            &item.label,
            font.clone(),
            color,
        );
    }
}
