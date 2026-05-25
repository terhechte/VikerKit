use viker_core::config::Config;
use viker_core::editor::Editor;
use viker_core::editor::document::Document;
use viker_core::editor::selection::{Position, SelectionMode};
use viker_core::input::mode::Mode;

fn editor_with_text(text: &str) -> Editor {
    let mut doc = Document::new_empty();
    doc.rope = ropey::Rope::from_str(text);
    Editor::new(doc)
}

// ── Document tests ──────────────────────────────────────────────────

#[test]
fn document_new_empty() {
    let doc = Document::new_empty();
    // ropey stores "\n" as 2 lines (empty line + trailing newline)
    assert_eq!(doc.line_count(), 2);
    assert_eq!(doc.line_len(0), 0);
    assert!(!doc.modified);
    assert!(doc.path.is_none());
}

#[test]
fn document_line_count() {
    let mut doc = Document::new_empty();
    doc.rope = ropey::Rope::from_str("hello\nworld\n");
    assert_eq!(doc.line_count(), 3); // ropey counts trailing newline as extra line
}

#[test]
fn document_insert_char() {
    let mut doc = Document::new_empty();
    doc.insert_char(
        viker_core::editor::selection::Position { row: 0, col: 0 },
        'A',
    );
    assert_eq!(doc.rope.to_string(), "A\n");
    assert!(doc.modified);
}

#[test]
fn document_delete_char_backward() {
    let mut doc = Document::new_empty();
    doc.rope = ropey::Rope::from_str("AB\n");
    let pos = viker_core::editor::selection::Position { row: 0, col: 1 };
    let new_pos = doc.delete_char_backward(pos);
    assert!(new_pos.is_some());
    assert_eq!(new_pos.unwrap().col, 0);
    assert_eq!(doc.rope.to_string(), "B\n");
}

#[test]
fn document_delete_char_backward_at_start() {
    let mut doc = Document::new_empty();
    let pos = viker_core::editor::selection::Position { row: 0, col: 0 };
    let new_pos = doc.delete_char_backward(pos);
    assert!(new_pos.is_none());
}

#[test]
fn document_insert_newline() {
    let mut doc = Document::new_empty();
    doc.rope = ropey::Rope::from_str("hello\n");
    doc.insert_newline(viker_core::editor::selection::Position { row: 0, col: 3 });
    assert_eq!(doc.line_count(), 3);
    assert_eq!(doc.rope.line(0).to_string(), "hel\n");
    assert_eq!(doc.rope.line(1).to_string(), "lo\n");
}

#[test]
fn document_file_name_no_path() {
    let doc = Document::new_empty();
    assert_eq!(doc.file_name(), "[No Name]");
}

// ── Editor cursor movement tests ────────────────────────────────────

#[test]
fn editor_move_right() {
    let mut editor = editor_with_text("hello\n");
    editor.mode = Mode::Normal;
    assert_eq!(editor.cursor.col, 0);
    editor.move_right();
    assert_eq!(editor.cursor.col, 1);
}

#[test]
fn editor_move_right_clamps_at_line_end() {
    let mut editor = editor_with_text("ab\n");
    editor.mode = Mode::Normal;
    editor.move_right(); // col 1
    editor.move_right(); // should stay at 1 (normal mode: last char)
    assert_eq!(editor.cursor.col, 1);
}

#[test]
fn editor_move_down() {
    let mut editor = editor_with_text("line1\nline2\n");
    editor.view.height = 20;
    assert_eq!(editor.cursor.row, 0);
    editor.move_down();
    assert_eq!(editor.cursor.row, 1);
}

#[test]
fn editor_move_up() {
    let mut editor = editor_with_text("line1\nline2\n");
    editor.view.height = 20;
    editor.cursor.row = 1;
    editor.move_up();
    assert_eq!(editor.cursor.row, 0);
}

#[test]
fn editor_move_line_start_end() {
    let mut editor = editor_with_text("  hello\n");
    editor.cursor.col = 3;
    editor.move_line_start();
    assert_eq!(editor.cursor.col, 0);
    editor.move_line_end();
    assert!(editor.cursor.col > 0);
}

#[test]
fn editor_command_toggles_relative_number() {
    let mut editor = editor_with_text("one\ntwo\n");

    editor.command_buffer = "relativenumber".to_string();
    editor.command_execute();

    assert!(editor.config.relative_number);
    assert_eq!(editor.status_message.as_deref(), Some("relativenumber on"));

    editor.command_buffer = "set norelativenumber".to_string();
    editor.command_execute();

    assert!(!editor.config.relative_number);
    assert_eq!(editor.status_message.as_deref(), Some("relativenumber off"));
}

