use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;
use viker_core::input::mode::Mode;

/// Draw the status line for a pane.
pub fn draw_status_line(
    editor: &Editor,
    pane_id: usize,
    is_active: bool,
    ui: &mut Ui,
    area: Rect,
    char_width: f32,
    line_height: f32,
) {
    let painter = ui.painter_at(area);
    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);

    // Background
    let bg = if is_active {
        Color32::from_rgb(50, 55, 65)
    } else {
        Color32::from_rgb(35, 38, 46)
    };
    painter.rect_filled(area, 0.0, bg);

    // Get relevant data
    let mode: Mode;
    let file_name: String;
    let cursor_row: usize;
    let cursor_col: usize;
    let modified: bool;

    if is_active {
        mode = editor.mode;
        file_name = editor.document.file_name().to_string();
        cursor_row = editor.cursor.row;
        cursor_col = editor.cursor.col;
        modified = editor.document.modified;
    } else {
        let pane = editor.panes.iter().find(|p| p.id == pane_id);
        if let Some(pane) = pane {
            let buf = &editor.buffers[pane.buffer_idx];
            mode = Mode::Normal;
            file_name = buf.document.file_name().to_string();
            cursor_row = pane.cursor.row;
            cursor_col = pane.cursor.col;
            modified = buf.document.modified;
        } else {
            mode = Mode::Normal;
            file_name = "[No Name]".to_string();
            cursor_row = 0;
            cursor_col = 0;
            modified = false;
        }
    }

    // Mode indicator
    let mode_str = match mode {
        Mode::Normal => " NORMAL ",
        Mode::Insert => " INSERT ",
        Mode::Replace => " REPLACE ",
        Mode::Visual => " VISUAL ",
        Mode::VisualLine => " V-LINE ",
        Mode::VisualBlock => " V-BLOCK ",
        Mode::Command => " COMMAND ",
        Mode::Search => " SEARCH ",
    };

    let mode_color = match mode {
        Mode::Normal => Color32::from_rgb(97, 175, 239), // blue
        Mode::Insert => Color32::from_rgb(152, 195, 121), // green
        Mode::Replace => Color32::from_rgb(224, 108, 117), // red
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => Color32::from_rgb(198, 120, 221), // purple
        _ => Color32::from_rgb(171, 178, 191),
    };

    painter.text(
        Pos2::new(area.min.x + 2.0, area.min.y),
        egui::Align2::LEFT_TOP,
        mode_str,
        font.clone(),
        mode_color,
    );

    // File name
    let modified_mark = if modified { " [+]" } else { "" };
    let file_str = format!(" {}{}", file_name, modified_mark);
    let mode_width = mode_str.len() as f32 * char_width;
    painter.text(
        Pos2::new(area.min.x + mode_width + 4.0, area.min.y),
        egui::Align2::LEFT_TOP,
        &file_str,
        font.clone(),
        Color32::from_rgb(171, 178, 191),
    );

    // Cursor position (right-aligned)
    let pos_str = format!("{}:{} ", cursor_row + 1, cursor_col + 1);
    painter.text(
        Pos2::new(area.max.x - 4.0, area.min.y),
        egui::Align2::RIGHT_TOP,
        &pos_str,
        font,
        Color32::from_rgb(171, 178, 191),
    );
}
