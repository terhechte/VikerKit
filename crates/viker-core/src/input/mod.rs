pub mod command;
pub mod mode;

use crate::editor::pane::{NavigateDir, SplitDirection};
use crate::editor::{DeferredAction, Editor, LastChange};
use crate::input::command::{Command, CommandInvocation};

pub fn execute_invocation(
    editor: &mut Editor,
    invocation: CommandInvocation,
) -> Option<DeferredAction> {
    let count = invocation.count.max(1);
    match invocation.command {
        Command::GotoBottom if count > 1 => {
            editor.goto_line(count);
            None
        }
        Command::GotoTop if count > 1 => {
            editor.goto_line(count);
            None
        }
        Command::GotoLine => {
            editor.goto_line(count);
            None
        }
        Command::DeleteLine => {
            editor.delete_lines(count);
            None
        }
        Command::YankLine => {
            editor.yank_lines(count);
            None
        }
        Command::DeleteMotion(motion) => {
            if count == 1 {
                editor.delete_motion(&motion);
            } else {
                editor.delete_motion_count(&motion, count);
            }
            None
        }
        Command::ChangeMotion(motion) => {
            if count == 1 {
                editor.change_motion(&motion);
            } else {
                editor.change_motion_count(&motion, count);
            }
            None
        }
        Command::YankMotion(motion) => {
            if count == 1 {
                editor.yank_motion(&motion);
            } else {
                editor.yank_motion_count(&motion, count);
            }
            None
        }
        Command::IndentMotion(motion) => {
            editor.indent_motion_count(&motion, count);
            None
        }
        Command::DedentMotion(motion) => {
            editor.dedent_motion_count(&motion, count);
            None
        }
        Command::FormatMotion(motion) => {
            editor.format_motion_count(&motion, count);
            None
        }
        Command::FilterMotion(motion) => {
            editor.filter_motion_count(&motion, count);
            None
        }
        Command::MoveColumn => {
            editor.move_column(count);
            None
        }
        Command::ReplaceChar(ch) => {
            editor.replace_chars(ch, count);
            None
        }
        command => {
            let mut deferred = None;
            for _ in 0..count {
                deferred = execute(editor, command.clone()).or(deferred);
                if editor.should_quit {
                    break;
                }
            }
            deferred
        }
    }
}

