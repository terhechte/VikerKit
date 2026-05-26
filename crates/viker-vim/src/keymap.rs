use viker_core::input::command::{CaseOp, Command, CommandInvocation, Motion};
use viker_core::input::mode::Mode;
use viker_core::key::{KeyCode, KeyInput};

pub trait KeymapState {
    fn mode(&self) -> Mode;
    fn pending_keys(&self) -> &[char];
    fn clear_pending_keys(&mut self);
    fn push_pending_key(&mut self, ch: char);
    fn count_prefix(&self) -> Option<usize>;
    fn set_count_prefix(&mut self, count: Option<usize>);
    fn pending_operator_count(&self) -> Option<usize>;
    fn set_pending_operator_count(&mut self, count: Option<usize>);
    fn set_selected_register(&mut self, ch: char);
    fn request_quit(&mut self);

    fn showing_file_finder(&self) -> bool {
        false
    }
    fn showing_workspace_symbols(&self) -> bool {
        false
    }
    fn showing_hover(&self) -> bool {
        false
    }
    fn dismiss_hover(&mut self) {}
    fn showing_references(&self) -> bool {
        false
    }
    fn showing_code_actions(&self) -> bool {
        false
    }
    fn showing_diagnostics(&self) -> bool {
        false
    }
    fn showing_completion(&self) -> bool {
        false
    }
    fn recording_macro(&self) -> bool {
        false
    }
}

fn push_count_digit<S: KeymapState + ?Sized>(editor: &mut S, digit: char) {
    let value = digit.to_digit(10).unwrap_or(0) as usize;
    let next = editor.count_prefix().unwrap_or(0).saturating_mul(10) + value;
    editor.set_count_prefix(Some(next.max(1)));
}

fn take_count<S: KeymapState + ?Sized>(editor: &mut S) -> usize {
    let count = editor.count_prefix().unwrap_or(1).max(1);
    editor.set_count_prefix(None);
    count
}

fn inv<S: KeymapState + ?Sized>(editor: &mut S, command: Command) -> Option<CommandInvocation> {
    Some(CommandInvocation::new(command, take_count(editor)))
}

fn pending_inv<S: KeymapState + ?Sized>(
    editor: &mut S,
    command: Command,
) -> Option<CommandInvocation> {
    let op_count = editor.pending_operator_count().unwrap_or(1).max(1);
    editor.set_pending_operator_count(None);
    Some(CommandInvocation::new(
        command,
        op_count.saturating_mul(take_count(editor)),
    ))
}

fn start_pending<S: KeymapState + ?Sized>(editor: &mut S, ch: char) {
    let count = take_count(editor);
    editor.set_pending_operator_count(Some(count));
    editor.push_pending_key(ch);
}

pub fn map_key<S: KeymapState + ?Sized>(
    editor: &mut S,
    key: KeyInput,
) -> Option<CommandInvocation> {
    // File finder intercepts all keys when showing
    if editor.showing_file_finder() {
        return map_file_finder(key).map(CommandInvocation::once);
    }

    // Workspace symbol search intercepts all keys when showing
    if editor.showing_workspace_symbols() {
        return map_workspace_symbols(key).map(CommandInvocation::once);
    }

    match editor.mode() {
        Mode::Normal => map_normal(editor, key),
        Mode::Insert => map_insert(editor, key),
        Mode::Replace => map_replace(key).map(CommandInvocation::once),
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => map_visual(editor, key),
        Mode::Command => map_command(key).map(CommandInvocation::once),
        Mode::Search => map_search(key).map(CommandInvocation::once),
    }
}