#[test]
fn editor_move_first_non_blank() {
    let mut editor = editor_with_text("  hello\n");
    editor.move_first_non_blank();
    assert_eq!(editor.cursor.col, 2);
}

#[test]
fn editor_goto_top_bottom() {
    let mut editor = editor_with_text("a\nb\nc\nd\n");
    editor.view.height = 20;
    editor.goto_bottom();
    // "a\nb\nc\nd\n" = 5 lines in ropey, last row = 4
    assert_eq!(editor.cursor.row, 4);
    editor.goto_top();
    assert_eq!(editor.cursor.row, 0);
}

// ── Editor editing tests ────────────────────────────────────────────

#[test]
fn editor_insert_char() {
    let mut editor = editor_with_text("\n");
    editor.mode = Mode::Insert;
    editor.insert_char('X');
    assert_eq!(editor.document.rope.line(0).to_string(), "X\n");
    assert_eq!(editor.cursor.col, 1);
}

#[test]
fn editor_delete_line() {
    let mut editor = editor_with_text("aaa\nbbb\nccc\n");
    editor.view.height = 20;
    let orig_count = editor.document.line_count();
    editor.delete_line();
    assert_eq!(editor.document.line_count(), orig_count - 1);
    assert_eq!(editor.document.rope.line(0).to_string(), "bbb\n");
}

#[test]
fn editor_delete_line_single_line() {
    let mut editor = editor_with_text("only\n");
    editor.view.height = 20;
    editor.delete_line();
    // Should have at least 1 line (empty)
    assert!(editor.document.line_count() >= 1);
}

#[test]
fn editor_join_lines() {
    let mut editor = editor_with_text("hello\nworld\n");
    editor.view.height = 20;
    editor.join_lines();
    assert_eq!(editor.document.rope.line(0).to_string(), "hello world\n");
}

#[test]
fn editor_join_lines_last_line_noop() {
    let mut editor = editor_with_text("only\n");
    editor.view.height = 20;
    editor.cursor.row = 0;
    editor.join_lines();
    // "only\n" = 2 lines in ropey, join merges the trailing empty line
    assert_eq!(editor.document.line_count(), 1);
}

#[test]
fn editor_indent_line() {
    let mut editor = editor_with_text("hello\n");
    editor.indent_line();
    assert!(editor.document.rope.line(0).to_string().starts_with("    "));
}

#[test]
fn editor_dedent_line() {
    let mut editor = editor_with_text("    hello\n");
    editor.dedent_line();
    assert_eq!(editor.document.rope.line(0).to_string(), "hello\n");
}

#[test]
fn editor_dedent_no_indent() {
    let mut editor = editor_with_text("hello\n");
    editor.dedent_line();
    assert_eq!(editor.document.rope.line(0).to_string(), "hello\n");
}

#[test]
fn editor_replace_char() {
    let mut editor = editor_with_text("abc\n");
    editor.replace_char('X');
    assert_eq!(editor.document.rope.line(0).to_string(), "Xbc\n");
}

#[test]
fn editor_undo_redo() {
    let mut editor = editor_with_text("original\n");
    editor.view.height = 20;
    // Enter insert mode properly (saves undo snapshot)
    editor.enter_insert_mode();
    editor.insert_char('Z');
    assert!(editor.document.rope.to_string().contains('Z'));
    editor.mode = Mode::Normal;
    editor.undo();
    assert_eq!(editor.document.rope.to_string(), "original\n");
    editor.redo();
    assert!(editor.document.rope.to_string().contains('Z'));
}

// ── Mode transition tests ───────────────────────────────────────────

#[test]
fn editor_enter_insert_mode() {
    let mut editor = editor_with_text("hello\n");
    assert_eq!(editor.mode, Mode::Normal);
    editor.enter_insert_mode();
    assert_eq!(editor.mode, Mode::Insert);
}

#[test]
fn editor_enter_visual_mode() {
    let mut editor = editor_with_text("hello\n");
    editor.enter_visual_mode();
    assert_eq!(editor.mode, Mode::Visual);
    assert!(editor.visual_anchor.is_some());
}

#[test]
fn editor_enter_command_mode() {
    let mut editor = editor_with_text("hello\n");
    editor.enter_command_mode();
    assert_eq!(editor.mode, Mode::Command);
    assert!(editor.command_buffer.is_empty());
}

// ── Cursor and selection placement tests ───────────────────────────

#[test]
fn editor_set_cursor_position_clamps_and_clears_visual_selection() {
    let mut editor = editor_with_text("abc\nde\n");
    editor.enter_visual_mode();

    let pos = editor.set_cursor_position(10, 99);

    assert_eq!(pos.row, 2);
    assert_eq!(pos.col, 0);
    assert_eq!(editor.mode, Mode::Normal);
    assert!(editor.visual_anchor.is_none());
}

