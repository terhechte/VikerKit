use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;

/// Draw the file finder popup.
pub fn draw_file_finder(
    editor: &Editor,
    ui: &mut Ui,
    area: Rect,
    char_width: f32,
    line_height: f32,
) {
    if !editor.showing_file_finder {
        return;
    }

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);
    let max_items = 15.min(editor.file_finder_filtered.len());
    let popup_width = 500.0_f32.min(area.width() * 0.8);
    let popup_height = (max_items as f32 + 2.0) * line_height;

    let popup_rect = Rect::from_min_size(
        Pos2::new(
            area.center().x - popup_width / 2.0,
            area.min.y + line_height,
        ),
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

    // Search input
    let input_str = format!("> {}", editor.file_finder_query);
    painter.text(
        Pos2::new(popup_rect.min.x + 8.0, popup_rect.min.y + 4.0),
        egui::Align2::LEFT_TOP,
        &input_str,
        font.clone(),
        Color32::WHITE,
    );

    // Cursor in search input
    let cursor_x =
        popup_rect.min.x + 8.0 + (2 + editor.file_finder_query.len()) as f32 * char_width;
    painter.vline(
        cursor_x,
        (popup_rect.min.y + 4.0)..=(popup_rect.min.y + 4.0 + line_height),
        egui::Stroke::new(2.0, Color32::WHITE),
    );

    // Results
    for (i, entry) in editor
        .file_finder_filtered
        .iter()
        .take(max_items)
        .enumerate()
    {
        let y = popup_rect.min.y + (i + 1) as f32 * line_height + 4.0 + line_height;
        let is_selected = i == editor.file_finder_index;

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
            Pos2::new(popup_rect.min.x + 16.0, y),
            egui::Align2::LEFT_TOP,
            entry,
            font.clone(),
            color,
        );
    }

    // Count indicator
    let count_str = format!(
        "{}/{}",
        editor.file_finder_filtered.len(),
        editor.file_finder_entries.len()
    );
    painter.text(
        Pos2::new(popup_rect.max.x - 8.0, popup_rect.min.y + 4.0),
        egui::Align2::RIGHT_TOP,
        &count_str,
        font,
        Color32::from_rgb(90, 90, 90),
    );
}