fn map_normal<S: KeymapState + ?Sized>(editor: &mut S, key: KeyInput) -> Option<CommandInvocation> {
    // Dismiss popups on any key if showing hover or references
    if editor.showing_hover() {
        editor.dismiss_hover();
        return None;
    }
    if editor.showing_references() {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => return inv(editor, Command::ReferenceNext),
            KeyCode::Char('k') | KeyCode::Up => return inv(editor, Command::ReferencePrev),
            KeyCode::Enter => return inv(editor, Command::ReferenceJump),
            KeyCode::Esc | KeyCode::Char('q') => return inv(editor, Command::DismissPopup),
            _ => return inv(editor, Command::DismissPopup),
        }
    }
    if editor.showing_code_actions() {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => return inv(editor, Command::CodeActionNext),
            KeyCode::Char('k') | KeyCode::Up => return inv(editor, Command::CodeActionPrev),
            KeyCode::Enter => return inv(editor, Command::CodeActionAccept),
            KeyCode::Esc | KeyCode::Char('q') => return inv(editor, Command::CodeActionDismiss),
            _ => return inv(editor, Command::CodeActionDismiss),
        }
    }
    if editor.showing_diagnostics() {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => return inv(editor, Command::DiagnosticNext),
            KeyCode::Char('k') | KeyCode::Up => return inv(editor, Command::DiagnosticPrev),
            KeyCode::Enter => return inv(editor, Command::DiagnosticJump),
            KeyCode::Esc | KeyCode::Char('q') => return inv(editor, Command::DismissPopup),
            _ => return None,
        }
    }

    if editor.pending_keys().is_empty() {
        if let KeyCode::Char(ch @ '1'..='9') = key.code {
            push_count_digit(editor, ch);
            return None;
        }
        if let KeyCode::Char('0') = key.code
            && editor.count_prefix().is_some()
        {
            push_count_digit(editor, '0');
            return None;
        }
    }

    // Handle pending keys (operators, g-prefix, etc.)
    if !editor.pending_keys().is_empty() {
        return handle_pending(editor, key);
    }

    // Ctrl-modified keys first (before plain char matches)
    if key.ctrl {
        match key.code {
            KeyCode::Char('d') => return inv(editor, Command::HalfPageDown),
            KeyCode::Char('e') => return inv(editor, Command::ScrollViewportDown),
            KeyCode::Char('u') => return inv(editor, Command::HalfPageUp),
            KeyCode::Char('y') => return inv(editor, Command::ScrollViewportUp),
            KeyCode::Char('f') => return inv(editor, Command::FullPageDown),
            KeyCode::Char('b') => return inv(editor, Command::FullPageUp),
            KeyCode::Char('r') => return inv(editor, Command::Redo),
            KeyCode::Char('p') => return inv(editor, Command::OpenFileFinder),
            KeyCode::Char('t') => return inv(editor, Command::WorkspaceSymbol),
            KeyCode::Char('o') => return inv(editor, Command::JumpBack),
            KeyCode::Char('i') => return inv(editor, Command::JumpForward),
            KeyCode::Char('a') => return inv(editor, Command::IncrementNumber),
            KeyCode::Char('x') => return inv(editor, Command::DecrementNumber),
            KeyCode::Char('v') => return inv(editor, Command::EnterVisualBlockMode),
            KeyCode::Char(']') => return inv(editor, Command::GotoDefinition),
            KeyCode::Char('w') => {
                start_pending(editor, 'W'); // uppercase to avoid collision
                return None;
            }
            KeyCode::Char('c') => {
                editor.request_quit();
                return None;
            }
            _ => return None,
        }
    }

    match key.code {
        // Movement
        KeyCode::Char('h') | KeyCode::Left => inv(editor, Command::MoveLeft),
        KeyCode::Char('j') | KeyCode::Down => inv(editor, Command::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => inv(editor, Command::MoveUp),
        KeyCode::Char('l') | KeyCode::Right => inv(editor, Command::MoveRight),
        KeyCode::Char('w') => inv(editor, Command::MoveWordForward),
        KeyCode::Char('b') => inv(editor, Command::MoveWordBackward),
        KeyCode::Char('e') => inv(editor, Command::MoveWordEnd),
        KeyCode::Char('0') => inv(editor, Command::MoveLineStart),
        KeyCode::Char('$') => inv(editor, Command::MoveLineEnd),
        KeyCode::Char('^') => inv(editor, Command::MoveFirstNonBlank),
        KeyCode::Char('W') => inv(editor, Command::MoveWORDForward),
        KeyCode::Char('B') => inv(editor, Command::MoveWORDBackward),
        KeyCode::Char('E') => inv(editor, Command::MoveWORDEnd),
        KeyCode::Char('{') => inv(editor, Command::MoveParagraphBackward),
        KeyCode::Char('}') => inv(editor, Command::MoveParagraphForward),
        KeyCode::Char('(') => inv(editor, Command::MoveSentenceBackward),
        KeyCode::Char(')') => inv(editor, Command::MoveSentenceForward),
        KeyCode::Char('|') => inv(editor, Command::MoveColumn),
        KeyCode::Char('+') => inv(editor, Command::MoveLineDownFirstNonBlank),
        KeyCode::Char('-') => inv(editor, Command::MoveLineUpFirstNonBlank),
        KeyCode::Char('_') => inv(editor, Command::MoveFirstNonBlank),
        KeyCode::Char('G') => inv(editor, Command::GotoBottom),

        // Enter insert mode
        KeyCode::Char('i') => inv(editor, Command::EnterInsertMode),
        KeyCode::Char('a') => inv(editor, Command::EnterInsertModeAfter),
        KeyCode::Char('A') => inv(editor, Command::EnterInsertModeLineEnd),
        KeyCode::Char('I') => inv(editor, Command::EnterInsertModeFirstNonBlank),

        // Editing
        KeyCode::Char('o') => inv(editor, Command::InsertNewlineBelow),
        KeyCode::Char('O') => inv(editor, Command::InsertNewlineAbove),
        KeyCode::Char('x') => inv(editor, Command::DeleteCharForward),
        KeyCode::Char('X') => inv(editor, Command::DeleteCharBackwardNormal),
        KeyCode::Char('s') => inv(editor, Command::SubstituteChar),
        KeyCode::Char('S') => inv(editor, Command::ChangeMotion(Motion::Line)),
        KeyCode::Char('R') => inv(editor, Command::EnterReplaceMode),
        KeyCode::Char('J') => inv(editor, Command::JoinLines),
        KeyCode::Char('Y') => inv(editor, Command::YankLine),
        KeyCode::Char('D') => inv(editor, Command::DeleteMotion(Motion::LineEnd)),
        KeyCode::Char('C') => inv(editor, Command::ChangeMotion(Motion::LineEnd)),

        // Repeat last change
        KeyCode::Char('.') => inv(editor, Command::RepeatLastChange),

        // Toggle case of char under cursor
        KeyCode::Char('~') => inv(editor, Command::ToggleCaseChar),

        // Search word under cursor
        KeyCode::Char('*') => inv(editor, Command::SearchWordForward),
        KeyCode::Char('#') => inv(editor, Command::SearchWordBackward),

        // Matching bracket
        KeyCode::Char('%') => inv(editor, Command::MatchBracket),

        // Viewport navigation
        KeyCode::Char('H') => inv(editor, Command::ViewportHigh),
        KeyCode::Char('M') => inv(editor, Command::ViewportMiddle),
        KeyCode::Char('L') => inv(editor, Command::ViewportLow),

        // Pending operators
        KeyCode::Char('d') => {
            start_pending(editor, 'd');
            None
        }
        KeyCode::Char('c') => {
            start_pending(editor, 'c');
            None
        }
        KeyCode::Char('y') => {
            start_pending(editor, 'y');
            None
        }
        KeyCode::Char('g') => {
            start_pending(editor, 'g');
            None
        }
        KeyCode::Char('>') => {
            start_pending(editor, '>');
            None
        }
        KeyCode::Char('<') => {
            start_pending(editor, '<');
            None
        }
        KeyCode::Char('=') => {
            start_pending(editor, '=');
            None
        }
        KeyCode::Char('!') => {
            start_pending(editor, '!');
            None
        }
        KeyCode::Char('f') => {
            start_pending(editor, 'f');
            None
        }
        KeyCode::Char('F') => {
            start_pending(editor, 'F');
            None
        }
        KeyCode::Char('t') => {
            start_pending(editor, 't');
            None
        }
        KeyCode::Char('T') => {
            start_pending(editor, 'T');
            None
        }
        KeyCode::Char('r') => {
            start_pending(editor, 'r');
            None
        }
        KeyCode::Char('z') => {
            start_pending(editor, 'z');
            None
        }

        // Diagnostic/bracket prefix
        KeyCode::Char(']') => {
            start_pending(editor, ']');
            None
        }
        KeyCode::Char('[') => {
            start_pending(editor, '[');
            None
        }

        // Register prefix
        KeyCode::Char('"') => {
            editor.push_pending_key('"');
            None
        }
        KeyCode::Char('m') => {
            editor.push_pending_key('m');
            None
        }
        KeyCode::Char('\'') => {
            editor.push_pending_key('\'');
            None
        }
        KeyCode::Char('`') => {
            editor.push_pending_key('`');
            None
        }

        // Macro: q to start/stop, @ to play
        KeyCode::Char('q') => {
            if editor.recording_macro() {
                inv(editor, Command::StopMacro)
            } else {
                editor.push_pending_key('q');
                None
            }
        }
        KeyCode::Char('@') => {
            editor.push_pending_key('@');
            None
        }

        // LSP: Hover
        KeyCode::Char('K') => inv(editor, Command::Hover),

        // Paste
        KeyCode::Char('p') => inv(editor, Command::PasteAfter),
        KeyCode::Char('P') => inv(editor, Command::PasteBefore),

        // Undo
        KeyCode::Char('u') => inv(editor, Command::Undo),

        // Visual mode
        KeyCode::Char('v') => inv(editor, Command::EnterVisualMode),
        KeyCode::Char('V') => inv(editor, Command::EnterVisualLineMode),

        // Search
        KeyCode::Char('/') => inv(editor, Command::EnterSearchMode),
        KeyCode::Char('?') => inv(editor, Command::EnterSearchBackwardMode),
        KeyCode::Char('n') => inv(editor, Command::SearchNext),
        KeyCode::Char('N') => inv(editor, Command::SearchPrev),
        KeyCode::Char(';') => inv(editor, Command::RepeatFindForward),
        KeyCode::Char(',') => inv(editor, Command::RepeatFindBackward),

        // Command mode
        KeyCode::Char(':') => inv(editor, Command::EnterCommandMode),

        _ => None,
    }
}

