use egui::{Color32, FontFamily, FontId, Pos2, Rect, Ui};

use viker_core::editor::Editor;
use viker_core::git::{GitDiffLine, GitLineKind, GitPatchHighlight};
use viker_core::highlight::style::SyntaxStyle;

pub fn draw_git_diff(editor: &Editor, ui: &mut Ui, area: Rect, char_width: f32, line_height: f32) {
    if !editor.showing_git_diff {
        return;
    }
    let Some(diff) = &editor.git_diff else {
        return;
    };

    let popup_width = area.width() * 0.9;
    let popup_height = area.height() * 0.82;
    let popup_rect = Rect::from_min_size(
        Pos2::new(
            area.center().x - popup_width / 2.0,
            area.center().y - popup_height / 2.0,
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

    let font = FontId::new(line_height / 1.4, FontFamily::Monospace);
    let mut y = popup_rect.min.y + 4.0;
    let left = popup_rect.min.x + 8.0;
    let right = popup_rect.max.x - 8.0;

    painter.text(
        Pos2::new(left, y),
        egui::Align2::LEFT_TOP,
        format!("Git Diff {:?} ({})", diff.mode, diff.files.len()),
        font.clone(),
        Color32::from_rgb(97, 175, 239),
    );
    y += line_height;

    for file in &diff.files {
        if y + line_height > popup_rect.max.y {
            break;
        }
        let path = file
            .new_path
            .as_deref()
            .or(file.old_path.as_deref())
            .unwrap_or("<unknown>");
        painter.text(
            Pos2::new(left, y),
            egui::Align2::LEFT_TOP,
            format!("{:?} {path}", file.change),
            font.clone(),
            Color32::from_rgb(97, 175, 239),
        );
        y += line_height;
        for hunk in &file.hunks {
            if y + line_height > popup_rect.max.y {
                break;
            }
            painter.text(
                Pos2::new(left, y),
                egui::Align2::LEFT_TOP,
                &hunk.header,
                font.clone(),
                Color32::from_rgb(198, 120, 221),
            );
            y += line_height;
            for line in &hunk.lines {
                if y + line_height > popup_rect.max.y {
                    break;
                }
                draw_diff_line(ui, line, left, right, y, char_width, line_height, &font);
                y += line_height;
            }
        }
    }
}

fn draw_diff_line(
    ui: &Ui,
    line: &GitDiffLine,
    left: f32,
    right: f32,
    y: f32,
    char_width: f32,
    line_height: f32,
    font: &FontId,
) {
    let bg = match line.kind {
        GitLineKind::Addition => Color32::from_rgb(25, 55, 35),
        GitLineKind::Deletion => Color32::from_rgb(65, 30, 35),
        GitLineKind::Context | GitLineKind::Other => Color32::from_rgb(30, 33, 40),
    };
    let prefix_color = match line.kind {
        GitLineKind::Addition => Color32::from_rgb(152, 195, 121),
        GitLineKind::Deletion => Color32::from_rgb(224, 108, 117),
        GitLineKind::Context | GitLineKind::Other => Color32::from_rgb(171, 178, 191),
    };
    let row_rect = Rect::from_min_max(Pos2::new(left - 2.0, y), Pos2::new(right, y + line_height));
    ui.painter().rect_filled(row_rect, 0.0, bg);
    let mut x = left;
    ui.painter().text(
        Pos2::new(x, y),
        egui::Align2::LEFT_TOP,
        line.prefix.as_str(),
        font.clone(),
        prefix_color,
    );
    x += char_width * 2.0;
    for (idx, ch) in line.content.chars().enumerate() {
        if x + char_width > right {
            break;
        }
        let color = highlight_style(&line.highlights, idx)
            .and_then(|style| style.fg)
            .map(|color| Color32::from_rgb(color.0, color.1, color.2))
            .unwrap_or(Color32::from_rgb(200, 200, 200));
        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            ch.to_string(),
            font.clone(),
            color,
        );
        x += char_width;
    }
}

fn highlight_style(highlights: &[GitPatchHighlight], column: usize) -> Option<SyntaxStyle> {
    highlights
        .iter()
        .find(|highlight| column >= highlight.start_column && column < highlight.end_column)
        .map(|highlight| highlight.style)
}
