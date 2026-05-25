use egui::{Color32, FontFamily, FontId, Pos2, Rect, Stroke, Ui};

use viker_core::buffer;
use viker_core::editor::Editor;
use viker_core::editor::display;
use viker_core::editor::pane::PaneRenderData;
use viker_core::editor::wrap;
use viker_core::highlight::style::SyntaxStyle;
use viker_core::input::mode::Mode;

/// Convert SyntaxStyle to egui Color32.
fn syntax_to_color(s: SyntaxStyle) -> Color32 {
    match s.fg {
        Some(c) => Color32::from_rgb(c.0, c.1, c.2),
        None => Color32::from_rgb(171, 178, 191), // default light gray
    }
}

/// Build PaneRenderData for a given pane (shared logic with TUI).
fn build_pane_render_data<'a>(
    editor: &'a Editor,
    pane_id: usize,
    is_active: bool,
) -> PaneRenderData<'a> {
    if is_active {
        let matching_bracket = editor.matching_bracket();
        PaneRenderData {
            document: &editor.document,
            cursor: editor.cursor,
            view: editor.view,
            mode: editor.mode,
            diagnostics: &editor.diagnostics,
            line_styles: &editor.line_styles,
            styles_offset: editor.styles_offset,
            search_matches: &editor.search_matches,
            search_query: &editor.search_query,
            visual_anchor: editor.visual_anchor,
            is_active: true,
            matching_bracket,
            relative_number: editor.config.relative_number,
            tab_width: editor.config.tab_width,
        }
    } else {
        let pane = editor.panes.iter().find(|p| p.id == pane_id);
        match pane {
            Some(pane) => {
                let buf = &editor.buffers[pane.buffer_idx];
                let matching_bracket =
                    PaneRenderData::compute_matching_bracket(&buf.document, pane.cursor);
                PaneRenderData {
                    document: &buf.document,
                    cursor: pane.cursor,
                    view: pane.view,
                    mode: Mode::Normal,
                    diagnostics: &buf.diagnostics,
                    line_styles: &pane.line_styles,
                    styles_offset: pane.styles_offset,
                    search_matches: &pane.search_matches,
                    search_query: &pane.search_query,
                    visual_anchor: None,
                    is_active: false,
                    matching_bracket,
                    relative_number: false,
                    tab_width: editor.config.tab_width,
                }
            }
            None => PaneRenderData {
                document: &editor.document,
                cursor: editor.cursor,
                view: editor.view,
                mode: editor.mode,
                diagnostics: &editor.diagnostics,
                line_styles: &editor.line_styles,
                styles_offset: editor.styles_offset,
                search_matches: &editor.search_matches,
                search_query: &editor.search_query,
                visual_anchor: editor.visual_anchor,
                is_active: false,
                matching_bracket: None,
                relative_number: false,
                tab_width: editor.config.tab_width,
            },
        }
    }
}