#[test]
fn editor_view_cell_maps_to_document_position_without_wrap() {
    let mut editor = editor_with_text("abc\ndefgh\n");
    editor.view.offset_row = 1;
    editor.view.offset_col = 2;
    editor.view.width = 20;
    editor.view.height = 10;

    let pos = editor.position_for_view_cell(0, 1);

    assert_eq!(pos, Position { row: 1, col: 3 });
}

#[test]
fn editor_view_cell_maps_to_document_position_with_wrap() {
    let mut editor = editor_with_text("abcdef\n");
    editor.config.wrap = true;
    editor.view.wrap = true;
    editor.view.width = editor.gutter_width() + 3;
    editor.view.height = 4;

    let pos = editor.position_for_view_cell(1, 1);

    assert_eq!(pos, Position { row: 0, col: 4 });
}

#[test]
fn editor_display_metrics_account_for_tabs_wide_and_combining_characters() {
    let editor = editor_with_text("\tab界e\u{301}x\n");

    assert_eq!(editor.line_display_width(0), 10);
    assert_eq!(editor.display_column_for_position(0, 4), 8);
    assert_eq!(editor.display_column_for_position(0, 6), 9);
    assert_eq!(
        editor.position_for_display_column(0, 7),
        Position { row: 0, col: 3 }
    );
    assert_eq!(
        editor.position_for_display_column(0, 8),
        Position { row: 0, col: 4 }
    );

    let cells = editor.display_cells_for_line(0);
    assert_eq!(cells[0].char_start, 0);
    assert_eq!(cells[0].char_end, 1);
    assert_eq!(cells[0].cell_width, 4);
    assert_eq!(cells[3].cell_start, 6);
    assert_eq!(cells[3].cell_width, 2);
    assert_eq!(cells[4].char_start, 4);
    assert_eq!(cells[4].char_end, 6);
    assert_eq!(cells[4].cell_start, 8);
}

#[test]
fn editor_view_cell_mapping_uses_display_cells_without_wrap() {
    let mut editor = editor_with_text("\tab界x\n");
    editor.view.width = 20;
    editor.view.height = 10;

    assert_eq!(
        editor.position_for_view_cell(0, 3),
        Position { row: 0, col: 0 }
    );
    assert_eq!(
        editor.position_for_view_cell(0, 4),
        Position { row: 0, col: 1 }
    );
    assert_eq!(
        editor.position_for_view_cell(0, 7),
        Position { row: 0, col: 3 }
    );
    assert_eq!(
        editor.position_for_view_cell(0, 8),
        Position { row: 0, col: 4 }
    );

    editor.cursor = Position { row: 0, col: 4 };
    assert_eq!(editor.cursor_display_column(), 8);
    assert_eq!(
        editor.cursor_view_cell(),
        Some(viker_core::editor::display::ViewCell { row: 0, col: 8 })
    );
}

#[test]
fn editor_highlight_spans_for_range_detect_path_language() {
    let mut editor = editor_with_text("# Title\n\nSome **bold** text\n");
    editor.document.path = Some(std::path::PathBuf::from("README.md"));

    assert_eq!(
        editor.syntax_language(),
        Some(viker_core::highlight::SyntaxLanguage::Markdown)
    );

    let spans = editor.highlight_spans_for_range(0, 3);

    assert!(spans.iter().any(|span| span.row == 0
        && span.start_col <= 2
        && 2 < span.end_col
        && span.token == viker_core::highlight::style::SyntaxToken::Heading
        && span.style.fg == Some(viker_core::highlight::style::RgbColor(97, 175, 239))));
    assert!(spans.iter().any(|span| span.row == 2
        && span.start_col <= 7
        && 7 < span.end_col
        && span.token == viker_core::highlight::style::SyntaxToken::Strong
        && span.style.fg == Some(viker_core::highlight::style::RgbColor(224, 108, 117))));
}

#[test]
fn editor_highlight_spans_for_range_respects_requested_line_window() {
    let mut editor = editor_with_text("plain\nfn main() {\n    let answer = 42;\n}\n");
    editor.document.path = Some(std::path::PathBuf::from("main.rs"));

    let spans = editor.highlight_spans_for_range(1, 1);

    assert!(!spans.is_empty());
    assert!(spans.iter().all(|span| span.row == 1));
    assert_eq!(editor.styles_offset, 1);
    assert_eq!(editor.line_styles.len(), 1);
}

