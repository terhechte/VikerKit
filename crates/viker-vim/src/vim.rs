use std::collections::HashMap;

use viker_core::buffer;
use viker_core::config::Config;
use viker_core::editor::document::Document;
use viker_core::editor::history::History;
use viker_core::editor::selection::Position;
use viker_core::editor::view::View;
use viker_core::editor::wrap;
use viker_core::input::command::{
    CaseOp, Command, CommandInvocation, FindDirection, FindKind, LastFind, Motion,
};
use viker_core::input::mode::Mode;
use viker_core::key::{KeyCode, KeyInput};

#[derive(Debug, Clone)]
pub enum LastChange {
    NormalCommand(Command),
    InsertSession {
        entry_cmd: Command,
        chars: Vec<char>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Register {
    pub content: String,
    pub linewise: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterOp {
    Yank,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VisualBlockEdit {
    start_row: usize,
    end_row: usize,
    col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    ShowMessage(String),
    Quit,
    SetClipboard(String),
    GotoDefinition,
    Hover,
    FindReferences,
    ReferenceJump,
    OpenFileFinder,
    CodeAction,
    CodeActionAccept,
    WorkspaceSymbol,
    WorkspaceSymbolConfirm,
    TriggerCompletion,
    FormatDocument,
    OpenFile(String),
    ShellCommand(String),
    Rename(String),
    Save,
    DidSave,
    SyncFileUri,
    PlayMacro(char),
    SplitHorizontal,
    SplitVertical,
    PaneLeft,
    PaneDown,
    PaneUp,
    PaneRight,
    PaneNext,
    PaneClose,
    NextBuffer,
    PrevBuffer,
    CloseBuffer,
    CloseBufferForce,
    AcceptCompletion,
    CancelCompletion,
    CompletionNext,
    CompletionPrev,
    DismissPopup,
    ReferenceNext,
    ReferencePrev,
    DiagnosticNext,
    DiagnosticPrev,
    DiagnosticList,
    DiagnosticJump,
    CodeActionNext,
    CodeActionPrev,
    CodeActionDismiss,
    WorkspaceSymbolInput(char),
    WorkspaceSymbolBackspace,
    WorkspaceSymbolCancel,
    WorkspaceSymbolNext,
    WorkspaceSymbolPrev,
    FileFinderInput(char),
    FileFinderBackspace,
    FileFinderConfirm,
    FileFinderCancel,
    FileFinderNext,
    FileFinderPrev,
}

pub struct VimCore {
    pub document: Document,
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
    pub search_query: String,
    pub search_matches: Vec<(usize, usize, usize)>,
    pub search_index: Option<usize>,
    pub search_regex: Option<regex::Regex>,
    pub search_start_cursor: Option<Position>,
    pub jump_list: Vec<Position>,
    pub jump_index: usize,
    pub marks: HashMap<char, Position>,
    pub previous_position: Option<Position>,
    pub last_find: Option<LastFind>,
    pub search_forward: bool,
    pub last_search_forward: bool,
    pub last_change: Option<LastChange>,
    pub recording_insert: bool,
    pub insert_entry_cmd: Option<Command>,
    pub insert_record: Vec<char>,
    pub command_history: Vec<String>,
    pub command_history_idx: Option<usize>,
    pub command_history_temp: String,
    pub registers: HashMap<char, Register>,
    pub selected_register: Option<char>,
    pub recording_macro: Option<char>,
    pub macro_buffer: Vec<KeyInput>,
    pub macros: HashMap<char, Vec<KeyInput>>,
    pub last_macro: Option<char>,
    pub view: View,
    pub pending_effects: Vec<Effect>,
    pub clipboard_content: Option<String>,
    visual_block_edit: Option<VisualBlockEdit>,
    visual_block_insert_text: String,
}

impl VimCore {
    pub fn from_text(text: &str) -> Self {
        let doc = Document {
            rope: ropey::Rope::from_str(text),
            path: None,
            modified: false,
            version: 0,
        };
        Self::with_config(doc, Config::default())
    }

    pub fn with_config(document: Document, config: Config) -> Self {
        let wrap = config.wrap;
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
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: None,
            search_regex: None,
            search_start_cursor: None,
            jump_list: Vec::new(),
            jump_index: 0,
            marks: HashMap::new(),
            previous_position: None,
            last_find: None,
            search_forward: true,
            last_search_forward: true,
            last_change: None,
            recording_insert: false,
            insert_entry_cmd: None,
            insert_record: Vec::new(),
            command_history: Vec::new(),
            command_history_idx: None,
            command_history_temp: String::new(),
            registers: HashMap::new(),
            selected_register: None,
            recording_macro: None,
            macro_buffer: Vec::new(),
            macros: HashMap::new(),
            last_macro: None,
            pending_effects: Vec::new(),
            clipboard_content: None,
            visual_block_edit: None,
            visual_block_insert_text: String::new(),
        }
    }

    fn set_message(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.status_message = Some(msg.clone());
        self.pending_effects.push(Effect::ShowMessage(msg));
    }

    fn do_quit(&mut self) {
        self.should_quit = true;
        self.pending_effects.push(Effect::Quit);
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

    pub fn gutter_width(&self) -> u16 {
        let lines = self.document.line_count();
        let digits = if lines == 0 {
            1
        } else {
            (lines as f64).log10().floor() as u16 + 1
        };
        digits + 2
    }

    fn text_width(&self) -> u16 {
        self.view.width.saturating_sub(self.gutter_width())
    }

    pub fn save_undo(&mut self) {
        self.history.save(&self.document.rope, self.cursor);
    }

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

    fn move_down_wrapped(&mut self) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let wc = wrap::wrap_count(line, text_width);
        let (seg, col_in_seg) = wrap::char_to_wrap_pos(line, self.cursor.col, text_width);

        if seg + 1 < wc {
            self.cursor.col = wrap::wrap_pos_to_char(line, seg + 1, col_in_seg, text_width);
        } else {
            let max_row = self.document.line_count().saturating_sub(1);
            if self.cursor.row < max_row {
                self.cursor.row += 1;
                let next_line = self.document.rope.line(self.cursor.row);
                self.cursor.col = wrap::wrap_pos_to_char(next_line, 0, col_in_seg, text_width);
            }
        }
        self.clamp_cursor();
    }

    fn move_up_wrapped(&mut self) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (seg, col_in_seg) = wrap::char_to_wrap_pos(line, self.cursor.col, text_width);

        if seg > 0 {
            self.cursor.col = wrap::wrap_pos_to_char(line, seg - 1, col_in_seg, text_width);
        } else {
            if self.cursor.row > 0 {
                self.cursor.row -= 1;
                let prev_line = self.document.rope.line(self.cursor.row);
                let prev_wc = wrap::wrap_count(prev_line, text_width);
                self.cursor.col =
                    wrap::wrap_pos_to_char(prev_line, prev_wc - 1, col_in_seg, text_width);
            }
        }
        self.clamp_cursor();
    }

    pub fn move_document_line_down(&mut self) {
        let max_row = self.document.line_count().saturating_sub(1);
        if self.cursor.row < max_row {
            self.cursor.row += 1;
        }
        self.clamp_cursor();
    }

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

            if !line.char(col).is_whitespace() {
                while col < line_len && !line.char(col).is_whitespace() {
                    col += 1;
                }
            }

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

        while col > 0 && line.char(col - 1).is_whitespace() {
            col -= 1;
        }

        if col == 0 {
            self.cursor.row = row;
            self.cursor.col = 0;
            return;
        }

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

        while row < line_count && !self.is_blank_line(row) {
            row += 1;
        }
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

        while row > 0 && self.is_blank_line(row) {
            row -= 1;
        }
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

    pub fn visual_swap_anchor(&mut self) {
        if let Some(ref mut anchor) = self.visual_anchor {
            std::mem::swap(anchor, &mut self.cursor);
        }
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

    // --- Editing ---

    pub fn insert_char(&mut self, ch: char) {
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
        let line: String = self.document.rope.line(self.cursor.row).to_string();
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        let add_indent = if self.cursor.col > 0 {
            let line_slice = self.document.rope.line(self.cursor.row);
            let prev_ch = line_slice.char(self.cursor.col - 1);
            matches!(prev_ch, '{' | '(' | '[')
        } else {
            false
        };

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
        self.set_message("1 line yanked");
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
            self.set_message(format!("{} line(s) yanked", count.max(1)));
        }
    }

    pub fn insert_newline_below(&mut self) {
        self.save_undo();
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
            self.set_message("Already at oldest change");
        }
    }

    pub fn redo(&mut self) {
        if let Some((rope, cursor)) = self.history.redo(&self.document.rope, self.cursor) {
            self.document.rope = rope;
            self.document.modified = true;
            self.cursor = cursor;
            self.clamp_cursor();
        } else {
            self.set_message("Already at newest change");
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
            self.set_message("block yanked");
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
                    self.set_message(format!(
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
        if self.mode == Mode::VisualBlock {
            if self.visual_block_range().is_some() {
                self.visual_delete();
                self.mode = Mode::Insert;
            }
        } else if self.selection_range().is_some() {
            let was_linewise = self.mode == Mode::VisualLine;
            self.visual_delete();
            if was_linewise {
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
                self.set_message("yanked");
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
                self.set_message("yanked");
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
        self.set_message("format operator is not implemented");
    }

    pub fn filter_motion_count(&mut self, motion: &Motion, count: usize) {
        let _ = self.motion_range_count(motion, count);
        self.set_message("filter operator is not implemented");
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
            Motion::SearchForward => self.search_step(true),
            Motion::SearchBackward => self.search_step(false),
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
        self.replace_chars(ch, 1);
    }

    pub fn replace_chars(&mut self, ch: char, count: usize) {
        let line_len = self.document.line_len(self.cursor.row);
        if self.cursor.col >= line_len {
            return;
        }
        let count = count.max(1).min(line_len.saturating_sub(self.cursor.col));
        if count == 0 {
            return;
        }
        self.save_undo();
        let idx = self.document.rope.line_to_char(self.cursor.row) + self.cursor.col;
        self.document.rope.remove(idx..idx + count);
        let replacement: String = std::iter::repeat_n(ch, count).collect();
        self.document.rope.insert(idx, &replacement);
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

        let next_line = self.document.rope.line(self.cursor.row + 1);
        let leading_ws: usize = next_line
            .chars()
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .count();

        let remove_end = (newline_idx + 1 + leading_ws).min(self.document.rope.len_chars());
        if newline_idx < remove_end {
            self.document.rope.remove(newline_idx..remove_end);
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
        self.set_message(format!("mark '{mark}' set"));
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
            self.set_message(format!("Mark not set: {mark}"));
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
                self.set_message(if reg_name == '"' {
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
                self.cursor.col += char_count;
            }
        }
        self.clamp_cursor();
        if reg_name != '"' {
            self.set_message(format!("pasted from register \"{}", reg_name));
        }
    }

    pub fn paste_before(&mut self) {
        let reg_name = self.consume_register();
        let reg = match self.read_register(reg_name) {
            Some(r) if !r.content.is_empty() => r,
            _ => {
                self.set_message(if reg_name == '"' {
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
            self.set_message(format!("pasted from register \"{}", reg_name));
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

    pub fn enter_insert_mode_first_non_blank(&mut self) {
        self.save_undo();
        self.move_first_non_blank();
        self.mode = Mode::Insert;
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.command_buffer.clear();
    }

    pub fn enter_replace_mode(&mut self) {
        self.save_undo();
        self.mode = Mode::Replace;
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
                self.set_message("Pattern not found");
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
        self.set_message(format!("[{}/{}]", next + 1, self.search_matches.len()));
    }

    pub fn search_next(&mut self) {
        self.search_step(self.last_search_forward);
    }

    pub fn search_prev(&mut self) {
        self.search_step(!self.last_search_forward);
    }

    fn build_search_regex(query: &str) -> Option<regex::Regex> {
        if query.is_empty() {
            return None;
        }

        let (pattern, force_case) = if let Some(stripped) = query.strip_suffix("\\c") {
            (stripped, Some(false))
        } else if let Some(stripped) = query.strip_suffix("\\C") {
            (stripped, Some(true))
        } else {
            (query, None)
        };

        if pattern.is_empty() {
            return None;
        }

        let case_sensitive = match force_case {
            Some(sensitive) => sensitive,
            None => pattern.chars().any(|c| c.is_uppercase()),
        };

        let regex_pattern = if case_sensitive {
            pattern.to_string()
        } else {
            format!("(?i){}", pattern)
        };

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
            let text = line.trim_end_matches('\n');
            for m in re.find_iter(text) {
                let match_len = m.as_str().chars().count();
                if match_len == 0 {
                    continue;
                }
                let col = text[..m.start()].chars().count();
                self.search_matches.push((row, col, match_len));
            }
        }
        self.search_regex = Some(re);
    }

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

    // --- Repeat last change ---

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
                self.exit_to_normal_mode();
            }
        }
    }

    // --- Search word under cursor ---

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

    // --- Matching bracket jump ---

    pub fn match_bracket_jump(&mut self) {
        if let Some(pos) = self.matching_bracket() {
            self.push_jump();
            self.cursor = pos;
        }
    }

    // --- Viewport navigation ---

    pub fn viewport_high(&mut self) {
        if self.config.wrap {
            let text_width = self.text_width();
            let map = wrap::build_screen_map(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
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
            let map = wrap::build_screen_map(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
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
            let map = wrap::build_screen_map(
                &self.document.rope,
                self.view.offset_row,
                self.view.offset_wrap,
                text_width,
                self.view.height,
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

    // --- Scroll positioning ---

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

    fn scroll_to_cursor_at_screen_row(&mut self, target_screen_row: usize, text_width: u16) {
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (cursor_seg, _) = wrap::char_to_wrap_pos(line, self.cursor.col, text_width);

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
                    seg = wrap::wrap_count(prev_line, text_width) - 1;
                } else {
                    seg = 0;
                    to_go = 0;
                }
            }
        }

        self.view.offset_row = row;
        self.view.offset_wrap = seg;
    }

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

    fn move_screen_lines_down(&mut self, n: usize) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (mut seg, col_in_seg) = wrap::char_to_wrap_pos(line, self.cursor.col, text_width);
        let mut row = self.cursor.row;
        let max_row = self.document.line_count().saturating_sub(1);
        let mut remaining = n;

        while remaining > 0 {
            let cur_line = self.document.rope.line(row);
            let wc = wrap::wrap_count(cur_line, text_width);
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
        self.cursor.col = wrap::wrap_pos_to_char(target_line, seg, col_in_seg, text_width);
        self.clamp_cursor();
    }

    fn move_screen_lines_up(&mut self, n: usize) {
        let text_width = self.text_width();
        if text_width == 0 {
            return;
        }
        let line = self.document.rope.line(self.cursor.row);
        let (mut seg, col_in_seg) = wrap::char_to_wrap_pos(line, self.cursor.col, text_width);
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
                    seg = wrap::wrap_count(prev_line, text_width) - 1;
                } else {
                    seg = 0;
                    remaining = 0;
                }
            }
        }

        self.cursor.row = row;
        let target_line = self.document.rope.line(row);
        self.cursor.col = wrap::wrap_pos_to_char(target_line, seg, col_in_seg, text_width);
        self.clamp_cursor();
    }

    // --- Command history ---

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

    // --- Named registers ---

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
            self.pending_effects.push(Effect::SetClipboard(content));
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

    pub fn read_register(&self, name: char) -> Option<Register> {
        if name == '+' || name == '*' {
            if let Some(ref text) = self.clipboard_content {
                let reg = Register {
                    linewise: text.ends_with('\n'),
                    content: text.clone(),
                };
                return Some(reg);
            }
        }
        self.registers.get(&name.to_ascii_lowercase()).cloned()
    }

    // --- Case change ---

    pub fn toggle_case_char(&mut self) {
        if self.mode.is_visual() {
            self.case_change_visual(CaseOp::Toggle);
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
        if self.cursor.col + 1 < self.document.line_len(self.cursor.row) {
            self.cursor.col += 1;
        }
    }

    pub fn case_change(&mut self, op: CaseOp, motion: &Motion) {
        self.save_undo();
        if let Some((start, end)) = self.motion_range(motion) {
            self.apply_case_change(start, end, op);
            self.reposition_cursor_to(start);
        }
        self.clamp_cursor();
    }

    pub fn case_change_line(&mut self, op: CaseOp) {
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

    fn case_change_visual(&mut self, op: CaseOp) {
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

    fn apply_case_change(&mut self, start: usize, end: usize, op: CaseOp) {
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

    // --- Number increment/decrement ---

    pub fn increment_number(&mut self, delta: i64) {
        let line = self.document.rope.line(self.cursor.row);
        let line_len = buffer::line_display_len(line);
        if line_len == 0 {
            return;
        }

        let line_str: String = line.to_string();
        let mut num_start = None;
        let mut num_end = 0;
        for start in self.cursor.col..line_str.len() {
            let ch = line_str.as_bytes()[start] as char;
            if ch.is_ascii_digit() {
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
            self.cursor.col = num_start + new_str.len() - 1;
            self.clamp_cursor();
        }
    }

    // --- Macro recording ---

    pub fn start_macro(&mut self, reg: char) {
        self.recording_macro = Some(reg);
        self.macro_buffer.clear();
        self.set_message(format!("recording @{reg}"));
    }

    pub fn stop_macro(&mut self) {
        if let Some(reg) = self.recording_macro.take() {
            self.macros.insert(reg, self.macro_buffer.clone());
            self.macro_buffer.clear();
            self.set_message("recorded");
        }
    }

    // --- Substitute ---

    pub fn execute_substitute(&mut self, cmd: &str) {
        let global = cmd.starts_with("%s/");
        let rest = if global { &cmd[3..] } else { &cmd[2..] };

        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() < 2 || parts[0].is_empty() {
            self.set_message("Usage: :%s/old/new/g or :s/old/new");
            return;
        }
        let pattern = parts[0];
        let replacement = parts[1];
        let flags = parts.get(2).unwrap_or(&"");
        let replace_all_in_line = flags.contains('g');
        let case_insensitive = flags.contains('i');

        let regex_pattern = if case_insensitive {
            format!("(?i){}", pattern)
        } else {
            pattern.to_string()
        };

        let re = match regex::Regex::new(&regex_pattern) {
            Ok(re) => re,
            Err(e) => {
                self.set_message(format!("Invalid regex: {e}"));
                return;
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
            self.set_message(format!("{count} substitution(s)"));
        } else {
            self.set_message("Pattern not found");
        }
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

        let count = if all { re.find_iter(text).count() } else { 1 };

        let line_end = line_start + text.chars().count();
        if line_end <= self.document.rope.len_chars() {
            self.document.rope.remove(line_start..line_end);
            self.document.rope.insert(line_start, &result);
        }
        count
    }

    // --- Command execute ---

    pub fn command_execute(&mut self) {
        let cmd = self.command_buffer.clone();
        self.mode = Mode::Normal;
        self.command_buffer.clear();
        self.command_history_idx = None;
        self.command_history_temp.clear();

        let trimmed = cmd.trim();

        if !trimmed.is_empty() {
            self.command_history.push(trimmed.to_string());
            if self.command_history.len() > 100 {
                self.command_history.remove(0);
            }
        }

        if let Some(shell_cmd) = trimmed.strip_prefix('!') {
            let shell_cmd = shell_cmd.trim().to_string();
            if shell_cmd.is_empty() {
                self.set_message("Usage: :!<command>");
                return;
            }
            self.pending_effects.push(Effect::ShellCommand(shell_cmd));
            return;
        }

        if trimmed == "format" || trimmed == "fmt" {
            self.pending_effects.push(Effect::FormatDocument);
            return;
        }

        if let Some(new_name) = trimmed.strip_prefix("rename ") {
            let new_name = new_name.trim().to_string();
            if new_name.is_empty() {
                self.set_message("Usage: :rename <new_name>");
                return;
            }
            self.pending_effects.push(Effect::Rename(new_name));
            return;
        }

        if trimmed.starts_with("%s/") || trimmed.starts_with("s/") {
            self.execute_substitute(trimmed);
            return;
        }

        if let Some(path) = trimmed
            .strip_prefix("split ")
            .or_else(|| trimmed.strip_prefix("sp "))
        {
            let path = path.trim().to_string();
            if !path.is_empty() {
                self.pending_effects.push(Effect::SplitHorizontal);
                self.pending_effects.push(Effect::OpenFile(path));
                return;
            }
        }
        if let Some(path) = trimmed
            .strip_prefix("vsplit ")
            .or_else(|| trimmed.strip_prefix("vs "))
        {
            let path = path.trim().to_string();
            if !path.is_empty() {
                self.pending_effects.push(Effect::SplitVertical);
                self.pending_effects.push(Effect::OpenFile(path));
                return;
            }
        }

        if let Some(path) = trimmed.strip_prefix("e ") {
            let path = path.trim().to_string();
            if path.is_empty() {
                self.set_message("Usage: :e <file>");
                return;
            }
            self.pending_effects.push(Effect::OpenFile(path));
            return;
        }

        if let Ok(line_num) = trimmed.parse::<usize>() {
            if line_num > 0 {
                self.cursor.row = (line_num - 1).min(self.document.line_count().saturating_sub(1));
                self.cursor.col = 0;
                self.clamp_cursor();
            }
            return;
        }

        match trimmed {
            "noh" | "nohlsearch" => {
                self.search_matches.clear();
                self.search_index = None;
                self.search_regex = None;
                self.set_message("search highlighting cleared");
            }
            "diagnostics" | "diags" => {
                self.pending_effects.push(Effect::DiagnosticList);
            }
            "registers" | "reg" => {
                let mut names: Vec<char> = self.registers.keys().copied().collect();
                names.sort_unstable();
                let summary = names
                    .into_iter()
                    .map(|name| format!("\"{name}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.set_message(if summary.is_empty() {
                    "No registers".to_string()
                } else {
                    format!("Registers: {summary}")
                });
            }
            "w" => {
                self.pending_effects.push(Effect::Save);
            }
            "q" => {
                if self.document.modified {
                    self.set_message("No write since last change (add ! to override)");
                } else {
                    self.do_quit();
                }
            }
            "q!" => {
                self.do_quit();
            }
            "wq" | "x" => {
                self.pending_effects.push(Effect::Save);
                self.do_quit();
            }
            "split" | "sp" => {
                self.pending_effects.push(Effect::SplitHorizontal);
            }
            "vsplit" | "vs" => {
                self.pending_effects.push(Effect::SplitVertical);
            }
            "bn" | "bnext" => {
                self.pending_effects.push(Effect::NextBuffer);
            }
            "bp" | "bprev" | "bprevious" => {
                self.pending_effects.push(Effect::PrevBuffer);
            }
            "bd" | "bdelete" => {
                self.pending_effects.push(Effect::CloseBuffer);
            }
            "bd!" | "bdelete!" => {
                self.pending_effects.push(Effect::CloseBufferForce);
            }
            "set wrap" => {
                self.config.wrap = true;
                self.view.wrap = true;
                self.view.offset_wrap = 0;
                self.view.offset_col = 0;
                self.set_message("wrap on");
            }
            "set nowrap" => {
                self.config.wrap = false;
                self.view.wrap = false;
                self.view.offset_wrap = 0;
                self.set_message("wrap off");
            }
            "relativenumber" | "rnu" | "set relativenumber" | "set rnu" => {
                self.config.relative_number = true;
                self.set_message("relativenumber on");
            }
            "norelativenumber" | "nornu" | "set norelativenumber" | "set nornu" => {
                self.config.relative_number = false;
                self.set_message("relativenumber off");
            }
            other if other.starts_with("set fontsize=") => {
                let val = &other["set fontsize=".len()..];
                match val.parse::<f32>() {
                    Ok(size) if (8.0..=48.0).contains(&size) => {
                        self.config.font_size = size;
                        self.set_message(format!("font size: {size}"));
                    }
                    Ok(size) => {
                        self.set_message(format!(
                            "Font size must be between 8 and 48 (got {size})"
                        ));
                    }
                    Err(_) => {
                        self.set_message(format!("Invalid font size: {val}"));
                    }
                }
            }
            other if other.starts_with("set font=") => {
                let name = other["set font=".len()..].trim();
                if name.is_empty() {
                    self.config.font_family = None;
                    self.set_message("font: default");
                } else {
                    self.config.font_family = Some(name.to_string());
                    self.set_message(format!("font: {name}"));
                }
            }
            other if other.starts_with("set scrolloff=") => {
                let val = &other["set scrolloff=".len()..];
                match val.parse::<usize>() {
                    Ok(n) if n <= 999 => {
                        self.config.scroll_off = n;
                        self.set_message(format!("scrolloff={n}"));
                    }
                    _ => {
                        self.set_message(format!("Invalid scrolloff: {val}"));
                    }
                }
            }
            other if other.starts_with("set tabstop=") => {
                let val = &other["set tabstop=".len()..];
                match val.parse::<usize>() {
                    Ok(n) if (1..=16).contains(&n) => {
                        self.config.tab_width = n;
                        self.set_message(format!("tabstop={n}"));
                    }
                    _ => {
                        self.set_message(format!("Invalid tabstop: {val}"));
                    }
                }
            }
            other => {
                self.set_message(format!("Unknown command: {other}"));
            }
        }
    }

    // --- Execute command ---

    fn track_change(&mut self, cmd: &Command) {
        match cmd {
            Command::DeleteCharForward
            | Command::DeleteCharBackwardNormal
            | Command::DeleteLine
            | Command::SubstituteChar
            | Command::DeleteMotion(_)
            | Command::IndentLine
            | Command::DedentLine
            | Command::JoinLines
            | Command::ReplaceChar(_)
            | Command::PasteAfter
            | Command::PasteBefore
            | Command::ToggleCaseChar
            | Command::CaseChange(_, _)
            | Command::CaseChangeLine(_)
            | Command::IncrementNumber
            | Command::DecrementNumber => {
                self.last_change = Some(LastChange::NormalCommand(cmd.clone()));
            }

            Command::EnterInsertMode
            | Command::EnterInsertModeAfter
            | Command::EnterInsertModeLineEnd
            | Command::EnterInsertModeFirstNonBlank
            | Command::InsertNewlineBelow
            | Command::InsertNewlineAbove
            | Command::ChangeMotion(_)
            | Command::VisualBlockInsert
            | Command::VisualBlockAppend => {
                self.recording_insert = true;
                self.insert_entry_cmd = Some(cmd.clone());
                self.insert_record.clear();
            }

            Command::InsertChar(ch) if self.recording_insert => {
                self.insert_record.push(*ch);
            }
            Command::DeleteCharBackward if self.recording_insert => {
                self.insert_record.push('\x08');
            }
            Command::DeleteWordBackward if self.recording_insert => {
                self.insert_record.push('\u{17}');
            }
            Command::DeleteLineBackward if self.recording_insert => {
                self.insert_record.push('\u{15}');
            }
            Command::InsertNewline if self.recording_insert => {
                self.insert_record.push('\n');
            }
            Command::InsertTab if self.recording_insert => {
                self.insert_record.push('\t');
            }

            Command::ExitToNormalMode if self.recording_insert => {
                self.recording_insert = false;
                if let Some(entry) = self.insert_entry_cmd.take() {
                    self.last_change = Some(LastChange::InsertSession {
                        entry_cmd: entry,
                        chars: self.insert_record.clone(),
                    });
                }
            }

            _ => {}
        }
    }

    pub fn execute(&mut self, cmd: Command) {
        self.status_message = None;

        self.track_change(&cmd);

        match cmd {
            Command::MoveLeft => self.move_left(),
            Command::MoveDown => self.move_down(),
            Command::MoveUp => self.move_up(),
            Command::MoveRight => self.move_right(),
            Command::MoveWordForward => self.move_word_forward(),
            Command::MoveWordBackward => self.move_word_backward(),
            Command::MoveWordEnd => self.move_word_end(),
            Command::MoveWordEndBackward => self.move_word_end_backward(),
            Command::MoveLineStart => self.move_line_start(),
            Command::MoveLineEnd => self.move_line_end(),
            Command::MoveFirstNonBlank => self.move_first_non_blank(),
            Command::MoveWORDForward => self.move_word_forward_big(),
            Command::MoveWORDBackward => self.move_word_backward_big(),
            Command::MoveWORDEnd => self.move_word_end_big(),
            Command::MoveWORDEndBackward => self.move_word_end_backward_big(),
            Command::MoveParagraphForward => self.move_paragraph_forward(),
            Command::MoveParagraphBackward => self.move_paragraph_backward(),
            Command::MoveSentenceForward => self.move_sentence_forward(),
            Command::MoveSentenceBackward => self.move_sentence_backward(),
            Command::MoveSectionForward => self.move_section_forward(),
            Command::MoveSectionBackward => self.move_section_backward(),
            Command::MoveColumn => self.move_column(1),
            Command::MoveLineDownFirstNonBlank => self.move_line_down_first_non_blank(),
            Command::MoveLineUpFirstNonBlank => self.move_line_up_first_non_blank(),
            Command::MoveDocumentLineDown => self.move_document_line_down(),
            Command::MoveDocumentLineUp => self.move_document_line_up(),

            Command::InsertChar(ch) => self.insert_char(ch),
            Command::DeleteCharForward => self.delete_char_forward(),
            Command::DeleteCharBackward => self.delete_char_backward(),
            Command::DeleteCharBackwardNormal => self.delete_char_backward_normal(),
            Command::DeleteWordBackward => self.delete_word_backward(),
            Command::DeleteLineBackward => self.delete_line_backward(),
            Command::DeleteLine => self.delete_line(),
            Command::SubstituteChar => self.substitute_char(),
            Command::InsertNewlineBelow => self.insert_newline_below(),
            Command::InsertNewlineAbove => self.insert_newline_above(),
            Command::InsertNewline => self.insert_newline(),
            Command::InsertTab => self.insert_tab(),
            Command::IndentLine => self.indent_line(),
            Command::DedentLine => self.dedent_line(),
            Command::FormatMotion(ref motion) => self.format_motion_count(motion, 1),
            Command::FilterMotion(ref motion) => self.filter_motion_count(motion, 1),

            Command::DeleteMotion(ref motion) => self.delete_motion(motion),
            Command::ChangeMotion(ref motion) => self.change_motion(motion),
            Command::YankMotion(ref motion) => self.yank_motion(motion),
            Command::IndentMotion(ref motion) => self.indent_motion_count(motion, 1),
            Command::DedentMotion(ref motion) => self.dedent_motion_count(motion, 1),

            Command::FindCharForward(ch) => self.find_char_forward(ch),
            Command::FindCharBackward(ch) => self.find_char_backward(ch),
            Command::TillCharForward(ch) => self.till_char_forward(ch),
            Command::TillCharBackward(ch) => self.till_char_backward(ch),
            Command::RepeatFindForward => self.repeat_find(false),
            Command::RepeatFindBackward => self.repeat_find(true),

            Command::ReplaceChar(ch) => self.replace_char(ch),

            Command::JoinLines => self.join_lines(),

            Command::Undo => self.undo(),
            Command::Redo => self.redo(),

            Command::EnterInsertMode => self.enter_insert_mode(),
            Command::EnterInsertModeAfter => self.enter_insert_mode_after(),
            Command::EnterInsertModeLineEnd => self.enter_insert_mode_line_end(),
            Command::EnterInsertModeFirstNonBlank => self.enter_insert_mode_first_non_blank(),
            Command::EnterVisualMode => self.enter_visual_mode(),
            Command::EnterVisualLineMode => self.enter_visual_line_mode(),
            Command::EnterVisualBlockMode => self.enter_visual_block_mode(),
            Command::EnterReplaceMode => self.enter_replace_mode(),
            Command::EnterCommandMode => self.enter_command_mode(),
            Command::ExitToNormalMode => self.exit_to_normal_mode(),

            Command::VisualDelete => self.visual_delete(),
            Command::VisualYank => self.visual_yank(),
            Command::VisualChange => self.visual_change(),
            Command::VisualIndent => self.visual_indent(),
            Command::VisualDedent => self.visual_dedent(),
            Command::VisualSwapAnchor => self.visual_swap_anchor(),
            Command::VisualSwapBlockCorner => self.visual_swap_block_corner(),
            Command::RestoreVisualSelection => self.restore_visual_selection(),
            Command::VisualSelect(ref motion) => self.visual_select_motion(motion),
            Command::VisualBlockInsert => self.visual_block_insert(),
            Command::VisualBlockAppend => self.visual_block_append(),

            Command::PasteAfter => self.paste_after(),
            Command::PasteBefore => self.paste_before(),

            Command::YankLine => self.yank_line(),

            Command::JumpBack => self.jump_back(),
            Command::JumpForward => self.jump_forward(),
            Command::SetMark(ch) => self.set_mark(ch),
            Command::GotoMark { mark, exact } => self.goto_mark(mark, exact),
            Command::GotoPreviousPosition { exact } => self.goto_previous_position(exact),

            Command::TriggerCompletion => {
                self.pending_effects.push(Effect::TriggerCompletion);
            }
            Command::AcceptCompletion => {
                self.pending_effects.push(Effect::AcceptCompletion);
            }
            Command::CancelCompletion => {
                self.pending_effects.push(Effect::CancelCompletion);
            }
            Command::CompletionNext => {
                self.pending_effects.push(Effect::CompletionNext);
            }
            Command::CompletionPrev => {
                self.pending_effects.push(Effect::CompletionPrev);
            }

            Command::GotoDefinition => {
                self.pending_effects.push(Effect::GotoDefinition);
            }
            Command::Hover => {
                self.pending_effects.push(Effect::Hover);
            }
            Command::FindReferences => {
                self.pending_effects.push(Effect::FindReferences);
            }
            Command::DismissPopup => {
                self.pending_effects.push(Effect::DismissPopup);
            }
            Command::ReferenceNext => {
                self.pending_effects.push(Effect::ReferenceNext);
            }
            Command::ReferencePrev => {
                self.pending_effects.push(Effect::ReferencePrev);
            }
            Command::ReferenceJump => {
                self.pending_effects.push(Effect::ReferenceJump);
            }

            Command::EnterSearchMode => self.enter_search_mode(),
            Command::EnterSearchBackwardMode => self.enter_search_backward_mode(),
            Command::SearchInput(ch) => self.search_input(ch),
            Command::SearchBackspace => self.search_backspace(),
            Command::SearchConfirm => self.search_confirm(),
            Command::SearchCancel => self.search_cancel(),
            Command::SearchNext => self.search_next(),
            Command::SearchPrev => self.search_prev(),

            Command::RepeatLastChange => self.repeat_last_change(),

            Command::SearchWordForward => self.search_word_forward(),
            Command::SearchWordBackward => self.search_word_backward(),

            Command::MatchBracket => self.match_bracket_jump(),

            Command::ViewportHigh => self.viewport_high(),
            Command::ViewportMiddle => self.viewport_middle(),
            Command::ViewportLow => self.viewport_low(),

            Command::ScrollCenter => self.scroll_center(),
            Command::ScrollTop => self.scroll_top(),
            Command::ScrollBottom => self.scroll_bottom(),

            Command::NextBuffer => {
                self.pending_effects.push(Effect::NextBuffer);
            }
            Command::PrevBuffer => {
                self.pending_effects.push(Effect::PrevBuffer);
            }

            Command::ToggleCaseChar => self.toggle_case_char(),
            Command::CaseChange(op, ref motion) => self.case_change(op, motion),
            Command::CaseChangeLine(op) => self.case_change_line(op),

            Command::IncrementNumber => self.increment_number(1),
            Command::DecrementNumber => self.increment_number(-1),

            Command::SelectRegister(_) => {}

            Command::StartMacro(ch) => self.start_macro(ch),
            Command::StopMacro => self.stop_macro(),
            Command::PlayMacro(ch) => {
                self.pending_effects.push(Effect::PlayMacro(ch));
            }
            Command::PlayLastMacro => {
                if let Some(ch) = self.last_macro {
                    self.pending_effects.push(Effect::PlayMacro(ch));
                }
            }

            Command::FormatDocument => {
                self.pending_effects.push(Effect::FormatDocument);
            }

            Command::DiagnosticNext => {
                self.pending_effects.push(Effect::DiagnosticNext);
            }
            Command::DiagnosticPrev => {
                self.pending_effects.push(Effect::DiagnosticPrev);
            }
            Command::DiagnosticList => {
                self.pending_effects.push(Effect::DiagnosticList);
            }
            Command::DiagnosticJump => {
                self.pending_effects.push(Effect::DiagnosticJump);
            }

            Command::CodeAction => {
                self.pending_effects.push(Effect::CodeAction);
            }
            Command::CodeActionNext => {
                self.pending_effects.push(Effect::CodeActionNext);
            }
            Command::CodeActionPrev => {
                self.pending_effects.push(Effect::CodeActionPrev);
            }
            Command::CodeActionAccept => {
                self.pending_effects.push(Effect::CodeActionAccept);
            }
            Command::CodeActionDismiss => {
                self.pending_effects.push(Effect::CodeActionDismiss);
            }

            Command::GotoTop => self.goto_top(),
            Command::GotoBottom => self.goto_bottom(),
            Command::GotoLine => self.goto_line(1),
            Command::HalfPageDown => self.half_page_down(),
            Command::HalfPageUp => self.half_page_up(),
            Command::FullPageDown => self.full_page_down(),
            Command::FullPageUp => self.full_page_up(),

            Command::WorkspaceSymbol => {
                self.pending_effects.push(Effect::WorkspaceSymbol);
            }
            Command::WorkspaceSymbolInput(ch) => {
                self.pending_effects.push(Effect::WorkspaceSymbolInput(ch));
            }
            Command::WorkspaceSymbolBackspace => {
                self.pending_effects.push(Effect::WorkspaceSymbolBackspace);
            }
            Command::WorkspaceSymbolConfirm => {
                self.pending_effects.push(Effect::WorkspaceSymbolConfirm);
            }
            Command::WorkspaceSymbolCancel => {
                self.pending_effects.push(Effect::WorkspaceSymbolCancel);
            }
            Command::WorkspaceSymbolNext => {
                self.pending_effects.push(Effect::WorkspaceSymbolNext);
            }
            Command::WorkspaceSymbolPrev => {
                self.pending_effects.push(Effect::WorkspaceSymbolPrev);
            }

            Command::OpenFileFinder => {
                self.pending_effects.push(Effect::OpenFileFinder);
            }
            Command::FileFinderInput(ch) => {
                self.pending_effects.push(Effect::FileFinderInput(ch));
            }
            Command::FileFinderBackspace => {
                self.pending_effects.push(Effect::FileFinderBackspace);
            }
            Command::FileFinderConfirm => {
                self.pending_effects.push(Effect::FileFinderConfirm);
            }
            Command::FileFinderCancel => {
                self.pending_effects.push(Effect::FileFinderCancel);
            }
            Command::FileFinderNext => {
                self.pending_effects.push(Effect::FileFinderNext);
            }
            Command::FileFinderPrev => {
                self.pending_effects.push(Effect::FileFinderPrev);
            }

            Command::SplitHorizontal => {
                self.pending_effects.push(Effect::SplitHorizontal);
            }
            Command::SplitVertical => {
                self.pending_effects.push(Effect::SplitVertical);
            }
            Command::PaneLeft => {
                self.pending_effects.push(Effect::PaneLeft);
            }
            Command::PaneDown => {
                self.pending_effects.push(Effect::PaneDown);
            }
            Command::PaneUp => {
                self.pending_effects.push(Effect::PaneUp);
            }
            Command::PaneRight => {
                self.pending_effects.push(Effect::PaneRight);
            }
            Command::PaneNext => {
                self.pending_effects.push(Effect::PaneNext);
            }
            Command::PaneClose => {
                self.pending_effects.push(Effect::PaneClose);
            }
            Command::PaneOnly => {
                self.set_message("pane-only is handled by the host editor");
            }
            Command::PaneEqualize => {
                self.set_message("pane equalize is handled by the host editor");
            }
            Command::PaneRotateForward => {
                self.set_message("pane rotate is handled by the host editor");
            }
            Command::PaneRotateBackward => {
                self.set_message("pane rotate is handled by the host editor");
            }
            Command::PaneMoveLeft => {
                self.pending_effects.push(Effect::PaneLeft);
            }
            Command::PaneMoveDown => {
                self.pending_effects.push(Effect::PaneDown);
            }
            Command::PaneMoveUp => {
                self.pending_effects.push(Effect::PaneUp);
            }
            Command::PaneMoveRight => {
                self.pending_effects.push(Effect::PaneRight);
            }
            Command::PaneResizeWider
            | Command::PaneResizeNarrower
            | Command::PaneResizeTaller
            | Command::PaneResizeShorter => {
                self.set_message("pane resize is handled by the host editor");
            }

            Command::CmdInput(ch) => self.command_input(ch),
            Command::CmdBackspace => self.command_backspace(),
            Command::CmdExecute => self.command_execute(),
            Command::CmdHistoryPrev => self.command_history_prev(),
            Command::CmdHistoryNext => self.command_history_next(),
        }
    }

    pub fn execute_invocation(&mut self, invocation: CommandInvocation) {
        let count = invocation.count.max(1);
        match invocation.command {
            Command::GotoBottom if count > 1 => self.goto_line(count),
            Command::GotoTop if count > 1 => self.goto_line(count),
            Command::GotoLine => self.goto_line(count),
            Command::DeleteLine => self.delete_lines(count),
            Command::YankLine => self.yank_lines(count),
            Command::DeleteMotion(motion) => {
                if count == 1 {
                    self.delete_motion(&motion);
                } else {
                    self.delete_motion_count(&motion, count);
                }
            }
            Command::ChangeMotion(motion) => {
                if count == 1 {
                    self.change_motion(&motion);
                } else {
                    self.change_motion_count(&motion, count);
                }
            }
            Command::YankMotion(motion) => {
                if count == 1 {
                    self.yank_motion(&motion);
                } else {
                    self.yank_motion_count(&motion, count);
                }
            }
            Command::IndentMotion(motion) => self.indent_motion_count(&motion, count),
            Command::DedentMotion(motion) => self.dedent_motion_count(&motion, count),
            Command::FormatMotion(motion) => self.format_motion_count(&motion, count),
            Command::FilterMotion(motion) => self.filter_motion_count(&motion, count),
            Command::MoveColumn => self.move_column(count),
            Command::ReplaceChar(ch) => self.replace_chars(ch, count),
            command => {
                for _ in 0..count {
                    self.execute(command.clone());
                    if self.should_quit {
                        break;
                    }
                }
            }
        }
    }

    pub fn process_command(&mut self, cmd: Command) -> Vec<Effect> {
        self.pending_effects.clear();
        self.execute(cmd);
        std::mem::take(&mut self.pending_effects)
    }

    pub fn process_key_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.pending_effects)
    }

    pub fn text(&self) -> String {
        self.document.rope.to_string()
    }

    pub fn set_clipboard_content(&mut self, text: impl Into<String>) {
        self.clipboard_content = Some(text.into());
    }

    pub fn process_key(&mut self, key: KeyInput) -> Vec<Effect> {
        self.pending_effects.clear();
        if let Some(invocation) = crate::keymap::map_key(self, key) {
            self.execute_invocation(invocation);
        }
        std::mem::take(&mut self.pending_effects)
    }
}

impl crate::keymap::KeymapState for VimCore {
    fn mode(&self) -> Mode {
        self.mode
    }

    fn pending_keys(&self) -> &[char] {
        &self.pending_keys
    }

    fn clear_pending_keys(&mut self) {
        self.pending_keys.clear();
    }

    fn push_pending_key(&mut self, ch: char) {
        self.pending_keys.push(ch);
    }

    fn count_prefix(&self) -> Option<usize> {
        self.count_prefix
    }

    fn set_count_prefix(&mut self, count: Option<usize>) {
        self.count_prefix = count;
    }

    fn pending_operator_count(&self) -> Option<usize> {
        self.pending_operator_count
    }

    fn set_pending_operator_count(&mut self, count: Option<usize>) {
        self.pending_operator_count = count;
    }

    fn set_selected_register(&mut self, ch: char) {
        self.selected_register = Some(ch);
    }

    fn request_quit(&mut self) {
        self.do_quit();
    }

    fn recording_macro(&self) -> bool {
        self.recording_macro.is_some()
    }
}
