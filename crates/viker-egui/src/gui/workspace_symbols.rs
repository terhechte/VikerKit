use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;
use viker_core::lsp;

/// Draw the workspace symbols popup.
pub fn draw_workspace_symbols(
    editor: &Editor,
    ui: &mut Ui,
    area: Rect,
    char_width: f32,
    line_height: f32,
) {
    if !editor.showing_workspace_symbols {
        return;
    }

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);
    let max_items = 15.min(editor.workspace_symbol_results.len());
    let popup_width = 600.0_f32.min(area.width() * 0.8);
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
    let input_str = format!("> {}", editor.workspace_symbol_query);
    painter.text(
        Pos2::new(popup_rect.min.x + 8.0, popup_rect.min.y + 4.0),
        egui::Align2::LEFT_TOP,
        &input_str,
        font.clone(),
        Color32::WHITE,
    );

    // Cursor in search input
    let cursor_x =
        popup_rect.min.x + 8.0 + (2 + editor.workspace_symbol_query.len()) as f32 * char_width;
    painter.vline(
        cursor_x,
        (popup_rect.min.y + 4.0)..=(popup_rect.min.y + 4.0 + line_height),
        egui::Stroke::new(2.0, Color32::WHITE),
    );

    // Results
    let selected = editor.workspace_symbol_index;
    let scroll = if selected >= max_items && max_items > 0 {
        selected - max_items + 1
    } else {
        0
    };

    for i in 0..max_items {
        let item_idx = scroll + i;
        if item_idx >= editor.workspace_symbol_results.len() {
            break;
        }
        let sym = &editor.workspace_symbol_results[item_idx];
        let y = popup_rect.min.y + (i + 1) as f32 * line_height + 4.0 + line_height;
        let is_selected = item_idx == selected;

        if is_selected {
            let sel_rect = Rect::from_min_size(
                Pos2::new(popup_rect.min.x + 1.0, y),
                egui::vec2(popup_width - 2.0, line_height),
            );
            painter.rect_filled(sel_rect, 2.0, Color32::from_rgb(50, 55, 65));
        }

        // Kind label with color
        let kind_label = lsp::symbol_kind_label(sym.kind);
        let kind_color = match sym.kind {
            6 | 12 => Color32::from_rgb(97, 175, 239), // Method/Function: blue
            23 | 5 => Color32::from_rgb(229, 192, 123), // Struct/Class: yellow
            10 | 22 => Color32::from_rgb(198, 120, 221), // Enum: purple
            11 => Color32::from_rgb(86, 182, 194),     // Interface/Trait: cyan
            14 => Color32::from_rgb(209, 154, 102),    // Constant: orange
            2 => Color32::from_rgb(152, 195, 121),     // Module: green
            _ => Color32::from_rgb(171, 178, 191),
        };

        let kind_text = format!("[{}]", kind_label);
        let kind_width = painter
            .text(
                Pos2::new(popup_rect.min.x + 16.0, y),
                egui::Align2::LEFT_TOP,
                &kind_text,
                font.clone(),
                kind_color,
            )
            .width();

        // Symbol name
        let name_color = if is_selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(171, 178, 191)
        };
        let file_name = sym.uri.rsplit('/').next().unwrap_or(&sym.uri);
        let display = format!(" {}  {}:{}", sym.name, file_name, sym.start_line + 1);

        painter.text(
            Pos2::new(popup_rect.min.x + 16.0 + kind_width, y),
            egui::Align2::LEFT_TOP,
            &display,
            font.clone(),
            name_color,
        );
    }

    // Count indicator
    let count_str = format!("{}", editor.workspace_symbol_results.len());
    painter.text(
        Pos2::new(popup_rect.max.x - 8.0, popup_rect.min.y + 4.0),
        egui::Align2::RIGHT_TOP,
        &count_str,
        font,
        Color32::from_rgb(90, 90, 90),
    );
}