#[test]
fn editor_highlight_spans_for_range_respects_forced_language() {
    let mut editor = editor_with_text("fn main() {\n    let answer = 42;\n}\n");

    assert_eq!(editor.syntax_language(), None);
    assert!(editor.highlight_spans_for_range(0, 3).is_empty());

    editor.set_syntax_language(Some(viker_core::highlight::SyntaxLanguage::Rust));
    assert_eq!(
        editor.syntax_language(),
        Some(viker_core::highlight::SyntaxLanguage::Rust)
    );

    let spans = editor.highlight_spans_for_range(0, 3);
    assert!(!spans.is_empty());
    assert!(spans.iter().any(|span| span.row == 0
        && span.start_col <= 0
        && 0 < span.end_col
        && span.token == viker_core::highlight::style::SyntaxToken::Keyword
        && span.style.fg == Some(viker_core::highlight::style::RgbColor(198, 120, 221))));

    editor.set_syntax_language(None);
    assert_eq!(editor.syntax_language(), None);
    assert!(editor.highlight_spans_for_range(0, 3).is_empty());
}

#[test]
fn editor_set_and_extend_selection_use_visual_modes() {
    let mut editor = editor_with_text("abc\ndef\n");

    editor.set_selection_positions(0, 1, 1, 2, SelectionMode::Character);
    assert_eq!(editor.mode, Mode::Visual);
    assert_eq!(editor.visual_anchor, Some(Position { row: 0, col: 1 }));
    assert_eq!(editor.cursor, Position { row: 1, col: 2 });

    editor.extend_selection_to(1, 99);
    assert_eq!(editor.cursor, Position { row: 1, col: 2 });

    editor.clear_selection();
    assert_eq!(editor.mode, Mode::Normal);
    assert!(editor.visual_anchor.is_none());
}

#[test]
fn editor_select_word_and_line_at_positions() {
    let mut editor = editor_with_text("one two\nthree\n");

    let selected = editor.select_word_at(0, 5);
    assert_eq!(
        selected,
        Some((Position { row: 0, col: 4 }, Position { row: 0, col: 6 }))
    );
    assert_eq!(editor.mode, Mode::Visual);

    let selected = editor.select_line_at(1);
    assert_eq!(
        selected,
        Some((Position { row: 1, col: 0 }, Position { row: 1, col: 0 }))
    );
    assert_eq!(editor.mode, Mode::VisualLine);
}

// ── Config tests ────────────────────────────────────────────────────

#[test]
fn config_default_values() {
    let config = Config::default();
    assert_eq!(config.tab_width, 4);
    assert_eq!(config.scroll_off, 5);
    assert!(!config.wrap);
    assert_eq!(config.font_size, 14.0);
    assert!(config.font_family.is_none());
}

#[test]
fn config_load_nonexistent_file() {
    let result = Config::load_from_path(std::path::Path::new("/nonexistent/config.json"));
    assert!(result.warning.is_none());
    assert_eq!(result.config.tab_width, 4); // defaults
}

#[test]
fn config_load_invalid_json() {
    let dir = std::env::temp_dir().join("viker_test_config");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bad.json");
    std::fs::write(&path, "not valid json{{{").unwrap();
    let result = Config::load_from_path(&path);
    assert!(result.warning.is_some());
    assert!(result.warning.unwrap().contains("parse error"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn config_load_partial_json() {
    let dir = std::env::temp_dir().join("viker_test_config2");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("partial.json");
    std::fs::write(&path, r#"{"scroll_off": 10}"#).unwrap();
    let result = Config::load_from_path(&path);
    assert!(result.warning.is_none());
    assert_eq!(result.config.scroll_off, 10);
    assert_eq!(result.config.tab_width, 4); // default
    let _ = std::fs::remove_file(&path);
}

#[test]
fn config_load_font_family() {
    let dir = std::env::temp_dir().join("viker_test_config3");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("font.json");
    std::fs::write(&path, r#"{"font_family": "Menlo"}"#).unwrap();
    let result = Config::load_from_path(&path);
    assert_eq!(result.config.font_family.as_deref(), Some("Menlo"));
    let _ = std::fs::remove_file(&path);
}

// ── Word movement tests ─────────────────────────────────────────────

#[test]
fn editor_move_word_forward() {
    let mut editor = editor_with_text("hello world\n");
    editor.move_word_forward();
    assert!(editor.cursor.col > 0);
    // Should jump past "hello" to "world"
    assert_eq!(editor.cursor.col, 6);
}

#[test]
fn editor_move_word_backward() {
    let mut editor = editor_with_text("hello world\n");
    editor.cursor.col = 8; // in "world"
    editor.move_word_backward();
    assert_eq!(editor.cursor.col, 6); // start of "world"
}
