pub mod code_actions;
pub mod command_line;
pub mod completion;
pub mod diagnostics;
pub mod editor_view;
pub mod file_finder;
pub mod git_diff;
pub mod hover;
pub mod references;
pub mod status_line;
pub mod tab_bar;
pub mod workspace_symbols;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};

use viker_core::editor::Editor;
use viker_core::editor::pane::{AreaRect, PaneRenderData};

use self::code_actions::CodeActionsPopup;
use self::command_line::CommandLine;
use self::completion::CompletionPopup;
use self::diagnostics::DiagnosticsPopup;
use self::editor_view::EditorView;
use self::file_finder::FileFinderPopup;
use self::git_diff::GitDiffPopup;
use self::hover::HoverPopup;
use self::references::ReferencesPopup;
use self::status_line::StatusLine;
use self::tab_bar::TabBar;
use self::workspace_symbols::WorkspaceSymbolsPopup;

/// Convert AreaRect to ratatui Rect.
fn to_rect(a: AreaRect) -> Rect {
    Rect::new(a.x, a.y, a.width, a.height)
}

/// Convert ratatui Rect to the core layout rectangle without making core depend on ratatui.
fn to_area_rect(r: Rect) -> AreaRect {
    AreaRect::new(r.x, r.y, r.width, r.height)
}

pub fn render(editor: &Editor, frame: &mut Frame) {
    let size = frame.area();

    let show_tabs = editor.buffers.len() > 1;

    // Layout: [tab_bar(0or1), pane_area(Min), command_line(1)]
    let chunks = if show_tabs {
        Layout::vertical([
            Constraint::Length(1), // tab bar
            Constraint::Min(1),    // pane area (includes per-pane status lines)
            Constraint::Length(1), // command line
        ])
        .split(size)
    } else {
        Layout::vertical([
            Constraint::Length(0), // no tab bar
            Constraint::Min(1),    // pane area
            Constraint::Length(1), // command line
        ])
        .split(size)
    };

    if show_tabs {
        frame.render_widget(TabBar::new(editor), chunks[0]);
    }

    let pane_area_rect = to_area_rect(chunks[1]);

    // Calculate pane layout rects
    let pane_rects = editor.pane_layout.layout(pane_area_rect);

    // Draw vertical separators
    let buf = frame.buffer_mut();
    let separators = editor.pane_layout.separators(pane_area_rect);
    let sep_style = Style::default().fg(Color::DarkGray);
    for (x, y, height) in separators {
        for dy in 0..height {
            let sy = y + dy;
            if x < size.width + size.x && sy < size.height + size.y {
                buf[(x, sy)].set_char('│').set_style(sep_style);
            }
        }
    }

    // Render each pane
    let mut active_pane_rect = chunks[1];
    for &(pane_id, arect) in &pane_rects {
        let rect = to_rect(arect);
        if rect.height < 2 {
            continue; // need at least 1 row for editor + 1 row for status
        }
        // Split pane rect into editor area + status line
        let editor_rect = Rect::new(rect.x, rect.y, rect.width, rect.height.saturating_sub(1));
        let status_rect = Rect::new(rect.x, rect.y + editor_rect.height, rect.width, 1);

        let is_active = pane_id == editor.active_pane_id;
        if is_active {
            active_pane_rect = editor_rect;
        }

        let render_data = build_pane_render_data(editor, pane_id, is_active);
        let status_data = build_pane_render_data(editor, pane_id, is_active);

        frame.render_widget(EditorView::new(render_data), editor_rect);
        frame.render_widget(StatusLine::new(status_data), status_rect);
    }

    // Popups render over the active pane's editor area only
    frame.render_widget(CompletionPopup::new(editor), active_pane_rect);
    frame.render_widget(HoverPopup::new(editor), active_pane_rect);
    frame.render_widget(ReferencesPopup::new(editor), active_pane_rect);
    frame.render_widget(CodeActionsPopup::new(editor), active_pane_rect);
    frame.render_widget(DiagnosticsPopup::new(editor), active_pane_rect);
    frame.render_widget(FileFinderPopup::new(editor), active_pane_rect);
    frame.render_widget(WorkspaceSymbolsPopup::new(editor), active_pane_rect);
    frame.render_widget(GitDiffPopup::new(editor), active_pane_rect);
    frame.render_widget(CommandLine::new(editor), chunks[2]);
}

/// Build PaneRenderData for a given pane.
fn build_pane_render_data<'a>(
    editor: &'a Editor,
    pane_id: usize,
    is_active: bool,
) -> PaneRenderData<'a> {
    if is_active {
        // Active pane uses editor's top-level fields
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
        // Inactive pane uses saved pane state + buffer state
        let pane = editor.panes.iter().find(|p| p.id == pane_id);
        match pane {
            Some(pane) => {
                let buffer = &editor.buffers[pane.buffer_idx];
                let matching_bracket =
                    PaneRenderData::compute_matching_bracket(&buffer.document, pane.cursor);
                PaneRenderData {
                    document: &buffer.document,
                    cursor: pane.cursor,
                    view: pane.view,
                    mode: Mode::Normal, // inactive panes show as normal
                    diagnostics: &buffer.diagnostics,
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
            None => {
                // Fallback: shouldn't happen, but be safe
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
                    is_active: false,
                    matching_bracket: None,
                    relative_number: false,
                    tab_width: editor.config.tab_width,
                }
            }
        }
    }
}

use viker_core::input::mode::Mode;