pub fn execute(editor: &mut Editor, cmd: Command) -> Option<DeferredAction> {
    // Clear status message on any input
    editor.status_message = None;

    // Track changes for `.` repeat
    track_change(editor, &cmd);

    match cmd {
        // Movement
        Command::MoveLeft => editor.move_left(),
        Command::MoveDown => editor.move_down(),
        Command::MoveUp => editor.move_up(),
        Command::MoveRight => editor.move_right(),
        Command::MoveWordForward => editor.move_word_forward(),
        Command::MoveWordBackward => editor.move_word_backward(),
        Command::MoveWordEnd => editor.move_word_end(),
        Command::MoveWordEndBackward => editor.move_word_end_backward(),
        Command::MoveLineStart => editor.move_line_start(),
        Command::MoveLineEnd => editor.move_line_end(),
        Command::MoveFirstNonBlank => editor.move_first_non_blank(),
        Command::MoveWORDForward => editor.move_word_forward_big(),
        Command::MoveWORDBackward => editor.move_word_backward_big(),
        Command::MoveWORDEnd => editor.move_word_end_big(),
        Command::MoveWORDEndBackward => editor.move_word_end_backward_big(),
        Command::MoveParagraphForward => editor.move_paragraph_forward(),
        Command::MoveParagraphBackward => editor.move_paragraph_backward(),
        Command::MoveSentenceForward => editor.move_sentence_forward(),
        Command::MoveSentenceBackward => editor.move_sentence_backward(),
        Command::MoveSectionForward => editor.move_section_forward(),
        Command::MoveSectionBackward => editor.move_section_backward(),
        Command::MoveColumn => editor.move_column(1),
        Command::MoveLineDownFirstNonBlank => editor.move_line_down_first_non_blank(),
        Command::MoveLineUpFirstNonBlank => editor.move_line_up_first_non_blank(),
        Command::MoveDocumentLineDown => editor.move_document_line_down(),
        Command::MoveDocumentLineUp => editor.move_document_line_up(),

        // Editing
        Command::InsertChar(ch) => editor.insert_char(ch),
        Command::DeleteCharForward => editor.delete_char_forward(),
        Command::DeleteCharBackward => editor.delete_char_backward(),
        Command::DeleteCharBackwardNormal => editor.delete_char_backward_normal(),
        Command::DeleteWordBackward => editor.delete_word_backward(),
        Command::DeleteLineBackward => editor.delete_line_backward(),
        Command::DeleteLine => editor.delete_line(),
        Command::SubstituteChar => editor.substitute_char(),
        Command::InsertNewlineBelow => editor.insert_newline_below(),
        Command::InsertNewlineAbove => editor.insert_newline_above(),
        Command::InsertNewline => editor.insert_newline(),
        Command::InsertTab => editor.insert_tab(),
        Command::IndentLine => editor.indent_line(),
        Command::DedentLine => editor.dedent_line(),
        Command::FormatMotion(ref motion) => editor.format_motion_count(motion, 1),
        Command::FilterMotion(ref motion) => editor.filter_motion_count(motion, 1),

        // Operator + motion
        Command::DeleteMotion(ref motion) => editor.delete_motion(motion),
        Command::ChangeMotion(ref motion) => editor.change_motion(motion),
        Command::YankMotion(ref motion) => editor.yank_motion(motion),
        Command::IndentMotion(ref motion) => editor.indent_motion_count(motion, 1),
        Command::DedentMotion(ref motion) => editor.dedent_motion_count(motion, 1),

        // Find/till character
        Command::FindCharForward(ch) => editor.find_char_forward(ch),
        Command::FindCharBackward(ch) => editor.find_char_backward(ch),
        Command::TillCharForward(ch) => editor.till_char_forward(ch),
        Command::TillCharBackward(ch) => editor.till_char_backward(ch),
        Command::RepeatFindForward => editor.repeat_find(false),
        Command::RepeatFindBackward => editor.repeat_find(true),

        // Replace character
        Command::ReplaceChar(ch) => editor.replace_char(ch),

        // Join lines
        Command::JoinLines => editor.join_lines(),

        // Undo/Redo
        Command::Undo => editor.undo(),
        Command::Redo => editor.redo(),

        // Mode changes
        Command::EnterInsertMode => editor.enter_insert_mode(),
        Command::EnterInsertModeAfter => editor.enter_insert_mode_after(),
        Command::EnterInsertModeLineEnd => editor.enter_insert_mode_line_end(),
        Command::EnterInsertModeFirstNonBlank => editor.enter_insert_mode_first_non_blank(),
        Command::EnterVisualMode => editor.enter_visual_mode(),
        Command::EnterVisualLineMode => editor.enter_visual_line_mode(),
        Command::EnterVisualBlockMode => editor.enter_visual_block_mode(),
        Command::EnterReplaceMode => editor.enter_replace_mode(),
        Command::EnterCommandMode => editor.enter_command_mode(),
        Command::ExitToNormalMode => editor.exit_to_normal_mode(),

        // Visual mode operations
        Command::VisualDelete => editor.visual_delete(),
        Command::VisualYank => editor.visual_yank(),
        Command::VisualChange => editor.visual_change(),
        Command::VisualIndent => editor.visual_indent(),
        Command::VisualDedent => editor.visual_dedent(),
        Command::VisualSwapAnchor => editor.visual_swap_anchor(),
        Command::VisualSwapBlockCorner => editor.visual_swap_block_corner(),
        Command::RestoreVisualSelection => editor.restore_visual_selection(),
        Command::VisualSelect(ref motion) => editor.visual_select_motion(motion),
        Command::VisualBlockInsert => editor.visual_block_insert(),
        Command::VisualBlockAppend => editor.visual_block_append(),

        // Paste
        Command::PasteAfter => editor.paste_after(),
        Command::PasteBefore => editor.paste_before(),

        // Yank line
        Command::YankLine => editor.yank_line(),

        // Jump list
        Command::JumpBack => editor.jump_back(),
        Command::JumpForward => editor.jump_forward(),
        Command::SetMark(ch) => editor.set_mark(ch),
        Command::GotoMark { mark, exact } => editor.goto_mark(mark, exact),
        Command::GotoPreviousPosition { exact } => editor.goto_previous_position(exact),

        // Completion
        Command::TriggerCompletion => {}
        Command::AcceptCompletion => editor.accept_completion(),
        Command::CancelCompletion => editor.cancel_completion(),
        Command::CompletionNext => editor.completion_next(),
        Command::CompletionPrev => editor.completion_prev(),

        // LSP actions (async requests handled by app.rs)
        Command::GotoDefinition => {}
        Command::Hover => {}
        Command::FindReferences => {}
        Command::DismissPopup => editor.dismiss_popup(),
        Command::ReferenceNext => editor.reference_next(),
        Command::ReferencePrev => editor.reference_prev(),
        Command::ReferenceJump => {}

        // Search
        Command::EnterSearchMode => editor.enter_search_mode(),
        Command::EnterSearchBackwardMode => editor.enter_search_backward_mode(),
        Command::SearchInput(ch) => editor.search_input(ch),
        Command::SearchBackspace => editor.search_backspace(),
        Command::SearchConfirm => editor.search_confirm(),
        Command::SearchCancel => editor.search_cancel(),
        Command::SearchNext => editor.search_next(),
        Command::SearchPrev => editor.search_prev(),

        // Repeat
        Command::RepeatLastChange => editor.repeat_last_change(),

        // Search word under cursor
        Command::SearchWordForward => editor.search_word_forward(),
        Command::SearchWordBackward => editor.search_word_backward(),

        // Bracket jump
        Command::MatchBracket => editor.match_bracket_jump(),

        // Viewport navigation
        Command::ViewportHigh => editor.viewport_high(),
        Command::ViewportMiddle => editor.viewport_middle(),
        Command::ViewportLow => editor.viewport_low(),

        // Scroll positioning
        Command::ScrollCenter => editor.scroll_center(),
        Command::ScrollTop => editor.scroll_top(),
        Command::ScrollBottom => editor.scroll_bottom(),

        // Buffer switching
        Command::NextBuffer => return editor.next_buffer(),
        Command::PrevBuffer => return editor.prev_buffer(),

        // Phase 10: Case change
        Command::ToggleCaseChar => editor.toggle_case_char(),
        Command::CaseChange(op, ref motion) => editor.case_change(op, motion),
        Command::CaseChangeLine(op) => editor.case_change_line(op),

        // Phase 10: Number increment/decrement
        Command::IncrementNumber => editor.increment_number(1),
        Command::DecrementNumber => editor.increment_number(-1),

        // Phase 10: Named registers (handled in keymap, no-op here)
        Command::SelectRegister(_) => {}

        // Phase 10: Macro (start/stop handled here, play handled by app.rs)
        Command::StartMacro(ch) => editor.start_macro(ch),
        Command::StopMacro => editor.stop_macro(),
        Command::PlayMacro(ch) => return Some(DeferredAction::PlayMacro(ch)),
        Command::PlayLastMacro => {
            if let Some(ch) = editor.last_macro {
                return Some(DeferredAction::PlayMacro(ch));
            }
        }

        // Phase 10: LSP formatting (handled by app.rs)
        Command::FormatDocument => return Some(DeferredAction::FormatDocument),

        // Phase 11: Diagnostic navigation
        Command::DiagnosticNext => {
            if editor.showing_diagnostics {
                editor.diagnostic_list_next();
            } else {
                editor.diagnostic_next();
            }
        }
        Command::DiagnosticPrev => {
            if editor.showing_diagnostics {
                editor.diagnostic_list_prev();
            } else {
                editor.diagnostic_prev();
            }
        }
        Command::DiagnosticList => editor.toggle_diagnostics_list(),
        Command::DiagnosticJump => editor.diagnostic_list_jump(),

        // Phase 11: LSP Code Actions
        Command::CodeAction => {} // async, handled by app.rs
        Command::CodeActionNext => editor.code_action_next(),
        Command::CodeActionPrev => editor.code_action_prev(),
        Command::CodeActionAccept => {} // async, handled by app.rs
        Command::CodeActionDismiss => editor.dismiss_code_actions(),

        // Extended movement
        Command::GotoTop => editor.goto_top(),
        Command::GotoBottom => editor.goto_bottom(),
        Command::GotoLine => editor.goto_line(1),
        Command::HalfPageDown => editor.half_page_down(),
        Command::HalfPageUp => editor.half_page_up(),
        Command::FullPageDown => editor.full_page_down(),
        Command::FullPageUp => editor.full_page_up(),

        // Workspace symbol search (async parts handled by app.rs)
        Command::WorkspaceSymbol => {}
        Command::WorkspaceSymbolInput(ch) => editor.workspace_symbol_input(ch),
        Command::WorkspaceSymbolBackspace => editor.workspace_symbol_backspace(),
        Command::WorkspaceSymbolConfirm => {} // handled by app.rs
        Command::WorkspaceSymbolCancel => editor.workspace_symbol_cancel(),
        Command::WorkspaceSymbolNext => editor.workspace_symbol_next(),
        Command::WorkspaceSymbolPrev => editor.workspace_symbol_prev(),

        // File finder (async parts handled by app.rs)
        Command::OpenFileFinder => {}
        Command::FileFinderInput(ch) => editor.file_finder_input(ch),
        Command::FileFinderBackspace => editor.file_finder_backspace(),
        Command::FileFinderConfirm => {
            if let Some(path) = editor.file_finder_selected() {
                editor.file_finder_cancel();
                return Some(DeferredAction::OpenFile(path));
            }
        }
        Command::FileFinderCancel => editor.file_finder_cancel(),
        Command::FileFinderNext => editor.file_finder_next(),
        Command::FileFinderPrev => editor.file_finder_prev(),

        // Window split
        Command::SplitHorizontal => return editor.split_pane(SplitDirection::Horizontal),
        Command::SplitVertical => return editor.split_pane(SplitDirection::Vertical),
        Command::PaneLeft => return editor.navigate_pane(NavigateDir::Left),
        Command::PaneDown => return editor.navigate_pane(NavigateDir::Down),
        Command::PaneUp => return editor.navigate_pane(NavigateDir::Up),
        Command::PaneRight => return editor.navigate_pane(NavigateDir::Right),
        Command::PaneNext => return editor.cycle_pane(),
        Command::PaneClose => return editor.close_pane(),
        Command::PaneOnly => return editor.pane_only(),
        Command::PaneEqualize => return editor.pane_equalize(),
        Command::PaneRotateForward => return editor.pane_rotate_forward(),
        Command::PaneRotateBackward => return editor.pane_rotate_backward(),
        Command::PaneMoveLeft => return editor.pane_move_left(),
        Command::PaneMoveDown => return editor.pane_move_down(),
        Command::PaneMoveUp => return editor.pane_move_up(),
        Command::PaneMoveRight => return editor.pane_move_right(),
        Command::PaneResizeWider => editor.pane_resize_wider(),
        Command::PaneResizeNarrower => editor.pane_resize_narrower(),
        Command::PaneResizeTaller => editor.pane_resize_taller(),
        Command::PaneResizeShorter => editor.pane_resize_shorter(),

        // Command mode
        Command::CmdInput(ch) => editor.command_input(ch),
        Command::CmdBackspace => editor.command_backspace(),
        Command::CmdExecute => return editor.command_execute(),
        Command::CmdHistoryPrev => editor.command_history_prev(),
        Command::CmdHistoryNext => editor.command_history_next(),
    }
    None
}

