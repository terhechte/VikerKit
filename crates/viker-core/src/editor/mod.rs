pub mod display;
pub mod document;
pub mod history;
pub mod pane;
pub mod selection;
pub mod view;
pub mod wrap;

use crate::buffer;
use crate::config::Config;
use crate::git::{GitDiff, GitDiffMode, GitEditorCommand};
use crate::highlight::style::{SyntaxStyle, SyntaxToken};
use crate::highlight::{self, Highlighter, LineStyles, SyntaxLanguage};
use crate::input::command::{FindDirection, FindKind, LastFind, Motion};
use crate::input::mode::Mode;
use crate::key::{KeyCode, KeyInput};
use crate::lsp::{self, LspCodeAction, LspCompletionItem, LspDiagnostic, LspLocation};
use crate::search;

use self::document::Document;
use self::history::History;
use self::pane::{AreaRect, NavigateDir, Pane, PaneNode, SplitDirection};
use self::selection::{Position, SelectionMode};
use self::view::View;

use crate::input::command::Command;

/// Represents a saved buffer state for multi-buffer support.
pub struct BufferState {
    pub document: Document,
    pub cursor: Position,
    pub view: View,
    pub history: History,
    pub syntax_language_override: Option<SyntaxLanguage>,
    pub syntax_tree: Option<tree_sitter::Tree>,
    pub line_styles: LineStyles,
    pub styles_offset: usize,
    pub diagnostics: Vec<LspDiagnostic>,
    pub search_query: String,
    pub search_matches: Vec<(usize, usize, usize)>,
    pub search_index: Option<usize>,
    pub search_regex: Option<regex::Regex>,
    pub jump_list: Vec<Position>,
    pub jump_index: usize,
}

impl BufferState {
    fn empty() -> Self {
        Self {
            document: Document::new_empty(),
            cursor: Position::default(),
            view: View::default(),
            history: History::new(),
            syntax_language_override: None,
            syntax_tree: None,
            line_styles: Vec::new(),
            styles_offset: 0,
            diagnostics: Vec::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: None,
            search_regex: None,
            jump_list: Vec::new(),
            jump_index: 0,
        }
    }
}

/// Represents the last text-changing action for `.` repeat.
#[derive(Debug, Clone)]
pub enum LastChange {
    NormalCommand(Command),
    InsertSession {
        entry_cmd: Command,
        chars: Vec<char>,
    },
}