/// Draw the editor view for a single pane.
pub fn draw_editor_view(
    editor: &Editor,
    pane_id: usize,
    is_active: bool,
    ui: &mut Ui,
    area: Rect,
    char_width: f32,
    line_height: f32,
) {
    let data = build_pane_render_data(editor, pane_id, is_active);
    let painter = ui.painter_at(area);

    // Background
    painter.rect_filled(area, 0.0, Color32::from_rgb(40, 44, 52));

    let line_count = data.document.line_count();
    let gutter_digits = if line_count == 0 {
        1
    } else {
        (line_count as f64).log10().floor() as usize + 1
    };
    let gutter_chars = gutter_digits + 2;
    let gutter_width = gutter_chars as f32 * char_width;
    let text_area_width = area.width() - gutter_width;
    let text_cols = (text_area_width / char_width) as u16;

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);

    let offset_row = data.view.offset_row;
    let offset_col = data.view.offset_col;
    let visible_rows = (area.height() / line_height) as usize;

    // Selection range
    if data.view.wrap && text_cols > 0 {
        // Wrapped rendering
        let screen_map = wrap::build_screen_map_with_tab_width(
            &data.document.rope,
            offset_row,
            data.view.offset_wrap,
            text_cols,
            visible_rows as u16,
            data.tab_width,
        );

        for (y, seg) in screen_map.iter().enumerate() {
            let screen_y = area.min.y + y as f32 * line_height;
            let doc_row = seg.doc_row;
            let is_first = seg.segment_index == 0;

            // Diagnostic background
            draw_diag_bg(
                &painter,
                &data,
                doc_row,
                area.min.x + gutter_width,
                screen_y,
                text_area_width,
                line_height,
            );

            // Gutter
            if is_first {
                draw_gutter_line(
                    &painter,
                    &data,
                    doc_row,
                    area.min.x,
                    screen_y,
                    gutter_chars,
                    char_width,
                    &font,
                );
            }

            // Text
            let line = data.document.rope.line(doc_row);
            let line_len = buffer::line_display_len(line);
            let mut text_x = 0.0_f32;

            for cell in display::display_cells_for_char_range(
                line,
                seg.char_start,
                seg.char_end.min(line_len),
                data.tab_width,
            ) {
                if text_x >= text_area_width {
                    break;
                }

                let sx = area.min.x + gutter_width + text_x;
                let is_cursor = data.is_active
                    && doc_row == data.cursor.row
                    && data.cursor.col >= cell.char_start
                    && data.cursor.col < cell.char_end;
                let is_sel = (cell.char_start..cell.char_end)
                    .any(|char_idx| data.is_selected(doc_row, char_idx));
                let is_search = (cell.char_start..cell.char_end)
                    .any(|char_idx| data.is_search_match(doc_row, char_idx));

                draw_cell(
                    &painter,
                    &cell.text,
                    sx,
                    screen_y,
                    char_width,
                    cell.cell_width,
                    line_height,
                    &font,
                    &data,
                    doc_row,
                    cell.char_start,
                    is_cursor,
                    is_sel,
                    is_search,
                );

                text_x += cell.cell_width as f32 * char_width;
            }

            // Cursor at end of line in insert mode
            if data.is_active
                && data.mode == Mode::Insert
                && doc_row == data.cursor.row
                && data.cursor.col >= line_len
                && is_first
                && seg.char_end >= line_len
            {
                let cx = area.min.x + gutter_width + text_x;
                draw_insert_cursor(&painter, cx, screen_y, char_width, line_height);
            }

            // Empty line cursor in normal mode
            if data.is_active
                && !matches!(data.mode, Mode::Insert)
                && doc_row == data.cursor.row
                && line_len == 0
                && is_first
            {
                let cx = area.min.x + gutter_width;
                draw_block_cursor(&painter, cx, screen_y, char_width, line_height);
            }
        }

        // Tildes for lines past end
        for y in screen_map.len()..visible_rows {
            let screen_y = area.min.y + y as f32 * line_height;
            let tilde_x = area.min.x + (gutter_chars.saturating_sub(2)) as f32 * char_width;
            painter.text(
                Pos2::new(tilde_x, screen_y),
                egui::Align2::LEFT_TOP,
                "~",
                font.clone(),
                Color32::from_rgb(90, 90, 90),
            );
        }
    } else {
        // Non-wrapped rendering
        for y in 0..visible_rows {
            let doc_row = offset_row + y;
            let screen_y = area.min.y + y as f32 * line_height;

            if doc_row < line_count {
                // Diagnostic background
                draw_diag_bg(
                    &painter,
                    &data,
                    doc_row,
                    area.min.x + gutter_width,
                    screen_y,
                    text_area_width,
                    line_height,
                );

                // Gutter
                draw_gutter_line(
                    &painter,
                    &data,
                    doc_row,
                    area.min.x,
                    screen_y,
                    gutter_chars,
                    char_width,
                    &font,
                );

                // Text
                let line = data.document.rope.line(doc_row);
                let line_len = buffer::line_display_len(line);
                let mut text_x = 0.0_f32;

                let offset_cell_col =
                    display::display_column_for_char(line, offset_col, data.tab_width);
                for cell in display::display_cells_for_char_range(
                    line,
                    offset_col,
                    line_len,
                    data.tab_width,
                ) {
                    if text_x >= text_area_width {
                        break;
                    }

                    let sx = area.min.x + gutter_width + text_x;
                    let is_cursor = data.is_active
                        && doc_row == data.cursor.row
                        && data.cursor.col >= cell.char_start
                        && data.cursor.col < cell.char_end;
                    let is_sel = (cell.char_start..cell.char_end)
                        .any(|char_idx| data.is_selected(doc_row, char_idx));
                    let is_search = (cell.char_start..cell.char_end)
                        .any(|char_idx| data.is_search_match(doc_row, char_idx));

                    draw_cell(
                        &painter,
                        &cell.text,
                        sx,
                        screen_y,
                        char_width,
                        cell.cell_width,
                        line_height,
                        &font,
                        &data,
                        doc_row,
                        cell.char_start,
                        is_cursor,
                        is_sel,
                        is_search,
                    );

                    text_x += cell.cell_width as f32 * char_width;
                }

                // Cursor at end of line in insert mode
                if data.is_active
                    && data.mode == Mode::Insert
                    && doc_row == data.cursor.row
                    && data.cursor.col >= line_len
                {
                    let cursor_cell_col =
                        display::display_column_for_char(line, data.cursor.col, data.tab_width);
                    let cx = area.min.x
                        + gutter_width
                        + cursor_cell_col.saturating_sub(offset_cell_col) as f32 * char_width;
                    draw_insert_cursor(&painter, cx, screen_y, char_width, line_height);
                }

                // Empty line cursor in normal mode
                if data.is_active
                    && !matches!(data.mode, Mode::Insert)
                    && doc_row == data.cursor.row
                    && line_len == 0
                {
                    let cx = area.min.x + gutter_width;
                    draw_block_cursor(&painter, cx, screen_y, char_width, line_height);
                }
            } else {
                // Tilde for past-end lines
                let tilde_x = area.min.x + (gutter_chars.saturating_sub(2)) as f32 * char_width;
                painter.text(
                    Pos2::new(tilde_x, screen_y),
                    egui::Align2::LEFT_TOP,
                    "~",
                    font.clone(),
                    Color32::from_rgb(90, 90, 90),
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    painter: &egui::Painter,
    text: &str,
    x: f32,
    y: f32,
    char_width: f32,
    cell_width: usize,
    line_height: f32,
    font: &FontId,
    data: &PaneRenderData,
    doc_row: usize,
    char_idx: usize,
    is_cursor: bool,
    is_selected: bool,
    is_search_match: bool,
) {
    let hl_style = data.highlight_style_at(doc_row, char_idx);
    let fg = syntax_to_color(hl_style);

    let rect_width = char_width * cell_width.max(1) as f32;
    let char_rect = Rect::from_min_size(Pos2::new(x, y), egui::vec2(rect_width, line_height));

    if is_cursor && !matches!(data.mode, Mode::Insert) {
        // Block cursor
        painter.rect_filled(char_rect, 0.0, Color32::WHITE);
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            Color32::BLACK,
        );
    } else if is_selected {
        painter.rect_filled(char_rect, 0.0, Color32::from_rgb(70, 130, 180));
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            Color32::BLACK,
        );
    } else if is_search_match {
        painter.rect_filled(char_rect, 0.0, Color32::from_rgb(180, 150, 50));
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            Color32::BLACK,
        );
    } else {
        // Check bracket matching
        let is_bracket = data
            .matching_bracket
            .is_some_and(|pos| pos.row == doc_row && pos.col == char_idx);
        if is_bracket {
            painter.rect_filled(char_rect, 0.0, Color32::from_rgb(60, 65, 80));
        }

        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            fg,
        );
    }

    if is_cursor && data.mode == Mode::Insert {
        draw_insert_cursor(painter, x, y, char_width, line_height);
    }
}