/// Track changes for `.` (repeat last change) support.
fn track_change(editor: &mut Editor, cmd: &Command) {
    match cmd {
        // Normal mode commands that directly change text
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
            editor.last_change = Some(LastChange::NormalCommand(cmd.clone()));
        }

        // Commands that enter insert mode - start recording
        Command::EnterInsertMode
        | Command::EnterInsertModeAfter
        | Command::EnterInsertModeLineEnd
        | Command::EnterInsertModeFirstNonBlank
        | Command::InsertNewlineBelow
        | Command::InsertNewlineAbove
        | Command::ChangeMotion(_)
        | Command::VisualBlockInsert
        | Command::VisualBlockAppend => {
            editor.recording_insert = true;
            editor.insert_entry_cmd = Some(cmd.clone());
            editor.insert_record.clear();
        }

        // Track keystrokes during insert recording
        Command::InsertChar(ch) if editor.recording_insert => {
            editor.insert_record.push(*ch);
        }
        Command::DeleteCharBackward if editor.recording_insert => {
            editor.insert_record.push('\x08');
        }
        Command::DeleteWordBackward if editor.recording_insert => {
            editor.insert_record.push('\u{17}');
        }
        Command::DeleteLineBackward if editor.recording_insert => {
            editor.insert_record.push('\u{15}');
        }
        Command::InsertNewline if editor.recording_insert => {
            editor.insert_record.push('\n');
        }
        Command::InsertTab if editor.recording_insert => {
            editor.insert_record.push('\t');
        }

        // Finalize insert recording on mode exit
        Command::ExitToNormalMode if editor.recording_insert => {
            editor.recording_insert = false;
            if let Some(entry) = editor.insert_entry_cmd.take() {
                editor.last_change = Some(LastChange::InsertSession {
                    entry_cmd: entry,
                    chars: editor.insert_record.clone(),
                });
            }
        }

        _ => {}
    }
}
