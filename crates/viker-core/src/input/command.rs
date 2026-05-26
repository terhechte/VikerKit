#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOp {
    Lower,
    Upper,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindKind {
    Find,
    Till,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastFind {
    pub target: char,
    pub kind: FindKind,
    pub direction: FindDirection,
}

#[derive(Debug, Clone)]
pub enum Motion {
    Line,
    WordForward,
    WordEnd,
    WordEndBackward,
    WordBackward,
    LineEnd,
    LineStart,
    FirstNonBlank,
    WORDForward,
    WORDEnd,
    WORDEndBackward,
    WORDBackward,
    ParagraphForward,
    ParagraphBackward,
    SentenceForward,
    SentenceBackward,
    SectionForward,
    SectionBackward,
    DocumentStart,
    DocumentEnd,
    MatchBracket,
    SearchForward,
    SearchBackward,
    Column,
    LineDownFirstNonBlank,
    LineUpFirstNonBlank,
    Inner(char),
    Around(char),
    FindForward(char),
    FindBackward(char),
    TillForward(char),
    TillBackward(char),
    Mark { mark: char, exact: bool },
}

#[derive(Debug, Clone)]
pub enum Command {
    // Movement
    MoveLeft,
    MoveDown,
    MoveUp,
    MoveRight,
    MoveWordForward,
    MoveWordBackward,
    MoveWordEnd,
    MoveWordEndBackward,
    MoveLineStart,
    MoveLineEnd,
    MoveFirstNonBlank,
    MoveWORDForward,
    MoveWORDBackward,
    MoveWORDEnd,
    MoveWORDEndBackward,
    MoveParagraphForward,
    MoveParagraphBackward,
    MoveSentenceForward,
    MoveSentenceBackward,
    MoveSectionForward,
    MoveSectionBackward,
    MoveColumn,
    MoveLineDownFirstNonBlank,
    MoveLineUpFirstNonBlank,
    MoveLastNonBlank,

    // Editing
    InsertChar(char),
    InsertRegister(char),
    DeleteCharForward,
    DeleteCharBackward,
    DeleteCharBackwardNormal,
    DeleteWordBackward,
    DeleteLineBackward,
    DeleteLine,
    SubstituteChar,
    InsertNewlineBelow,
    InsertNewlineAbove,
    InsertNewline,
    InsertTab,
    IndentLine,
    DedentLine,
    FormatMotion(Motion),
    FilterMotion(Motion),

    // Operator + motion
    DeleteMotion(Motion),
    ChangeMotion(Motion),
    YankMotion(Motion),
    IndentMotion(Motion),
    DedentMotion(Motion),

    // Find/till character (standalone motion)
    FindCharForward(char),
    FindCharBackward(char),
    TillCharForward(char),
    TillCharBackward(char),
    RepeatFindForward,
    RepeatFindBackward,

    // Replace character
    ReplaceChar(char),

    // Join lines
    JoinLines,
    JoinLinesNoSpace,

    // Undo/Redo
    Undo,
    Redo,

    // Mode changes
    EnterInsertMode,
    EnterInsertModeAfter,
    EnterInsertModeLineEnd,
    EnterInsertModeFirstNonBlank,
    EnterVisualMode,
    EnterVisualLineMode,
    EnterVisualBlockMode,
    EnterReplaceMode,
    EnterCommandMode,
    ExitToNormalMode,

    // Visual mode operations
    VisualDelete,
    VisualYank,
    VisualChange,
    VisualIndent,
    VisualDedent,
    VisualSwapAnchor,
    VisualSwapBlockCorner,
    RestoreVisualSelection,
    VisualSelect(Motion),
    VisualBlockInsert,
    VisualBlockAppend,

    // Paste
    PasteAfter,
    PasteBefore,
    PasteAfterLeaveAfter,
    PasteBeforeLeaveAfter,

    // Yank line
    YankLine,

    // Jump list
    JumpBack,
    JumpForward,
    SetMark(char),
    GotoMark {
        mark: char,
        exact: bool,
    },
    GotoPreviousPosition {
        exact: bool,
    },

    // Completion
    TriggerCompletion,
    AcceptCompletion,
    CancelCompletion,
    CompletionNext,
    CompletionPrev,

    // LSP actions
    GotoDefinition,
    Hover,
    FindReferences,
    DismissPopup,
    ReferenceNext,
    ReferencePrev,
    ReferenceJump,

    // Search
    EnterSearchMode,
    SearchInput(char),
    SearchBackspace,
    SearchConfirm,
    SearchCancel,
    SearchNext,
    SearchPrev,
    EnterSearchBackwardMode,

    // Extended movement
    GotoTop,
    GotoBottom,
    #[allow(dead_code)]
    GotoLine,
    HalfPageDown,
    HalfPageUp,
    FullPageDown,
    FullPageUp,

    // File finder
    OpenFileFinder,
    FileFinderInput(char),
    FileFinderBackspace,
    FileFinderConfirm,
    FileFinderCancel,
    FileFinderNext,
    FileFinderPrev,

    // Phase 9: Repeat
    RepeatLastChange,

    // Phase 9: Search word under cursor
    SearchWordForward,
    SearchWordBackward,

    // Phase 9: Bracket jump
    MatchBracket,

    // Phase 9: Viewport navigation
    ViewportHigh,
    ViewportMiddle,
    ViewportLow,

    // Phase 9: Scroll positioning
    ScrollCenter,
    ScrollTop,
    ScrollBottom,
    ScrollViewportDown,
    ScrollViewportUp,

    // Phase 9: Buffer switching
    NextBuffer,
    PrevBuffer,

    // Phase 10: Case change
    ToggleCaseChar,
    CaseChange(CaseOp, Motion),
    CaseChangeLine(CaseOp),

    // Phase 10: Number increment/decrement
    IncrementNumber,
    DecrementNumber,

    // Phase 10: Named registers
    #[allow(dead_code)]
    SelectRegister(char),

    // Phase 10: Macro recording
    StartMacro(char),
    StopMacro,
    PlayMacro(char),
    PlayLastMacro,

    // Phase 10: LSP formatting
    #[allow(dead_code)]
    FormatDocument,

    // Phase 11: Diagnostic navigation
    DiagnosticNext,
    DiagnosticPrev,
    DiagnosticList,
    DiagnosticJump,

    // Phase 11: LSP Code Actions
    CodeAction,
    CodeActionNext,
    CodeActionPrev,
    CodeActionAccept,
    CodeActionDismiss,

    // Document-line movement (gj/gk in wrap mode)
    MoveDocumentLineDown,
    MoveDocumentLineUp,

    // Workspace symbol search
    WorkspaceSymbol,
    WorkspaceSymbolInput(char),
    WorkspaceSymbolBackspace,
    WorkspaceSymbolConfirm,
    WorkspaceSymbolCancel,
    WorkspaceSymbolNext,
    WorkspaceSymbolPrev,

    // Window split
    SplitHorizontal,
    SplitVertical,
    PaneLeft,
    PaneDown,
    PaneUp,
    PaneRight,
    PaneNext,
    PaneClose,
    PaneOnly,
    PaneEqualize,
    PaneRotateForward,
    PaneRotateBackward,
    PaneMoveLeft,
    PaneMoveDown,
    PaneMoveUp,
    PaneMoveRight,
    PaneResizeWider,
    #[allow(dead_code)]
    PaneResizeNarrower,
    PaneResizeTaller,
    PaneResizeShorter,

    // Command mode
    CmdInput(char),
    CmdBackspace,
    CmdExecute,
    CmdHistoryPrev,
    CmdHistoryNext,
}

#[derive(Debug, Clone)]
pub struct CommandInvocation {
    pub command: Command,
    pub count: usize,
}

impl CommandInvocation {
    pub fn new(command: Command, count: usize) -> Self {
        Self {
            command,
            count: count.max(1),
        }
    }

    pub fn once(command: Command) -> Self {
        Self::new(command, 1)
    }
}