fn draw_insert_cursor(painter: &egui::Painter, x: f32, y: f32, _char_width: f32, line_height: f32) {
    painter.vline(x, y..=(y + line_height), Stroke::new(2.0, Color32::WHITE));
}

fn draw_block_cursor(painter: &egui::Painter, x: f32, y: f32, char_width: f32, line_height: f32) {
    let rect = Rect::from_min_size(Pos2::new(x, y), egui::vec2(char_width, line_height));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_gutter_line(
    painter: &egui::Painter,
    data: &PaneRenderData,
    doc_row: usize,
    x: f32,
    y: f32,
    gutter_chars: usize,
    _char_width: f32,
    font: &FontId,
) {
    let line_num = format!(
        "{:>width$} ",
        data.line_number_label(doc_row),
        width = gutter_chars - 1
    );
    let is_cursor_line = data.is_active && doc_row == data.cursor.row;

    // Diagnostic severity
    let diag_sev = diagnostic_severity_at(data, doc_row);
    let color = match diag_sev {
        Some(1) => Color32::from_rgb(224, 108, 117), // red
        Some(2) => Color32::from_rgb(229, 192, 123), // yellow
        _ if is_cursor_line => Color32::from_rgb(229, 192, 123), // yellow
        _ => Color32::from_rgb(90, 90, 90),          // dark gray
    };

    painter.text(
        Pos2::new(x, y),
        egui::Align2::LEFT_TOP,
        &line_num,
        font.clone(),
        color,
    );

    // Diagnostic sign
    if let Some(sev) = diag_sev {
        let (sign, sign_color) = match sev {
            1 => ("●", Color32::from_rgb(224, 108, 117)),
            2 => ("▲", Color32::from_rgb(229, 192, 123)),
            3 => ("■", Color32::from_rgb(97, 175, 239)),
            _ => ("·", Color32::from_rgb(86, 182, 194)),
        };
        painter.text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            sign,
            font.clone(),
            sign_color,
        );
    }
}

fn draw_diag_bg(
    painter: &egui::Painter,
    data: &PaneRenderData,
    doc_row: usize,
    x: f32,
    y: f32,
    width: f32,
    line_height: f32,
) {
    let diag_sev = diagnostic_severity_at(data, doc_row);
    let bg = match diag_sev {
        Some(1) => Some(Color32::from_rgba_premultiplied(50, 20, 20, 128)),
        Some(2) => Some(Color32::from_rgba_premultiplied(45, 40, 20, 128)),
        _ => None,
    };
    if let Some(bg) = bg {
        let rect = Rect::from_min_size(Pos2::new(x, y), egui::vec2(width, line_height));
        painter.rect_filled(rect, 0.0, bg);
    }
}

fn diagnostic_severity_at(data: &PaneRenderData, row: usize) -> Option<u8> {
    let mut worst: Option<u8> = None;
    for d in data.diagnostics {
        if d.start_line as usize <= row && row <= d.end_line as usize {
            let sev = d.severity;
            worst = Some(match worst {
                Some(w) => w.min(sev),
                None => sev,
            });
        }
    }
    worst
}