#[derive(Clone, Default)]
pub struct Register {
    pub content: String,
    pub linewise: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegisterOp {
    Yank,
    Delete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct VisualBlockEdit {
    start_row: usize,
    end_row: usize,
    col: usize,
}

#[derive(Clone, Debug)]
pub struct HighlightSpan {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub token: SyntaxToken,
    pub style: SyntaxStyle,
}

fn clipboard_get() -> Option<String> {
    use clipboard_rs::{Clipboard, ClipboardContext};

    ClipboardContext::new()
        .ok()
        .and_then(|clipboard| clipboard.get_text().ok())
        .filter(|s| !s.is_empty())
}

fn clipboard_set(content: &str) {
    use clipboard_rs::{Clipboard, ClipboardContext};

    if let Ok(clipboard) = ClipboardContext::new() {
        let _ = clipboard.set_text(content.to_owned());
    }
}

pub struct Editor {
    pub document: Document,
    pub view: View,
    pub cursor: Position,
    pub mode: Mode,
    pub command_buffer: String,
    pub pending_keys: Vec<char>,
    pub count_prefix: Option<usize>,
    pub pending_operator_count: Option<usize>,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub config: Config,
    pub history: History,
    pub visual_anchor: Option<Position>,
    pub syntax_language_override: Option<SyntaxLanguage>,
    pub highlighter: Option<Highlighter>,
    pub syntax_tree: Option<tree_sitter::Tree>,
    pub line_styles: LineStyles,
    pub styles_offset: usize,
    // LSP state
    pub diagnostics: Vec<LspDiagnostic>,
    pub completions: Vec<LspCompletionItem>,
    pub completion_index: usize,
    pub showing_completion: bool,
    pub pending_completion_id: Option<i64>,
    // LSP Phase 5: hover, goto, references, rename
    pub hover_text: Option<String>,
    pub showing_hover: bool,
    pub references: Vec<LspLocation>,
    pub reference_index: usize,
    pub showing_references: bool,
    pub pending_goto_id: Option<i64>,
    pub pending_hover_id: Option<i64>,
    pub pending_references_id: Option<i64>,
    pub pending_rename_id: Option<i64>,
    pub pending_format_id: Option<i64>,
    // Search state
    pub search_query: String,
    pub search_matches: Vec<(usize, usize, usize)>, // (row, col, len)
    pub search_index: Option<usize>,
    pub search_regex: Option<regex::Regex>,
    pub search_start_cursor: Option<Position>,
    // File finder state
    pub showing_file_finder: bool,
    pub file_finder_query: String,
    pub file_finder_entries: Vec<String>,  // all project files
    pub file_finder_filtered: Vec<String>, // filtered results
    pub file_finder_index: usize,
    // Jump list
    pub jump_list: Vec<Position>,
    pub jump_index: usize,
    pub marks: std::collections::HashMap<char, Position>,
    pub previous_position: Option<Position>,
    pub last_find: Option<LastFind>,
    pub search_forward: bool,
    pub last_search_forward: bool,
    // Multi-buffer
    pub buffers: Vec<BufferState>,
    pub current_buffer: usize,
    // `.` repeat
    pub last_change: Option<LastChange>,
    pub recording_insert: bool,
    pub insert_entry_cmd: Option<Command>,
    pub insert_record: Vec<char>,
    // Command history
    pub command_history: Vec<String>,
    pub command_history_idx: Option<usize>,
    pub command_history_temp: String,
    // Named registers
    pub registers: std::collections::HashMap<char, Register>,
    pub selected_register: Option<char>,
    // Macro recording
    pub recording_macro: Option<char>,
    pub macro_buffer: Vec<KeyInput>,
    pub macros: std::collections::HashMap<char, Vec<KeyInput>>,
    pub last_macro: Option<char>,
    // Phase 11: Code actions
    pub code_actions: Vec<LspCodeAction>,
    pub code_action_index: usize,
    pub showing_code_actions: bool,
    pub pending_code_action_id: Option<i64>,
    // Phase 11: Diagnostics list
    pub showing_diagnostics: bool,
    pub diagnostic_list_index: usize,
    // Workspace symbol search
    pub showing_workspace_symbols: bool,
    pub workspace_symbol_query: String,
    pub workspace_symbol_results: Vec<lsp::LspSymbolInfo>,
    pub workspace_symbol_index: usize,
    pub pending_workspace_symbol_id: Option<i64>,
    pub workspace_symbol_needs_request: bool,
    // Git diff popup
    pub git_diff: Option<GitDiff>,
    pub showing_git_diff: bool,
    // Window split (panes)
    pub panes: Vec<Pane>,
    pub active_pane_id: usize,
    pub pane_layout: PaneNode,
    pub next_pane_id: usize,
    pub editor_area: AreaRect,
    pub font_family_changed: bool,
    visual_block_edit: Option<VisualBlockEdit>,
    visual_block_insert_text: String,
}

#[allow(dead_code)]
impl Editor {
    pub fn new(document: Document) -> Self {
        Self::with_config(document, Config::default())
    }

    pub fn with_config(document: Document, config: Config) -> Self {
        let wrap = config.wrap;
        let syntax_language = SyntaxLanguage::from_path_and_text(
            document.path.as_deref(),
            &document.rope.to_string(),
        );
        Self {
            document,
            view: View {
                wrap,
                ..View::default()
            },
            cursor: Position::default(),
            mode: Mode::Normal,
            command_buffer: String::new(),
            pending_keys: Vec::new(),
            count_prefix: None,
            pending_operator_count: None,
            should_quit: false,
            status_message: None,
            config,
            history: History::new(),
            visual_anchor: None,
            syntax_language_override: None,
            highlighter: syntax_language.and_then(Highlighter::new),
            syntax_tree: None,
            line_styles: Vec::new(),
            styles_offset: 0,
            diagnostics: Vec::new(),
            completions: Vec::new(),
            completion_index: 0,
            showing_completion: false,
            pending_completion_id: None,
            hover_text: None,
            showing_hover: false,
            references: Vec::new(),
            reference_index: 0,
            showing_references: false,
            pending_goto_id: None,
            pending_hover_id: None,
            pending_references_id: None,
            pending_rename_id: None,
            pending_format_id: None,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: None,
            search_regex: None,
            search_start_cursor: None,
            showing_file_finder: false,
            file_finder_query: String::new(),
            file_finder_entries: Vec::new(),
            file_finder_filtered: Vec::new(),
            file_finder_index: 0,
            jump_list: Vec::new(),
            jump_index: 0,
            marks: std::collections::HashMap::new(),
            previous_position: None,
            last_find: None,
            search_forward: true,
            last_search_forward: true,
            buffers: vec![BufferState::empty()],
            current_buffer: 0,
            last_change: None,
            recording_insert: false,
            insert_entry_cmd: None,
            insert_record: Vec::new(),
            command_history: Vec::new(),
            command_history_idx: None,
            command_history_temp: String::new(),
            registers: std::collections::HashMap::new(),
            selected_register: None,
            recording_macro: None,
            macro_buffer: Vec::new(),
            macros: std::collections::HashMap::new(),
            last_macro: None,
            code_actions: Vec::new(),
            code_action_index: 0,
            showing_code_actions: false,
            pending_code_action_id: None,
            showing_diagnostics: false,
            diagnostic_list_index: 0,
            showing_workspace_symbols: false,
            workspace_symbol_query: String::new(),
            workspace_symbol_results: Vec::new(),
            workspace_symbol_index: 0,
            pending_workspace_symbol_id: None,
            workspace_symbol_needs_request: false,
            git_diff: None,
            showing_git_diff: false,
            panes: vec![Pane::new(0, 0)],
            active_pane_id: 0,
            pane_layout: PaneNode::Leaf(0),
            next_pane_id: 1,
            editor_area: AreaRect::default(),
            font_family_changed: false,
            visual_block_edit: None,
            visual_block_insert_text: String::new(),
        }
    }

    pub fn clamp_cursor(&mut self) {
        let max_row = self.document.line_count().saturating_sub(1);
        if self.cursor.row > max_row {
            self.cursor.row = max_row;
        }

        let line_len = self.document.line_len(self.cursor.row);
        let max_col = if self.mode == Mode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
        if self.cursor.col > max_col {
            self.cursor.col = max_col;
        }
    }

    pub fn scroll(&mut self) {
        if self.config.wrap {
            let gutter_w = self.gutter_width();
            let text_width = self.view.width.saturating_sub(gutter_w);
            self.view.ensure_cursor_visible_wrapped(
                &self.cursor,
                self.config.scroll_off,
                &self.document.rope,
                text_width,
                self.config.tab_width,
            );
        } else {
            self.view
                .ensure_cursor_visible(&self.cursor, self.config.scroll_off);
        }
    }

    // --- Cursor and selection placement ---

    pub fn clamped_position(&self, row: usize, col: usize, allow_line_end: bool) -> Position {
        let row = row.min(self.document.line_count().saturating_sub(1));
        let line_len = self.document.line_len(row);
        let max_col = if allow_line_end {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
        Position {
            row,
            col: col.min(max_col),
        }
    }

    fn clamped_position_for_current_mode(&self, row: usize, col: usize) -> Position {
        self.clamped_position(row, col, self.mode == Mode::Insert)
    }

    fn mode_for_selection(selection_mode: SelectionMode) -> Mode {
        match selection_mode {
            SelectionMode::Character => Mode::Visual,
            SelectionMode::Line => Mode::VisualLine,
            SelectionMode::Block => Mode::VisualBlock,
        }
    }

    pub fn line_display_width(&self, row: usize) -> usize {
        let row = row.min(self.document.line_count().saturating_sub(1));
        display::line_display_width(self.document.rope.line(row), self.config.tab_width)
    }

    pub fn display_cells_for_line(&self, row: usize) -> Vec<display::DisplayCell> {
        let row = row.min(self.document.line_count().saturating_sub(1));
        display::line_display_cells(self.document.rope.line(row), self.config.tab_width)
    }

    pub fn display_column_for_position(&self, row: usize, col: usize) -> usize {
        let row = row.min(self.document.line_count().saturating_sub(1));
        display::display_column_for_char(self.document.rope.line(row), col, self.config.tab_width)
    }

    pub fn position_for_display_column(&self, row: usize, display_col: usize) -> Position {
        let row = row.min(self.document.line_count().saturating_sub(1));
        let col = display::char_for_display_column(
            self.document.rope.line(row),
            display_col,
            self.config.tab_width,
        );
        self.clamped_position_for_current_mode(row, col)
    }

    pub fn cursor_display_column(&self) -> usize {
        self.display_column_for_position(self.cursor.row, self.cursor.col)
    }

    pub fn set_view_size(&mut self, width: usize, height: usize) {
        self.view.width = width.min(u16::MAX as usize) as u16;
        self.view.height = height.min(u16::MAX as usize) as u16;
        self.scroll();
    }

    pub fn view_cell_for_position(&self, position: Position) -> Option<display::ViewCell> {
        let position = self.clamped_position(position.row, position.col, true);
        if self.config.wrap {
            let text_width = self.text_width();
            let screen_map = wrap::build_screen_map_with_tab_width(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
                self.config.tab_width,
            );
            for (screen_row, seg) in screen_map.iter().enumerate() {
                if seg.doc_row != position.row {
                    continue;
                }
                let line = self.document.rope.line(position.row);
                let line_len = buffer::line_display_len(line);
                let in_segment = if position.col == line_len {
                    position.col >= seg.char_start && position.col <= seg.char_end
                } else {
                    position.col >= seg.char_start && position.col < seg.char_end
                };
                if in_segment {
                    let col = display::display_column_for_char_range(
                        line,
                        seg.char_start,
                        position.col,
                        self.config.tab_width,
                    );
                    return Some(display::ViewCell {
                        row: screen_row,
                        col,
                    });
                }
            }
            return None;
        }

        let screen_row = position.row.checked_sub(self.view.offset_row)?;
        if self.view.height > 0 && screen_row >= self.view.height as usize {
            return None;
        }
        let line = self.document.rope.line(position.row);
        let offset_col =
            display::display_column_for_char(line, self.view.offset_col, self.config.tab_width);
        let position_col =
            display::display_column_for_char(line, position.col, self.config.tab_width);
        let screen_col = position_col.checked_sub(offset_col)?;
        if self.view.width > 0 && screen_col >= self.text_width() as usize {
            return None;
        }
        Some(display::ViewCell {
            row: screen_row,
            col: screen_col,
        })
    }

    pub fn cursor_view_cell(&self) -> Option<display::ViewCell> {
        self.view_cell_for_position(self.cursor)
    }

    pub fn position_for_view_cell(&self, screen_row: usize, screen_col: usize) -> Position {
        if self.config.wrap {
            let text_width = self.text_width();
            let screen_map = wrap::build_screen_map_with_tab_width(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
                self.config.tab_width,
            );
            if let Some(seg) = screen_map.get(screen_row) {
                let line = self.document.rope.line(seg.doc_row);
                let col = display::char_for_display_column_in_range(
                    line,
                    seg.char_start,
                    seg.char_end,
                    screen_col,
                    self.config.tab_width,
                );
                return self.clamped_position_for_current_mode(seg.doc_row, col);
            }
        }

        let row = self.view.offset_row.saturating_add(screen_row);
        let row = row.min(self.document.line_count().saturating_sub(1));
        let line = self.document.rope.line(row);
        let offset_col =
            display::display_column_for_char(line, self.view.offset_col, self.config.tab_width);
        let col = display::char_for_display_column(
            line,
            offset_col.saturating_add(screen_col),
            self.config.tab_width,
        );
        self.clamped_position_for_current_mode(row, col)
    }

    pub fn set_cursor_position(&mut self, row: usize, col: usize) -> Position {
        let position = self.clamped_position_for_current_mode(row, col);
        self.cursor = position;
        if self.mode.is_visual() {
            self.mode = Mode::Normal;
        }
        self.visual_anchor = None;
        self.clamp_cursor();
        self.scroll();
        self.cursor
    }

    pub fn set_cursor_for_view_cell(&mut self, screen_row: usize, screen_col: usize) -> Position {
        let position = self.position_for_view_cell(screen_row, screen_col);
        self.set_cursor_position(position.row, position.col)
    }

    pub fn set_selection(
        &mut self,
        anchor: Position,
        cursor: Position,
        selection_mode: SelectionMode,
    ) {
        self.mode = Self::mode_for_selection(selection_mode);
        self.visual_anchor = Some(self.clamped_position(anchor.row, anchor.col, false));
        self.cursor = self.clamped_position(cursor.row, cursor.col, false);
        self.scroll();
    }

    pub fn set_selection_positions(
        &mut self,
        anchor_row: usize,
        anchor_col: usize,
        cursor_row: usize,
        cursor_col: usize,
        selection_mode: SelectionMode,
    ) {
        self.set_selection(
            Position {
                row: anchor_row,
                col: anchor_col,
            },
            Position {
                row: cursor_row,
                col: cursor_col,
            },
            selection_mode,
        );
    }

    pub fn begin_selection_at(
        &mut self,
        row: usize,
        col: usize,
        selection_mode: SelectionMode,
    ) -> Position {
        let position = self.clamped_position(row, col, false);
        self.mode = Self::mode_for_selection(selection_mode);
        self.visual_anchor = Some(position);
        self.cursor = position;
        self.scroll();
        position
    }

    pub fn extend_selection_to(&mut self, row: usize, col: usize) -> Position {
        if !self.mode.is_visual() {
            self.mode = Mode::Visual;
            self.visual_anchor = Some(self.cursor);
        } else if self.visual_anchor.is_none() {
            self.visual_anchor = Some(self.cursor);
        }
        self.cursor = self.clamped_position(row, col, false);
        self.scroll();
        self.cursor
    }

    pub fn begin_selection_at_view_cell(
        &mut self,
        screen_row: usize,
        screen_col: usize,
        selection_mode: SelectionMode,
    ) -> Position {
        let position = self.position_for_view_cell(screen_row, screen_col);
        self.begin_selection_at(position.row, position.col, selection_mode)
    }

    pub fn extend_selection_to_view_cell(
        &mut self,
        screen_row: usize,
        screen_col: usize,
    ) -> Position {
        let position = self.position_for_view_cell(screen_row, screen_col);
        self.extend_selection_to(position.row, position.col)
    }

    pub fn clear_selection(&mut self) {
        self.visual_anchor = None;
        if self.mode.is_visual() {
            self.mode = Mode::Normal;
        }
    }

    pub fn select_word_at(&mut self, row: usize, col: usize) -> Option<(Position, Position)> {
        let position = self.clamped_position(row, col, false);
        let line = self.document.rope.line(position.row);
        let line_len = buffer::line_display_len(line);
        if position.col >= line_len {
            return None;
        }
        let ch = line.char(position.col);
        if !buffer::is_word_char(ch) {
            return None;
        }

        let mut start = position.col;
        while start > 0 && buffer::is_word_char(line.char(start - 1)) {
            start -= 1;
        }
        let mut end = position.col;
        while end + 1 < line_len && buffer::is_word_char(line.char(end + 1)) {
            end += 1;
        }

        let anchor = Position {
            row: position.row,
            col: start,
        };
        let cursor = Position {
            row: position.row,
            col: end,
        };
        self.set_selection(anchor, cursor, SelectionMode::Character);
        Some((anchor, cursor))
    }

    pub fn select_line_at(&mut self, row: usize) -> Option<(Position, Position)> {
        if self.document.line_count() == 0 {
            return None;
        }
        let row = row.min(self.document.line_count().saturating_sub(1));
        let anchor = Position { row, col: 0 };
        let cursor = Position { row, col: 0 };
        self.set_selection(anchor, cursor, SelectionMode::Line);
        Some((anchor, cursor))
    }

    /// Gutter width (line numbers + padding). Same logic as EditorView::gutter_width().
    pub fn gutter_width(&self) -> u16 {
        let lines = self.document.line_count();
        let digits = if lines == 0 {
            1
        } else {
            (lines as f64).log10().floor() as u16 + 1
        };
        digits + 2
    }

    /// Text area width (total width minus gutter).
    fn text_width(&self) -> u16 {
        self.view.width.saturating_sub(self.gutter_width())
    }

    /// Save a snapshot for undo before a destructive operation.
    fn save_undo(&mut self) {
        self.history.save(&self.document.rope, self.cursor);
    }

    /// Returns the selection range (start, end) if in visual mode.
    /// For VisualLine, col values span the full lines.
    pub fn selection_range(&self) -> Option<(Position, Position)> {
        let anchor = self.visual_anchor?;
        if !self.mode.is_visual() {
            return None;
        }
        let (start, end) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        if self.mode == Mode::VisualLine {
            Some((
                Position {
                    row: start.row,
                    col: 0,
                },
                Position {
                    row: end.row,
                    col: usize::MAX,
                },
            ))
        } else {
            Some((start, end))
        }
    }

    // --- Movement ---

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.config.wrap {
            self.move_down_wrapped();
        } else {
            let max_row = self.document.line_count().saturating_sub(1);
            if self.cursor.row < max_row {
                self.cursor.row += 1;
            }
            self.clamp_cursor();
        }
    }

    pub fn move_up(&mut self) {
        if self.config.wrap {
            self.move_up_wrapped();
        } else {
            if self.cursor.row > 0 {
                self.cursor.row -= 1;
            }
            self.clamp_cursor();
        }
    }

    /// Move down by one screen line in wrap mode.
    fn move_down_wrapped(&mut self) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let wc = wrap::wrap_count_with_tab_width(line, text_width, self.config.tab_width);
        let (seg, col_in_seg) = wrap::char_to_wrap_pos_with_tab_width(
            line,
            self.cursor.col,
            text_width,
            self.config.tab_width,
        );

        if seg + 1 < wc {
            // Move to next segment within same line
            self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
                line,
                seg + 1,
                col_in_seg,
                text_width,
                self.config.tab_width,
            );
        } else {
            // Move to next document line, segment 0
            let max_row = self.document.line_count().saturating_sub(1);
            if self.cursor.row < max_row {
                self.cursor.row += 1;
                let next_line = self.document.rope.line(self.cursor.row);
                self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
                    next_line,
                    0,
                    col_in_seg,
                    text_width,
                    self.config.tab_width,
                );
            }
        }
        self.clamp_cursor();
    }

    /// Move up by one screen line in wrap mode.
    fn move_up_wrapped(&mut self) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (seg, col_in_seg) = wrap::char_to_wrap_pos_with_tab_width(
            line,
            self.cursor.col,
            text_width,
            self.config.tab_width,
        );

        if seg > 0 {
            // Move to previous segment within same line
            self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
                line,
                seg - 1,
                col_in_seg,
                text_width,
                self.config.tab_width,
            );
        } else {
            // Move to last segment of previous document line
            if self.cursor.row > 0 {
                self.cursor.row -= 1;
                let prev_line = self.document.rope.line(self.cursor.row);
                let prev_wc =
                    wrap::wrap_count_with_tab_width(prev_line, text_width, self.config.tab_width);
                self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
                    prev_line,
                    prev_wc - 1,
                    col_in_seg,
                    text_width,
                    self.config.tab_width,
                );
            }
        }
        self.clamp_cursor();
    }

    /// Move down by one document line (gj), ignoring wrap.
    pub fn move_document_line_down(&mut self) {
        let max_row = self.document.line_count().saturating_sub(1);
        if self.cursor.row < max_row {
            self.cursor.row += 1;
        }
        self.clamp_cursor();
    }

    /// Move up by one document line (gk), ignoring wrap.
    pub fn move_document_line_up(&mut self) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
        }
        self.clamp_cursor();
    }

    pub fn move_right(&mut self) {
        let line_len = self.document.line_len(self.cursor.row);
        let max_col = if self.mode == Mode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
        if self.cursor.col < max_col {
            self.cursor.col += 1;
        }
    }

    pub fn move_line_start(&mut self) {
        self.cursor.col = 0;
    }

    pub fn move_line_end(&mut self) {
        let line_len = self.document.line_len(self.cursor.row);
        self.cursor.col = if self.mode == Mode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
    }

    pub fn move_word_forward(&mut self) {
        let line_count = self.document.line_count();
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        loop {
            let line = self.document.rope.line(row);
            let line_len = buffer::line_display_len(line);

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    let next_line = self.document.rope.line(row);
                    let next_len = buffer::line_display_len(next_line);
                    if next_len > 0 {
                        self.cursor.row = row;
                        self.cursor.col = 0;
                        return;
                    }
                    continue;
                } else {
                    return;
                }
            }

            let ch = line.char(col);

            if buffer::is_word_char(ch) {
                while col < line_len && buffer::is_word_char(line.char(col)) {
                    col += 1;
                }
            } else if !ch.is_whitespace() {
                while col < line_len {
                    let c = line.char(col);
                    if buffer::is_word_char(c) || c.is_whitespace() {
                        break;
                    }
                    col += 1;
                }
            }

            while col < line_len && line.char(col).is_whitespace() {
                col += 1;
            }

            if col < line_len {
                self.cursor.row = row;
                self.cursor.col = col;
                return;
            }

            if row + 1 < line_count {
                row += 1;
                col = 0;
            } else {
                self.cursor.row = row;
                self.cursor.col = line_len.saturating_sub(1);
                return;
            }
        }
    }

    pub fn move_word_backward(&mut self) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if col == 0 {
            if row == 0 {
                return;
            }
            row -= 1;
            col = buffer::line_display_len(self.document.rope.line(row));
        }

        let line = self.document.rope.line(row);
        let line_len = buffer::line_display_len(line);
        if col > line_len {
            col = line_len;
        }

        while col > 0 && line.char(col - 1).is_whitespace() {
            col -= 1;
        }

        if col == 0 {
            self.cursor.row = row;
            self.cursor.col = 0;
            return;
        }

        let ch = line.char(col - 1);
        if buffer::is_word_char(ch) {
            while col > 0 && buffer::is_word_char(line.char(col - 1)) {
                col -= 1;
            }
        } else {
            while col > 0 {
                let c = line.char(col - 1);
                if buffer::is_word_char(c) || c.is_whitespace() {
                    break;
                }
                col -= 1;
            }
        }

        self.cursor.row = row;
        self.cursor.col = col;
    }

    pub fn move_word_end(&mut self) {
        let line_count = self.document.line_count();
        let mut row = self.cursor.row;
        let mut col = self.cursor.col + 1;

        loop {
            let line = self.document.rope.line(row);
            let line_len = buffer::line_display_len(line);

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    continue;
                } else {
                    return;
                }
            }

            while col < line_len && line.char(col).is_whitespace() {
                col += 1;
            }

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    continue;
                } else {
                    return;
                }
            }

            let ch = line.char(col);
            if buffer::is_word_char(ch) {
                while col + 1 < line_len && buffer::is_word_char(line.char(col + 1)) {
                    col += 1;
                }
            } else {
                while col + 1 < line_len {
                    let c = line.char(col + 1);
                    if buffer::is_word_char(c) || c.is_whitespace() {
                        break;
                    }
                    col += 1;
                }
            }

            self.cursor.row = row;
            self.cursor.col = col;
            return;
        }
    }

    // --- ^ (first non-blank) ---

    pub fn move_first_non_blank(&mut self) {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        let mut col = 0;
        while col < line_len {
            let ch = line.char(col);
            if !ch.is_whitespace() || ch == '\n' {
                break;
            }
            col += 1;
        }
        self.cursor.col = col;
    }

    // --- I (insert at first non-blank) ---

    pub fn enter_insert_mode_first_non_blank(&mut self) {
        self.save_undo();
        self.move_first_non_blank();
        self.mode = Mode::Insert;
    }

    // --- W/B/E (WORD motions, whitespace-delimited) ---

    pub fn move_word_forward_big(&mut self) {
        let line_count = self.document.line_count();
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        loop {
            let line = self.document.rope.line(row);
            let line_len = buffer::line_display_len(line);

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    let next_line = self.document.rope.line(row);
                    let next_len = buffer::line_display_len(next_line);
                    if next_len > 0 && !next_line.char(0).is_whitespace() {
                        self.cursor.row = row;
                        self.cursor.col = 0;
                        return;
                    }
                    continue;
                } else {
                    return;
                }
            }

            // Skip non-whitespace
            if !line.char(col).is_whitespace() {
                while col < line_len && !line.char(col).is_whitespace() {
                    col += 1;
                }
            }

            // Skip whitespace
            while col < line_len && line.char(col).is_whitespace() && line.char(col) != '\n' {
                col += 1;
            }

            if col < line_len && line.char(col) != '\n' {
                self.cursor.row = row;
                self.cursor.col = col;
                return;
            }

            if row + 1 < line_count {
                row += 1;
                col = 0;
            } else {
                self.cursor.row = row;
                self.cursor.col = line_len.saturating_sub(1);
                return;
            }
        }
    }

    pub fn move_word_backward_big(&mut self) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if col == 0 {
            if row == 0 {
                return;
            }
            row -= 1;
            col = buffer::line_display_len(self.document.rope.line(row));
        }

        let line = self.document.rope.line(row);
        let line_len = buffer::line_display_len(line);
        if col > line_len {
            col = line_len;
        }

        // Skip whitespace backward
        while col > 0 && line.char(col - 1).is_whitespace() {
            col -= 1;
        }

        if col == 0 {
            self.cursor.row = row;
            self.cursor.col = 0;
            return;
        }

        // Skip non-whitespace backward
        while col > 0 && !line.char(col - 1).is_whitespace() {
            col -= 1;
        }

        self.cursor.row = row;
        self.cursor.col = col;
    }

    pub fn move_word_end_big(&mut self) {
        let line_count = self.document.line_count();
        let mut row = self.cursor.row;
        let mut col = self.cursor.col + 1;

        loop {
            let line = self.document.rope.line(row);
            let line_len = buffer::line_display_len(line);

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    continue;
                } else {
                    return;
                }
            }

            // Skip whitespace
            while col < line_len && line.char(col).is_whitespace() {
                col += 1;
            }

            if col >= line_len {
                if row + 1 < line_count {
                    row += 1;
                    col = 0;
                    continue;
                } else {
                    return;
                }
            }

            // Move to end of non-whitespace
            while col + 1 < line_len && !line.char(col + 1).is_whitespace() {
                col += 1;
            }

            self.cursor.row = row;
            self.cursor.col = col;
            return;
        }
    }

    pub fn move_word_end_backward(&mut self) {
        let mut idx = self
            .document
            .rope
            .line_to_char(self.cursor.row)
            .saturating_add(self.cursor.col)
            .saturating_sub(1);
        while idx > 0 && self.document.rope.char(idx).is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !buffer::is_word_char(self.document.rope.char(idx)) {
            idx -= 1;
        }
        while idx > 0 && buffer::is_word_char(self.document.rope.char(idx - 1)) {
            idx -= 1;
        }
        while idx + 1 < self.document.rope.len_chars()
            && buffer::is_word_char(self.document.rope.char(idx + 1))
        {
            idx += 1;
        }
        self.reposition_cursor_to(idx);
        self.clamp_cursor();
    }

    pub fn move_word_end_backward_big(&mut self) {
        let mut idx = self
            .document
            .rope
            .line_to_char(self.cursor.row)
            .saturating_add(self.cursor.col)
            .saturating_sub(1);
        while idx > 0 && self.document.rope.char(idx).is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !self.document.rope.char(idx - 1).is_whitespace() {
            idx -= 1;
        }
        while idx + 1 < self.document.rope.len_chars()
            && !self.document.rope.char(idx + 1).is_whitespace()
        {
            idx += 1;
        }
        self.reposition_cursor_to(idx);
        self.clamp_cursor();
    }

    // --- {/} (paragraph motions) ---

    fn is_blank_line(&self, row: usize) -> bool {
        let line = self.document.rope.line(row);
        let line_len = buffer::line_display_len(line);
        if line_len == 0 {
            return true;
        }
        for i in 0..line_len {
            let ch = line.char(i);
            if !ch.is_whitespace() {
                return false;
            }
        }
        true
    }

    pub fn move_paragraph_forward(&mut self) {
        let line_count = self.document.line_count();
        let mut row = self.cursor.row;

        // Skip current non-blank lines
        while row < line_count && !self.is_blank_line(row) {
            row += 1;
        }
        // Skip blank lines
        while row < line_count && self.is_blank_line(row) {
            row += 1;
        }

        if row >= line_count {
            row = line_count.saturating_sub(1);
        }
        self.cursor.row = row;
        self.cursor.col = 0;
        self.clamp_cursor();
    }

    pub fn move_paragraph_backward(&mut self) {
        let mut row = self.cursor.row;
        if row == 0 {
            return;
        }
        row -= 1;

        // Skip current blank lines
        while row > 0 && self.is_blank_line(row) {
            row -= 1;
        }
        // Skip non-blank lines
        while row > 0 && !self.is_blank_line(row) {
            row -= 1;
        }

        self.cursor.row = row;
        self.cursor.col = 0;
        self.clamp_cursor();
    }

    pub fn move_sentence_forward(&mut self) {
        let start = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col + 1;
        let len = self.document.rope.len_chars();
        for idx in start..len {
            let ch = self.document.rope.char(idx);
            if matches!(ch, '.' | '!' | '?') {
                self.reposition_cursor_to((idx + 1).min(len.saturating_sub(1)));
                self.move_first_non_blank();
                return;
            }
        }
        self.goto_bottom();
    }

    pub fn move_sentence_backward(&mut self) {
        let cur = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        for idx in (0..cur.saturating_sub(1)).rev() {
            let ch = self.document.rope.char(idx);
            if matches!(ch, '.' | '!' | '?') {
                self.reposition_cursor_to((idx + 1).min(self.document.rope.len_chars()));
                self.move_first_non_blank();
                return;
            }
        }
        self.goto_top();
    }

    pub fn move_section_forward(&mut self) {
        let line_count = self.document.line_count();
        for row in (self.cursor.row + 1)..line_count {
            let line = self.document.rope.line(row);
            if buffer::line_display_len(line) > 0 && matches!(line.char(0), '{' | '}' | '\x0c') {
                self.cursor.row = row;
                self.cursor.col = 0;
                return;
            }
        }
        self.goto_bottom();
    }

    pub fn move_section_backward(&mut self) {
        for row in (0..self.cursor.row).rev() {
            let line = self.document.rope.line(row);
            if buffer::line_display_len(line) > 0 && matches!(line.char(0), '{' | '}' | '\x0c') {
                self.cursor.row = row;
                self.cursor.col = 0;
                return;
            }
        }
        self.goto_top();
    }

    pub fn move_column(&mut self, count: usize) {
        self.cursor.col = count.saturating_sub(1);
        self.clamp_cursor();
    }

    pub fn move_line_down_first_non_blank(&mut self) {
        self.move_down();
        self.move_first_non_blank();
    }

    pub fn move_line_up_first_non_blank(&mut self) {
        self.move_up();
        self.move_first_non_blank();
    }

    // --- Visual mode swap anchor ---

    pub fn visual_swap_anchor(&mut self) {
        if let Some(ref mut anchor) = self.visual_anchor {
            std::mem::swap(anchor, &mut self.cursor);
        }
    }

    // --- Editing ---

    pub fn insert_char(&mut self, ch: char) {
        // Auto-closing pairs: skip over closing char if it's already there
        if matches!(ch, ')' | '}' | ']' | '"' | '\'') {
            let line = self.document.rope.line(self.cursor.row);
            let line_len = buffer::line_display_len(line);
            if self.cursor.col < line_len && line.char(self.cursor.col) == ch {
                self.cursor.col += 1;
                return;
            }
        }

        self.document.insert_char(self.cursor, ch);
        self.cursor.col += 1;
        if self.visual_block_edit.is_some() {
            self.visual_block_insert_text.push(ch);
        }

        // Auto-closing pairs: insert matching closing char
        let closing = match ch {
            '{' => Some('}'),
            '(' => Some(')'),
            '[' => Some(']'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        if let Some(close) = closing {
            self.document.insert_char(self.cursor, close);
            // cursor stays between the pair
        }
    }

    pub fn insert_tab(&mut self) {
        let spaces = "    ";
        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        self.document.rope.insert(idx, spaces);
        self.document.modified = true;
        self.document.bump_version();
        self.cursor.col += 4;
        if self.visual_block_edit.is_some() {
            self.visual_block_insert_text.push_str(spaces);
        }
    }

    pub fn insert_newline(&mut self) {
        self.visual_block_edit = None;
        self.visual_block_insert_text.clear();
        // Get current line's leading whitespace
        let line: String = self.document.rope.line(self.cursor.row).to_string();
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        // Check if char before cursor is an opening brace
        let add_indent = if self.cursor.col > 0 {
            let line_slice = self.document.rope.line(self.cursor.row);
            let prev_ch = line_slice.char(self.cursor.col - 1);
            matches!(prev_ch, '{' | '(' | '[')
        } else {
            false
        };

        // Check if char after cursor is the matching closing brace
        let split_braces = if add_indent {
            let line_slice = self.document.rope.line(self.cursor.row);
            let line_len = buffer::line_display_len(line_slice);
            if self.cursor.col < line_len {
                let next_ch = line_slice.char(self.cursor.col);
                matches!(next_ch, '}' | ')' | ']')
            } else {
                false
            }
        } else {
            false
        };

        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;

        if split_braces {
            // {|} → {\n    |\n}
            let insert_text = format!("\n{}    \n{}", indent, indent);
            self.document.rope.insert(idx, &insert_text);
            self.document.modified = true;
            self.document.bump_version();
            self.cursor.row += 1;
            self.cursor.col = indent.len() + 4;
        } else if add_indent {
            let insert_text = format!("\n{}    ", indent);
            self.document.rope.insert(idx, &insert_text);
            self.document.modified = true;
            self.document.bump_version();
            self.cursor.row += 1;
            self.cursor.col = indent.len() + 4;
        } else {
            let insert_text = format!("\n{}", indent);
            self.document.rope.insert(idx, &insert_text);
            self.document.modified = true;
            self.document.bump_version();
            self.cursor.row += 1;
            self.cursor.col = indent.len();
        }
    }

    pub fn indent_line(&mut self) {
        self.save_undo();
        let idx = self.document.rope.line_to_char(self.cursor.row);
        self.document.rope.insert(idx, "    ");
        self.document.modified = true;
        self.document.bump_version();
        self.cursor.col += 4;
    }

    pub fn dedent_line(&mut self) {
        self.save_undo();
        let line = self.document.rope.line(self.cursor.row);
        let spaces: usize = line.chars().take(4).take_while(|c| *c == ' ').count();
        if spaces > 0 {
            let idx = self.document.rope.line_to_char(self.cursor.row);
            self.document.rope.remove(idx..idx + spaces);
            self.document.modified = true;
            self.document.bump_version();
            self.cursor.col = self.cursor.col.saturating_sub(spaces);
        }
    }

    pub fn delete_char_backward(&mut self) {
        if let Some(new_pos) = self.document.delete_char_backward(self.cursor) {
            self.cursor = new_pos;
            if self.visual_block_edit.is_some() {
                self.visual_block_insert_text.pop();
            }
        }
    }

    pub fn delete_char_backward_normal(&mut self) {
        if self.cursor.col == 0 {
            return;
        }
        self.save_undo();
        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col - 1;
        let ch = self.document.rope.char(idx);
        self.store_delete_register(ch.to_string(), false);
        self.document.rope.remove(idx..idx + 1);
        self.document.modified = true;
        self.document.bump_version();
        self.cursor.col -= 1;
        self.clamp_cursor();
    }

    pub fn delete_word_backward(&mut self) {
        if self.cursor.col == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let mut start = self.cursor.col;

        while start > 0 && line.char(start - 1).is_whitespace() && line.char(start - 1) != '\n' {
            start -= 1;
        }
        if start > 0 {
            let ch = line.char(start - 1);
            if buffer::is_word_char(ch) {
                while start > 0 && buffer::is_word_char(line.char(start - 1)) {
                    start -= 1;
                }
            } else {
                while start > 0 {
                    let c = line.char(start - 1);
                    if c.is_whitespace() || buffer::is_word_char(c) {
                        break;
                    }
                    start -= 1;
                }
            }
        }

        self.delete_current_line_range(start, self.cursor.col);
        self.cursor.col = start;
    }

    pub fn delete_line_backward(&mut self) {
        if self.cursor.col == 0 {
            return;
        }
        self.delete_current_line_range(0, self.cursor.col);
        self.cursor.col = 0;
    }

    fn delete_current_line_range(&mut self, start_col: usize, end_col: usize) {
        if start_col >= end_col {
            return;
        }
        let line_start = self.document.rope.line_to_char(self.cursor.row);
        self.document
            .rope
            .remove(line_start + start_col..line_start + end_col);
        self.document.modified = true;
        self.document.bump_version();
    }

    fn adjust_marks_after_line_delete(&mut self, start_row: usize, end_row: usize) {
        let deleted = end_row.saturating_sub(start_row);
        let max_row = self.document.line_count().saturating_sub(1);
        for mark in self.marks.values_mut() {
            if mark.row >= end_row {
                mark.row = mark.row.saturating_sub(deleted);
            } else if mark.row >= start_row {
                mark.row = start_row.min(max_row);
            }
            mark.row = mark.row.min(max_row);
            let line_len = self.document.line_len(mark.row);
            mark.col = mark.col.min(line_len.saturating_sub(1));
        }
    }

    pub fn delete_char_forward(&mut self) {
        self.save_undo();
        let line_len = self.document.line_len(self.cursor.row);
        // Yank the deleted char into register
        if self.cursor.col < line_len {
            let line = self.document.rope.line(self.cursor.row);
            let ch = line.char(self.cursor.col);
            self.store_delete_register(ch.to_string(), false);
        }
        self.document.delete_char_forward(self.cursor);
        self.clamp_cursor();
    }

    pub fn substitute_char(&mut self) {
        let line_len = self.document.line_len(self.cursor.row);
        if self.cursor.col < line_len {
            self.save_undo();
            let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
            let ch = self.document.rope.char(idx);
            self.store_delete_register(ch.to_string(), false);
            self.document.rope.remove(idx..idx + 1);
            self.document.modified = true;
            self.document.bump_version();
        }
        self.mode = Mode::Insert;
        self.clamp_cursor();
    }

    pub fn delete_line(&mut self) {
        self.save_undo();
        let line_text: String = self.document.rope.line(self.cursor.row).to_string();
        self.store_delete_register(line_text, true);
        self.document.delete_line(self.cursor.row);
        self.adjust_marks_after_line_delete(self.cursor.row, self.cursor.row + 1);
        self.clamp_cursor();
    }

    pub fn delete_lines(&mut self, count: usize) {
        self.save_undo();
        let start_row = self
            .cursor
            .row
            .min(self.document.line_count().saturating_sub(1));
        let end_row = (start_row + count.max(1)).min(self.document.line_count());
        let start = self.document.rope.line_to_char(start_row);
        let end = if end_row < self.document.line_count() {
            self.document.rope.line_to_char(end_row)
        } else {
            self.document.rope.len_chars()
        };
        if start < end {
            let text = self.document.rope.slice(start..end).to_string();
            self.store_delete_register(text, true);
            self.document.rope.remove(start..end);
            if self.document.rope.len_chars() == 0 {
                self.document.rope.insert(0, "\n");
            }
            self.document.modified = true;
            self.document.bump_version();
        }
        self.adjust_marks_after_line_delete(start_row, end_row);
        self.cursor.row = start_row.min(self.document.line_count().saturating_sub(1));
        self.cursor.col = 0;
        self.clamp_cursor();
    }

    pub fn yank_line(&mut self) {
        let line_text: String = self.document.rope.line(self.cursor.row).to_string();
        self.store_yank_register(line_text, true);
        self.status_message = Some("1 line yanked".to_string());
    }

    pub fn yank_lines(&mut self, count: usize) {
        let start_row = self
            .cursor
            .row
            .min(self.document.line_count().saturating_sub(1));
        let end_row = (start_row + count.max(1)).min(self.document.line_count());
        let start = self.document.rope.line_to_char(start_row);
        let end = if end_row < self.document.line_count() {
            self.document.rope.line_to_char(end_row)
        } else {
            self.document.rope.len_chars()
        };
        if start < end {
            let text = self.document.rope.slice(start..end).to_string();
            self.store_yank_register(text, true);
            self.status_message = Some(format!("{} line(s) yanked", count.max(1)));
        }
    }

    pub fn insert_newline_below(&mut self) {
        self.save_undo();
        // Get current line's indent and check for trailing brace
        let line: String = self.document.rope.line(self.cursor.row).to_string();
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let trimmed_end = line.trim_end_matches(['\n', '\r']);
        let extra = if trimmed_end.ends_with('{')
            || trimmed_end.ends_with('(')
            || trimmed_end.ends_with('[')
        {
            "    "
        } else {
            ""
        };

        let line_len = self.document.line_len(self.cursor.row);
        let idx = self.document.rope.line_to_char(self.cursor.row) + line_len;
        let insert_text = format!("\n{}{}", indent, extra);
        self.document.rope.insert(idx, &insert_text);
        self.document.modified = true;
        self.document.bump_version();
        self.cursor.row += 1;
        self.cursor.col = indent.len() + extra.len();
        self.mode = Mode::Insert;
    }

    pub fn insert_newline_above(&mut self) {
        self.save_undo();
        // Use the current line's indent for the new line above
        let line: String = self.document.rope.line(self.cursor.row).to_string();
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        let idx = self.document.rope.line_to_char(self.cursor.row);
        let insert_text = format!("{}\n", indent);
        self.document.rope.insert(idx, &insert_text);
        self.document.modified = true;
        self.document.bump_version();
        self.cursor.col = indent.len();
        self.mode = Mode::Insert;
    }

    // --- Undo/Redo ---

    pub fn undo(&mut self) {
        if let Some((rope, cursor)) = self.history.undo(&self.document.rope, self.cursor) {
            self.document.rope = rope;
            self.document.modified = true;
            self.cursor = cursor;
            self.clamp_cursor();
        } else {
            self.status_message = Some("Already at oldest change".to_string());
        }
    }

    pub fn redo(&mut self) {
        if let Some((rope, cursor)) = self.history.redo(&self.document.rope, self.cursor) {
            self.document.rope = rope;
            self.document.modified = true;
            self.cursor = cursor;
            self.clamp_cursor();
        } else {
            self.status_message = Some("Already at newest change".to_string());
        }
    }

    // --- Visual mode ---

    pub fn enter_visual_mode(&mut self) {
        self.mode = Mode::Visual;
        self.visual_anchor = Some(self.cursor);
    }

    pub fn enter_visual_line_mode(&mut self) {
        self.mode = Mode::VisualLine;
        self.visual_anchor = Some(self.cursor);
    }

    pub fn enter_visual_block_mode(&mut self) {
        if self.visual_anchor.is_none() {
            self.visual_anchor = Some(self.cursor);
        }
        self.mode = Mode::VisualBlock;
    }

    pub fn restore_visual_selection(&mut self) {
        if let Some(anchor) = self.visual_anchor {
            self.mode = Mode::Visual;
            self.cursor = anchor;
        }
    }

    pub fn visual_swap_block_corner(&mut self) {
        self.visual_swap_anchor();
    }

    fn visual_block_range(&self) -> Option<(usize, usize, usize, usize)> {
        let anchor = self.visual_anchor?;
        if self.mode != Mode::VisualBlock {
            return None;
        }
        Some((
            anchor.row.min(self.cursor.row),
            anchor.row.max(self.cursor.row),
            anchor.col.min(self.cursor.col),
            anchor.col.max(self.cursor.col),
        ))
    }

    fn visual_block_text(
        &self,
        start_row: usize,
        end_row: usize,
        start_col: usize,
        end_col: usize,
    ) -> String {
        let mut lines = Vec::new();
        for row in start_row..=end_row.min(self.document.line_count().saturating_sub(1)) {
            let line_len = self.document.line_len(row);
            if start_col >= line_len {
                lines.push(String::new());
                continue;
            }
            let line_start = self.document.rope.line_to_char(row);
            let end = (end_col + 1).min(line_len);
            lines.push(
                self.document
                    .rope
                    .slice(line_start + start_col..line_start + end)
                    .to_string(),
            );
        }
        lines.join("\n")
    }

    fn delete_visual_block_range(
        &mut self,
        start_row: usize,
        end_row: usize,
        start_col: usize,
        end_col: usize,
    ) {
        for row in (start_row..=end_row.min(self.document.line_count().saturating_sub(1))).rev() {
            let line_len = self.document.line_len(row);
            if start_col >= line_len {
                continue;
            }
            let line_start = self.document.rope.line_to_char(row);
            let end = (end_col + 1).min(line_len);
            self.document
                .rope
                .remove(line_start + start_col..line_start + end);
        }
        self.document.modified = true;
        self.document.bump_version();
    }

    pub fn visual_delete(&mut self) {
        if let Some((start_row, end_row, start_col, end_col)) = self.visual_block_range() {
            self.save_undo();
            let text = self.visual_block_text(start_row, end_row, start_col, end_col);
            self.store_delete_register(text, false);
            self.delete_visual_block_range(start_row, end_row, start_col, end_col);
            self.cursor = Position {
                row: start_row,
                col: start_col,
            };
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
            return;
        }
        if let Some((start, end)) = self.selection_range() {
            self.save_undo();
            let linewise = self.mode == Mode::VisualLine;

            let (start_idx, end_idx) = if linewise {
                let s = self.document.rope.line_to_char(start.row);
                let e = if end.row + 1 < self.document.line_count() {
                    self.document.rope.line_to_char(end.row + 1)
                } else {
                    self.document.rope.len_chars()
                };
                (s, e)
            } else {
                let s = self.document.rope.line_to_char(start.row) + start.col;
                let e_col = end.col.min(self.document.line_len(end.row));
                let e = self.document.rope.line_to_char(end.row) + e_col + 1;
                let e = e.min(self.document.rope.len_chars());
                (s, e)
            };

            if start_idx < end_idx {
                let text: String = self.document.rope.slice(start_idx..end_idx).to_string();
                self.store_delete_register(text, linewise);
                self.document.rope.remove(start_idx..end_idx);
                self.document.modified = true;
            }

            self.cursor = start;
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
        }
    }

    pub fn visual_yank(&mut self) {
        if let Some((start_row, end_row, start_col, end_col)) = self.visual_block_range() {
            let text = self.visual_block_text(start_row, end_row, start_col, end_col);
            self.store_yank_register(text, false);
            self.status_message = Some("block yanked".to_string());
            self.cursor = Position {
                row: start_row,
                col: start_col,
            };
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
            return;
        }
        if let Some((start, end)) = self.selection_range() {
            let linewise = self.mode == Mode::VisualLine;

            let (start_idx, end_idx) = if linewise {
                let s = self.document.rope.line_to_char(start.row);
                let e = if end.row + 1 < self.document.line_count() {
                    self.document.rope.line_to_char(end.row + 1)
                } else {
                    self.document.rope.len_chars()
                };
                (s, e)
            } else {
                let s = self.document.rope.line_to_char(start.row) + start.col;
                let e_col = end.col.min(self.document.line_len(end.row));
                let e = self.document.rope.line_to_char(end.row) + e_col + 1;
                let e = e.min(self.document.rope.len_chars());
                (s, e)
            };

            if start_idx < end_idx {
                let text: String = self.document.rope.slice(start_idx..end_idx).to_string();
                let line_count = if linewise { end.row - start.row + 1 } else { 0 };
                self.store_yank_register(text, linewise);
                if linewise {
                    self.status_message = Some(format!(
                        "{line_count} line{} yanked",
                        if line_count > 1 { "s" } else { "" }
                    ));
                }
            }

            self.cursor = start;
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
        }
    }

    pub fn visual_change(&mut self) {
        // Delete selection then enter insert mode
        if self.mode == Mode::VisualBlock {
            if self.visual_block_range().is_some() {
                self.visual_delete();
                self.mode = Mode::Insert;
            }
        } else if self.selection_range().is_some() {
            let was_linewise = self.mode == Mode::VisualLine;
            self.visual_delete();
            if was_linewise {
                // Insert a blank line and enter insert mode on it
                let pos = Position {
                    row: self.cursor.row,
                    col: 0,
                };
                self.document.insert_newline(pos);
                self.cursor.col = 0;
            }
            self.mode = Mode::Insert;
        }
    }

    pub fn visual_indent(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.save_undo();
            for row in start.row..=end.row.min(self.document.line_count().saturating_sub(1)) {
                let idx = self.document.rope.line_to_char(row);
                self.document.rope.insert(idx, "    ");
            }
            self.document.modified = true;
            self.document.bump_version();
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.cursor = start;
            self.cursor.col += 4;
        }
    }

    pub fn visual_dedent(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.save_undo();
            for row in start.row..=end.row.min(self.document.line_count().saturating_sub(1)) {
                let line = self.document.rope.line(row);
                let spaces: usize = line.chars().take(4).take_while(|c| *c == ' ').count();
                if spaces > 0 {
                    let idx = self.document.rope.line_to_char(row);
                    self.document.rope.remove(idx..idx + spaces);
                }
            }
            self.document.modified = true;
            self.document.bump_version();
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.cursor = start;
            self.clamp_cursor();
        }
    }

    fn position_for_char(&self, char_idx: usize) -> Position {
        let idx = char_idx.min(self.document.rope.len_chars().saturating_sub(1));
        let row = self.document.rope.char_to_line(idx);
        let col = idx - self.document.rope.line_to_char(row);
        Position { row, col }
    }

    pub fn visual_select_motion(&mut self, motion: &Motion) {
        if let Some((start, end)) = self.motion_range(motion) {
            if start >= end {
                return;
            }
            self.mode = Mode::Visual;
            self.visual_anchor = Some(self.position_for_char(start));
            self.cursor = self.position_for_char(end.saturating_sub(1));
            self.clamp_cursor();
        }
    }

    pub fn visual_block_insert(&mut self) {
        if let Some((start_row, end_row, start_col, _)) = self.visual_block_range() {
            self.save_undo();
            self.visual_block_edit = Some(VisualBlockEdit {
                start_row,
                end_row,
                col: start_col,
            });
            self.visual_block_insert_text.clear();
            self.cursor = Position {
                row: start_row,
                col: start_col.min(self.document.line_len(start_row)),
            };
            self.visual_anchor = None;
            self.mode = Mode::Insert;
        }
    }

    pub fn visual_block_append(&mut self) {
        if let Some((start_row, end_row, _, end_col)) = self.visual_block_range() {
            self.save_undo();
            let col = end_col.saturating_add(1);
            self.visual_block_edit = Some(VisualBlockEdit {
                start_row,
                end_row,
                col,
            });
            self.visual_block_insert_text.clear();
            self.cursor = Position {
                row: start_row,
                col: col.min(self.document.line_len(start_row)),
            };
            self.visual_anchor = None;
            self.mode = Mode::Insert;
        }
    }

    fn finish_visual_block_edit(&mut self) {
        let Some(edit) = self.visual_block_edit.take() else {
            return;
        };
        if self.visual_block_insert_text.is_empty()
            || self.visual_block_insert_text.contains('\n')
            || edit.start_row >= edit.end_row
        {
            self.visual_block_insert_text.clear();
            return;
        }
        let text = std::mem::take(&mut self.visual_block_insert_text);
        for row in (edit.start_row + 1)
            ..=edit
                .end_row
                .min(self.document.line_count().saturating_sub(1))
        {
            let col = edit.col.min(self.document.line_len(row));
            let idx = self.document.rope.line_to_char(row) + col;
            self.document.rope.insert(idx, &text);
        }
        self.document.modified = true;
        self.document.bump_version();
    }

    // --- Bracket matching ---

    pub fn matching_bracket(&self) -> Option<Position> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if self.cursor.col >= line_len {
            return None;
        }
        let ch = line.char(self.cursor.col);
        let (target, forward) = match ch {
            '(' => (')', true),
            '{' => ('}', true),
            '[' => (']', true),
            ')' => ('(', false),
            '}' => ('{', false),
            ']' => ('[', false),
            _ => return None,
        };

        if forward {
            self.find_matching_forward(ch, target)
        } else {
            self.find_matching_backward(ch, target)
        }
    }

    fn find_matching_forward(&self, open: char, close: char) -> Option<Position> {
        let mut depth = 0i32;
        let line_count = self.document.line_count();
        for row in self.cursor.row..line_count {
            let line = self.document.rope.line(row);
            let start_col = if row == self.cursor.row {
                self.cursor.col
            } else {
                0
            };
            let line_len = buffer::line_display_len(line);
            for col in start_col..line_len {
                let c = line.char(col);
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row, col });
                    }
                }
            }
        }
        None
    }

    fn find_matching_backward(&self, close: char, open: char) -> Option<Position> {
        let mut depth = 0i32;
        for row in (0..=self.cursor.row).rev() {
            let line = self.document.rope.line(row);
            let line_len = buffer::line_display_len(line);
            let end_col = if row == self.cursor.row {
                self.cursor.col
            } else {
                line_len.saturating_sub(1)
            };
            for col in (0..=end_col).rev() {
                if col >= line_len {
                    continue;
                }
                let c = line.char(col);
                if c == close {
                    depth += 1;
                } else if c == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row, col });
                    }
                }
            }
        }
        None
    }

    // --- Operator + motion ---

    fn delete_range_internal(&mut self, start: usize, end: usize, linewise: bool) {
        let end = end.min(self.document.rope.len_chars());
        if start < end {
            let text: String = self.document.rope.slice(start..end).to_string();
            self.store_delete_register(text, linewise);
            self.document.rope.remove(start..end);
            self.document.modified = true;
            self.document.bump_version();
        }
    }

    fn reposition_cursor_to(&mut self, char_idx: usize) {
        let idx = char_idx.min(self.document.rope.len_chars().saturating_sub(1));
        let line = self.document.rope.char_to_line(idx);
        let col = idx - self.document.rope.line_to_char(line);
        self.cursor.row = line;
        self.cursor.col = col;
    }

    pub fn delete_motion(&mut self, motion: &Motion) {
        if matches!(motion, Motion::Line) {
            self.delete_line();
            return;
        }
        self.save_undo();
        if let Some((start, end)) = self.motion_range(motion) {
            self.delete_range_internal(start, end, false);
            self.reposition_cursor_to(start);
        }
        self.clamp_cursor();
    }

    pub fn delete_motion_count(&mut self, motion: &Motion, count: usize) {
        if matches!(motion, Motion::Line) {
            self.delete_lines(count);
            return;
        }
        self.save_undo();
        if let Some((start, end)) = self.motion_range_count(motion, count) {
            self.delete_range_internal(start, end, false);
            self.reposition_cursor_to(start);
        }
        self.clamp_cursor();
    }

    pub fn change_motion(&mut self, motion: &Motion) {
        if matches!(motion, Motion::Line) {
            // cc: clear line content, preserve indent, enter insert mode
            self.save_undo();
            let line: String = self.document.rope.line(self.cursor.row).to_string();
            let indent: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            let line_start = self.document.rope.line_to_char(self.cursor.row);
            let line_end_idx =
                line_start + buffer::line_display_len(self.document.rope.line(self.cursor.row));
            if line_start < line_end_idx {
                self.delete_range_internal(line_start, line_end_idx, false);
                self.document.rope.insert(line_start, &indent);
            }
            self.cursor.col = indent.len();
            self.mode = Mode::Insert;
            return;
        }
        self.save_undo();
        if let Some((start, end)) = self.motion_range(motion) {
            self.delete_range_internal(start, end, false);
            self.reposition_cursor_to(start);
        }
        self.mode = Mode::Insert;
        self.clamp_cursor();
    }

    pub fn change_motion_count(&mut self, motion: &Motion, count: usize) {
        if matches!(motion, Motion::Line) {
            if count <= 1 {
                self.change_motion(motion);
                return;
            }
            self.delete_lines(count);
            self.mode = Mode::Insert;
            return;
        }
        self.save_undo();
        if let Some((start, end)) = self.motion_range_count(motion, count) {
            self.delete_range_internal(start, end, false);
            self.reposition_cursor_to(start);
        }
        self.mode = Mode::Insert;
        self.clamp_cursor();
    }

    pub fn yank_motion(&mut self, motion: &Motion) {
        if matches!(motion, Motion::Line) {
            self.yank_line();
            return;
        }
        if let Some((start, end)) = self.motion_range(motion) {
            let end = end.min(self.document.rope.len_chars());
            if start < end {
                let text: String = self.document.rope.slice(start..end).to_string();
                self.store_yank_register(text, false);
                self.status_message = Some("yanked".to_string());
            }
        }
    }

    pub fn yank_motion_count(&mut self, motion: &Motion, count: usize) {
        if matches!(motion, Motion::Line) {
            self.yank_lines(count);
            return;
        }
        if let Some((start, end)) = self.motion_range_count(motion, count) {
            let end = end.min(self.document.rope.len_chars());
            if start < end {
                let text: String = self.document.rope.slice(start..end).to_string();
                self.store_yank_register(text, false);
                self.status_message = Some("yanked".to_string());
            }
        }
    }

    pub fn indent_motion_count(&mut self, motion: &Motion, count: usize) {
        if let Some((start, end)) = self.motion_range_count(motion, count) {
            let start_row = self.document.rope.char_to_line(start);
            let end_row = self
                .document
                .rope
                .char_to_line(end.min(self.document.rope.len_chars().saturating_sub(1)));
            self.save_undo();
            for row in start_row..=end_row {
                let idx = self.document.rope.line_to_char(row);
                self.document.rope.insert(idx, "    ");
            }
            self.document.modified = true;
            self.document.bump_version();
        }
    }

    pub fn dedent_motion_count(&mut self, motion: &Motion, count: usize) {
        if let Some((start, end)) = self.motion_range_count(motion, count) {
            let start_row = self.document.rope.char_to_line(start);
            let end_row = self
                .document
                .rope
                .char_to_line(end.min(self.document.rope.len_chars().saturating_sub(1)));
            self.save_undo();
            for row in start_row..=end_row {
                let line = self.document.rope.line(row);
                let spaces = line.chars().take(4).take_while(|c| *c == ' ').count();
                if spaces > 0 {
                    let idx = self.document.rope.line_to_char(row);
                    self.document.rope.remove(idx..idx + spaces);
                }
            }
            self.document.modified = true;
            self.document.bump_version();
        }
    }

    pub fn format_motion_count(&mut self, motion: &Motion, count: usize) {
        let _ = self.motion_range_count(motion, count);
        self.status_message = Some("format operator is not implemented".to_string());
    }

    pub fn filter_motion_count(&mut self, motion: &Motion, count: usize) {
        let _ = self.motion_range_count(motion, count);
        self.status_message = Some("filter operator is not implemented".to_string());
    }

    fn motion_range_count(&mut self, motion: &Motion, count: usize) -> Option<(usize, usize)> {
        if matches!(motion, Motion::Inner(_) | Motion::Around(_)) {
            return self.motion_range(motion);
        }
        let saved = self.cursor;
        let start_idx = self.document.rope.line_to_char(saved.row) + saved.col;
        for _ in 0..count.max(1) {
            self.apply_motion_for_range(motion)?;
        }
        let end = self.cursor;
        self.cursor = saved;
        let end_idx = self.document.rope.line_to_char(end.row) + end.col;
        if end_idx >= start_idx {
            let inclusive = matches!(
                motion,
                Motion::WordEnd
                    | Motion::WordEndBackward
                    | Motion::WORDEnd
                    | Motion::WORDEndBackward
                    | Motion::FindForward(_)
                    | Motion::MatchBracket
            );
            let end = if inclusive {
                end_idx
                    .saturating_add(1)
                    .min(self.document.rope.len_chars())
            } else if matches!(motion, Motion::DocumentEnd) {
                self.document.rope.len_chars()
            } else {
                end_idx
            };
            if end > start_idx {
                Some((start_idx, end))
            } else {
                None
            }
        } else {
            Some((end_idx, start_idx))
        }
    }

    fn apply_motion_for_range(&mut self, motion: &Motion) -> Option<()> {
        match motion {
            Motion::Line => {}
            Motion::WordForward => self.move_word_forward(),
            Motion::WordEnd => self.move_word_end(),
            Motion::WordEndBackward => self.move_word_end_backward(),
            Motion::WordBackward => self.move_word_backward(),
            Motion::LineEnd => self.move_line_end(),
            Motion::LineStart => self.move_line_start(),
            Motion::FirstNonBlank => self.move_first_non_blank(),
            Motion::WORDForward => self.move_word_forward_big(),
            Motion::WORDEnd => self.move_word_end_big(),
            Motion::WORDEndBackward => self.move_word_end_backward_big(),
            Motion::WORDBackward => self.move_word_backward_big(),
            Motion::ParagraphForward => self.move_paragraph_forward(),
            Motion::ParagraphBackward => self.move_paragraph_backward(),
            Motion::SentenceForward => self.move_sentence_forward(),
            Motion::SentenceBackward => self.move_sentence_backward(),
            Motion::SectionForward => self.move_section_forward(),
            Motion::SectionBackward => self.move_section_backward(),
            Motion::DocumentStart => self.goto_top(),
            Motion::DocumentEnd => self.goto_bottom(),
            Motion::MatchBracket => self.match_bracket_jump(),
            Motion::SearchForward => self.search_next(),
            Motion::SearchBackward => self.search_prev(),
            Motion::Column => self.move_column(1),
            Motion::LineDownFirstNonBlank => self.move_line_down_first_non_blank(),
            Motion::LineUpFirstNonBlank => self.move_line_up_first_non_blank(),
            Motion::FindForward(ch) => self.find_char_forward(*ch),
            Motion::FindBackward(ch) => self.find_char_backward(*ch),
            Motion::TillForward(ch) => self.till_char_forward(*ch),
            Motion::TillBackward(ch) => self.till_char_backward(*ch),
            Motion::Inner(_) | Motion::Around(_) => return None,
        }
        Some(())
    }

    fn motion_range(&mut self, motion: &Motion) -> Option<(usize, usize)> {
        let cursor_idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;

        match motion {
            Motion::Line => unreachable!(),
            Motion::WordForward => {
                let saved = self.cursor;
                self.move_word_forward();
                let end = self.cursor;
                self.cursor = saved;
                let end_idx = self.document.rope.line_to_char(end.row) + end.col;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::WordEnd => {
                let saved = self.cursor;
                self.move_word_end();
                let end = self.cursor;
                self.cursor = saved;
                let end_idx = self.document.rope.line_to_char(end.row) + end.col + 1;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::WordBackward => {
                let saved = self.cursor;
                self.move_word_backward();
                let start = self.cursor;
                self.cursor = saved;
                let start_idx = self.document.rope.line_to_char(start.row) + start.col;
                if cursor_idx > start_idx {
                    Some((start_idx, cursor_idx))
                } else {
                    None
                }
            }
            Motion::LineEnd => {
                let line_len = self.document.line_len(self.cursor.row);
                let end_idx = self.document.rope.line_to_char(self.cursor.row) + line_len;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::LineStart => {
                let start_idx = self.document.rope.line_to_char(self.cursor.row);
                if cursor_idx > start_idx {
                    Some((start_idx, cursor_idx))
                } else {
                    None
                }
            }
            Motion::Inner(ch) => self.find_inner_range(*ch),
            Motion::Around(ch) => self.find_around_range(*ch),
            Motion::FindForward(ch) => {
                let line = self.document.rope.line(self.cursor.row);
                let line_len = buffer::line_display_len(line);
                for col in (self.cursor.col + 1)..line_len {
                    if line.char(col) == *ch {
                        let end_idx = self.document.rope.line_to_char(self.cursor.row) + col + 1;
                        return Some((cursor_idx, end_idx));
                    }
                }
                None
            }
            Motion::FindBackward(ch) => {
                let line = self.document.rope.line(self.cursor.row);
                for col in (0..self.cursor.col).rev() {
                    if line.char(col) == *ch {
                        let start_idx = self.document.rope.line_to_char(self.cursor.row) + col;
                        return Some((start_idx, cursor_idx));
                    }
                }
                None
            }
            Motion::TillForward(ch) => {
                let line = self.document.rope.line(self.cursor.row);
                let line_len = buffer::line_display_len(line);
                for col in (self.cursor.col + 1)..line_len {
                    if line.char(col) == *ch {
                        let end_idx = self.document.rope.line_to_char(self.cursor.row) + col;
                        if end_idx > cursor_idx {
                            return Some((cursor_idx, end_idx));
                        }
                    }
                }
                None
            }
            Motion::TillBackward(ch) => {
                let line = self.document.rope.line(self.cursor.row);
                for col in (0..self.cursor.col).rev() {
                    if line.char(col) == *ch {
                        let start_idx = self.document.rope.line_to_char(self.cursor.row) + col + 1;
                        if start_idx < cursor_idx {
                            return Some((start_idx, cursor_idx));
                        }
                    }
                }
                None
            }
            Motion::FirstNonBlank => {
                let saved = self.cursor;
                self.move_first_non_blank();
                let target = self.cursor;
                self.cursor = saved;
                let target_idx = self.document.rope.line_to_char(target.row) + target.col;
                if target_idx < cursor_idx {
                    Some((target_idx, cursor_idx))
                } else if target_idx > cursor_idx {
                    Some((cursor_idx, target_idx))
                } else {
                    None
                }
            }
            Motion::WORDForward => {
                let saved = self.cursor;
                self.move_word_forward_big();
                let end = self.cursor;
                self.cursor = saved;
                let end_idx = self.document.rope.line_to_char(end.row) + end.col;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::WORDEnd => {
                let saved = self.cursor;
                self.move_word_end_big();
                let end = self.cursor;
                self.cursor = saved;
                let end_idx = self.document.rope.line_to_char(end.row) + end.col + 1;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::WORDBackward => {
                let saved = self.cursor;
                self.move_word_backward_big();
                let start = self.cursor;
                self.cursor = saved;
                let start_idx = self.document.rope.line_to_char(start.row) + start.col;
                if cursor_idx > start_idx {
                    Some((start_idx, cursor_idx))
                } else {
                    None
                }
            }
            Motion::ParagraphForward => {
                let saved = self.cursor;
                self.move_paragraph_forward();
                let end = self.cursor;
                self.cursor = saved;
                let end_idx = self.document.rope.line_to_char(end.row) + end.col;
                if end_idx > cursor_idx {
                    Some((cursor_idx, end_idx))
                } else {
                    None
                }
            }
            Motion::ParagraphBackward => {
                let saved = self.cursor;
                self.move_paragraph_backward();
                let start = self.cursor;
                self.cursor = saved;
                let start_idx = self.document.rope.line_to_char(start.row) + start.col;
                if cursor_idx > start_idx {
                    Some((start_idx, cursor_idx))
                } else {
                    None
                }
            }
            _ => self.motion_range_count(motion, 1),
        }
    }

    // --- Text objects ---

    fn find_inner_range(&self, ch: char) -> Option<(usize, usize)> {
        match ch {
            '{' | '}' | 'B' => self.find_inner_brackets('{', '}'),
            '(' | ')' | 'b' => self.find_inner_brackets('(', ')'),
            '[' | ']' => self.find_inner_brackets('[', ']'),
            '<' | '>' => self.find_inner_brackets('<', '>'),
            '"' => self.find_inner_quotes('"'),
            '\'' => self.find_inner_quotes('\''),
            '`' => self.find_inner_quotes('`'),
            'w' => self.find_inner_word(),
            'W' => self.find_inner_big_word(),
            'p' => self.find_paragraph_range(false),
            's' => self.find_sentence_range(false),
            't' => self.find_tag_range(false),
            ',' => self.find_argument_range(false),
            _ => None,
        }
    }

    fn find_around_range(&self, ch: char) -> Option<(usize, usize)> {
        match ch {
            '{' | '}' | 'B' => {
                let (s, e) = self.find_inner_brackets('{', '}')?;
                Some((s - 1, (e + 1).min(self.document.rope.len_chars())))
            }
            '(' | ')' | 'b' => {
                let (s, e) = self.find_inner_brackets('(', ')')?;
                Some((s - 1, (e + 1).min(self.document.rope.len_chars())))
            }
            '[' | ']' => {
                let (s, e) = self.find_inner_brackets('[', ']')?;
                Some((s - 1, (e + 1).min(self.document.rope.len_chars())))
            }
            '<' | '>' => {
                let (s, e) = self.find_inner_brackets('<', '>')?;
                Some((
                    s.saturating_sub(1),
                    (e + 1).min(self.document.rope.len_chars()),
                ))
            }
            '"' => {
                let (s, e) = self.find_inner_quotes('"')?;
                Some((
                    s.saturating_sub(1),
                    (e + 1).min(self.document.rope.len_chars()),
                ))
            }
            '\'' => {
                let (s, e) = self.find_inner_quotes('\'')?;
                Some((
                    s.saturating_sub(1),
                    (e + 1).min(self.document.rope.len_chars()),
                ))
            }
            '`' => {
                let (s, e) = self.find_inner_quotes('`')?;
                Some((
                    s.saturating_sub(1),
                    (e + 1).min(self.document.rope.len_chars()),
                ))
            }
            'w' => self.find_around_word(),
            'W' => self.find_around_big_word(),
            'p' => self.find_paragraph_range(true),
            's' => self.find_sentence_range(true),
            't' => self.find_tag_range(true),
            ',' => self.find_argument_range(true),
            _ => None,
        }
    }

    fn find_inner_brackets(&self, open: char, close: char) -> Option<(usize, usize)> {
        let cursor_idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        let len = self.document.rope.len_chars();

        // Scan backward to find unmatched opening bracket
        let mut depth = 0i32;
        let mut open_idx = None;
        for i in (0..=cursor_idx.min(len.saturating_sub(1))).rev() {
            let c = self.document.rope.char(i);
            if c == close && i != cursor_idx {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    open_idx = Some(i);
                    break;
                }
                depth -= 1;
            }
        }
        let open_idx = open_idx?;

        // Scan forward to find matching close
        let mut depth = 0i32;
        let mut close_idx = None;
        for i in open_idx..len {
            let c = self.document.rope.char(i);
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(i);
                    break;
                }
            }
        }
        let close_idx = close_idx?;

        Some((open_idx + 1, close_idx))
    }

    fn find_inner_quotes(&self, quote: char) -> Option<(usize, usize)> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        let line_start = self.document.rope.line_to_char(self.cursor.row);

        let mut first = None;
        for col in 0..line_len {
            if line.char(col) == quote {
                if let Some(start) = first {
                    // Check if cursor is between these quotes
                    if self.cursor.col >= start && self.cursor.col <= col {
                        return Some((line_start + start + 1, line_start + col));
                    }
                    first = None;
                } else {
                    first = Some(col);
                }
            }
        }
        None
    }

    fn find_inner_word(&self) -> Option<(usize, usize)> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if self.cursor.col >= line_len {
            return None;
        }

        let ch = line.char(self.cursor.col);
        if !buffer::is_word_char(ch) {
            return None;
        }

        let mut start = self.cursor.col;
        while start > 0 && buffer::is_word_char(line.char(start - 1)) {
            start -= 1;
        }

        let mut end = self.cursor.col;
        while end + 1 < line_len && buffer::is_word_char(line.char(end + 1)) {
            end += 1;
        }

        let line_start = self.document.rope.line_to_char(self.cursor.row);
        Some((line_start + start, line_start + end + 1))
    }

    fn find_around_word(&self) -> Option<(usize, usize)> {
        let (start, end) = self.find_inner_word()?;
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        let line_start = self.document.rope.line_to_char(self.cursor.row);

        // Include trailing whitespace
        let mut new_end = end - line_start;
        while new_end < line_len && line.char(new_end).is_whitespace() && line.char(new_end) != '\n'
        {
            new_end += 1;
        }

        Some((start, line_start + new_end))
    }

    fn find_inner_big_word(&self) -> Option<(usize, usize)> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if self.cursor.col >= line_len || line.char(self.cursor.col).is_whitespace() {
            return None;
        }
        let mut start = self.cursor.col;
        while start > 0 && !line.char(start - 1).is_whitespace() {
            start -= 1;
        }
        let mut end = self.cursor.col;
        while end + 1 < line_len && !line.char(end + 1).is_whitespace() {
            end += 1;
        }
        let line_start = self.document.rope.line_to_char(self.cursor.row);
        Some((line_start + start, line_start + end + 1))
    }

    fn find_around_big_word(&self) -> Option<(usize, usize)> {
        let (start, end) = self.find_inner_big_word()?;
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        let line_start = self.document.rope.line_to_char(self.cursor.row);
        let mut new_end = end - line_start;
        while new_end < line_len && line.char(new_end).is_whitespace() && line.char(new_end) != '\n'
        {
            new_end += 1;
        }
        Some((start, line_start + new_end))
    }

    fn trim_range_whitespace(&self, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
        end = end.min(self.document.rope.len_chars());
        while start < end && self.document.rope.char(start).is_whitespace() {
            start += 1;
        }
        while end > start && self.document.rope.char(end - 1).is_whitespace() {
            end -= 1;
        }
        (start < end).then_some((start, end))
    }

    fn find_paragraph_range(&self, around: bool) -> Option<(usize, usize)> {
        let mut start_row = self
            .cursor
            .row
            .min(self.document.line_count().saturating_sub(1));
        while start_row > 0 && !self.is_blank_line(start_row - 1) {
            start_row -= 1;
        }
        let mut end_row = self
            .cursor
            .row
            .min(self.document.line_count().saturating_sub(1));
        while end_row + 1 < self.document.line_count() && !self.is_blank_line(end_row + 1) {
            end_row += 1;
        }
        if around {
            while end_row + 1 < self.document.line_count() && self.is_blank_line(end_row + 1) {
                end_row += 1;
            }
        }
        let start = self.document.rope.line_to_char(start_row);
        let end = if end_row + 1 < self.document.line_count() {
            self.document.rope.line_to_char(end_row + 1)
        } else {
            self.document.rope.len_chars()
        };
        (start < end).then_some((start, end))
    }

    fn find_sentence_range(&self, around: bool) -> Option<(usize, usize)> {
        let len = self.document.rope.len_chars();
        let cursor_idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        let mut start = 0;
        for idx in (0..cursor_idx.min(len)).rev() {
            if matches!(self.document.rope.char(idx), '.' | '!' | '?') {
                start = (idx + 1).min(len);
                break;
            }
        }
        let mut end = len;
        for idx in cursor_idx.min(len)..len {
            if matches!(self.document.rope.char(idx), '.' | '!' | '?') {
                end = (idx + 1).min(len);
                break;
            }
        }
        if around {
            while end < len && self.document.rope.char(end).is_whitespace() {
                end += 1;
            }
            (start < end).then_some((start, end))
        } else {
            self.trim_range_whitespace(start, end)
        }
    }

    fn char_to_byte(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    fn byte_to_char(text: &str, byte_idx: usize) -> usize {
        text[..byte_idx.min(text.len())].chars().count()
    }

    fn find_tag_range(&self, around: bool) -> Option<(usize, usize)> {
        let text = self.document.rope.to_string();
        let cursor_idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        let cursor_byte = Self::char_to_byte(&text, cursor_idx);
        let open_re = regex::Regex::new(r"<([A-Za-z][A-Za-z0-9:_-]*)(?:\s[^>]*)?>").ok()?;
        let mut open_match = None;
        for caps in open_re.captures_iter(&text[..cursor_byte.min(text.len())]) {
            if caps.get(0)?.as_str().starts_with("</") {
                continue;
            }
            open_match = Some((caps.get(0)?, caps.get(1)?.as_str().to_string()));
        }
        let (open, tag) = open_match?;
        let close_re = regex::Regex::new(&format!(r"</{}\s*>", regex::escape(&tag))).ok()?;
        let after_open = open.end();
        let close = close_re.find(&text[after_open..])?;
        let close_start = after_open + close.start();
        let close_end = after_open + close.end();
        if cursor_byte > close_start {
            return None;
        }
        if around {
            Some((
                Self::byte_to_char(&text, open.start()),
                Self::byte_to_char(&text, close_end),
            ))
        } else {
            Some((
                Self::byte_to_char(&text, open.end()),
                Self::byte_to_char(&text, close_start),
            ))
        }
    }

    fn find_argument_range(&self, around: bool) -> Option<(usize, usize)> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if line_len == 0 {
            return None;
        }
        let mut start_col = 0;
        for col in (0..self.cursor.col.min(line_len)).rev() {
            if matches!(line.char(col), ',' | '(' | '[' | '{') {
                start_col = if around { col } else { col + 1 };
                break;
            }
        }
        let mut end_col = line_len;
        for col in self.cursor.col.min(line_len)..line_len {
            if matches!(line.char(col), ',' | ')' | ']' | '}') {
                end_col = if around { (col + 1).min(line_len) } else { col };
                break;
            }
        }
        let line_start = self.document.rope.line_to_char(self.cursor.row);
        if around {
            (start_col < end_col).then_some((line_start + start_col, line_start + end_col))
        } else {
            self.trim_range_whitespace(line_start + start_col, line_start + end_col)
        }
    }

    // --- Find/till character (standalone motion) ---

    pub fn find_char_forward(&mut self, ch: char) {
        self.last_find = Some(LastFind {
            target: ch,
            kind: FindKind::Find,
            direction: FindDirection::Forward,
        });
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        for col in (self.cursor.col + 1)..line_len {
            if line.char(col) == ch {
                self.cursor.col = col;
                return;
            }
        }
    }

    pub fn find_char_backward(&mut self, ch: char) {
        self.last_find = Some(LastFind {
            target: ch,
            kind: FindKind::Find,
            direction: FindDirection::Backward,
        });
        let line = self.document.rope.line(self.cursor.row);
        for col in (0..self.cursor.col).rev() {
            if line.char(col) == ch {
                self.cursor.col = col;
                return;
            }
        }
    }

    pub fn till_char_forward(&mut self, ch: char) {
        self.last_find = Some(LastFind {
            target: ch,
            kind: FindKind::Till,
            direction: FindDirection::Forward,
        });
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        for col in (self.cursor.col + 1)..line_len {
            if line.char(col) == ch {
                if col > 0 {
                    self.cursor.col = col - 1;
                }
                return;
            }
        }
    }

    pub fn till_char_backward(&mut self, ch: char) {
        self.last_find = Some(LastFind {
            target: ch,
            kind: FindKind::Till,
            direction: FindDirection::Backward,
        });
        let line = self.document.rope.line(self.cursor.row);
        for col in (0..self.cursor.col).rev() {
            if line.char(col) == ch {
                self.cursor.col = col + 1;
                return;
            }
        }
    }

    pub fn repeat_find(&mut self, reverse: bool) {
        let Some(last) = self.last_find else {
            return;
        };
        let direction = if reverse {
            match last.direction {
                FindDirection::Forward => FindDirection::Backward,
                FindDirection::Backward => FindDirection::Forward,
            }
        } else {
            last.direction
        };
        match (last.kind, direction) {
            (FindKind::Find, FindDirection::Forward) => self.find_char_forward(last.target),
            (FindKind::Find, FindDirection::Backward) => self.find_char_backward(last.target),
            (FindKind::Till, FindDirection::Forward) => self.till_char_forward(last.target),
            (FindKind::Till, FindDirection::Backward) => self.till_char_backward(last.target),
        }
        self.last_find = Some(last);
    }

    // --- Replace character ---

    pub fn replace_char(&mut self, ch: char) {
        let line_len = self.document.line_len(self.cursor.row);
        if self.cursor.col >= line_len {
            return;
        }
        self.save_undo();
        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        self.document.rope.remove(idx..idx + 1);
        self.document.rope.insert_char(idx, ch);
        self.document.modified = true;
        self.document.bump_version();
        if self.mode == Mode::Replace {
            let line_len = self.document.line_len(self.cursor.row);
            if self.cursor.col + 1 < line_len {
                self.cursor.col += 1;
            }
        }
    }

    // --- Join lines ---

    pub fn join_lines(&mut self) {
        if self.cursor.row + 1 >= self.document.line_count() {
            return;
        }
        self.save_undo();
        let line_len = self.document.line_len(self.cursor.row);
        let newline_idx = self.document.rope.line_to_char(self.cursor.row) + line_len;

        // Count leading whitespace on next line
        let next_line = self.document.rope.line(self.cursor.row + 1);
        let leading_ws: usize = next_line
            .chars()
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .count();

        // Remove newline + leading whitespace on next line
        let remove_end = (newline_idx + 1 + leading_ws).min(self.document.rope.len_chars());
        if newline_idx < remove_end {
            self.document.rope.remove(newline_idx..remove_end);
            // Insert a space to separate
            if newline_idx < self.document.rope.len_chars() {
                self.document.rope.insert_char(newline_idx, ' ');
            }
            self.document.modified = true;
            self.document.bump_version();
            self.cursor.col = line_len;
        }
    }

    // --- Jump list ---

    pub fn push_jump(&mut self) {
        if self.jump_index < self.jump_list.len() {
            self.jump_list.truncate(self.jump_index);
        }
        self.jump_list.push(self.cursor);
        self.jump_index = self.jump_list.len();
        if self.jump_list.len() > 100 {
            self.jump_list.remove(0);
            self.jump_index -= 1;
        }
    }

    pub fn jump_back(&mut self) {
        if self.jump_index == 0 {
            return;
        }
        // Save current position if at end
        if self.jump_index == self.jump_list.len() {
            self.jump_list.push(self.cursor);
        }
        self.jump_index -= 1;
        let pos = self.jump_list[self.jump_index];
        self.cursor = pos;
        self.clamp_cursor();
    }

    pub fn jump_forward(&mut self) {
        if self.jump_index + 1 >= self.jump_list.len() {
            return;
        }
        self.jump_index += 1;
        let pos = self.jump_list[self.jump_index];
        self.cursor = pos;
        self.clamp_cursor();
    }

    fn remember_previous_position(&mut self) {
        self.previous_position = Some(self.cursor);
    }

    pub fn set_mark(&mut self, mark: char) {
        self.marks.insert(mark, self.cursor);
        self.status_message = Some(format!("mark '{mark}' set"));
    }

    pub fn goto_mark(&mut self, mark: char, exact: bool) {
        if let Some(pos) = self.marks.get(&mark).copied() {
            self.remember_previous_position();
            self.cursor = pos;
            if !exact {
                self.cursor.col = 0;
                self.move_first_non_blank();
            }
            self.clamp_cursor();
        } else {
            self.status_message = Some(format!("Mark not set: {mark}"));
        }
    }

    pub fn goto_previous_position(&mut self, exact: bool) {
        if let Some(pos) = self.previous_position {
            let current = self.cursor;
            self.cursor = pos;
            self.previous_position = Some(current);
            if !exact {
                self.cursor.col = 0;
                self.move_first_non_blank();
            }
            self.clamp_cursor();
        }
    }

    // --- Paste ---

    pub fn paste_after(&mut self) {
        let reg_name = self.consume_register();
        let reg = match self.read_register(reg_name) {
            Some(r) if !r.content.is_empty() => r,
            _ => {
                self.status_message = Some(if reg_name == '"' {
                    "Nothing in register".to_string()
                } else {
                    format!("register \"{} is empty", reg_name)
                });
                return;
            }
        };
        self.save_undo();

        if reg.linewise {
            let insert_row = self.cursor.row + 1;
            let idx = if insert_row < self.document.line_count() {
                self.document.rope.line_to_char(insert_row)
            } else {
                let len = self.document.rope.len_chars();
                // Ensure trailing newline
                if len > 0 && self.document.rope.char(len - 1) != '\n' {
                    self.document.rope.insert_char(len, '\n');
                }
                self.document.rope.len_chars()
            };
            self.document.rope.insert(idx, &reg.content);
            self.document.modified = true;
            self.cursor.row = insert_row;
            self.cursor.col = 0;
        } else {
            let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col + 1;
            let idx = idx.min(self.document.rope.len_chars());
            self.document.rope.insert(idx, &reg.content);
            self.document.modified = true;
            let char_count = reg.content.chars().count();
            if char_count > 0 {
                // Cursor on last pasted char
                self.cursor.col += char_count;
            }
        }
        self.clamp_cursor();
        if reg_name != '"' {
            self.status_message = Some(format!("pasted from register \"{}", reg_name));
        }
    }

    pub fn paste_before(&mut self) {
        let reg_name = self.consume_register();
        let reg = match self.read_register(reg_name) {
            Some(r) if !r.content.is_empty() => r,
            _ => {
                self.status_message = Some(if reg_name == '"' {
                    "Nothing in register".to_string()
                } else {
                    format!("register \"{} is empty", reg_name)
                });
                return;
            }
        };
        self.save_undo();

        if reg.linewise {
            let idx = self.document.rope.line_to_char(self.cursor.row);
            self.document.rope.insert(idx, &reg.content);
            self.document.modified = true;
            self.cursor.col = 0;
        } else {
            let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
            let char_count = reg.content.chars().count();
            self.document.rope.insert(idx, &reg.content);
            self.document.modified = true;
            if char_count > 0 {
                self.cursor.col += char_count.saturating_sub(1);
            }
        }
        self.clamp_cursor();
        if reg_name != '"' {
            self.status_message = Some(format!("pasted from register \"{}", reg_name));
        }
    }

    // --- Mode changes ---

    pub fn enter_insert_mode(&mut self) {
        self.save_undo();
        self.mode = Mode::Insert;
    }

    pub fn enter_insert_mode_after(&mut self) {
        self.save_undo();
        self.mode = Mode::Insert;
        let line_len = self.document.line_len(self.cursor.row);
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        }
    }

    pub fn enter_insert_mode_line_end(&mut self) {
        self.save_undo();
        self.mode = Mode::Insert;
        self.cursor.col = self.document.line_len(self.cursor.row);
    }

    pub fn enter_replace_mode(&mut self) {
        self.save_undo();
        self.mode = Mode::Replace;
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.command_buffer.clear();
    }

    pub fn exit_to_normal_mode(&mut self) {
        self.finish_visual_block_edit();
        self.mode = Mode::Normal;
        self.command_buffer.clear();
        self.pending_keys.clear();
        self.count_prefix = None;
        self.pending_operator_count = None;
        self.visual_anchor = None;
        self.clamp_cursor();
    }

    // --- Command mode ---

    pub fn command_input(&mut self, ch: char) {
        self.command_buffer.push(ch);
    }

    pub fn command_backspace(&mut self) {
        if self.command_buffer.pop().is_none() {
            self.exit_to_normal_mode();
        }
    }

    // --- Syntax highlighting ---

    pub fn syntax_language(&self) -> Option<SyntaxLanguage> {
        self.syntax_language_override.or_else(|| {
            SyntaxLanguage::from_path_and_text(
                self.document.path.as_deref(),
                &self.document.rope.to_string(),
            )
        })
    }

    pub fn set_syntax_language(&mut self, language: Option<SyntaxLanguage>) {
        if self.syntax_language_override == language {
            return;
        }

        let previous_language = self.syntax_language();
        self.syntax_language_override = language;
        if let Some(buffer) = self.buffers.get_mut(self.current_buffer) {
            buffer.syntax_language_override = language;
        }
        let effective_language = self.syntax_language();
        if previous_language != effective_language {
            self.syntax_tree = None;
            self.line_styles.clear();
            self.styles_offset = 0;
            for pane in self
                .panes
                .iter_mut()
                .filter(|pane| pane.buffer_idx == self.current_buffer)
            {
                pane.syntax_tree = None;
                pane.line_styles.clear();
                pane.styles_offset = 0;
            }
        }
        self.sync_highlighter(effective_language);
    }

    fn sync_highlighter(&mut self, language: Option<SyntaxLanguage>) {
        let needs_highlighter = self
            .highlighter
            .as_ref()
            .map(|hl| Some(hl.language()) != language)
            .unwrap_or(language.is_some());
        if needs_highlighter {
            self.highlighter = language.and_then(Highlighter::new);
            self.syntax_tree = None;
            self.line_styles.clear();
            self.styles_offset = 0;
        }
    }

    pub fn update_highlights(&mut self) {
        let start = self.view.offset_row;
        let end = (start + self.view.height as usize).min(self.document.line_count());
        self.update_highlights_for_range(start, end);
    }

    pub fn update_highlights_for_range(&mut self, start_line: usize, end_line: usize) {
        let line_count = self.document.line_count();
        let start = start_line.min(line_count);
        let end = end_line.min(line_count);
        if start >= end {
            self.line_styles.clear();
            self.styles_offset = start;
            return;
        }

        let language = self.syntax_language();
        self.sync_highlighter(language);

        if let Some(hl) = &mut self.highlighter {
            self.syntax_tree = hl.parse(&self.document.rope, self.syntax_tree.as_ref());
            if let Some(tree) = &self.syntax_tree {
                self.line_styles = hl.highlight_lines(tree, &self.document.rope, start, end);
                self.styles_offset = start;
            }
        } else {
            self.syntax_tree = None;
            self.line_styles.clear();
            self.styles_offset = 0;
        }
    }

    pub fn highlight_spans_for_range(
        &mut self,
        start_line: usize,
        count: usize,
    ) -> Vec<HighlightSpan> {
        let line_count = self.document.line_count();
        if start_line >= line_count || count == 0 {
            self.line_styles.clear();
            self.styles_offset = start_line.min(line_count);
            return Vec::new();
        }

        let end_line = start_line.saturating_add(count).min(line_count);
        self.update_highlights_for_range(start_line, end_line);
        let styles_offset = self.styles_offset;
        self.line_styles
            .iter()
            .enumerate()
            .flat_map(|(rel_row, spans)| {
                spans
                    .iter()
                    .map(move |&(start_col, end_col, highlight)| HighlightSpan {
                        row: styles_offset + rel_row,
                        start_col,
                        end_col,
                        token: highlight.token,
                        style: highlight.style,
                    })
            })
            .collect()
    }

    pub fn highlight_style_at(&self, doc_row: usize, col: usize) -> SyntaxStyle {
        if let Some(rel) = doc_row.checked_sub(self.styles_offset) {
            highlight::style_at(&self.line_styles, rel, col)
        } else {
            highlight::theme::default_style()
        }
    }

    // --- Completion ---

    pub fn accept_completion(&mut self) {
        if !self.showing_completion || self.completions.is_empty() {
            self.cancel_completion();
            return;
        }
        let item = &self.completions[self.completion_index];
        let text = item
            .insert_text
            .clone()
            .unwrap_or_else(|| item.label.clone());

        self.cancel_completion();

        // Insert completion text at cursor
        self.save_undo();
        for ch in text.chars() {
            self.document.insert_char(self.cursor, ch);
            self.cursor.col += 1;
        }
    }

    pub fn cancel_completion(&mut self) {
        self.showing_completion = false;
        self.completions.clear();
        self.completion_index = 0;
        self.pending_completion_id = None;
    }

    pub fn completion_next(&mut self) {
        if !self.completions.is_empty() {
            self.completion_index = (self.completion_index + 1) % self.completions.len();
        }
    }

    pub fn completion_prev(&mut self) {
        if !self.completions.is_empty() {
            self.completion_index = if self.completion_index == 0 {
                self.completions.len() - 1
            } else {
                self.completion_index - 1
            };
        }
    }

    // --- Popup dismiss / references navigation ---

    pub fn dismiss_popup(&mut self) {
        self.showing_hover = false;
        self.hover_text = None;
        self.showing_references = false;
        self.references.clear();
        self.reference_index = 0;
        self.dismiss_code_actions();
        self.dismiss_diagnostics_list();
        self.workspace_symbol_cancel();
        self.dismiss_git_diff();
    }

    pub fn open_git_diff(&mut self, diff: GitDiff) {
        self.git_diff = Some(diff);
        self.showing_git_diff = true;
    }

    pub fn dismiss_git_diff(&mut self) {
        self.showing_git_diff = false;
        self.git_diff = None;
    }

    pub fn reference_next(&mut self) {
        if !self.references.is_empty() {
            self.reference_index = (self.reference_index + 1) % self.references.len();
        }
    }

    pub fn reference_prev(&mut self) {
        if !self.references.is_empty() {
            self.reference_index = if self.reference_index == 0 {
                self.references.len() - 1
            } else {
                self.reference_index - 1
            };
        }
    }

    // --- Code Actions ---

    pub fn code_action_next(&mut self) {
        if !self.code_actions.is_empty() {
            self.code_action_index = (self.code_action_index + 1) % self.code_actions.len();
        }
    }

    pub fn code_action_prev(&mut self) {
        if !self.code_actions.is_empty() {
            self.code_action_index = if self.code_action_index == 0 {
                self.code_actions.len() - 1
            } else {
                self.code_action_index - 1
            };
        }
    }

    pub fn diagnostic_next(&mut self) {
        if self.diagnostics.is_empty() {
            self.status_message = Some("No diagnostics".to_string());
            return;
        }
        // Find first diagnostic starting after current cursor row
        let mut best: Option<(usize, usize)> = None;
        let mut first: Option<(usize, usize)> = None;
        for d in &self.diagnostics {
            let row = d.start_line as usize;
            let col = d.start_col as usize;
            if first.is_none() || (row, col) < first.unwrap() {
                first = Some((row, col));
            }
            if (row > self.cursor.row || (row == self.cursor.row && col > self.cursor.col))
                && (best.is_none() || (row, col) < best.unwrap())
            {
                best = Some((row, col));
            }
        }
        // Wrap around
        let (row, col) = best.or(first).unwrap();
        self.cursor.row = row;
        self.cursor.col = col;
        self.clamp_cursor();
        if let Some(msg) = self.diagnostic_at_cursor() {
            self.status_message = Some(msg.to_string());
        }
    }

    pub fn diagnostic_prev(&mut self) {
        if self.diagnostics.is_empty() {
            self.status_message = Some("No diagnostics".to_string());
            return;
        }
        // Find last diagnostic starting before current cursor row
        let mut best: Option<(usize, usize)> = None;
        let mut last: Option<(usize, usize)> = None;
        for d in &self.diagnostics {
            let row = d.start_line as usize;
            let col = d.start_col as usize;
            if last.is_none() || (row, col) > last.unwrap() {
                last = Some((row, col));
            }
            if (row < self.cursor.row || (row == self.cursor.row && col < self.cursor.col))
                && (best.is_none() || (row, col) > best.unwrap())
            {
                best = Some((row, col));
            }
        }
        // Wrap around
        let (row, col) = best.or(last).unwrap();
        self.cursor.row = row;
        self.cursor.col = col;
        self.clamp_cursor();
        if let Some(msg) = self.diagnostic_at_cursor() {
            self.status_message = Some(msg.to_string());
        }
    }

    pub fn dismiss_code_actions(&mut self) {
        self.showing_code_actions = false;
        self.code_actions.clear();
        self.code_action_index = 0;
        self.pending_code_action_id = None;
    }

    // --- Diagnostics list ---

    pub fn toggle_diagnostics_list(&mut self) {
        if self.showing_diagnostics {
            self.showing_diagnostics = false;
        } else if self.diagnostics.is_empty() {
            self.status_message = Some("No diagnostics".to_string());
        } else {
            self.showing_diagnostics = true;
            self.diagnostic_list_index = 0;
        }
    }

    pub fn diagnostic_list_next(&mut self) {
        if !self.diagnostics.is_empty() {
            self.diagnostic_list_index = (self.diagnostic_list_index + 1) % self.diagnostics.len();
        }
    }

    pub fn diagnostic_list_prev(&mut self) {
        if !self.diagnostics.is_empty() {
            self.diagnostic_list_index = if self.diagnostic_list_index == 0 {
                self.diagnostics.len() - 1
            } else {
                self.diagnostic_list_index - 1
            };
        }
    }

    pub fn diagnostic_list_jump(&mut self) {
        if let Some(d) = self.diagnostics.get(self.diagnostic_list_index) {
            self.cursor.row = d.start_line as usize;
            self.cursor.col = d.start_col as usize;
            self.clamp_cursor();
            self.showing_diagnostics = false;
            if let Some(msg) = self.diagnostic_at_cursor() {
                self.status_message = Some(msg.to_string());
            }
        }
    }

    pub fn dismiss_diagnostics_list(&mut self) {
        self.showing_diagnostics = false;
        self.diagnostic_list_index = 0;
    }

    // --- Search ---

    pub fn enter_search_mode(&mut self) {
        self.mode = Mode::Search;
        self.search_forward = true;
        self.search_query.clear();
        self.search_start_cursor = Some(self.cursor);
    }

    pub fn enter_search_backward_mode(&mut self) {
        self.mode = Mode::Search;
        self.search_forward = false;
        self.search_query.clear();
        self.search_start_cursor = Some(self.cursor);
    }

    pub fn search_input(&mut self, ch: char) {
        self.search_query.push(ch);
        self.update_search_matches();
        self.incremental_jump();
    }

    pub fn search_backspace(&mut self) {
        if self.search_query.pop().is_none() {
            self.mode = Mode::Normal;
            if let Some(pos) = self.search_start_cursor.take() {
                self.cursor = pos;
                self.clamp_cursor();
            }
        } else {
            self.update_search_matches();
            if self.search_query.is_empty() {
                if let Some(pos) = self.search_start_cursor {
                    self.cursor = pos;
                    self.clamp_cursor();
                }
            } else {
                self.incremental_jump();
            }
        }
    }

    pub fn search_confirm(&mut self) {
        self.mode = Mode::Normal;
        let origin = self.search_start_cursor.take().unwrap_or(self.cursor);
        self.last_search_forward = self.search_forward;
        self.push_jump();
        if !self.search_matches.is_empty() {
            let idx = if self.search_forward {
                self.search_matches
                    .iter()
                    .position(|&(r, c, _)| r > origin.row || (r == origin.row && c >= origin.col))
                    .unwrap_or(0)
            } else {
                self.search_matches
                    .iter()
                    .rposition(|&(r, c, _)| r < origin.row || (r == origin.row && c <= origin.col))
                    .unwrap_or_else(|| self.search_matches.len() - 1)
            };
            self.search_index = Some(idx);
            let (row, col, _) = self.search_matches[idx];
            self.cursor.row = row;
            self.cursor.col = col;
            self.clamp_cursor();
        }
    }

    pub fn search_cancel(&mut self) {
        self.mode = Mode::Normal;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_index = None;
        self.search_regex = None;
        if let Some(pos) = self.search_start_cursor.take() {
            self.cursor = pos;
            self.clamp_cursor();
        }
    }

    fn search_step(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            if !self.search_query.is_empty() {
                self.update_search_matches();
            }
            if self.search_matches.is_empty() {
                self.status_message = Some("Pattern not found".to_string());
                return;
            }
        }
        let current = self.search_index.unwrap_or(0);
        let next = if forward {
            (current + 1) % self.search_matches.len()
        } else if current == 0 {
            self.search_matches.len().saturating_sub(1)
        } else {
            current - 1
        };
        self.search_index = Some(next);
        let (row, col, _) = self.search_matches[next];
        self.cursor.row = row;
        self.cursor.col = col;
        self.clamp_cursor();
        self.status_message = Some(format!("[{}/{}]", next + 1, self.search_matches.len()));
    }

    pub fn search_next(&mut self) {
        self.search_step(self.last_search_forward);
    }

    pub fn search_prev(&mut self) {
        self.search_step(!self.last_search_forward);
    }

    /// Build the regex for the current search query, applying smart case
    /// and optional `\c` (force case-insensitive) / `\C` (force case-sensitive) suffixes.
    fn build_search_regex(query: &str) -> Option<regex::Regex> {
        if query.is_empty() {
            return None;
        }

        // Check for explicit case modifiers at the end
        let (pattern, force_case) = if let Some(stripped) = query.strip_suffix("\\c") {
            (stripped, Some(false)) // force case-insensitive
        } else if let Some(stripped) = query.strip_suffix("\\C") {
            (stripped, Some(true)) // force case-sensitive
        } else {
            (query, None)
        };

        if pattern.is_empty() {
            return None;
        }

        // Determine case sensitivity: explicit > smart case (all-lowercase = insensitive)
        let case_sensitive = match force_case {
            Some(sensitive) => sensitive,
            None => pattern.chars().any(|c| c.is_uppercase()),
        };

        let regex_pattern = if case_sensitive {
            pattern.to_string()
        } else {
            format!("(?i){}", pattern)
        };

        // Try compiling as regex; on failure, escape and retry as literal
        match regex::Regex::new(&regex_pattern) {
            Ok(re) => Some(re),
            Err(_) => {
                let escaped = regex::escape(pattern);
                let escaped_pattern = if case_sensitive {
                    escaped
                } else {
                    format!("(?i){}", escaped)
                };
                regex::Regex::new(&escaped_pattern).ok()
            }
        }
    }

    fn update_search_matches(&mut self) {
        self.search_matches.clear();
        self.search_index = None;
        self.search_regex = None;
        if self.search_query.is_empty() {
            return;
        }
        let re = match Self::build_search_regex(&self.search_query) {
            Some(re) => re,
            None => return,
        };
        let line_count = self.document.line_count();
        for row in 0..line_count {
            let line: String = self.document.rope.line(row).to_string();
            // Strip trailing newline for matching
            let text = line.trim_end_matches('\n');
            for m in re.find_iter(text) {
                let match_len = m.as_str().chars().count();
                if match_len == 0 {
                    continue; // skip zero-length matches
                }
                // Convert byte offset to char index
                let col = text[..m.start()].chars().count();
                self.search_matches.push((row, col, match_len));
            }
        }
        self.search_regex = Some(re);
    }

    /// During incremental search, jump to the nearest match from the saved start position.
    fn incremental_jump(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let origin = self.search_start_cursor.unwrap_or(self.cursor);
        let idx = if self.search_forward {
            self.search_matches
                .iter()
                .position(|&(r, c, _)| r > origin.row || (r == origin.row && c >= origin.col))
                .unwrap_or(0)
        } else {
            self.search_matches
                .iter()
                .rposition(|&(r, c, _)| r < origin.row || (r == origin.row && c <= origin.col))
                .unwrap_or_else(|| self.search_matches.len() - 1)
        };
        self.search_index = Some(idx);
        let (row, col, _) = self.search_matches[idx];
        self.cursor.row = row;
        self.cursor.col = col;
        self.clamp_cursor();
    }

    pub fn is_search_match(&self, row: usize, col: usize) -> bool {
        if self.search_query.is_empty() {
            return false;
        }
        self.search_matches
            .iter()
            .any(|&(r, c, len)| r == row && col >= c && col < c + len)
    }

    // --- Phase 9: Repeat last change ---

    pub fn repeat_last_change(&mut self) {
        let change = match &self.last_change {
            Some(c) => c.clone(),
            None => return,
        };

        match change {
            LastChange::NormalCommand(cmd) => match cmd {
                Command::DeleteCharForward => self.delete_char_forward(),
                Command::DeleteCharBackwardNormal => self.delete_char_backward_normal(),
                Command::DeleteLine => self.delete_line(),
                Command::SubstituteChar => self.substitute_char(),
                Command::DeleteMotion(ref m) => self.delete_motion(m),
                Command::IndentLine => self.indent_line(),
                Command::DedentLine => self.dedent_line(),
                Command::JoinLines => self.join_lines(),
                Command::ReplaceChar(ch) => self.replace_char(ch),
                Command::PasteAfter => self.paste_after(),
                Command::PasteBefore => self.paste_before(),
                _ => {}
            },
            LastChange::InsertSession { entry_cmd, chars } => {
                // Execute the entry command
                match entry_cmd {
                    Command::EnterInsertMode => self.enter_insert_mode(),
                    Command::EnterInsertModeAfter => self.enter_insert_mode_after(),
                    Command::EnterInsertModeLineEnd => self.enter_insert_mode_line_end(),
                    Command::EnterInsertModeFirstNonBlank => {
                        self.enter_insert_mode_first_non_blank()
                    }
                    Command::InsertNewlineBelow => self.insert_newline_below(),
                    Command::InsertNewlineAbove => self.insert_newline_above(),
                    Command::ChangeMotion(ref m) => self.change_motion(m),
                    _ => {}
                }
                // Replay typed characters
                for ch in &chars {
                    match *ch {
                        '\x08' => self.delete_char_backward(),
                        '\u{17}' => self.delete_word_backward(),
                        '\u{15}' => self.delete_line_backward(),
                        '\n' => self.insert_newline(),
                        '\t' => self.insert_tab(),
                        c => self.insert_char(c),
                    }
                }
                // Exit insert mode
                self.exit_to_normal_mode();
            }
        }
    }

    // --- Phase 9: Search word under cursor ---

    pub fn word_under_cursor(&self) -> Option<String> {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if self.cursor.col >= line_len {
            return None;
        }
        let ch = line.char(self.cursor.col);
        if !buffer::is_word_char(ch) {
            return None;
        }
        let mut start = self.cursor.col;
        while start > 0 && buffer::is_word_char(line.char(start - 1)) {
            start -= 1;
        }
        let mut end = self.cursor.col;
        while end + 1 < line_len && buffer::is_word_char(line.char(end + 1)) {
            end += 1;
        }
        Some(line.slice(start..=end).to_string())
    }

    pub fn search_word_forward(&mut self) {
        if let Some(word) = self.word_under_cursor() {
            self.search_query = format!("\\b{}\\b", regex::escape(&word));
            self.update_search_matches();
            self.last_search_forward = true;
            self.search_step(true);
        }
    }

    pub fn search_word_backward(&mut self) {
        if let Some(word) = self.word_under_cursor() {
            self.search_query = format!("\\b{}\\b", regex::escape(&word));
            self.update_search_matches();
            self.last_search_forward = false;
            self.search_step(false);
        }
    }

    // --- Phase 9: Matching bracket jump ---

    pub fn match_bracket_jump(&mut self) {
        if let Some(pos) = self.matching_bracket() {
            self.push_jump();
            self.cursor = pos;
        }
    }

    // --- Phase 9: Viewport navigation ---

    pub fn viewport_high(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let map = wrap::build_screen_map_with_tab_width(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
                self.config.tab_width,
            );
            let target = self.config.scroll_off.min(map.len().saturating_sub(1));
            if let Some(seg) = map.get(target) {
                self.cursor.row = seg.doc_row;
                self.cursor.col = seg.char_start;
            }
        } else {
            self.cursor.row = self.view.offset_row + self.config.scroll_off;
        }
        self.clamp_cursor();
    }

    pub fn viewport_middle(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let map = wrap::build_screen_map_with_tab_width(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
                self.config.tab_width,
            );
            let target = map.len() / 2;
            if let Some(seg) = map.get(target) {
                self.cursor.row = seg.doc_row;
                self.cursor.col = seg.char_start;
            }
        } else {
            self.cursor.row = self.view.offset_row + (self.view.height as usize) / 2;
        }
        self.clamp_cursor();
    }

    pub fn viewport_low(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let map = wrap::build_screen_map_with_tab_width(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
                self.config.tab_width,
            );
            let target = map
                .len()
                .saturating_sub(1)
                .saturating_sub(self.config.scroll_off);
            if let Some(seg) = map.get(target) {
                self.cursor.row = seg.doc_row;
                self.cursor.col = seg.char_start;
            }
        } else {
            self.cursor.row = self.view.offset_row + (self.view.height as usize).saturating_sub(1)
                - self.config.scroll_off;
        }
        self.clamp_cursor();
    }

    // --- Phase 9: Scroll positioning ---

    pub fn scroll_center(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let half = (self.view.height as usize) / 2;
            self.scroll_to_cursor_at_screen_row(half, text_width);
        } else {
            let half = (self.view.height as usize) / 2;
            self.view.offset_row = self.cursor.row.saturating_sub(half);
        }
    }

    pub fn scroll_top(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            self.scroll_to_cursor_at_screen_row(0, text_width);
        } else {
            self.view.offset_row = self.cursor.row;
        }
    }

    pub fn scroll_bottom(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let target = (self.view.height as usize).saturating_sub(1);
            self.scroll_to_cursor_at_screen_row(target, text_width);
        } else {
            self.view.offset_row = self
                .cursor
                .row
                .saturating_sub(self.view.height as usize - 1);
        }
    }

    /// Position viewport so cursor appears at the given screen row (wrap-aware).
    fn scroll_to_cursor_at_screen_row(&mut self, target_screen_row: usize, text_width: u16) {
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (cursor_seg, _) = wrap::char_to_wrap_pos_with_tab_width(
            line,
            self.cursor.col,
            text_width,
            self.config.tab_width,
        );

        // Go backwards from cursor by target_screen_row screen lines
        let mut row = self.cursor.row;
        let mut seg = cursor_seg;
        let mut to_go = target_screen_row;

        while to_go > 0 {
            if seg >= to_go {
                seg -= to_go;
                to_go = 0;
            } else {
                to_go -= seg + 1;
                if row > 0 {
                    row -= 1;
                    let prev_line = self.document.rope.line(row);
                    seg = wrap::wrap_count_with_tab_width(
                        prev_line,
                        text_width,
                        self.config.tab_width,
                    ) - 1;
                } else {
                    seg = 0;
                    to_go = 0;
                }
            }
        }

        self.view.offset_row = row;
        self.view.offset_wrap = seg;
    }

    // --- Phase 9: Multi-buffer ---

    fn save_to_current_buffer(&mut self) {
        let buf = &mut self.buffers[self.current_buffer];
        buf.document = std::mem::replace(&mut self.document, Document::new_empty());
        buf.cursor = self.cursor;
        buf.view = self.view;
        buf.history = std::mem::take(&mut self.history);
        buf.syntax_language_override = self.syntax_language_override;
        buf.syntax_tree = self.syntax_tree.take();
        buf.line_styles = std::mem::take(&mut self.line_styles);
        buf.styles_offset = self.styles_offset;
        buf.diagnostics = std::mem::take(&mut self.diagnostics);
        buf.search_query = std::mem::take(&mut self.search_query);
        buf.search_matches = std::mem::take(&mut self.search_matches);
        buf.search_index = self.search_index;
        buf.search_regex = self.search_regex.take();
        buf.jump_list = std::mem::take(&mut self.jump_list);
        buf.jump_index = self.jump_index;
    }

    fn load_from_buffer(&mut self, idx: usize) {
        let buf = &mut self.buffers[idx];
        self.document = std::mem::replace(&mut buf.document, Document::new_empty());
        self.cursor = buf.cursor;
        self.view = buf.view;
        self.history = std::mem::take(&mut buf.history);
        self.syntax_language_override = buf.syntax_language_override;
        self.syntax_tree = buf.syntax_tree.take();
        self.line_styles = std::mem::take(&mut buf.line_styles);
        self.styles_offset = buf.styles_offset;
        self.diagnostics = std::mem::take(&mut buf.diagnostics);
        self.search_query = std::mem::take(&mut buf.search_query);
        self.search_matches = std::mem::take(&mut buf.search_matches);
        self.search_index = buf.search_index;
        self.search_regex = buf.search_regex.take();
        self.jump_list = std::mem::take(&mut buf.jump_list);
        self.jump_index = buf.jump_index;
        self.current_buffer = idx;
    }

    pub fn switch_buffer(&mut self, idx: usize) -> Option<DeferredAction> {
        if idx >= self.buffers.len() || idx == self.current_buffer {
            return None;
        }
        self.save_to_current_buffer();
        self.load_from_buffer(idx);
        // Update active pane's buffer_idx
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == self.active_pane_id) {
            pane.buffer_idx = idx;
        }
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.pending_keys.clear();
        Some(DeferredAction::SyncFileUri)
    }

    pub fn add_buffer(&mut self, doc: Document) -> usize {
        self.save_to_current_buffer();
        let new_idx = self.buffers.len();
        self.buffers.push(BufferState::empty());
        self.current_buffer = new_idx;
        // Update active pane's buffer_idx
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == self.active_pane_id) {
            pane.buffer_idx = new_idx;
        }
        self.document = doc;
        self.cursor = Position::default();
        self.view = View::default();
        self.history = History::new();
        self.syntax_language_override = None;
        self.syntax_tree = None;
        self.line_styles = Vec::new();
        self.styles_offset = 0;
        self.diagnostics = Vec::new();
        self.search_query = String::new();
        self.search_matches = Vec::new();
        self.search_index = None;
        self.search_regex = None;
        self.jump_list = Vec::new();
        self.jump_index = 0;
        new_idx
    }

    pub fn replace_document_text(&mut self, text: &str) {
        if self.document.rope.to_string() == text {
            return;
        }
        self.history.save(&self.document.rope, self.cursor);
        let len = self.document.rope.len_chars();
        self.document.rope.remove(0..len);
        self.document.rope.insert(0, text);
        self.document.modified = true;
        self.document.bump_version();
        self.syntax_tree = None;
        self.line_styles.clear();
        self.styles_offset = 0;
        self.clamp_cursor();
    }

    pub fn next_buffer(&mut self) -> Option<DeferredAction> {
        if self.buffers.len() <= 1 {
            return None;
        }
        let next = (self.current_buffer + 1) % self.buffers.len();
        self.switch_buffer(next)
    }

    pub fn prev_buffer(&mut self) -> Option<DeferredAction> {
        if self.buffers.len() <= 1 {
            return None;
        }
        let prev = if self.current_buffer == 0 {
            self.buffers.len() - 1
        } else {
            self.current_buffer - 1
        };
        self.switch_buffer(prev)
    }

    pub fn close_buffer(&mut self) -> Option<DeferredAction> {
        if self.document.modified {
            self.status_message =
                Some("No write since last change (add ! to override)".to_string());
            return None;
        }
        self.close_buffer_force()
    }

    pub fn close_buffer_force(&mut self) -> Option<DeferredAction> {
        if self.buffers.len() <= 1 {
            self.should_quit = true;
            return None;
        }
        let removed_idx = self.current_buffer;
        self.buffers.remove(removed_idx);
        // Adjust all pane buffer indices after the removed buffer
        for pane in &mut self.panes {
            if pane.buffer_idx > removed_idx {
                pane.buffer_idx -= 1;
            } else if pane.buffer_idx == removed_idx {
                // Pane was pointing to the removed buffer, reassign
                pane.buffer_idx = if removed_idx >= self.buffers.len() {
                    self.buffers.len() - 1
                } else {
                    removed_idx
                };
            }
        }
        let new_idx = if removed_idx >= self.buffers.len() {
            self.buffers.len() - 1
        } else {
            removed_idx
        };
        self.load_from_buffer(new_idx);
        // Update active pane's buffer_idx
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == self.active_pane_id) {
            pane.buffer_idx = new_idx;
        }
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.pending_keys.clear();
        Some(DeferredAction::SyncFileUri)
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Returns (filename, modified, is_current) for each buffer.
    pub fn buffer_info(&self) -> Vec<(String, bool, bool)> {
        self.buffers
            .iter()
            .enumerate()
            .map(|(i, buf)| {
                if i == self.current_buffer {
                    (
                        self.document.file_name().to_string(),
                        self.document.modified,
                        true,
                    )
                } else {
                    (
                        buf.document.file_name().to_string(),
                        buf.document.modified,
                        false,
                    )
                }
            })
            .collect()
    }

    pub fn find_buffer_by_path(&self, path: &std::path::Path) -> Option<usize> {
        if let Some(p) = &self.document.path
            && p == path
        {
            return Some(self.current_buffer);
        }
        for (i, buf) in self.buffers.iter().enumerate() {
            if i == self.current_buffer {
                continue;
            }
            if let Some(p) = &buf.document.path
                && p == path
            {
                return Some(i);
            }
        }
        None
    }

    // --- Phase 9: Command history ---

    pub fn command_history_prev(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        match self.command_history_idx {
            None => {
                self.command_history_temp = self.command_buffer.clone();
                let last = self.command_history.len() - 1;
                self.command_history_idx = Some(last);
                self.command_buffer = self.command_history[last].clone();
            }
            Some(0) => {}
            Some(idx) => {
                self.command_history_idx = Some(idx - 1);
                self.command_buffer = self.command_history[idx - 1].clone();
            }
        }
    }

    pub fn command_history_next(&mut self) {
        match self.command_history_idx {
            None => {}
            Some(idx) => {
                if idx + 1 >= self.command_history.len() {
                    self.command_history_idx = None;
                    self.command_buffer = std::mem::take(&mut self.command_history_temp);
                } else {
                    self.command_history_idx = Some(idx + 1);
                    self.command_buffer = self.command_history[idx + 1].clone();
                }
            }
        }
    }

    // --- Phase 10: Named registers ---

    /// Consume the selected register (or default to unnamed `"`).
    fn consume_register(&mut self) -> char {
        self.selected_register.take().unwrap_or('"')
    }

    fn preview_text(content: &str, max_chars: usize) -> String {
        if content.is_empty() {
            return "(empty)".to_string();
        }
        let mut out = String::new();
        for ch in content.chars().take(max_chars) {
            match ch {
                '\n' | '\r' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
        if content.chars().count() > max_chars {
            out.push_str("...");
        }
        out
    }

    fn preview_key(key: &KeyInput) -> String {
        let base = match key.code {
            KeyCode::Char(ch) => ch.to_string(),
            KeyCode::Esc => "<Esc>".to_string(),
            KeyCode::Enter => "<CR>".to_string(),
            KeyCode::Backspace => "<BS>".to_string(),
            KeyCode::Tab => "<Tab>".to_string(),
            KeyCode::BackTab => "<S-Tab>".to_string(),
            KeyCode::Up => "<Up>".to_string(),
            KeyCode::Down => "<Down>".to_string(),
            KeyCode::Left => "<Left>".to_string(),
            KeyCode::Right => "<Right>".to_string(),
        };
        match (key.ctrl, key.alt) {
            (true, true) => format!("<C-M-{base}>"),
            (true, false) => format!("<C-{base}>"),
            (false, true) => format!("<M-{base}>"),
            (false, false) => base,
        }
    }

    fn preview_macro(keys: &[KeyInput]) -> String {
        if keys.is_empty() {
            return "(empty)".to_string();
        }
        let mut out = String::new();
        for key in keys.iter().take(12) {
            out.push_str(&Self::preview_key(key));
        }
        if keys.len() > 12 {
            out.push_str("...");
        }
        out
    }

    fn register_overview(&self) -> String {
        let mut entries = Vec::new();
        let mut registers: Vec<_> = self
            .registers
            .iter()
            .filter(|(_, reg)| !reg.content.is_empty())
            .collect();
        registers.sort_by_key(|(name, _)| **name);
        for (name, reg) in registers {
            entries.push(format!(
                "\"{}:{}",
                name,
                Self::preview_text(&reg.content, 24)
            ));
        }

        let mut macros: Vec<_> = self
            .macros
            .iter()
            .filter(|(_, keys)| !keys.is_empty())
            .collect();
        macros.sort_by_key(|(name, _)| **name);
        for (name, keys) in macros {
            entries.push(format!("@{}:{}", name, Self::preview_macro(keys)));
        }

        if entries.is_empty() {
            "(no registers)".to_string()
        } else {
            entries.join("  ")
        }
    }

    pub fn register_display(&self) -> Option<String> {
        match self.pending_keys.as_slice() {
            ['"'] => return Some(format!("select register: {}", self.register_overview())),
            ['q'] => return Some(format!("record macro in: {}", self.register_overview())),
            _ => {}
        }
        if let Some(ch) = self.selected_register {
            let preview = self
                .registers
                .get(&ch.to_ascii_lowercase())
                .map(|reg| Self::preview_text(&reg.content, 40))
                .unwrap_or_else(|| "(empty)".to_string());
            return Some(format!("\"{} {}", ch, preview));
        }
        None
    }

    /// Store content into a register, also updating the unnamed register.
    pub fn store_register(&mut self, name: char, content: String, linewise: bool) {
        if name == '_' {
            return;
        }
        self.write_register('"', content.clone(), linewise, false);
        if name != '"' {
            let append = name.is_ascii_uppercase();
            self.write_register(name.to_ascii_lowercase(), content, linewise, append);
        }
    }

    fn write_register(&mut self, name: char, content: String, linewise: bool, append: bool) {
        let (content, linewise) = if append {
            if let Some(existing) = self.registers.get(&name) {
                (
                    format!("{}{}", existing.content, content),
                    existing.linewise || linewise,
                )
            } else {
                (content, linewise)
            }
        } else {
            (content, linewise)
        };
        self.registers.insert(
            name,
            Register {
                content: content.clone(),
                linewise,
            },
        );
        if name == '+' || name == '*' {
            clipboard_set(&content);
        }
    }

    fn store_operation_register(&mut self, content: String, linewise: bool, op: RegisterOp) {
        let selected = self.selected_register.take();
        if selected == Some('_') {
            return;
        }
        self.write_register('"', content.clone(), linewise, false);
        if let Some(name) = selected {
            if name != '"' {
                let append = name.is_ascii_uppercase();
                self.write_register(name.to_ascii_lowercase(), content, linewise, append);
            }
            return;
        }
        match op {
            RegisterOp::Yank => self.write_register('0', content, linewise, false),
            RegisterOp::Delete => {
                if linewise || content.contains('\n') {
                    for n in (2..=9).rev() {
                        let prev = char::from_digit(n - 1, 10).unwrap();
                        let dest = char::from_digit(n, 10).unwrap();
                        if let Some(reg) = self.registers.get(&prev).cloned() {
                            self.write_register(dest, reg.content, reg.linewise, false);
                        }
                    }
                    self.write_register('1', content, linewise, false);
                } else {
                    self.write_register('-', content, linewise, false);
                }
            }
        }
    }

    fn store_yank_register(&mut self, content: String, linewise: bool) {
        self.store_operation_register(content, linewise, RegisterOp::Yank);
    }

    fn store_delete_register(&mut self, content: String, linewise: bool) {
        self.store_operation_register(content, linewise, RegisterOp::Delete);
    }

    /// Read from a register.
    pub fn read_register(&mut self, name: char) -> Option<Register> {
        // System clipboard
        if (name == '+' || name == '*')
            && let Some(text) = clipboard_get()
        {
            let reg = Register {
                linewise: text.ends_with('\n'),
                content: text,
            };
            return Some(reg);
        }
        self.registers.get(&name.to_ascii_lowercase()).cloned()
    }

    // --- Phase 10: Case change ---

    pub fn toggle_case_char(&mut self) {
        // In visual mode, toggle case of selection
        if self.mode.is_visual() {
            self.case_change_visual(crate::input::command::CaseOp::Toggle);
            return;
        }
        let line_len = self.document.line_len(self.cursor.row);
        if self.cursor.col >= line_len {
            return;
        }
        self.save_undo();
        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        let ch = self.document.rope.char(idx);
        let toggled: char = if ch.is_uppercase() {
            ch.to_lowercase().next().unwrap_or(ch)
        } else {
            ch.to_uppercase().next().unwrap_or(ch)
        };
        self.document.rope.remove(idx..idx + 1);
        self.document.rope.insert_char(idx, toggled);
        self.document.modified = true;
        self.document.bump_version();
        // Move right (vim behavior)
        if self.cursor.col + 1 < self.document.line_len(self.cursor.row) {
            self.cursor.col += 1;
        }
    }

    pub fn case_change(&mut self, op: crate::input::command::CaseOp, motion: &Motion) {
        self.save_undo();
        if let Some((start, end)) = self.motion_range(motion) {
            self.apply_case_change(start, end, op);
            self.reposition_cursor_to(start);
        }
        self.clamp_cursor();
    }

    pub fn case_change_line(&mut self, op: crate::input::command::CaseOp) {
        // In visual mode, apply to selection
        if self.mode.is_visual() {
            self.case_change_visual(op);
            return;
        }
        self.save_undo();
        let line_start = self.document.rope.line_to_char(self.cursor.row);
        let line_len = self.document.line_len(self.cursor.row);
        let line_end = line_start + line_len;
        self.apply_case_change(line_start, line_end, op);
    }

    fn case_change_visual(&mut self, op: crate::input::command::CaseOp) {
        if let Some((start_row, end_row, start_col, end_col)) = self.visual_block_range() {
            self.save_undo();
            for row in start_row..=end_row.min(self.document.line_count().saturating_sub(1)) {
                let line_len = self.document.line_len(row);
                if start_col >= line_len {
                    continue;
                }
                let line_start = self.document.rope.line_to_char(row);
                let end = (end_col + 1).min(line_len);
                self.apply_case_change(line_start + start_col, line_start + end, op);
            }
            self.cursor = Position {
                row: start_row,
                col: start_col,
            };
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
            return;
        }
        if let Some((start, end)) = self.selection_range() {
            self.save_undo();
            let linewise = self.mode == Mode::VisualLine;
            let (start_idx, end_idx) = if linewise {
                let s = self.document.rope.line_to_char(start.row);
                let e = if end.row + 1 < self.document.line_count() {
                    self.document.rope.line_to_char(end.row + 1)
                } else {
                    self.document.rope.len_chars()
                };
                (s, e)
            } else {
                let s = self.document.rope.line_to_char(start.row) + start.col;
                let e_col = end.col.min(self.document.line_len(end.row));
                let e = self.document.rope.line_to_char(end.row) + e_col + 1;
                (s, e.min(self.document.rope.len_chars()))
            };
            self.apply_case_change(start_idx, end_idx, op);
            self.cursor = start;
            self.mode = Mode::Normal;
            self.visual_anchor = None;
            self.clamp_cursor();
        }
    }

    fn apply_case_change(&mut self, start: usize, end: usize, op: crate::input::command::CaseOp) {
        use crate::input::command::CaseOp;
        let end = end.min(self.document.rope.len_chars());
        if start >= end {
            return;
        }
        let text: String = self.document.rope.slice(start..end).to_string();
        let changed: String = match op {
            CaseOp::Lower => text.to_lowercase(),
            CaseOp::Upper => text.to_uppercase(),
            CaseOp::Toggle => text
                .chars()
                .map(|c| {
                    if c.is_uppercase() {
                        c.to_lowercase().next().unwrap_or(c)
                    } else {
                        c.to_uppercase().next().unwrap_or(c)
                    }
                })
                .collect(),
        };
        if changed != text {
            self.document.rope.remove(start..end);
            self.document.rope.insert(start, &changed);
            self.document.modified = true;
            self.document.bump_version();
        }
    }

    // --- Phase 10: Number increment/decrement ---

    pub fn increment_number(&mut self, delta: i64) {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if line_len == 0 {
            return;
        }

        // Find number at or after cursor on current line
        let line_str: String = line.to_string();
        let mut num_start = None;
        let mut num_end = 0;
        // Search from cursor position forward
        for start in self.cursor.col..line_str.len() {
            let ch = line_str.as_bytes()[start] as char;
            if ch.is_ascii_digit() {
                // Check for negative sign
                let negative = start > 0 && line_str.as_bytes()[start - 1] == b'-';
                num_start = Some(if negative { start - 1 } else { start });
                num_end = start + 1;
                while num_end < line_str.len()
                    && (line_str.as_bytes()[num_end] as char).is_ascii_digit()
                {
                    num_end += 1;
                }
                break;
            }
        }

        let num_start = match num_start {
            Some(s) => s,
            None => return,
        };

        let num_str = &line_str[num_start..num_end];
        if let Ok(num) = num_str.parse::<i64>() {
            let new_num = num + delta;
            let new_str = new_num.to_string();

            self.save_undo();
            let line_char_start = self.document.rope.line_to_char(self.cursor.row);
            let abs_start = line_char_start + num_start;
            let abs_end = line_char_start + num_end;
            self.document.rope.remove(abs_start..abs_end);
            self.document.rope.insert(abs_start, &new_str);
            self.document.modified = true;
            self.document.bump_version();
            // Position cursor on last digit of new number
            self.cursor.col = num_start + new_str.len() - 1;
            self.clamp_cursor();
        }
    }

    // --- Phase 10: Macro recording ---

    pub fn start_macro(&mut self, reg: char) {
        self.recording_macro = Some(reg);
        self.macro_buffer.clear();
        self.status_message = Some(format!("recording @{reg}"));
    }

    pub fn stop_macro(&mut self) {
        if let Some(reg) = self.recording_macro.take() {
            self.macros.insert(reg, self.macro_buffer.clone());
            self.macro_buffer.clear();
            self.status_message = Some("recorded".to_string());
        }
    }

    // --- Extended movement ---

    pub fn goto_top(&mut self) {
        self.push_jump();
        self.cursor.row = 0;
        self.cursor.col = 0;
    }

    pub fn goto_bottom(&mut self) {
        self.push_jump();
        self.cursor.row = self.document.line_count().saturating_sub(1);
        self.cursor.col = 0;
        self.clamp_cursor();
    }

    pub fn goto_line(&mut self, line: usize) {
        self.push_jump();
        self.cursor.row = line
            .saturating_sub(1)
            .min(self.document.line_count().saturating_sub(1));
        self.cursor.col = 0;
        self.clamp_cursor();
    }

    pub fn half_page_down(&mut self) {
        if self.config.wrap {
            self.move_screen_lines_down((self.view.height as usize) / 2);
        } else {
            let half = (self.view.height as usize) / 2;
            let max_row = self.document.line_count().saturating_sub(1);
            self.cursor.row = (self.cursor.row + half).min(max_row);
            self.clamp_cursor();
        }
    }

    pub fn half_page_up(&mut self) {
        if self.config.wrap {
            self.move_screen_lines_up((self.view.height as usize) / 2);
        } else {
            let half = (self.view.height as usize) / 2;
            self.cursor.row = self.cursor.row.saturating_sub(half);
            self.clamp_cursor();
        }
    }

    pub fn scroll_viewport_down(&mut self, n: usize) {
        if self.config.wrap {
            let text_width = self.text_width();
            self.view
                .scroll_down_by(n, &self.document.rope, text_width, self.config.tab_width);
        } else {
            let max = self
                .document
                .line_count()
                .saturating_sub(self.view.height as usize);
            self.view.offset_row = (self.view.offset_row + n).min(max);
        }
        self.scroll();
        self.clamp_cursor();
    }

    pub fn scroll_viewport_up(&mut self, n: usize) {
        if self.config.wrap {
            let text_width = self.text_width();
            self.view
                .scroll_up_by(n, &self.document.rope, text_width, self.config.tab_width);
        } else {
            self.view.offset_row = self.view.offset_row.saturating_sub(n);
        }
        self.scroll();
        self.clamp_cursor();
    }

    pub fn full_page_down(&mut self) {
        if self.config.wrap {
            self.move_screen_lines_down(self.view.height as usize);
        } else {
            let page = self.view.height as usize;
            let max_row = self.document.line_count().saturating_sub(1);
            self.cursor.row = (self.cursor.row + page).min(max_row);
            self.clamp_cursor();
        }
    }

    pub fn full_page_up(&mut self) {
        if self.config.wrap {
            self.move_screen_lines_up(self.view.height as usize);
        } else {
            let page = self.view.height as usize;
            self.cursor.row = self.cursor.row.saturating_sub(page);
            self.clamp_cursor();
        }
    }

    /// Move cursor down by `n` screen lines (wrap-aware).
    fn move_screen_lines_down(&mut self, n: usize) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (mut seg, col_in_seg) = wrap::char_to_wrap_pos_with_tab_width(
            line,
            self.cursor.col,
            text_width,
            self.config.tab_width,
        );
        let mut row = self.cursor.row;
        let max_row = self.document.line_count().saturating_sub(1);
        let mut remaining = n;

        while remaining > 0 {
            let cur_line = self.document.rope.line(row);
            let wc = wrap::wrap_count_with_tab_width(cur_line, text_width, self.config.tab_width);
            let segs_avail = wc - seg - 1;
            if segs_avail >= remaining {
                seg += remaining;
                remaining = 0;
            } else {
                remaining -= segs_avail + 1;
                if row < max_row {
                    row += 1;
                    seg = 0;
                } else {
                    seg = wc - 1;
                    remaining = 0;
                }
            }
        }

        self.cursor.row = row;
        let target_line = self.document.rope.line(row);
        self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
            target_line,
            seg,
            col_in_seg,
            text_width,
            self.config.tab_width,
        );
        self.clamp_cursor();
    }

    /// Move cursor up by `n` screen lines (wrap-aware).
    fn move_screen_lines_up(&mut self, n: usize) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (mut seg, col_in_seg) = wrap::char_to_wrap_pos_with_tab_width(
            line,
            self.cursor.col,
            text_width,
            self.config.tab_width,
        );
        let mut row = self.cursor.row;
        let mut remaining = n;

        while remaining > 0 {
            if seg >= remaining {
                seg -= remaining;
                remaining = 0;
            } else {
                remaining -= seg + 1;
                if row > 0 {
                    row -= 1;
                    let prev_line = self.document.rope.line(row);
                    seg = wrap::wrap_count_with_tab_width(
                        prev_line,
                        text_width,
                        self.config.tab_width,
                    ) - 1;
                } else {
                    seg = 0;
                    remaining = 0;
                }
            }
        }

        self.cursor.row = row;
        let target_line = self.document.rope.line(row);
        self.cursor.col = wrap::wrap_pos_to_char_with_tab_width(
            target_line,
            seg,
            col_in_seg,
            text_width,
            self.config.tab_width,
        );
        self.clamp_cursor();
    }

    // --- File finder ---

    pub fn open_file_finder(&mut self, entries: Vec<String>) {
        self.file_finder_entries = entries;
        self.file_finder_query.clear();
        self.file_finder_filtered = self.file_finder_entries.clone();
        self.file_finder_index = 0;
        self.showing_file_finder = true;
    }

    pub fn file_finder_input(&mut self, ch: char) {
        self.file_finder_query.push(ch);
        self.update_file_finder_filter();
    }

    pub fn file_finder_backspace(&mut self) {
        self.file_finder_query.pop();
        self.update_file_finder_filter();
    }

    pub fn file_finder_next(&mut self) {
        if !self.file_finder_filtered.is_empty() {
            self.file_finder_index = (self.file_finder_index + 1) % self.file_finder_filtered.len();
        }
    }

    pub fn file_finder_prev(&mut self) {
        if !self.file_finder_filtered.is_empty() {
            self.file_finder_index = if self.file_finder_index == 0 {
                self.file_finder_filtered.len() - 1
            } else {
                self.file_finder_index - 1
            };
        }
    }

    pub fn file_finder_cancel(&mut self) {
        self.showing_file_finder = false;
        self.file_finder_query.clear();
        self.file_finder_filtered.clear();
        self.file_finder_index = 0;
    }

    pub fn file_finder_selected(&self) -> Option<String> {
        if self.file_finder_filtered.is_empty() {
            None
        } else {
            Some(self.file_finder_filtered[self.file_finder_index].clone())
        }
    }

    fn update_file_finder_filter(&mut self) {
        self.file_finder_filtered =
            search::filter_file_paths(&self.file_finder_entries, &self.file_finder_query, 0)
                .into_iter()
                .map(|result| result.path)
                .collect();
        self.file_finder_index = 0;
    }

    // --- Workspace symbol search ---

    pub fn open_workspace_symbols(&mut self) {
        self.workspace_symbol_query.clear();
        self.workspace_symbol_results.clear();
        self.workspace_symbol_index = 0;
        self.workspace_symbol_needs_request = false;
        self.showing_workspace_symbols = true;
    }

    pub fn workspace_symbol_input(&mut self, ch: char) {
        self.workspace_symbol_query.push(ch);
        self.workspace_symbol_needs_request = true;
    }

    pub fn workspace_symbol_backspace(&mut self) {
        self.workspace_symbol_query.pop();
        self.workspace_symbol_needs_request = true;
    }

    pub fn workspace_symbol_next(&mut self) {
        if !self.workspace_symbol_results.is_empty() {
            self.workspace_symbol_index =
                (self.workspace_symbol_index + 1) % self.workspace_symbol_results.len();
        }
    }

    pub fn workspace_symbol_prev(&mut self) {
        if !self.workspace_symbol_results.is_empty() {
            self.workspace_symbol_index = if self.workspace_symbol_index == 0 {
                self.workspace_symbol_results.len() - 1
            } else {
                self.workspace_symbol_index - 1
            };
        }
    }

    pub fn workspace_symbol_selected(&self) -> Option<lsp::LspSymbolInfo> {
        if self.workspace_symbol_results.is_empty() {
            None
        } else {
            Some(self.workspace_symbol_results[self.workspace_symbol_index].clone())
        }
    }

    pub fn workspace_symbol_cancel(&mut self) {
        self.showing_workspace_symbols = false;
        self.workspace_symbol_query.clear();
        self.workspace_symbol_results.clear();
        self.workspace_symbol_index = 0;
        self.workspace_symbol_needs_request = false;
    }

    /// Get diagnostic message for the current cursor line.
    pub fn diagnostic_at_cursor(&self) -> Option<&str> {
        for d in &self.diagnostics {
            if d.start_line as usize <= self.cursor.row && self.cursor.row <= d.end_line as usize {
                return Some(&d.message);
            }
        }
        None
    }

    // --- Pane management ---

    /// Save the current editor state into the active pane.
    pub fn save_active_pane(&mut self) {
        // Save document + diagnostics to the buffer
        let buf = &mut self.buffers[self.current_buffer];
        buf.document = std::mem::replace(&mut self.document, Document::new_empty());
        buf.diagnostics = std::mem::take(&mut self.diagnostics);
        buf.syntax_language_override = self.syntax_language_override;

        // Save pane-specific state (cursor, view, history, highlights, search, jump)
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == self.active_pane_id) {
            pane.buffer_idx = self.current_buffer;
            pane.cursor = self.cursor;
            pane.view = self.view;
            pane.history = std::mem::take(&mut self.history);
            pane.syntax_tree = self.syntax_tree.take();
            pane.line_styles = std::mem::take(&mut self.line_styles);
            pane.styles_offset = self.styles_offset;
            pane.search_query = std::mem::take(&mut self.search_query);
            pane.search_matches = std::mem::take(&mut self.search_matches);
            pane.search_index = self.search_index;
            pane.search_regex = self.search_regex.take();
            pane.search_start_cursor = self.search_start_cursor.take();
            pane.jump_list = std::mem::take(&mut self.jump_list);
            pane.jump_index = self.jump_index;
        }
    }

    /// Load pane state into the editor's active fields.
    pub fn load_pane(&mut self, pane_id: usize) {
        self.active_pane_id = pane_id;
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == pane_id) {
            let buffer_idx = pane.buffer_idx;

            // Load document + diagnostics from buffer (but NOT cursor/view/history —
            // those are pane-specific, not buffer-specific)
            let buf = &mut self.buffers[buffer_idx];
            self.document = std::mem::replace(&mut buf.document, Document::new_empty());
            self.diagnostics = std::mem::take(&mut buf.diagnostics);
            self.syntax_language_override = buf.syntax_language_override;
            self.current_buffer = buffer_idx;

            // Load pane-specific state (cursor, view, history, highlights, search, jump)
            self.cursor = pane.cursor;
            self.view = pane.view;
            self.config.wrap = pane.view.wrap;
            self.history = std::mem::take(&mut pane.history);
            self.syntax_tree = pane.syntax_tree.take();
            self.line_styles = std::mem::take(&mut pane.line_styles);
            self.styles_offset = pane.styles_offset;
            self.search_query = std::mem::take(&mut pane.search_query);
            self.search_matches = std::mem::take(&mut pane.search_matches);
            self.search_index = pane.search_index;
            self.search_regex = pane.search_regex.take();
            self.search_start_cursor = pane.search_start_cursor.take();
            self.jump_list = std::mem::take(&mut pane.jump_list);
            self.jump_index = pane.jump_index;
        }
    }

    pub fn split_pane(&mut self, direction: SplitDirection) -> Option<DeferredAction> {
        self.save_active_pane();

        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        // Create new pane with same buffer, cursor, and view as current
        let current_pane = self.panes.iter().find(|p| p.id == self.active_pane_id);
        let (buffer_idx, cursor, view) = match current_pane {
            Some(p) => (p.buffer_idx, p.cursor, p.view),
            None => (self.current_buffer, self.cursor, self.view),
        };

        let mut new_pane = Pane::new(new_id, buffer_idx);
        new_pane.cursor = cursor;
        new_pane.view = view;
        self.panes.push(new_pane);

        self.pane_layout
            .split(self.active_pane_id, new_id, direction);
        self.load_pane(new_id);
        None
    }

    pub fn close_pane(&mut self) -> Option<DeferredAction> {
        if self.pane_layout.is_single() {
            // Only one pane, behave like :q
            return self.close_buffer();
        }
        self.save_active_pane();
        let old_id = self.active_pane_id;
        self.pane_layout.remove(old_id);
        self.panes.retain(|p| p.id != old_id);

        // Load an adjacent pane
        let leaves = self.pane_layout.leaves();
        if let Some(&next_id) = leaves.first() {
            self.load_pane(next_id);
        }
        None
    }

    pub fn navigate_pane(&mut self, dir: NavigateDir) -> Option<DeferredAction> {
        let rects = self.pane_layout.layout(self.editor_area);
        if let Some(target_id) = self
            .pane_layout
            .find_adjacent(self.active_pane_id, dir, &rects)
        {
            self.save_active_pane();
            self.load_pane(target_id);
        }
        None
    }

    pub fn cycle_pane(&mut self) -> Option<DeferredAction> {
        let leaves = self.pane_layout.leaves();
        if leaves.len() <= 1 {
            return None;
        }
        let current_pos = leaves
            .iter()
            .position(|&id| id == self.active_pane_id)
            .unwrap_or(0);
        let next_pos = (current_pos + 1) % leaves.len();
        let next_id = leaves[next_pos];
        self.save_active_pane();
        self.load_pane(next_id);
        None
    }

    pub fn pane_only(&mut self) -> Option<DeferredAction> {
        let active = self.active_pane_id;
        self.save_active_pane();
        self.panes.retain(|p| p.id == active);
        self.pane_layout = PaneNode::Leaf(active);
        self.load_pane(active);
        None
    }

    pub fn pane_equalize(&mut self) -> Option<DeferredAction> {
        None
    }

    pub fn pane_rotate_forward(&mut self) -> Option<DeferredAction> {
        self.cycle_pane()
    }

    pub fn pane_rotate_backward(&mut self) -> Option<DeferredAction> {
        let leaves = self.pane_layout.leaves();
        if leaves.len() <= 1 {
            return None;
        }
        let current_pos = leaves
            .iter()
            .position(|&id| id == self.active_pane_id)
            .unwrap_or(0);
        let prev_pos = if current_pos == 0 {
            leaves.len() - 1
        } else {
            current_pos - 1
        };
        self.save_active_pane();
        self.load_pane(leaves[prev_pos]);
        None
    }

    pub fn pane_move_left(&mut self) -> Option<DeferredAction> {
        self.navigate_pane(NavigateDir::Left)
    }

    pub fn pane_move_down(&mut self) -> Option<DeferredAction> {
        self.navigate_pane(NavigateDir::Down)
    }

    pub fn pane_move_up(&mut self) -> Option<DeferredAction> {
        self.navigate_pane(NavigateDir::Up)
    }

    pub fn pane_move_right(&mut self) -> Option<DeferredAction> {
        self.navigate_pane(NavigateDir::Right)
    }

    pub fn pane_resize_wider(&mut self) {}
    pub fn pane_resize_narrower(&mut self) {}
    pub fn pane_resize_taller(&mut self) {}
    pub fn pane_resize_shorter(&mut self) {}

    pub fn has_splits(&self) -> bool {
        !self.pane_layout.is_single()
    }

    /// Execute a command-mode command.
    /// Returns a deferred action for app.rs to handle async operations.
    pub fn command_execute(&mut self) -> Option<DeferredAction> {
        let cmd = self.command_buffer.clone();
        self.mode = Mode::Normal;
        self.command_buffer.clear();
        self.command_history_idx = None;
        self.command_history_temp.clear();

        let trimmed = cmd.trim();

        // Save to command history
        if !trimmed.is_empty() {
            self.command_history.push(trimmed.to_string());
            if self.command_history.len() > 100 {
                self.command_history.remove(0);
            }
        }

        // Handle `:!command` (shell command)
        if let Some(shell_cmd) = trimmed.strip_prefix('!') {
            let shell_cmd = shell_cmd.trim().to_string();
            if shell_cmd.is_empty() {
                self.status_message = Some("Usage: :!<command>".to_string());
                return None;
            }
            return Some(DeferredAction::ShellCommand(shell_cmd));
        }

        // Handle `:format`
        if trimmed == "format" || trimmed == "fmt" {
            return Some(DeferredAction::FormatDocument);
        }

        if trimmed == "gd" || trimmed == "gdiff" {
            return Some(DeferredAction::Git(GitEditorCommand::Diff {
                mode: GitDiffMode::Worktree,
                paths: Vec::new(),
            }));
        }
        if trimmed == "gds" || trimmed == "gdiff --staged" || trimmed == "gdiff --cached" {
            return Some(DeferredAction::Git(GitEditorCommand::Diff {
                mode: GitDiffMode::Staged,
                paths: Vec::new(),
            }));
        }
        if let Some(git_cmd) = trimmed.strip_prefix("git ") {
            return self.execute_git_command(git_cmd);
        }

        // Handle `:rename <new_name>`
        if let Some(new_name) = trimmed.strip_prefix("rename ") {
            let new_name = new_name.trim().to_string();
            if new_name.is_empty() {
                self.status_message = Some("Usage: :rename <new_name>".to_string());
                return None;
            }
            return Some(DeferredAction::Rename(new_name));
        }

        // Handle `:%s/old/new/g` or `:s/old/new`
        if trimmed.starts_with("%s/") || trimmed.starts_with("s/") {
            return self.execute_substitute(trimmed);
        }

        // Handle `:split <file>` and `:vsplit <file>`
        if let Some(path) = trimmed
            .strip_prefix("split ")
            .or_else(|| trimmed.strip_prefix("sp "))
        {
            let path = path.trim().to_string();
            if !path.is_empty() {
                self.split_pane(SplitDirection::Horizontal);
                return Some(DeferredAction::OpenFile(path));
            }
        }
        if let Some(path) = trimmed
            .strip_prefix("vsplit ")
            .or_else(|| trimmed.strip_prefix("vs "))
        {
            let path = path.trim().to_string();
            if !path.is_empty() {
                self.split_pane(SplitDirection::Vertical);
                return Some(DeferredAction::OpenFile(path));
            }
        }

        // Handle `:e <file>` to open a file
        if let Some(path) = trimmed.strip_prefix("e ") {
            let path = path.trim().to_string();
            if path.is_empty() {
                self.status_message = Some("Usage: :e <file>".to_string());
                return None;
            }
            return Some(DeferredAction::OpenFile(path));
        }

        // Handle `:N` for line jumping
        if let Ok(line_num) = trimmed.parse::<usize>() {
            if line_num > 0 {
                self.cursor.row = (line_num - 1).min(self.document.line_count().saturating_sub(1));
                self.cursor.col = 0;
                self.clamp_cursor();
            }
            return None;
        }

        match trimmed {
            "noh" | "nohlsearch" => {
                self.search_matches.clear();
                self.search_index = None;
                self.search_regex = None;
                self.status_message = Some("search highlighting cleared".to_string());
            }
            "diagnostics" | "diags" => {
                self.toggle_diagnostics_list();
            }
            "registers" | "reg" => {
                let mut names: Vec<char> = self.registers.keys().copied().collect();
                names.sort_unstable();
                let summary = names
                    .into_iter()
                    .map(|name| format!("\"{name}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.status_message = Some(if summary.is_empty() {
                    "No registers".to_string()
                } else {
                    format!("Registers: {summary}")
                });
            }
            "w" => match self.document.save() {
                Ok(()) => {
                    self.status_message =
                        Some(format!("\"{}\" written", self.document.file_name()));
                    return Some(DeferredAction::DidSave);
                }
                Err(e) => {
                    self.status_message = Some(format!("Error: {e}"));
                }
            },
            "q" => {
                if self.has_splits() {
                    return self.close_pane();
                }
                if self.document.modified {
                    self.status_message =
                        Some("No write since last change (add ! to override)".to_string());
                } else {
                    self.should_quit = true;
                }
            }
            "q!" => {
                if self.has_splits() {
                    // Force close pane even if modified
                    let old_id = self.active_pane_id;
                    self.save_active_pane();
                    self.pane_layout.remove(old_id);
                    self.panes.retain(|p| p.id != old_id);
                    let leaves = self.pane_layout.leaves();
                    if let Some(&next_id) = leaves.first() {
                        self.load_pane(next_id);
                    }
                    return None;
                }
                self.should_quit = true;
            }
            "wq" | "x" => {
                if let Err(e) = self.document.save() {
                    self.status_message = Some(format!("Error: {e}"));
                } else {
                    self.should_quit = true;
                    return Some(DeferredAction::DidSave);
                }
            }
            // Split commands
            "split" | "sp" => return self.split_pane(SplitDirection::Horizontal),
            "vsplit" | "vs" => return self.split_pane(SplitDirection::Vertical),
            // Buffer commands
            "bn" | "bnext" => return self.next_buffer(),
            "bp" | "bprev" | "bprevious" => return self.prev_buffer(),
            "bd" | "bdelete" => return self.close_buffer(),
            "bd!" | "bdelete!" => return self.close_buffer_force(),
            "ls" | "buffers" => {
                let info = self.buffer_info();
                let msg = info
                    .iter()
                    .enumerate()
                    .map(|(i, (name, modified, current))| {
                        let marker = if *current { "%" } else { " " };
                        let mod_mark = if *modified { "+" } else { "" };
                        format!("{}{} {}{}", marker, i + 1, name, mod_mark)
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                self.status_message = Some(msg);
            }
            "set wrap" => {
                self.config.wrap = true;
                self.view.wrap = true;
                self.view.offset_wrap = 0;
                self.view.offset_col = 0;
                self.status_message = Some("wrap on".to_string());
            }
            "set nowrap" => {
                self.config.wrap = false;
                self.view.wrap = false;
                self.view.offset_wrap = 0;
                self.status_message = Some("wrap off".to_string());
            }
            "relativenumber" | "rnu" | "set relativenumber" | "set rnu" => {
                self.config.relative_number = true;
                self.status_message = Some("relativenumber on".to_string());
            }
            "norelativenumber" | "nornu" | "set norelativenumber" | "set nornu" => {
                self.config.relative_number = false;
                self.status_message = Some("relativenumber off".to_string());
            }
            other if other.starts_with("set fontsize=") => {
                let val = &other["set fontsize=".len()..];
                match val.parse::<f32>() {
                    Ok(size) if (8.0..=48.0).contains(&size) => {
                        self.config.font_size = size;
                        self.status_message = Some(format!("font size: {size}"));
                    }
                    Ok(size) => {
                        self.status_message =
                            Some(format!("Font size must be between 8 and 48 (got {size})"));
                    }
                    Err(_) => {
                        self.status_message = Some(format!("Invalid font size: {val}"));
                    }
                }
            }
            other if other.starts_with("set font=") => {
                let name = other["set font=".len()..].trim();
                if name.is_empty() {
                    self.config.font_family = None;
                    self.font_family_changed = true;
                    self.status_message = Some("font: default".to_string());
                } else {
                    self.config.font_family = Some(name.to_string());
                    self.font_family_changed = true;
                    self.status_message = Some(format!("font: {name}"));
                }
            }
            other if other.starts_with("set scrolloff=") => {
                let val = &other["set scrolloff=".len()..];
                match val.parse::<usize>() {
                    Ok(n) if n <= 999 => {
                        self.config.scroll_off = n;
                        self.status_message = Some(format!("scrolloff={n}"));
                    }
                    _ => {
                        self.status_message = Some(format!("Invalid scrolloff: {val}"));
                    }
                }
            }
            other if other.starts_with("set tabstop=") => {
                let val = &other["set tabstop=".len()..];
                match val.parse::<usize>() {
                    Ok(n) if (1..=16).contains(&n) => {
                        self.config.tab_width = n;
                        self.status_message = Some(format!("tabstop={n}"));
                    }
                    _ => {
                        self.status_message = Some(format!("Invalid tabstop: {val}"));
                    }
                }
            }
            other => {
                self.status_message = Some(format!("Unknown command: {other}"));
            }
        }
        None
    }

    fn execute_git_command(&mut self, cmd: &str) -> Option<DeferredAction> {
        let mut parts = cmd.split_whitespace();
        let Some(name) = parts.next() else {
            self.status_message =
                Some("Usage: :git <status|diff|add|reset|switch|stash>".to_string());
            return None;
        };
        let rest = parts.collect::<Vec<_>>();
        let action = match name {
            "status" | "st" => GitEditorCommand::Status,
            "branches" => GitEditorCommand::Branches,
            "branch" if rest.is_empty() => GitEditorCommand::Branches,
            "branch" => GitEditorCommand::CreateBranch(rest.join(" ")),
            "diff" => {
                let mut mode = GitDiffMode::Worktree;
                let mut paths = Vec::new();
                for part in rest {
                    match part {
                        "--staged" | "--cached" => mode = GitDiffMode::Staged,
                        "--head" | "HEAD" => mode = GitDiffMode::Head,
                        path => paths.push(path.to_string()),
                    }
                }
                GitEditorCommand::Diff { mode, paths }
            }
            "add" | "stage" => GitEditorCommand::StageFiles(strings_from_parts(rest)),
            "reset" | "unstage" => GitEditorCommand::UnstageFiles(strings_from_parts(rest)),
            "stage-hunk" => {
                if rest.len() < 2 {
                    self.status_message =
                        Some("Usage: :git stage-hunk <path> <hunk-id>".to_string());
                    return None;
                }
                GitEditorCommand::StageHunk {
                    path: rest[0].to_string(),
                    hunk_id: rest[1].to_string(),
                }
            }
            "unstage-hunk" => {
                if rest.len() < 2 {
                    self.status_message =
                        Some("Usage: :git unstage-hunk <path> <hunk-id>".to_string());
                    return None;
                }
                GitEditorCommand::UnstageHunk {
                    path: rest[0].to_string(),
                    hunk_id: rest[1].to_string(),
                }
            }
            "rm" | "delete" => GitEditorCommand::DeleteFiles(strings_from_parts(rest)),
            "checkout" | "switch" => {
                if rest.is_empty() {
                    self.status_message = Some(format!("Usage: :git {name} <branch>"));
                    return None;
                }
                GitEditorCommand::CheckoutBranch(rest.join(" "))
            }
            "amend" => GitEditorCommand::Amend {
                message: non_empty_join(rest),
            },
            "stash" => match rest.as_slice() {
                [] => GitEditorCommand::StashPush { message: None },
                ["push"] => GitEditorCommand::StashPush { message: None },
                ["push", message @ ..] => GitEditorCommand::StashPush {
                    message: non_empty_join(message.to_vec()),
                },
                ["apply"] => GitEditorCommand::StashApply { index: 0 },
                ["apply", index] => GitEditorCommand::StashApply {
                    index: index.parse().unwrap_or(0),
                },
                ["pop"] => GitEditorCommand::StashPop { index: 0 },
                ["pop", index] => GitEditorCommand::StashPop {
                    index: index.parse().unwrap_or(0),
                },
                message => GitEditorCommand::StashPush {
                    message: non_empty_join(message.to_vec()),
                },
            },
            "merge" => {
                if rest.is_empty() {
                    self.status_message = Some("Usage: :git merge <branch>".to_string());
                    return None;
                }
                GitEditorCommand::Merge {
                    branch: rest.join(" "),
                }
            }
            "rebase" => {
                if rest.is_empty() {
                    self.status_message = Some("Usage: :git rebase <upstream>".to_string());
                    return None;
                }
                GitEditorCommand::Rebase {
                    upstream: rest.join(" "),
                }
            }
            other => {
                self.status_message = Some(format!("Unknown git command: {other}"));
                return None;
            }
        };
        Some(DeferredAction::Git(action))
    }

    fn execute_substitute(&mut self, cmd: &str) -> Option<DeferredAction> {
        let global = cmd.starts_with("%s/");
        let rest = if global { &cmd[3..] } else { &cmd[2..] };

        // Parse /old/new/ or /old/new/g or /old/new/gi
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() < 2 || parts[0].is_empty() {
            self.status_message = Some("Usage: :%s/old/new/g or :s/old/new".to_string());
            return None;
        }
        let pattern = parts[0];
        let replacement = parts[1];
        let flags = parts.get(2).unwrap_or(&"");
        let replace_all_in_line = flags.contains('g');
        let case_insensitive = flags.contains('i');

        // Build regex pattern with optional case-insensitive flag
        let regex_pattern = if case_insensitive {
            format!("(?i){}", pattern)
        } else {
            pattern.to_string()
        };

        let re = match regex::Regex::new(&regex_pattern) {
            Ok(re) => re,
            Err(e) => {
                self.status_message = Some(format!("Invalid regex: {e}"));
                return None;
            }
        };

        self.save_undo();
        let mut count = 0;

        if global {
            for row in 0..self.document.line_count() {
                count += self.substitute_line_regex(row, &re, replacement, replace_all_in_line);
            }
        } else {
            count =
                self.substitute_line_regex(self.cursor.row, &re, replacement, replace_all_in_line);
        }

        if count > 0 {
            self.document.modified = true;
            self.document.bump_version();
            self.status_message = Some(format!("{count} substitution(s)"));
        } else {
            self.status_message = Some("Pattern not found".to_string());
        }
        None
    }

    fn substitute_line_regex(
        &mut self,
        row: usize,
        re: &regex::Regex,
        replacement: &str,
        all: bool,
    ) -> usize {
        let line_start = self.document.rope.line_to_char(row);
        let line: String = self.document.rope.line(row).to_string();
        let text = line.trim_end_matches('\n');

        let result = if all {
            re.replace_all(text, replacement)
        } else {
            re.replace(text, replacement)
        };

        if result == text {
            return 0;
        }

        // Count replacements
        let count = if all { re.find_iter(text).count() } else { 1 };

        // Replace the line content in the rope
        let line_end = line_start + text.chars().count();
        if line_end <= self.document.rope.len_chars() {
            self.document.rope.remove(line_start..line_end);
            self.document.rope.insert(line_start, &result);
        }
        count
    }
}

fn strings_from_parts(parts: Vec<&str>) -> Vec<String> {
    parts.into_iter().map(ToOwned::to_owned).collect()
}

fn non_empty_join(parts: Vec<&str>) -> Option<String> {
    let joined = parts.join(" ");
    (!joined.trim().is_empty()).then_some(joined)
}

/// Actions that require async handling by app.rs after command execution.
pub enum DeferredAction {
    Rename(String),
    DidSave,
    OpenFile(String),
    SyncFileUri,
    ShellCommand(String),
    FormatDocument,
    PlayMacro(char),
    Git(GitEditorCommand),
}