fn handle_pending<S: KeymapState + ?Sized>(
    editor: &mut S,
    key: KeyInput,
) -> Option<CommandInvocation> {
    // Esc always cancels pending
    if key.code == KeyCode::Esc {
        editor.clear_pending_keys();
        return None;
    }

    let ch = match key.code {
        KeyCode::Char(ch) => ch,
        _ => {
            editor.clear_pending_keys();
            return None;
        }
    };

    let pending = editor.pending_keys().to_vec();
    editor.clear_pending_keys();

    if ch.is_ascii_digit() && (ch != '0' || editor.count_prefix().is_some()) {
        push_count_digit(editor, ch);
        for pending_ch in pending {
            editor.push_pending_key(pending_ch);
        }
        return None;
    }

    match *pending.as_slice() {
        // --- Register prefix ---
        ['"'] => {
            editor.set_selected_register(ch);
            None
        }

        // --- Macro ---
        ['q'] => pending_inv(editor, Command::StartMacro(ch)),
        ['@'] => {
            if ch == '@' {
                pending_inv(editor, Command::PlayLastMacro)
            } else {
                pending_inv(editor, Command::PlayMacro(ch))
            }
        }

        // --- Marks ---
        ['m'] => pending_inv(editor, Command::SetMark(ch)),
        ['\''] => {
            if ch == '\'' {
                pending_inv(editor, Command::GotoPreviousPosition { exact: false })
            } else {
                pending_inv(
                    editor,
                    Command::GotoMark {
                        mark: ch,
                        exact: false,
                    },
                )
            }
        }
        ['`'] => {
            if ch == '`' {
                pending_inv(editor, Command::GotoPreviousPosition { exact: true })
            } else {
                pending_inv(
                    editor,
                    Command::GotoMark {
                        mark: ch,
                        exact: true,
                    },
                )
            }
        }

        // --- Operators: d, c, y ---
        ['d'] => match ch {
            'd' => pending_inv(editor, Command::DeleteLine),
            'w' => pending_inv(editor, Command::DeleteMotion(Motion::WordForward)),
            'e' => pending_inv(editor, Command::DeleteMotion(Motion::WordEnd)),
            'b' => pending_inv(editor, Command::DeleteMotion(Motion::WordBackward)),
            'W' => pending_inv(editor, Command::DeleteMotion(Motion::WORDForward)),
            'E' => pending_inv(editor, Command::DeleteMotion(Motion::WORDEnd)),
            'B' => pending_inv(editor, Command::DeleteMotion(Motion::WORDBackward)),
            'G' => pending_inv(editor, Command::DeleteMotion(Motion::DocumentEnd)),
            '$' => pending_inv(editor, Command::DeleteMotion(Motion::LineEnd)),
            '0' => pending_inv(editor, Command::DeleteMotion(Motion::LineStart)),
            '^' => pending_inv(editor, Command::DeleteMotion(Motion::FirstNonBlank)),
            '%' => pending_inv(editor, Command::DeleteMotion(Motion::MatchBracket)),
            '}' => pending_inv(editor, Command::DeleteMotion(Motion::ParagraphForward)),
            '{' => pending_inv(editor, Command::DeleteMotion(Motion::ParagraphBackward)),
            ')' => pending_inv(editor, Command::DeleteMotion(Motion::SentenceForward)),
            '(' => pending_inv(editor, Command::DeleteMotion(Motion::SentenceBackward)),
            '/' => pending_inv(editor, Command::DeleteMotion(Motion::SearchForward)),
            '?' => pending_inv(editor, Command::DeleteMotion(Motion::SearchBackward)),
            '|' => pending_inv(editor, Command::DeleteMotion(Motion::Column)),
            '+' => pending_inv(editor, Command::DeleteMotion(Motion::LineDownFirstNonBlank)),
            '-' => pending_inv(editor, Command::DeleteMotion(Motion::LineUpFirstNonBlank)),
            'i' | 'a' | 'f' | 'F' | 't' | 'T' | 'g' | '[' | ']' | '\'' | '`' => {
                editor.push_pending_key('d');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['d', 'g'] => match ch {
            'g' => pending_inv(editor, Command::DeleteMotion(Motion::DocumentStart)),
            'e' => pending_inv(editor, Command::DeleteMotion(Motion::WordEndBackward)),
            'E' => pending_inv(editor, Command::DeleteMotion(Motion::WORDEndBackward)),
            _ => None,
        },
        ['d', 'i'] => pending_inv(editor, Command::DeleteMotion(Motion::Inner(ch))),
        ['d', 'a'] => pending_inv(editor, Command::DeleteMotion(Motion::Around(ch))),
        ['d', 'f'] => pending_inv(editor, Command::DeleteMotion(Motion::FindForward(ch))),
        ['d', 'F'] => pending_inv(editor, Command::DeleteMotion(Motion::FindBackward(ch))),
        ['d', 't'] => pending_inv(editor, Command::DeleteMotion(Motion::TillForward(ch))),
        ['d', 'T'] => pending_inv(editor, Command::DeleteMotion(Motion::TillBackward(ch))),
        ['d', '\''] => pending_inv(
            editor,
            Command::DeleteMotion(Motion::Mark {
                mark: ch,
                exact: false,
            }),
        ),
        ['d', '`'] => pending_inv(
            editor,
            Command::DeleteMotion(Motion::Mark {
                mark: ch,
                exact: true,
            }),
        ),
        ['d', ']'] => match ch {
            ']' | '[' => pending_inv(editor, Command::DeleteMotion(Motion::SectionForward)),
            _ => None,
        },
        ['d', '['] => match ch {
            '[' | ']' => pending_inv(editor, Command::DeleteMotion(Motion::SectionBackward)),
            _ => None,
        },

        ['c'] => match ch {
            'c' => pending_inv(editor, Command::ChangeMotion(Motion::Line)),
            'w' => pending_inv(editor, Command::ChangeMotion(Motion::WordForward)),
            'e' => pending_inv(editor, Command::ChangeMotion(Motion::WordEnd)),
            'b' => pending_inv(editor, Command::ChangeMotion(Motion::WordBackward)),
            'W' => pending_inv(editor, Command::ChangeMotion(Motion::WORDForward)),
            'E' => pending_inv(editor, Command::ChangeMotion(Motion::WORDEnd)),
            'B' => pending_inv(editor, Command::ChangeMotion(Motion::WORDBackward)),
            'G' => pending_inv(editor, Command::ChangeMotion(Motion::DocumentEnd)),
            '$' => pending_inv(editor, Command::ChangeMotion(Motion::LineEnd)),
            '0' => pending_inv(editor, Command::ChangeMotion(Motion::LineStart)),
            '^' => pending_inv(editor, Command::ChangeMotion(Motion::FirstNonBlank)),
            '%' => pending_inv(editor, Command::ChangeMotion(Motion::MatchBracket)),
            '}' => pending_inv(editor, Command::ChangeMotion(Motion::ParagraphForward)),
            '{' => pending_inv(editor, Command::ChangeMotion(Motion::ParagraphBackward)),
            ')' => pending_inv(editor, Command::ChangeMotion(Motion::SentenceForward)),
            '(' => pending_inv(editor, Command::ChangeMotion(Motion::SentenceBackward)),
            '/' => pending_inv(editor, Command::ChangeMotion(Motion::SearchForward)),
            '?' => pending_inv(editor, Command::ChangeMotion(Motion::SearchBackward)),
            '|' => pending_inv(editor, Command::ChangeMotion(Motion::Column)),
            '+' => pending_inv(editor, Command::ChangeMotion(Motion::LineDownFirstNonBlank)),
            '-' => pending_inv(editor, Command::ChangeMotion(Motion::LineUpFirstNonBlank)),
            'i' | 'a' | 'f' | 'F' | 't' | 'T' | 'g' | '[' | ']' | '\'' | '`' => {
                editor.push_pending_key('c');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['c', 'g'] => match ch {
            'g' => pending_inv(editor, Command::ChangeMotion(Motion::DocumentStart)),
            'e' => pending_inv(editor, Command::ChangeMotion(Motion::WordEndBackward)),
            'E' => pending_inv(editor, Command::ChangeMotion(Motion::WORDEndBackward)),
            _ => None,
        },
        ['c', 'i'] => pending_inv(editor, Command::ChangeMotion(Motion::Inner(ch))),
        ['c', 'a'] => pending_inv(editor, Command::ChangeMotion(Motion::Around(ch))),
        ['c', 'f'] => pending_inv(editor, Command::ChangeMotion(Motion::FindForward(ch))),
        ['c', 'F'] => pending_inv(editor, Command::ChangeMotion(Motion::FindBackward(ch))),
        ['c', 't'] => pending_inv(editor, Command::ChangeMotion(Motion::TillForward(ch))),
        ['c', 'T'] => pending_inv(editor, Command::ChangeMotion(Motion::TillBackward(ch))),
        ['c', '\''] => pending_inv(
            editor,
            Command::ChangeMotion(Motion::Mark {
                mark: ch,
                exact: false,
            }),
        ),
        ['c', '`'] => pending_inv(
            editor,
            Command::ChangeMotion(Motion::Mark {
                mark: ch,
                exact: true,
            }),
        ),
        ['c', ']'] => match ch {
            ']' | '[' => pending_inv(editor, Command::ChangeMotion(Motion::SectionForward)),
            _ => None,
        },
        ['c', '['] => match ch {
            '[' | ']' => pending_inv(editor, Command::ChangeMotion(Motion::SectionBackward)),
            _ => None,
        },

        ['y'] => match ch {
            'y' => pending_inv(editor, Command::YankLine),
            'w' => pending_inv(editor, Command::YankMotion(Motion::WordForward)),
            'e' => pending_inv(editor, Command::YankMotion(Motion::WordEnd)),
            'b' => pending_inv(editor, Command::YankMotion(Motion::WordBackward)),
            'W' => pending_inv(editor, Command::YankMotion(Motion::WORDForward)),
            'E' => pending_inv(editor, Command::YankMotion(Motion::WORDEnd)),
            'B' => pending_inv(editor, Command::YankMotion(Motion::WORDBackward)),
            'G' => pending_inv(editor, Command::YankMotion(Motion::DocumentEnd)),
            '$' => pending_inv(editor, Command::YankMotion(Motion::LineEnd)),
            '0' => pending_inv(editor, Command::YankMotion(Motion::LineStart)),
            '^' => pending_inv(editor, Command::YankMotion(Motion::FirstNonBlank)),
            '%' => pending_inv(editor, Command::YankMotion(Motion::MatchBracket)),
            '}' => pending_inv(editor, Command::YankMotion(Motion::ParagraphForward)),
            '{' => pending_inv(editor, Command::YankMotion(Motion::ParagraphBackward)),
            ')' => pending_inv(editor, Command::YankMotion(Motion::SentenceForward)),
            '(' => pending_inv(editor, Command::YankMotion(Motion::SentenceBackward)),
            '/' => pending_inv(editor, Command::YankMotion(Motion::SearchForward)),
            '?' => pending_inv(editor, Command::YankMotion(Motion::SearchBackward)),
            '|' => pending_inv(editor, Command::YankMotion(Motion::Column)),
            '+' => pending_inv(editor, Command::YankMotion(Motion::LineDownFirstNonBlank)),
            '-' => pending_inv(editor, Command::YankMotion(Motion::LineUpFirstNonBlank)),
            'i' | 'a' | 'g' | '[' | ']' | '\'' | '`' => {
                editor.push_pending_key('y');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['y', 'g'] => match ch {
            'g' => pending_inv(editor, Command::YankMotion(Motion::DocumentStart)),
            'e' => pending_inv(editor, Command::YankMotion(Motion::WordEndBackward)),
            'E' => pending_inv(editor, Command::YankMotion(Motion::WORDEndBackward)),
            _ => None,
        },
        ['y', 'i'] => pending_inv(editor, Command::YankMotion(Motion::Inner(ch))),
        ['y', 'a'] => pending_inv(editor, Command::YankMotion(Motion::Around(ch))),
        ['y', '\''] => pending_inv(
            editor,
            Command::YankMotion(Motion::Mark {
                mark: ch,
                exact: false,
            }),
        ),
        ['y', '`'] => pending_inv(
            editor,
            Command::YankMotion(Motion::Mark {
                mark: ch,
                exact: true,
            }),
        ),
        ['y', ']'] => match ch {
            ']' | '[' => pending_inv(editor, Command::YankMotion(Motion::SectionForward)),
            _ => None,
        },
        ['y', '['] => match ch {
            '[' | ']' => pending_inv(editor, Command::YankMotion(Motion::SectionBackward)),
            _ => None,
        },

        // --- g-prefix ---
        ['g'] => match ch {
            'd' => pending_inv(editor, Command::GotoDefinition),
            'D' => pending_inv(editor, Command::GotoDefinition),
            'r' => pending_inv(editor, Command::FindReferences),
            'g' => pending_inv(editor, Command::GotoTop),
            'e' => pending_inv(editor, Command::MoveWordEndBackward),
            'E' => pending_inv(editor, Command::MoveWORDEndBackward),
            '_' => pending_inv(editor, Command::MoveLastNonBlank),
            'J' => pending_inv(editor, Command::JoinLinesNoSpace),
            'p' => pending_inv(editor, Command::PasteAfterLeaveAfter),
            'P' => pending_inv(editor, Command::PasteBeforeLeaveAfter),
            'a' => pending_inv(editor, Command::CodeAction),
            't' => pending_inv(editor, Command::NextBuffer),
            'T' => pending_inv(editor, Command::PrevBuffer),
            'j' => pending_inv(editor, Command::MoveDown),
            'k' => pending_inv(editor, Command::MoveUp),
            'v' => pending_inv(editor, Command::RestoreVisualSelection),
            // Case change: gu/gU/g~ + motion
            'u' | 'U' | '~' => {
                editor.push_pending_key('g');
                editor.push_pending_key(ch);
                None
            }
            'q' => {
                editor.push_pending_key('g');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },

        // --- Case change: gu{motion}, gU{motion}, g~{motion} ---
        ['g', op @ ('u' | 'U' | '~')] => map_case_motion(editor, op, ch),
        ['g', 'u', 'i'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Lower, Motion::Inner(ch)),
        ),
        ['g', 'u', 'a'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Lower, Motion::Around(ch)),
        ),
        ['g', 'U', 'i'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Upper, Motion::Inner(ch)),
        ),
        ['g', 'U', 'a'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Upper, Motion::Around(ch)),
        ),
        ['g', '~', 'i'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Toggle, Motion::Inner(ch)),
        ),
        ['g', '~', 'a'] => pending_inv(
            editor,
            Command::CaseChange(CaseOp::Toggle, Motion::Around(ch)),
        ),
        ['g', 'q'] => match ch {
            'q' => pending_inv(editor, Command::FormatMotion(Motion::Line)),
            'w' => pending_inv(editor, Command::FormatMotion(Motion::WordForward)),
            '}' => pending_inv(editor, Command::FormatMotion(Motion::ParagraphForward)),
            'i' | 'a' => {
                editor.push_pending_key('g');
                editor.push_pending_key('q');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['g', 'q', 'i'] => pending_inv(editor, Command::FormatMotion(Motion::Inner(ch))),
        ['g', 'q', 'a'] => pending_inv(editor, Command::FormatMotion(Motion::Around(ch))),

        // --- Indent/dedent ---
        ['>'] => match ch {
            '>' => pending_inv(editor, Command::IndentLine),
            'w' => pending_inv(editor, Command::IndentMotion(Motion::WordForward)),
            '%' => pending_inv(editor, Command::IndentMotion(Motion::MatchBracket)),
            '$' => pending_inv(editor, Command::IndentMotion(Motion::LineEnd)),
            'G' => pending_inv(editor, Command::IndentMotion(Motion::DocumentEnd)),
            '}' => pending_inv(editor, Command::IndentMotion(Motion::ParagraphForward)),
            'i' | 'a' => {
                editor.push_pending_key('>');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['>', 'i'] => pending_inv(editor, Command::IndentMotion(Motion::Inner(ch))),
        ['>', 'a'] => pending_inv(editor, Command::IndentMotion(Motion::Around(ch))),
        ['<'] => match ch {
            '<' => pending_inv(editor, Command::DedentLine),
            'w' => pending_inv(editor, Command::DedentMotion(Motion::WordForward)),
            '%' => pending_inv(editor, Command::DedentMotion(Motion::MatchBracket)),
            '$' => pending_inv(editor, Command::DedentMotion(Motion::LineEnd)),
            'G' => pending_inv(editor, Command::DedentMotion(Motion::DocumentEnd)),
            '}' => pending_inv(editor, Command::DedentMotion(Motion::ParagraphForward)),
            'i' | 'a' => {
                editor.push_pending_key('<');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['<', 'i'] => pending_inv(editor, Command::DedentMotion(Motion::Inner(ch))),
        ['<', 'a'] => pending_inv(editor, Command::DedentMotion(Motion::Around(ch))),
        ['='] => match ch {
            '=' => pending_inv(editor, Command::FormatMotion(Motion::Line)),
            'w' => pending_inv(editor, Command::FormatMotion(Motion::WordForward)),
            '%' => pending_inv(editor, Command::FormatMotion(Motion::MatchBracket)),
            '$' => pending_inv(editor, Command::FormatMotion(Motion::LineEnd)),
            'G' => pending_inv(editor, Command::FormatMotion(Motion::DocumentEnd)),
            '}' => pending_inv(editor, Command::FormatMotion(Motion::ParagraphForward)),
            'i' | 'a' => {
                editor.push_pending_key('=');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['=', 'i'] => pending_inv(editor, Command::FormatMotion(Motion::Inner(ch))),
        ['=', 'a'] => pending_inv(editor, Command::FormatMotion(Motion::Around(ch))),
        ['!'] => match ch {
            '!' => pending_inv(editor, Command::FilterMotion(Motion::Line)),
            'w' => pending_inv(editor, Command::FilterMotion(Motion::WordForward)),
            '%' => pending_inv(editor, Command::FilterMotion(Motion::MatchBracket)),
            '$' => pending_inv(editor, Command::FilterMotion(Motion::LineEnd)),
            'G' => pending_inv(editor, Command::FilterMotion(Motion::DocumentEnd)),
            '}' => pending_inv(editor, Command::FilterMotion(Motion::ParagraphForward)),
            'i' | 'a' => {
                editor.push_pending_key('!');
                editor.push_pending_key(ch);
                None
            }
            _ => None,
        },
        ['!', 'i'] => pending_inv(editor, Command::FilterMotion(Motion::Inner(ch))),
        ['!', 'a'] => pending_inv(editor, Command::FilterMotion(Motion::Around(ch))),

        // --- Find/till character ---
        ['f'] => pending_inv(editor, Command::FindCharForward(ch)),
        ['F'] => pending_inv(editor, Command::FindCharBackward(ch)),
        ['t'] => pending_inv(editor, Command::TillCharForward(ch)),
        ['T'] => pending_inv(editor, Command::TillCharBackward(ch)),

        // --- Replace character ---
        ['r'] => pending_inv(editor, Command::ReplaceChar(ch)),

        // --- Diagnostic navigation ---
        [']'] => match ch {
            'd' => pending_inv(editor, Command::DiagnosticNext),
            'D' => pending_inv(editor, Command::DiagnosticList),
            ']' | '[' => pending_inv(editor, Command::MoveSectionForward),
            'p' => pending_inv(editor, Command::PasteAfter),
            _ => None,
        },
        ['['] => match ch {
            'd' => pending_inv(editor, Command::DiagnosticPrev),
            '[' | ']' => pending_inv(editor, Command::MoveSectionBackward),
            _ => None,
        },

        // --- Window split (Ctrl-W prefix) ---
        ['W'] => match ch {
            'v' => pending_inv(editor, Command::SplitVertical),
            's' => pending_inv(editor, Command::SplitHorizontal),
            'h' => pending_inv(editor, Command::PaneLeft),
            'j' => pending_inv(editor, Command::PaneDown),
            'k' => pending_inv(editor, Command::PaneUp),
            'l' => pending_inv(editor, Command::PaneRight),
            'w' => pending_inv(editor, Command::PaneNext),
            'q' => pending_inv(editor, Command::PaneClose),
            'o' => pending_inv(editor, Command::PaneOnly),
            '=' => pending_inv(editor, Command::PaneEqualize),
            'r' => pending_inv(editor, Command::PaneRotateForward),
            'R' => pending_inv(editor, Command::PaneRotateBackward),
            'H' => pending_inv(editor, Command::PaneMoveLeft),
            'J' => pending_inv(editor, Command::PaneMoveDown),
            'K' => pending_inv(editor, Command::PaneMoveUp),
            'L' => pending_inv(editor, Command::PaneMoveRight),
            '|' => pending_inv(editor, Command::PaneResizeWider),
            '_' => pending_inv(editor, Command::PaneResizeTaller),
            '+' => pending_inv(editor, Command::PaneResizeTaller),
            '-' => pending_inv(editor, Command::PaneResizeShorter),
            _ => None,
        },

        // --- Scroll positioning ---
        ['z'] => match ch {
            'z' => pending_inv(editor, Command::ScrollCenter),
            't' => pending_inv(editor, Command::ScrollTop),
            'b' => pending_inv(editor, Command::ScrollBottom),
            _ => None,
        },

        _ => None,
    }
}

/// Helper for case change motions (gu/gU/g~ + motion key).
fn map_case_motion<S: KeymapState + ?Sized>(
    editor: &mut S,
    op: char,
    ch: char,
) -> Option<CommandInvocation> {
    let case_op = match op {
        'u' => CaseOp::Lower,
        'U' => CaseOp::Upper,
        '~' => CaseOp::Toggle,
        _ => return None,
    };
    match ch {
        c if c == op => pending_inv(editor, Command::CaseChangeLine(case_op)),
        'w' => pending_inv(editor, Command::CaseChange(case_op, Motion::WordForward)),
        'e' => pending_inv(editor, Command::CaseChange(case_op, Motion::WordEnd)),
        'b' => pending_inv(editor, Command::CaseChange(case_op, Motion::WordBackward)),
        'W' => pending_inv(editor, Command::CaseChange(case_op, Motion::WORDForward)),
        'E' => pending_inv(editor, Command::CaseChange(case_op, Motion::WORDEnd)),
        'B' => pending_inv(editor, Command::CaseChange(case_op, Motion::WORDBackward)),
        'G' => pending_inv(editor, Command::CaseChange(case_op, Motion::DocumentEnd)),
        '$' => pending_inv(editor, Command::CaseChange(case_op, Motion::LineEnd)),
        '0' => pending_inv(editor, Command::CaseChange(case_op, Motion::LineStart)),
        '^' => pending_inv(editor, Command::CaseChange(case_op, Motion::FirstNonBlank)),
        '%' => pending_inv(editor, Command::CaseChange(case_op, Motion::MatchBracket)),
        'i' | 'a' => {
            editor.push_pending_key('g');
            editor.push_pending_key(op);
            editor.push_pending_key(ch);
            None
        }
        _ => None,
    }
}

fn map_visual<S: KeymapState + ?Sized>(editor: &mut S, key: KeyInput) -> Option<CommandInvocation> {
    if !editor.pending_keys().is_empty() {
        return handle_visual_pending(editor, key);
    }

    match key.code {
        KeyCode::Char('"') => {
            editor.push_pending_key('"');
            return None;
        }
        KeyCode::Char('i') => {
            editor.push_pending_key('i');
            return None;
        }
        KeyCode::Char('a') => {
            editor.push_pending_key('a');
            return None;
        }
        KeyCode::Char('I') if editor.mode() == Mode::VisualBlock => {
            return Some(CommandInvocation::once(Command::VisualBlockInsert));
        }
        KeyCode::Char('A') if editor.mode() == Mode::VisualBlock => {
            return Some(CommandInvocation::once(Command::VisualBlockAppend));
        }
        _ => {}
    }

    map_visual_command(key).map(CommandInvocation::once)
}

fn handle_visual_pending<S: KeymapState + ?Sized>(
    editor: &mut S,
    key: KeyInput,
) -> Option<CommandInvocation> {
    if key.code == KeyCode::Esc {
        editor.clear_pending_keys();
        return None;
    }
    let ch = match key.code {
        KeyCode::Char(ch) => ch,
        _ => {
            editor.clear_pending_keys();
            return None;
        }
    };
    let pending = editor.pending_keys().to_vec();
    editor.clear_pending_keys();
    match *pending.as_slice() {
        ['"'] => {
            editor.set_selected_register(ch);
            None
        }
        ['i'] => Some(CommandInvocation::once(Command::VisualSelect(
            Motion::Inner(ch),
        ))),
        ['a'] => Some(CommandInvocation::once(Command::VisualSelect(
            Motion::Around(ch),
        ))),
        _ => None,
    }
}

fn map_visual_command(key: KeyInput) -> Option<Command> {
    // Ctrl-modified keys
    if key.ctrl {
        match key.code {
            KeyCode::Char('d') => return Some(Command::HalfPageDown),
            KeyCode::Char('u') => return Some(Command::HalfPageUp),
            KeyCode::Char('f') => return Some(Command::FullPageDown),
            KeyCode::Char('b') => return Some(Command::FullPageUp),
            KeyCode::Char('v') => return Some(Command::EnterVisualBlockMode),
            KeyCode::Char('c') => return Some(Command::ExitToNormalMode),
            _ => return None,
        }
    }

    match key.code {
        // Movement
        KeyCode::Char('h') | KeyCode::Left => Some(Command::MoveLeft),
        KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
        KeyCode::Char('l') | KeyCode::Right => Some(Command::MoveRight),
        KeyCode::Char('w') => Some(Command::MoveWordForward),
        KeyCode::Char('b') => Some(Command::MoveWordBackward),
        KeyCode::Char('e') => Some(Command::MoveWordEnd),
        KeyCode::Char('W') => Some(Command::MoveWORDForward),
        KeyCode::Char('B') => Some(Command::MoveWORDBackward),
        KeyCode::Char('E') => Some(Command::MoveWORDEnd),
        KeyCode::Char('0') => Some(Command::MoveLineStart),
        KeyCode::Char('$') => Some(Command::MoveLineEnd),
        KeyCode::Char('^') => Some(Command::MoveFirstNonBlank),
        KeyCode::Char('{') => Some(Command::MoveParagraphBackward),
        KeyCode::Char('}') => Some(Command::MoveParagraphForward),
        KeyCode::Char('G') => Some(Command::GotoBottom),
        KeyCode::Char('%') => Some(Command::MatchBracket),

        // Swap anchor/cursor
        KeyCode::Char('o') => Some(Command::VisualSwapAnchor),
        KeyCode::Char('O') => Some(Command::VisualSwapBlockCorner),

        // Case change on selection
        KeyCode::Char('~') => Some(Command::ToggleCaseChar),
        KeyCode::Char('u') => Some(Command::CaseChangeLine(CaseOp::Lower)),
        KeyCode::Char('U') => Some(Command::CaseChangeLine(CaseOp::Upper)),

        // Operations on selection
        KeyCode::Char('d') | KeyCode::Char('x') => Some(Command::VisualDelete),
        KeyCode::Char('y') => Some(Command::VisualYank),
        KeyCode::Char('c') => Some(Command::VisualChange),
        KeyCode::Char('>') => Some(Command::VisualIndent),
        KeyCode::Char('<') => Some(Command::VisualDedent),

        // Exit
        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => Some(Command::ExitToNormalMode),

        _ => None,
    }
}

fn map_insert<S: KeymapState + ?Sized>(editor: &mut S, key: KeyInput) -> Option<CommandInvocation> {
    if !editor.pending_keys().is_empty() {
        return handle_insert_pending(editor, key);
    }

    // When completion popup is showing, intercept navigation keys
    if editor.showing_completion() {
        match key.code {
            KeyCode::Down | KeyCode::Tab => {
                return Some(CommandInvocation::once(Command::CompletionNext));
            }
            KeyCode::Up | KeyCode::BackTab => {
                return Some(CommandInvocation::once(Command::CompletionPrev));
            }
            KeyCode::Enter => return Some(CommandInvocation::once(Command::AcceptCompletion)),
            KeyCode::Esc => return Some(CommandInvocation::once(Command::CancelCompletion)),
            _ => {
                // Any other key dismisses completion and falls through
            }
        }
    }

    let command = match key.code {
        KeyCode::Esc => Some(Command::ExitToNormalMode),
        KeyCode::Backspace => Some(Command::DeleteCharBackward),
        KeyCode::Enter => Some(Command::InsertNewline),
        KeyCode::Left => Some(Command::MoveLeft),
        KeyCode::Right => Some(Command::MoveRight),
        KeyCode::Up => Some(Command::MoveUp),
        KeyCode::Down => Some(Command::MoveDown),
        KeyCode::Char('a') if key.ctrl => Some(Command::MoveLineStart),
        KeyCode::Char('e') if key.ctrl => Some(Command::MoveLineEnd),
        KeyCode::Char('f') if key.ctrl => Some(Command::MoveRight),
        KeyCode::Char('b') if key.ctrl => Some(Command::MoveLeft),
        KeyCode::Char('n') if key.ctrl => Some(Command::MoveDown),
        KeyCode::Char('p') if key.ctrl => Some(Command::MoveUp),
        KeyCode::Char('f') if key.alt => Some(Command::MoveWordForward),
        KeyCode::Char('b') if key.alt => Some(Command::MoveWordBackward),
        KeyCode::Char('w') if key.ctrl => Some(Command::DeleteWordBackward),
        KeyCode::Char('u') if key.ctrl => Some(Command::DeleteLineBackward),
        KeyCode::Char('o') if key.ctrl => {
            editor.push_pending_key('O');
            return None;
        }
        KeyCode::Char('r') if key.ctrl => {
            editor.push_pending_key('R');
            return None;
        }
        // Ctrl-Space to trigger completion
        KeyCode::Char(' ') if key.ctrl => Some(Command::TriggerCompletion),
        KeyCode::Char(ch) => Some(Command::InsertChar(ch)),
        KeyCode::Tab => Some(Command::InsertTab),
        _ => None,
    };
    command.map(CommandInvocation::once)
}

fn handle_insert_pending<S: KeymapState + ?Sized>(
    editor: &mut S,
    key: KeyInput,
) -> Option<CommandInvocation> {
    let pending = editor.pending_keys().to_vec();
    if pending.first() == Some(&'O') {
        return handle_insert_normal_pending(editor, key, &pending[1..]);
    }

    if key.code == KeyCode::Esc {
        editor.clear_pending_keys();
        return Some(CommandInvocation::once(Command::ExitToNormalMode));
    }
    let ch = match key.code {
        KeyCode::Char(ch) => ch,
        _ => {
            editor.clear_pending_keys();
            return None;
        }
    };
    editor.clear_pending_keys();
    match *pending.as_slice() {
        ['R'] => Some(CommandInvocation::once(Command::InsertRegister(ch))),
        _ => None,
    }
}

fn handle_insert_normal_pending<S: KeymapState + ?Sized>(
    editor: &mut S,
    key: KeyInput,
    normal_pending: &[char],
) -> Option<CommandInvocation> {
    editor.clear_pending_keys();
    for ch in normal_pending {
        editor.push_pending_key(*ch);
    }

    let invocation = map_normal(editor, key);
    if invocation.is_none()
        && (!editor.pending_keys().is_empty()
            || editor.count_prefix().is_some()
            || editor.pending_operator_count().is_some())
    {
        let pending = editor.pending_keys().to_vec();
        editor.clear_pending_keys();
        editor.push_pending_key('O');
        for ch in pending {
            editor.push_pending_key(ch);
        }
    }
    invocation
}

fn map_replace(key: KeyInput) -> Option<Command> {
    match key.code {
        KeyCode::Esc => Some(Command::ExitToNormalMode),
        KeyCode::Backspace => Some(Command::DeleteCharBackward),
        KeyCode::Enter => Some(Command::InsertNewline),
        KeyCode::Left => Some(Command::MoveLeft),
        KeyCode::Right => Some(Command::MoveRight),
        KeyCode::Up => Some(Command::MoveUp),
        KeyCode::Down => Some(Command::MoveDown),
        KeyCode::Char(ch) => Some(Command::ReplaceChar(ch)),
        KeyCode::Tab => Some(Command::ReplaceChar('\t')),
        _ => None,
    }
}

fn map_command(key: KeyInput) -> Option<Command> {
    match key.code {
        KeyCode::Esc => Some(Command::ExitToNormalMode),
        KeyCode::Enter => Some(Command::CmdExecute),
        KeyCode::Backspace => Some(Command::CmdBackspace),
        KeyCode::Up => Some(Command::CmdHistoryPrev),
        KeyCode::Down => Some(Command::CmdHistoryNext),
        KeyCode::Char(ch) => Some(Command::CmdInput(ch)),
        _ => None,
    }
}

fn map_search(key: KeyInput) -> Option<Command> {
    match key.code {
        KeyCode::Esc => Some(Command::SearchCancel),
        KeyCode::Enter => Some(Command::SearchConfirm),
        KeyCode::Backspace => Some(Command::SearchBackspace),
        KeyCode::Char(ch) => Some(Command::SearchInput(ch)),
        _ => None,
    }
}

fn map_workspace_symbols(key: KeyInput) -> Option<Command> {
    match key.code {
        KeyCode::Esc => Some(Command::WorkspaceSymbolCancel),
        KeyCode::Enter => Some(Command::WorkspaceSymbolConfirm),
        KeyCode::Backspace => Some(Command::WorkspaceSymbolBackspace),
        KeyCode::Down | KeyCode::Tab => Some(Command::WorkspaceSymbolNext),
        KeyCode::Up | KeyCode::BackTab => Some(Command::WorkspaceSymbolPrev),
        KeyCode::Char('n') if key.ctrl => Some(Command::WorkspaceSymbolNext),
        KeyCode::Char('p') if key.ctrl => Some(Command::WorkspaceSymbolPrev),
        KeyCode::Char(ch) => Some(Command::WorkspaceSymbolInput(ch)),
        _ => None,
    }
}

fn map_file_finder(key: KeyInput) -> Option<Command> {
    match key.code {
        KeyCode::Esc => Some(Command::FileFinderCancel),
        KeyCode::Enter => Some(Command::FileFinderConfirm),
        KeyCode::Backspace => Some(Command::FileFinderBackspace),
        KeyCode::Down | KeyCode::Tab => Some(Command::FileFinderNext),
        KeyCode::Up | KeyCode::BackTab => Some(Command::FileFinderPrev),
        KeyCode::Char('n') if key.ctrl => Some(Command::FileFinderNext),
        KeyCode::Char('p') if key.ctrl => Some(Command::FileFinderPrev),
        KeyCode::Char(ch) => Some(Command::FileFinderInput(ch)),
        _ => None,
    }
}
