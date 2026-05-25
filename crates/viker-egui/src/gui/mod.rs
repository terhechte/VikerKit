pub mod code_actions;
pub mod command_line;
pub mod completion;
pub mod diagnostics;
pub mod editor_view;
pub mod file_finder;
pub mod git_diff;
pub mod hover;
pub mod project_sidebar;
pub mod references;
pub mod status_line;
pub mod tab_bar;
pub mod workspace_symbols;

use egui::{CentralPanel, Color32, Frame as EguiFrame, Margin, Rect, Sense, TopBottomPanel};

use viker_core::editor::Editor;
use viker_core::editor::pane::AreaRect;

use self::code_actions::draw_code_actions;
use self::command_line::{draw_command_line, draw_command_line_in_rect};
use self::completion::draw_completion;
use self::diagnostics::draw_diagnostics;
use self::editor_view::draw_editor_view;
use self::file_finder::draw_file_finder;
use self::git_diff::draw_git_diff;
use self::hover::draw_hover;
use self::references::draw_references;
use self::status_line::draw_status_line;
use self::tab_bar::{draw_tab_bar, draw_tab_bar_in_rect};
use self::workspace_symbols::draw_workspace_symbols;

/// One Dark background color.
pub(crate) const BG_COLOR: Color32 = Color32::from_rgb(40, 44, 52);

/// Render the full editor UI using egui.
/// Returns pane pixel rects for mouse hit-testing: Vec<(pane_id, egui::Rect)>.
pub fn render(editor: &Editor, ctx: &egui::Context) -> Vec<(usize, egui::Rect)> {
    let show_tabs = editor.buffers.len() > 1;

    // Tab bar at top
    if show_tabs {
        TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            draw_tab_bar(editor, ui);
        });
    }

    // Command line at bottom
    TopBottomPanel::bottom("command_line").show(ctx, |ui| {
        draw_command_line(editor, ui);
    });

    let mut collected_pane_rects: Vec<(usize, egui::Rect)> = Vec::new();

    CentralPanel::default()
        .frame(EguiFrame::new().fill(BG_COLOR).inner_margin(Margin::ZERO))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            collected_pane_rects = render_panes(editor, ui, rect);
        });

    collected_pane_rects
}

/// Render the editor inside the caller's current egui layout.
///
/// This is the embeddable entry point for host egui apps that do not want the
/// editor to claim top, bottom, or central panels.
pub fn render_in_ui(editor: &Editor, ui: &mut egui::Ui) -> Vec<(usize, egui::Rect)> {
    let rect = ui.available_rect_before_wrap();
    let pane_rects = render_in_rect(editor, ui, rect);
    ui.allocate_rect(rect, Sense::hover());
    pane_rects
}

pub fn render_in_rect(editor: &Editor, ui: &mut egui::Ui, rect: Rect) -> Vec<(usize, egui::Rect)> {
    ui.painter_at(rect).rect_filled(rect, 0.0, BG_COLOR);

    let font_size = editor.config.font_size;
    let line_height = font_size * 1.4;
    let show_tabs = editor.buffers.len() > 1;
    let tab_height = if show_tabs { line_height } else { 0.0 };
    let command_height = line_height;

    if show_tabs {
        let tab_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.max.x, (rect.min.y + tab_height).min(rect.max.y)),
        );
        draw_tab_bar_in_rect(editor, ui, tab_rect);
    }

    let command_rect = Rect::from_min_max(
        egui::pos2(rect.min.x, (rect.max.y - command_height).max(rect.min.y)),
        rect.max,
    );
    draw_command_line_in_rect(editor, ui, command_rect);

    let pane_rect = Rect::from_min_max(
        egui::pos2(rect.min.x, rect.min.y + tab_height),
        egui::pos2(rect.max.x, command_rect.min.y),
    );
    render_panes(editor, ui, pane_rect)
}

fn render_panes(editor: &Editor, ui: &mut egui::Ui, rect: Rect) -> Vec<(usize, egui::Rect)> {
    let font_size = editor.config.font_size;
    let char_width = font_size * 0.6;
    let line_height = font_size * 1.4;

    // Calculate pane layout using the editor's AreaRect system
    let cols = (rect.width() / char_width) as u16;
    let rows = (rect.height() / line_height) as u16;

    let pane_area = AreaRect::new(0, 0, cols, rows);
    let pane_rects = editor.pane_layout.layout(pane_area);
    let mut collected_pane_rects: Vec<(usize, egui::Rect)> = Vec::new();

    for &(pane_id, arect) in &pane_rects {
        if arect.height < 2 {
            continue;
        }
        let is_active = pane_id == editor.active_pane_id;

        // Convert AreaRect to screen coordinates
        let pane_rect = egui::Rect::from_min_size(
            egui::pos2(
                rect.min.x + arect.x as f32 * char_width,
                rect.min.y + arect.y as f32 * line_height,
            ),
            egui::vec2(
                arect.width as f32 * char_width,
                arect.height as f32 * line_height,
            ),
        );

        collected_pane_rects.push((pane_id, pane_rect));

        let editor_rows = arect.height.saturating_sub(1);
        let editor_rect = egui::Rect::from_min_size(
            pane_rect.min,
            egui::vec2(pane_rect.width(), editor_rows as f32 * line_height),
        );
        let status_rect = egui::Rect::from_min_size(
            egui::pos2(
                pane_rect.min.x,
                pane_rect.min.y + editor_rows as f32 * line_height,
            ),
            egui::vec2(pane_rect.width(), line_height),
        );

        draw_editor_view(
            editor,
            pane_id,
            is_active,
            ui,
            editor_rect,
            char_width,
            line_height,
        );
        draw_status_line(
            editor,
            pane_id,
            is_active,
            ui,
            status_rect,
            char_width,
            line_height,
        );
    }

    // Draw popups over active pane
    let active_arect = pane_rects
        .iter()
        .find(|(id, _)| *id == editor.active_pane_id)
        .map(|(_, r)| *r)
        .unwrap_or(pane_area);

    let popup_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.min.x + active_arect.x as f32 * char_width,
            rect.min.y + active_arect.y as f32 * line_height,
        ),
        egui::vec2(
            active_arect.width as f32 * char_width,
            active_arect.height.saturating_sub(1) as f32 * line_height,
        ),
    );

    draw_completion(editor, ui, popup_rect, char_width, line_height);
    draw_hover(editor, ui, popup_rect, char_width, line_height);
    draw_references(editor, ui, popup_rect, char_width, line_height);
    draw_code_actions(editor, ui, popup_rect, char_width, line_height);
    draw_diagnostics(editor, ui, popup_rect, char_width, line_height);
    draw_file_finder(editor, ui, popup_rect, char_width, line_height);
    draw_workspace_symbols(editor, ui, popup_rect, char_width, line_height);
    draw_git_diff(editor, ui, popup_rect, char_width, line_height);

    collected_pane_rects
}
