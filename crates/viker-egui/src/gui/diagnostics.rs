use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;

/// Draw the diagnostics list popup.
pub fn draw_diagnostics(
    editor: &Editor,
    ui: &mut Ui,
    area: Rect,
    _char_width: f32,
    line_height: f32,
) {
    if !editor.showing_diagnostics || editor.diagnostics.is_empty() {
        return;
    }

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);
    let max_items = 15.min(editor.diagnostics.len());
    let popup_width = area.width() * 0.9;
    let popup_height = (max_items as f32 + 1.0) * line_height;

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

    // Title
    painter.text(
        Pos2::new(popup_rect.min.x + 8.0, popup_rect.min.y + 2.0),
        egui::Align2::LEFT_TOP,
        format!("Diagnostics ({})", editor.diagnostics.len()),
        font.clone(),
        Color32::from_rgb(97, 175, 239),
    );

    for (i, diag) in editor.diagnostics.iter().take(max_items).enumerate() {
        let y = popup_rect.min.y + (i + 1) as f32 * line_height + 2.0;
        let is_selected = i == editor.diagnostic_list_index;

        if is_selected {
            let sel_rect = Rect::from_min_size(
                Pos2::new(popup_rect.min.x + 1.0, y),
                egui::vec2(popup_width - 2.0, line_height),
            );
            painter.rect_filled(sel_rect, 2.0, Color32::from_rgb(50, 55, 65));
        }

        let sev_str = match diag.severity {
            1 => "E",
            2 => "W",
            3 => "I",
            _ => "H",
        };
        let sev_color = match diag.severity {
            1 => Color32::from_rgb(224, 108, 117),
            2 => Color32::from_rgb(229, 192, 123),
            _ => Color32::from_rgb(171, 178, 191),
        };

        let display = format!(" [{}] L{}: {}", sev_str, diag.start_line + 1, diag.message);

        let color = if is_selected {
            Color32::WHITE
        } else {
            sev_color
        };

        painter.text(
            Pos2::new(popup_rect.min.x + 8.0, y),
            egui::Align2::LEFT_TOP,
            &display,
            font.clone(),
            color,
        );
    }
}
