use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc;
use viker_core::editor::document::Document;
use viker_core::editor::selection::{Position, SelectionMode};
use viker_core::editor::{DeferredAction, Editor, HighlightSpan};
use viker_core::highlight::SyntaxLanguage;
use viker_core::highlight::style::{RgbColor, SyntaxStyle, SyntaxToken};
use viker_core::input;
use viker_core::input::mode::Mode;
use viker_core::key::{KeyCode, KeyInput};
use viker_core::lsp::{self, AppEvent, LspClient, LspMessage};
use viker_core::{formatter, git as core_git, language, search as core_search};
use viker_vim::keymap;

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
pub enum VikerError {
    Io { message: String },
    InvalidInput { message: String },
    EditorUnavailable { message: String },
}

impl std::fmt::Display for VikerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { message } => write!(f, "{message}"),
            Self::InvalidInput { message } => write!(f, "{message}"),
            Self::EditorUnavailable { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VikerError {}

impl From<anyhow::Error> for VikerError {
    fn from(value: anyhow::Error) -> Self {
        Self::Io {
            message: value.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerMode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
    Search,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerSelectionMode {
    Character,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerSyntaxLanguage {
    Rust,
    Markdown,
    Html,
    Css,
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Python,
    Fish,
    Bash,
    Zsh,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerSyntaxToken {
    Text,
    Keyword,
    TypeName,
    Tag,
    Attribute,
    Constructor,
    Function,
    Method,
    Macro,
    StringLiteral,
    Escape,
    Character,
    NumberLiteral,
    BooleanLiteral,
    Constant,
    Comment,
    Variable,
    Parameter,
    Property,
    Module,
    Label,
    Punctuation,
    OperatorToken,
    Heading,
    RawText,
    Link,
    LinkUrl,
    Emphasis,
    Strong,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerPosition {
    pub row: u64,
    pub column: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerViewCell {
    pub row: u64,
    pub column: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerDisplayCell {
    pub row: u64,
    pub char_start: u64,
    pub char_end: u64,
    pub cell_start: u64,
    pub cell_width: u64,
    pub text: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerHighlightStyle {
    pub foreground: Option<VikerColor>,
    pub italic: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerHighlightSpan {
    pub row: u64,
    pub start_column: u64,
    pub end_column: u64,
    pub token: VikerSyntaxToken,
    pub style: VikerHighlightStyle,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspServerInfo {
    pub language: VikerSyntaxLanguage,
    pub language_id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub installed: bool,
    pub installable: bool,
    pub install_hint: Option<String>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerLspEventKind {
    Ready,
    DiagnosticsUpdated,
    CompletionUpdated,
    HoverUpdated,
    ReferencesUpdated,
    WorkspaceSymbolsUpdated,
    FormattingApplied,
    RenameApplied,
    Error,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspEvent {
    pub kind: VikerLspEventKind,
    pub message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspStatus {
    pub running: bool,
    pub initialized: bool,
    pub language: Option<VikerSyntaxLanguage>,
    pub root_path: Option<String>,
    pub file_uri: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspRequest {
    pub id: Option<u64>,
    pub completed: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerDiagnostic {
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
    pub severity: u8,
    pub message: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub kind: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLocation {
    pub uri: String,
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerWorkspaceSymbol {
    pub name: String,
    pub kind: u64,
    pub kind_label: String,
    pub uri: String,
    pub start_line: u64,
    pub start_column: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspDocument {
    pub uri: String,
    pub path: String,
    pub language: VikerSyntaxLanguage,
    pub version: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspServerStatus {
    pub running: bool,
    pub initialized: bool,
    pub language: VikerSyntaxLanguage,
    pub root_path: String,
    pub open_document_count: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspWorkspaceStatus {
    pub root_path: String,
    pub servers: Vec<VikerLspServerStatus>,
    pub documents: Vec<VikerLspDocument>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerLspWorkspaceEvent {
    pub kind: VikerLspEventKind,
    pub language: Option<VikerSyntaxLanguage>,
    pub uri: Option<String>,
    pub request_id: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerGitDiffMode {
    Worktree,
    Staged,
    Head,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerGitChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Typechange,
    Untracked,
    Conflicted,
    Unknown,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerGitLineKind {
    Context,
    Addition,
    Deletion,
    Other,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitPatchHighlight {
    pub start_column: u64,
    pub end_column: u64,
    pub token: VikerSyntaxToken,
    pub style: VikerHighlightStyle,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitDiffLine {
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub kind: VikerGitLineKind,
    pub prefix: String,
    pub content: String,
    pub highlights: Vec<VikerGitPatchHighlight>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitDiffHunk {
    pub id: String,
    pub header: String,
    pub old_start: u64,
    pub old_lines: u64,
    pub new_start: u64,
    pub new_lines: u64,
    pub lines: Vec<VikerGitDiffLine>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitFileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub change: VikerGitChangeKind,
    pub binary: bool,
    pub hunks: Vec<VikerGitDiffHunk>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitDiff {
    pub repository_root: String,
    pub mode: VikerGitDiffMode,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub files: Vec<VikerGitFileDiff>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitFileStatus {
    pub path: String,
    pub old_path: Option<String>,
    pub index: Option<VikerGitChangeKind>,
    pub worktree: Option<VikerGitChangeKind>,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitBranch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub target: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitStash {
    pub index: u64,
    pub message: String,
    pub oid: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitStatus {
    pub repository_root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub detached: bool,
    pub files: Vec<VikerGitFileStatus>,
    pub branches: Vec<VikerGitBranch>,
    pub stashes: Vec<VikerGitStash>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerGitOperationReport {
    pub message: String,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerFileSearchResult {
    pub path: String,
    pub score: i64,
    pub matched_indices: Vec<u64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerFileContentSearchResult {
    pub path: String,
    pub row: u64,
    pub column: u64,
    pub text: String,
    pub score: i64,
    pub matched_indices: Vec<u64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerRegisterSummary {
    pub name: String,
    pub prefix: String,
    pub linewise: bool,
    pub is_macro: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerSnapshot {
    pub text: String,
    pub line_count: u64,
    pub cursor: VikerPosition,
    pub visual_anchor: Option<VikerPosition>,
    pub mode: VikerMode,
    pub file_path: Option<String>,
    pub file_name: String,
    pub modified: bool,
    pub status_message: Option<String>,
    pub command_buffer: String,
    pub search_query: String,
    pub register_display: Option<String>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerKey {
    Character,
    Escape,
    Enter,
    Backspace,
    Tab,
    Backtab,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerKeyEvent {
    pub key: VikerKey,
    pub text: Option<String>,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VikerEffectKind {
    Rename,
    DidSave,
    OpenFile,
    SyncFileUri,
    ShellCommand,
    FormatDocument,
    PlayMacro,
    Git,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VikerEffect {
    pub kind: VikerEffectKind,
    pub payload: Option<String>,
}

#[derive(uniffi::Object)]
pub struct VikerEditor {
    editor: Mutex<Editor>,
    lsp: Mutex<VikerLspState>,
    runtime: tokio::runtime::Runtime,
}

struct VikerLspState {
    client: Option<LspClient>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    language: Option<language::LanguageKind>,
    root_path: Option<PathBuf>,
    file_uri: Option<String>,
    last_notified_version: i64,
}

#[derive(uniffi::Object)]
pub struct VikerLspWorkspace {
    root_path: PathBuf,
    state: Mutex<VikerLspWorkspaceState>,
    runtime: tokio::runtime::Runtime,
}

struct VikerLspWorkspaceState {
    servers: HashMap<WorkspaceLspServerKey, WorkspaceLspServer>,
    documents: HashMap<String, WorkspaceDocument>,
    pending: HashMap<i64, WorkspacePendingRequest>,
    diagnostics: HashMap<String, Vec<lsp::LspDiagnostic>>,
    completions: HashMap<u64, Vec<lsp::LspCompletionItem>>,
    hovers: HashMap<u64, Option<String>>,
    references: HashMap<u64, Vec<lsp::LspLocation>>,
    workspace_symbols: HashMap<u64, Vec<lsp::LspSymbolInfo>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WorkspaceLspServerKey {
    command: String,
    args: Vec<String>,
}

struct WorkspaceLspServer {
    client: LspClient,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    primary_language: language::LanguageKind,
}

struct WorkspaceDocument {
    uri: String,
    path: PathBuf,
    language: language::LanguageKind,
    server_key: WorkspaceLspServerKey,
    text: String,
    version: i64,
    tab_width: usize,
    opened: bool,
    editor: Option<Arc<VikerEditor>>,
}

struct WorkspacePendingRequest {
    kind: WorkspacePendingKind,
    language: language::LanguageKind,
    server_key: WorkspaceLspServerKey,
    uri: Option<String>,
    editor: Option<Arc<VikerEditor>>,
}

enum WorkspacePendingKind {
    Completion,
    Hover,
    References,
    WorkspaceSymbols,
    Format,
}

#[uniffi::export]
pub fn viker_git_status(path: String) -> Result<VikerGitStatus, VikerError> {
    Ok(git_status_from_core(core_git::repository_status(path)?))
}

#[uniffi::export]
pub fn viker_git_branches(path: String) -> Result<Vec<VikerGitBranch>, VikerError> {
    Ok(core_git::repository_branches(path)?
        .into_iter()
        .map(git_branch_from_core)
        .collect())
}

#[uniffi::export]
pub fn viker_git_diff(
    path: String,
    mode: VikerGitDiffMode,
    context_lines: u64,
    pathspecs: Vec<String>,
) -> Result<VikerGitDiff, VikerError> {
    Ok(git_diff_from_core(core_git::repository_diff(
        path,
        core_git::GitDiffOptions {
            mode: git_diff_mode_to_core(mode),
            context_lines: checked_u32(context_lines, "context_lines")?,
            pathspecs,
            ..core_git::GitDiffOptions::default()
        },
    )?))
}

#[uniffi::export]
pub fn viker_git_diff_json(
    path: String,
    mode: VikerGitDiffMode,
    context_lines: u64,
    pathspecs: Vec<String>,
) -> Result<String, VikerError> {
    Ok(core_git::repository_diff_json(
        path,
        core_git::GitDiffOptions {
            mode: git_diff_mode_to_core(mode),
            context_lines: checked_u32(context_lines, "context_lines")?,
            pathspecs,
            ..core_git::GitDiffOptions::default()
        },
    )?)
}

#[uniffi::export]
pub fn viker_project_files(path: String) -> Result<Vec<String>, VikerError> {
    Ok(core_search::scan_project_files(path))
}

#[uniffi::export]
pub fn viker_search_files(
    path: String,
    query: String,
    limit: u64,
) -> Result<Vec<VikerFileSearchResult>, VikerError> {
    Ok(
        core_search::search_project_files(path, &query, checked_index(limit, "limit")?)
            .into_iter()
            .map(file_search_result_from_core)
            .collect(),
    )
}

#[uniffi::export]
pub fn viker_search_file_contents(
    path: String,
    query: String,
    limit: u64,
) -> Result<Vec<VikerFileContentSearchResult>, VikerError> {
    Ok(
        core_search::search_file_contents(path, &query, checked_index(limit, "limit")?)
            .into_iter()
            .map(file_content_search_result_from_core)
            .collect(),
    )
}

#[uniffi::export]
pub fn viker_git_stage_files(
    path: String,
    paths: Vec<String>,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::stage_files(path, &paths)?))
}

#[uniffi::export]
pub fn viker_git_unstage_files(
    path: String,
    paths: Vec<String>,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::unstage_files(path, &paths)?))
}

#[uniffi::export]
pub fn viker_git_stage_hunk(
    path: String,
    file_path: String,
    hunk_id: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::stage_hunk(
        path, &file_path, &hunk_id,
    )?))
}

#[uniffi::export]
pub fn viker_git_unstage_hunk(
    path: String,
    file_path: String,
    hunk_id: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::unstage_hunk(
        path, &file_path, &hunk_id,
    )?))
}

#[uniffi::export]
pub fn viker_git_delete_files(
    path: String,
    paths: Vec<String>,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::delete_files(path, &paths)?))
}

#[uniffi::export]
pub fn viker_git_create_branch(
    path: String,
    name: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::create_branch(path, &name)?))
}

#[uniffi::export]
pub fn viker_git_checkout_branch(
    path: String,
    name: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::checkout_branch(
        path, &name,
    )?))
}

#[uniffi::export]
pub fn viker_git_amend(
    path: String,
    message: Option<String>,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::amend(
        path,
        message.as_deref(),
    )?))
}

#[uniffi::export]
pub fn viker_git_stash_push(
    path: String,
    message: Option<String>,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::stash_push(
        path,
        message.as_deref(),
    )?))
}

#[uniffi::export]
pub fn viker_git_stash_apply(
    path: String,
    index: u64,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::stash_apply(
        path,
        checked_index(index, "index")?,
    )?))
}

#[uniffi::export]
pub fn viker_git_stash_pop(
    path: String,
    index: u64,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::stash_pop(
        path,
        checked_index(index, "index")?,
    )?))
}

#[uniffi::export]
pub fn viker_git_merge(
    path: String,
    branch: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::merge_branch(path, &branch)?))
}

#[uniffi::export]
pub fn viker_git_rebase(
    path: String,
    upstream: String,
) -> Result<VikerGitOperationReport, VikerError> {
    Ok(git_report_from_core(core_git::rebase_onto(
        path, &upstream,
    )?))
}

#[uniffi::export]
impl VikerEditor {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::from_core_editor(Editor::new(Document::new_empty()))
    }

    #[uniffi::constructor]
    pub fn from_text(text: String) -> Arc<Self> {
        let mut editor = Editor::new(Document::new_empty());
        editor.replace_document_text(&normalize_text(&text));
        editor.document.modified = false;
        Self::from_core_editor(editor)
    }

    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, VikerError> {
        let document = Document::open(&path)?;
        Ok(Self::from_core_editor(Editor::new(document)))
    }

    pub fn text(&self) -> Result<String, VikerError> {
        Ok(self.lock()?.document.rope.to_string())
    }

    pub fn set_text(&self, text: String) -> Result<(), VikerError> {
        {
            let mut editor = self.lock()?;
            editor.replace_document_text(&normalize_text(&text));
        }
        self.notify_lsp_change()?;
        Ok(())
    }

    pub fn line_count(&self) -> Result<u64, VikerError> {
        Ok(self.lock()?.document.line_count() as u64)
    }

    pub fn line(&self, row: u64) -> Result<String, VikerError> {
        let editor = self.lock()?;
        let row = checked_index(row, "row")?;
        if row >= editor.document.line_count() {
            return Err(VikerError::InvalidInput {
                message: format!("row {row} is out of bounds"),
            });
        }
        Ok(line_without_trailing_newline(
            &editor.document.rope.line(row).to_string(),
        ))
    }

    pub fn lines(&self, start: u64, count: u64) -> Result<Vec<String>, VikerError> {
        let editor = self.lock()?;
        let start = checked_index(start, "start")?;
        let count = checked_index(count, "count")?;
        let line_count = editor.document.line_count();
        let end = start.saturating_add(count).min(line_count);
        if start > line_count {
            return Err(VikerError::InvalidInput {
                message: format!("start row {start} is out of bounds"),
            });
        }

        Ok((start..end)
            .map(|row| line_without_trailing_newline(&editor.document.rope.line(row).to_string()))
            .collect())
    }

    pub fn cursor(&self) -> Result<VikerPosition, VikerError> {
        Ok(position_from_core(self.lock()?.cursor))
    }

    pub fn set_viewport_size(&self, width: u64, height: u64) -> Result<(), VikerError> {
        let width = checked_index(width, "width")?;
        let height = checked_index(height, "height")?;
        self.lock()?.set_view_size(width, height);
        Ok(())
    }

    pub fn set_cursor(&self, row: u64, column: u64) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(editor.set_cursor_position(row, column)))
    }

    pub fn line_display_width(&self, row: u64) -> Result<u64, VikerError> {
        let row = checked_index(row, "row")?;
        let editor = self.lock()?;
        ensure_row_in_bounds(&editor, row)?;
        Ok(editor.line_display_width(row) as u64)
    }

    pub fn display_cells(&self, row: u64) -> Result<Vec<VikerDisplayCell>, VikerError> {
        let row = checked_index(row, "row")?;
        let editor = self.lock()?;
        if row >= editor.document.line_count() {
            return Err(VikerError::InvalidInput {
                message: format!("row {row} is out of bounds"),
            });
        }
        Ok(editor
            .display_cells_for_line(row)
            .into_iter()
            .map(|cell| VikerDisplayCell {
                row: row as u64,
                char_start: cell.char_start as u64,
                char_end: cell.char_end as u64,
                cell_start: cell.cell_start as u64,
                cell_width: cell.cell_width as u64,
                text: cell.text,
            })
            .collect())
    }

    pub fn display_column_for_position(&self, row: u64, column: u64) -> Result<u64, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let editor = self.lock()?;
        ensure_row_in_bounds(&editor, row)?;
        Ok(editor.display_column_for_position(row, column) as u64)
    }

    pub fn position_for_display_column(
        &self,
        row: u64,
        display_column: u64,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let display_column = checked_index(display_column, "display_column")?;
        let editor = self.lock()?;
        ensure_row_in_bounds(&editor, row)?;
        Ok(position_from_core(
            editor.position_for_display_column(row, display_column),
        ))
    }

    pub fn cursor_display_column(&self) -> Result<u64, VikerError> {
        Ok(self.lock()?.cursor_display_column() as u64)
    }

    pub fn view_cell_for_position(
        &self,
        row: u64,
        column: u64,
    ) -> Result<Option<VikerViewCell>, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        Ok(self
            .lock()?
            .view_cell_for_position(Position { row, col: column })
            .map(view_cell_from_core))
    }

    pub fn cursor_view_cell(&self) -> Result<Option<VikerViewCell>, VikerError> {
        Ok(self.lock()?.cursor_view_cell().map(view_cell_from_core))
    }

    pub fn syntax_language(&self) -> Result<Option<VikerSyntaxLanguage>, VikerError> {
        Ok(self
            .lock()?
            .syntax_language()
            .map(syntax_language_from_core))
    }

    pub fn set_language(&self, language: Option<VikerSyntaxLanguage>) -> Result<(), VikerError> {
        self.lock()?
            .set_syntax_language(language.map(syntax_language_to_core));
        Ok(())
    }

    pub fn highlight_spans(
        &self,
        start: u64,
        count: u64,
    ) -> Result<Vec<VikerHighlightSpan>, VikerError> {
        let start = checked_index(start, "start")?;
        let count = checked_index(count, "count")?;
        let mut editor = self.lock()?;
        let line_count = editor.document.line_count();
        if start > line_count {
            return Err(VikerError::InvalidInput {
                message: format!("start row {start} is out of bounds"),
            });
        }

        Ok(editor
            .highlight_spans_for_range(start, count)
            .into_iter()
            .map(highlight_span_from_core)
            .collect())
    }

    pub fn highlight_style_at(
        &self,
        row: u64,
        column: u64,
    ) -> Result<VikerHighlightStyle, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        ensure_row_in_bounds(&editor, row)?;
        editor.highlight_spans_for_range(row, 1);
        Ok(highlight_style_from_core(
            editor.highlight_style_at(row, column),
        ))
    }

    pub fn list_lsp_servers(&self) -> Result<Vec<VikerLspServerInfo>, VikerError> {
        Ok(lsp_server_infos())
    }

    pub fn start_lsp(&self) -> Result<VikerLspStatus, VikerError> {
        let (path, text, version, spec, invocation, root) = {
            let editor = self.lock()?;
            let path = editor
                .document
                .path
                .clone()
                .ok_or_else(|| VikerError::InvalidInput {
                    message: "LSP requires a file-backed document".to_string(),
                })?;
            let spec = editor
                .syntax_language()
                .map(|language| language.spec())
                .ok_or_else(|| VikerError::InvalidInput {
                    message: format!("no LSP language is configured for {}", path.display()),
                })?;
            let invocation =
                language::resolve_lsp(spec, &editor.config, &path).ok_or_else(|| {
                    VikerError::InvalidInput {
                        message: format!("LSP is disabled or unavailable for {}", spec.id),
                    }
                })?;
            let root = language::find_project_root_for_language(&path, spec.kind);
            (
                path,
                editor.document.rope.to_string(),
                editor.document.version,
                spec,
                invocation,
                root,
            )
        };

        let mut state = self.lock_lsp()?;
        if state.client.is_some()
            && state.language == Some(spec.kind)
            && state.root_path.as_deref() == Some(root.as_path())
        {
            return Ok(lsp_status_from_state(
                &state,
                Some("LSP already running".to_string()),
            ));
        }

        if let Some(client) = &mut state.client {
            let _ = self.runtime.block_on(client.shutdown());
        }
        state.clear();

        let client = self.runtime.block_on(LspClient::start(
            &root,
            spec.kind,
            invocation,
            state.event_tx.clone(),
        ))?;
        state.client = Some(client);
        state.language = Some(spec.kind);
        state.root_path = Some(root);
        state.file_uri = Some(lsp::path_to_uri(&path));
        state.last_notified_version = version;

        let file_uri = state.file_uri.clone();
        if let Some(client) = &mut state.client
            && client.initialized
            && let Some(uri) = file_uri.as_deref()
        {
            let _ = self.runtime.block_on(client.did_open(uri, &text, version));
        }

        Ok(lsp_status_from_state(
            &state,
            Some(format!("LSP starting {}", spec.id)),
        ))
    }

    pub fn stop_lsp(&self) -> Result<(), VikerError> {
        let mut state = self.lock_lsp()?;
        if let Some(client) = &mut state.client {
            let _ = self.runtime.block_on(client.shutdown());
        }
        state.clear();
        Ok(())
    }

    pub fn lsp_status(&self) -> Result<VikerLspStatus, VikerError> {
        let state = self.lock_lsp()?;
        Ok(lsp_status_from_state(&state, None))
    }

    pub fn poll_lsp(&self) -> Result<Vec<VikerLspEvent>, VikerError> {
        let mut events = Vec::new();
        loop {
            let message = {
                let mut state = self.lock_lsp()?;
                match state.event_rx.try_recv() {
                    Ok(AppEvent::Lsp(message)) => Some(message),
                    Ok(_) => None,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                }
            };
            if let Some(message) = message {
                events.extend(self.handle_lsp_message(message)?);
            }
        }
        Ok(events)
    }

    pub fn sync_lsp_document(&self) -> Result<(), VikerError> {
        self.notify_lsp_change()
    }

    pub fn diagnostics(&self) -> Result<Vec<VikerDiagnostic>, VikerError> {
        Ok(self
            .lock()?
            .diagnostics
            .iter()
            .cloned()
            .map(diagnostic_from_core)
            .collect())
    }

    pub fn request_completion(&self, row: u64, column: u64) -> Result<VikerLspRequest, VikerError> {
        self.notify_lsp_change()?;
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let id = {
            let mut state = self.lock_lsp()?;
            let uri = ready_lsp_uri(&state)?;
            let client = state.client.as_mut().expect("checked by ready_lsp_uri");
            self.runtime
                .block_on(client.completion(&uri, row, column))?
        };
        self.lock()?.pending_completion_id = Some(id);
        Ok(request_from_id(id))
    }

    pub fn completion_items(&self) -> Result<Vec<VikerCompletionItem>, VikerError> {
        Ok(self
            .lock()?
            .completions
            .iter()
            .cloned()
            .map(completion_from_core)
            .collect())
    }

    pub fn request_hover(&self, row: u64, column: u64) -> Result<VikerLspRequest, VikerError> {
        self.notify_lsp_change()?;
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let id = {
            let mut state = self.lock_lsp()?;
            let uri = ready_lsp_uri(&state)?;
            let client = state.client.as_mut().expect("checked by ready_lsp_uri");
            self.runtime.block_on(client.hover(&uri, row, column))?
        };
        self.lock()?.pending_hover_id = Some(id);
        Ok(request_from_id(id))
    }

    pub fn hover_text(&self) -> Result<Option<String>, VikerError> {
        Ok(self.lock()?.hover_text.clone())
    }

    pub fn request_references(&self, row: u64, column: u64) -> Result<VikerLspRequest, VikerError> {
        self.notify_lsp_change()?;
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let id = {
            let mut state = self.lock_lsp()?;
            let uri = ready_lsp_uri(&state)?;
            let client = state.client.as_mut().expect("checked by ready_lsp_uri");
            self.runtime
                .block_on(client.references(&uri, row, column))?
        };
        self.lock()?.pending_references_id = Some(id);
        Ok(request_from_id(id))
    }

    pub fn references(&self) -> Result<Vec<VikerLocation>, VikerError> {
        Ok(self
            .lock()?
            .references
            .iter()
            .cloned()
            .map(location_from_core)
            .collect())
    }

    pub fn request_workspace_symbols(&self, query: String) -> Result<VikerLspRequest, VikerError> {
        let id = {
            let mut state = self.lock_lsp()?;
            let client = ready_lsp_client(&mut state)?;
            self.runtime.block_on(client.workspace_symbol(&query))?
        };
        self.lock()?.pending_workspace_symbol_id = Some(id);
        Ok(request_from_id(id))
    }

    pub fn workspace_symbols(&self) -> Result<Vec<VikerWorkspaceSymbol>, VikerError> {
        Ok(self
            .lock()?
            .workspace_symbol_results
            .iter()
            .cloned()
            .map(workspace_symbol_from_core)
            .collect())
    }

    pub fn format_document(&self) -> Result<VikerLspRequest, VikerError> {
        let external = {
            let editor = self.lock()?;
            editor.document.path.clone().and_then(|path| {
                editor.syntax_language().and_then(|syntax_language| {
                    let spec = syntax_language.spec();
                    language::resolve_formatter(spec, &editor.config, &path).map(|invocation| {
                        (
                            invocation,
                            language::find_project_root_for_language(&path, spec.kind),
                            editor.document.rope.to_string(),
                        )
                    })
                })
            })
        };

        if let Some((invocation, cwd, input)) = external {
            let output = formatter::format_text(&invocation, &cwd, &input)?;
            if output != input {
                self.lock()?.replace_document_text(&output);
                self.notify_lsp_change()?;
            }
            return Ok(VikerLspRequest {
                id: None,
                completed: true,
            });
        }

        self.notify_lsp_change()?;
        let id = {
            let tab_width = self.lock()?.config.tab_width;
            let mut state = self.lock_lsp()?;
            let uri = ready_lsp_uri(&state)?;
            let client = state.client.as_mut().expect("checked by ready_lsp_uri");
            let params = serde_json::json!({
                "textDocument": { "uri": uri },
                "options": { "tabSize": tab_width, "insertSpaces": true }
            });
            self.runtime
                .block_on(client.send_request("textDocument/formatting", params))?
        };
        self.lock()?.pending_format_id = Some(id);
        Ok(request_from_id(id))
    }

    pub fn position_for_view_cell(
        &self,
        row: u64,
        column: u64,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let editor = self.lock()?;
        Ok(position_from_core(
            editor.position_for_view_cell(row, column),
        ))
    }

    pub fn set_cursor_for_view_cell(
        &self,
        row: u64,
        column: u64,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(
            editor.set_cursor_for_view_cell(row, column),
        ))
    }

    pub fn begin_selection(
        &self,
        row: u64,
        column: u64,
        mode: VikerSelectionMode,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(editor.begin_selection_at(
            row,
            column,
            selection_mode_to_core(mode),
        )))
    }

    pub fn extend_selection(&self, row: u64, column: u64) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(editor.extend_selection_to(row, column)))
    }

    pub fn begin_selection_for_view_cell(
        &self,
        row: u64,
        column: u64,
        mode: VikerSelectionMode,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(editor.begin_selection_at_view_cell(
            row,
            column,
            selection_mode_to_core(mode),
        )))
    }

    pub fn extend_selection_to_view_cell(
        &self,
        row: u64,
        column: u64,
    ) -> Result<VikerPosition, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        let mut editor = self.lock()?;
        Ok(position_from_core(
            editor.extend_selection_to_view_cell(row, column),
        ))
    }

    pub fn set_selection(
        &self,
        anchor: VikerPosition,
        cursor: VikerPosition,
        mode: VikerSelectionMode,
    ) -> Result<(), VikerError> {
        let anchor = position_to_core(anchor)?;
        let cursor = position_to_core(cursor)?;
        self.lock()?
            .set_selection(anchor, cursor, selection_mode_to_core(mode));
        Ok(())
    }

    pub fn clear_selection(&self) -> Result<(), VikerError> {
        self.lock()?.clear_selection();
        Ok(())
    }

    pub fn select_word_at(&self, row: u64, column: u64) -> Result<bool, VikerError> {
        let row = checked_index(row, "row")?;
        let column = checked_index(column, "column")?;
        Ok(self.lock()?.select_word_at(row, column).is_some())
    }

    pub fn select_line_at(&self, row: u64) -> Result<bool, VikerError> {
        let row = checked_index(row, "row")?;
        Ok(self.lock()?.select_line_at(row).is_some())
    }

    pub fn mode(&self) -> Result<VikerMode, VikerError> {
        Ok(mode_from_core(self.lock()?.mode))
    }

    pub fn file_path(&self) -> Result<Option<String>, VikerError> {
        Ok(self
            .lock()?
            .document
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()))
    }

    pub fn is_modified(&self) -> Result<bool, VikerError> {
        Ok(self.lock()?.document.modified)
    }

    pub fn status_message(&self) -> Result<Option<String>, VikerError> {
        Ok(self.lock()?.status_message.clone())
    }

    pub fn snapshot(&self) -> Result<VikerSnapshot, VikerError> {
        let editor = self.lock()?;
        Ok(snapshot_from_editor(&editor))
    }

    pub fn register_summaries(&self) -> Result<Vec<VikerRegisterSummary>, VikerError> {
        let editor = self.lock()?;
        Ok(register_summaries(&editor))
    }

    pub fn input_text(&self, text: String) -> Result<Vec<VikerEffect>, VikerError> {
        let mut effects = Vec::new();
        {
            let mut editor = self.lock()?;
            for ch in text.chars() {
                effects.extend(process_core_key(
                    &mut editor,
                    KeyInput {
                        code: KeyCode::Char(ch),
                        ctrl: false,
                        alt: false,
                    },
                ));
            }
        }
        self.notify_lsp_change()?;
        Ok(effects)
    }

    pub fn process_key(&self, event: VikerKeyEvent) -> Result<Vec<VikerEffect>, VikerError> {
        let key = key_input_from_event(event)?;
        let effects = {
            let mut editor = self.lock()?;
            process_core_key(&mut editor, key)
        };
        self.notify_lsp_change()?;
        Ok(effects)
    }

    pub fn execute_command(&self, command: String) -> Result<Vec<VikerEffect>, VikerError> {
        let effects = {
            let mut editor = self.lock()?;
            let command = command.strip_prefix(':').unwrap_or(&command);
            editor.command_buffer = command.to_string();
            editor
                .command_execute()
                .into_iter()
                .map(effect_from_deferred)
                .collect()
        };
        self.notify_lsp_change()?;
        Ok(effects)
    }

    pub fn save(&self) -> Result<Vec<VikerEffect>, VikerError> {
        {
            let mut editor = self.lock()?;
            editor.document.save()?;
        }
        self.notify_lsp_did_save()?;
        Ok(vec![VikerEffect {
            kind: VikerEffectKind::DidSave,
            payload: None,
        }])
    }

    pub fn save_as(&self, path: String) -> Result<Vec<VikerEffect>, VikerError> {
        {
            let mut editor = self.lock()?;
            editor.document.path = Some(PathBuf::from(path));
            editor.document.save()?;
        }
        Ok(vec![VikerEffect {
            kind: VikerEffectKind::DidSave,
            payload: None,
        }])
    }
}

#[uniffi::export]
impl VikerLspWorkspace {
    #[uniffi::constructor]
    pub fn open(root_path: String) -> Result<Arc<Self>, VikerError> {
        let root_path = canonical_workspace_root(&root_path)?;
        let runtime =
            tokio::runtime::Runtime::new().expect("failed to create VikerKit LSP runtime");
        Ok(Arc::new(Self {
            root_path,
            state: Mutex::new(VikerLspWorkspaceState::new()),
            runtime,
        }))
    }

    pub fn list_lsp_servers(&self) -> Result<Vec<VikerLspServerInfo>, VikerError> {
        Ok(lsp_server_infos())
    }

    pub fn status(&self) -> Result<VikerLspWorkspaceStatus, VikerError> {
        let state = self.lock_state()?;
        Ok(workspace_status_from_state(&self.root_path, &state))
    }

    pub fn project_files(&self) -> Result<Vec<String>, VikerError> {
        Ok(core_search::scan_project_files(&self.root_path))
    }

    pub fn search_files(
        &self,
        query: String,
        limit: u64,
    ) -> Result<Vec<VikerFileSearchResult>, VikerError> {
        Ok(core_search::search_project_files(
            &self.root_path,
            &query,
            checked_index(limit, "limit")?,
        )
        .into_iter()
        .map(file_search_result_from_core)
        .collect())
    }

    pub fn search_file_contents(
        &self,
        query: String,
        limit: u64,
    ) -> Result<Vec<VikerFileContentSearchResult>, VikerError> {
        Ok(core_search::search_file_contents(
            &self.root_path,
            &query,
            checked_index(limit, "limit")?,
        )
        .into_iter()
        .map(file_content_search_result_from_core)
        .collect())
    }

    pub fn start_lsp(
        &self,
        language: VikerSyntaxLanguage,
    ) -> Result<VikerLspServerStatus, VikerError> {
        let language = syntax_language_to_core(language);
        let mut state = self.lock_state()?;
        let server_key = self.ensure_workspace_server(
            &mut state,
            language,
            &viker_core::config::Config::default(),
        )?;
        Ok(workspace_server_status_from_state(
            &self.root_path,
            &server_key,
            language,
            &state,
            Some(format!("LSP starting {}", language.spec().id)),
        ))
    }

    pub fn stop_lsp(&self, language: VikerSyntaxLanguage) -> Result<(), VikerError> {
        let language = syntax_language_to_core(language);
        let mut state = self.lock_state()?;
        let server_key = workspace_server_key(
            language,
            &viker_core::config::Config::default(),
            &self.root_path,
        )?
        .0;
        if let Some(mut server) = state.servers.remove(&server_key) {
            let _ = self.runtime.block_on(server.client.shutdown());
        }
        state
            .pending
            .retain(|_, request| request.server_key != server_key);
        for document in state.documents.values_mut() {
            if document.server_key == server_key {
                document.opened = false;
            }
        }
        Ok(())
    }

    pub fn stop_all_lsp(&self) -> Result<(), VikerError> {
        let mut state = self.lock_state()?;
        for (_, mut server) in state.servers.drain() {
            let _ = self.runtime.block_on(server.client.shutdown());
        }
        state.pending.clear();
        for document in state.documents.values_mut() {
            document.opened = false;
        }
        Ok(())
    }

    pub fn open_document(&self, editor: Arc<VikerEditor>) -> Result<VikerLspDocument, VikerError> {
        let snapshot = self.snapshot_editor_document(editor)?;
        let document = snapshot.document();
        self.sync_snapshot(snapshot, true)?;
        Ok(document)
    }

    pub fn sync_document(&self, editor: Arc<VikerEditor>) -> Result<VikerLspDocument, VikerError> {
        let snapshot = self.snapshot_editor_document(editor)?;
        let document = snapshot.document();
        self.sync_snapshot(snapshot, true)?;
        Ok(document)
    }

    pub fn save_document(&self, editor: Arc<VikerEditor>) -> Result<(), VikerError> {
        let document = self.sync_document(editor)?;
        let mut state = self.lock_state()?;
        let Some(server_key) = state
            .documents
            .get(&document.uri)
            .map(|document| document.server_key.clone())
        else {
            return Ok(());
        };
        if let Some(server) = state.servers.get_mut(&server_key)
            && server.client.initialized
        {
            self.runtime
                .block_on(server.client.did_save(&document.uri))?;
        }
        Ok(())
    }

    pub fn close_document(&self, uri: String) -> Result<(), VikerError> {
        let mut state = self.lock_state()?;
        let Some(document) = state.documents.remove(&uri) else {
            return Ok(());
        };
        state.diagnostics.remove(&uri);
        if document.opened
            && let Some(server) = state.servers.get_mut(&document.server_key)
            && server.client.initialized
        {
            self.runtime.block_on(server.client.did_close(&uri))?;
        }
        Ok(())
    }

    pub fn poll_lsp(&self) -> Result<Vec<VikerLspWorkspaceEvent>, VikerError> {
        let mut messages = Vec::new();
        {
            let mut state = self.lock_state()?;
            for (server_key, server) in state.servers.iter_mut() {
                loop {
                    match server.event_rx.try_recv() {
                        Ok(AppEvent::Lsp(message)) => {
                            messages.push((server_key.clone(), server.primary_language, message))
                        }
                        Ok(_) => {}
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => break,
                    }
                }
            }
        }

        let mut events = Vec::new();
        for (server_key, primary_language, message) in messages {
            events.extend(self.handle_workspace_lsp_message(
                server_key,
                primary_language,
                message,
            )?);
        }
        Ok(events)
    }

    pub fn diagnostics(&self, uri: String) -> Result<Vec<VikerDiagnostic>, VikerError> {
        Ok(self
            .lock_state()?
            .diagnostics
            .get(&uri)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(diagnostic_from_core)
            .collect())
    }

    pub fn diagnostics_for_editor(
        &self,
        editor: Arc<VikerEditor>,
    ) -> Result<Vec<VikerDiagnostic>, VikerError> {
        let uri = self.snapshot_editor_document(editor)?.uri;
        self.diagnostics(uri)
    }

    pub fn request_completion(
        &self,
        editor: Arc<VikerEditor>,
        row: u64,
        column: u64,
    ) -> Result<VikerLspRequest, VikerError> {
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let document = self.sync_document(editor.clone())?;
        let language = syntax_language_to_core(document.language);
        let id = {
            let mut state = self.lock_state()?;
            let server_key = document_server_key(&state, &document.uri)?;
            let server = ready_workspace_server(&mut state, &server_key)?;
            let id = self
                .runtime
                .block_on(server.client.completion(&document.uri, row, column))?;
            state.pending.insert(
                id,
                WorkspacePendingRequest {
                    kind: WorkspacePendingKind::Completion,
                    language,
                    server_key,
                    uri: Some(document.uri.clone()),
                    editor: Some(editor),
                },
            );
            id
        };
        Ok(request_from_id(id))
    }

    pub fn completion_items(
        &self,
        request_id: u64,
    ) -> Result<Vec<VikerCompletionItem>, VikerError> {
        Ok(self
            .lock_state()?
            .completions
            .get(&request_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(completion_from_core)
            .collect())
    }

    pub fn request_hover(
        &self,
        editor: Arc<VikerEditor>,
        row: u64,
        column: u64,
    ) -> Result<VikerLspRequest, VikerError> {
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let document = self.sync_document(editor.clone())?;
        let language = syntax_language_to_core(document.language);
        let id = {
            let mut state = self.lock_state()?;
            let server_key = document_server_key(&state, &document.uri)?;
            let server = ready_workspace_server(&mut state, &server_key)?;
            let id = self
                .runtime
                .block_on(server.client.hover(&document.uri, row, column))?;
            state.pending.insert(
                id,
                WorkspacePendingRequest {
                    kind: WorkspacePendingKind::Hover,
                    language,
                    server_key,
                    uri: Some(document.uri.clone()),
                    editor: Some(editor),
                },
            );
            id
        };
        Ok(request_from_id(id))
    }

    pub fn hover_text(&self, request_id: u64) -> Result<Option<String>, VikerError> {
        Ok(self
            .lock_state()?
            .hovers
            .get(&request_id)
            .cloned()
            .unwrap_or(None))
    }

    pub fn request_references(
        &self,
        editor: Arc<VikerEditor>,
        row: u64,
        column: u64,
    ) -> Result<VikerLspRequest, VikerError> {
        let row = checked_u32(row, "row")?;
        let column = checked_u32(column, "column")?;
        let document = self.sync_document(editor.clone())?;
        let language = syntax_language_to_core(document.language);
        let id = {
            let mut state = self.lock_state()?;
            let server_key = document_server_key(&state, &document.uri)?;
            let server = ready_workspace_server(&mut state, &server_key)?;
            let id = self
                .runtime
                .block_on(server.client.references(&document.uri, row, column))?;
            state.pending.insert(
                id,
                WorkspacePendingRequest {
                    kind: WorkspacePendingKind::References,
                    language,
                    server_key,
                    uri: Some(document.uri.clone()),
                    editor: Some(editor),
                },
            );
            id
        };
        Ok(request_from_id(id))
    }

    pub fn references(&self, request_id: u64) -> Result<Vec<VikerLocation>, VikerError> {
        Ok(self
            .lock_state()?
            .references
            .get(&request_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(location_from_core)
            .collect())
    }

    pub fn request_workspace_symbols(
        &self,
        language: VikerSyntaxLanguage,
        query: String,
    ) -> Result<VikerLspRequest, VikerError> {
        let language = syntax_language_to_core(language);
        let id = {
            let mut state = self.lock_state()?;
            let server_key = self.ensure_workspace_server(
                &mut state,
                language,
                &viker_core::config::Config::default(),
            )?;
            let server = ready_workspace_server(&mut state, &server_key)?;
            let id = self
                .runtime
                .block_on(server.client.workspace_symbol(&query))?;
            state.pending.insert(
                id,
                WorkspacePendingRequest {
                    kind: WorkspacePendingKind::WorkspaceSymbols,
                    language,
                    server_key,
                    uri: None,
                    editor: None,
                },
            );
            id
        };
        Ok(request_from_id(id))
    }

    pub fn workspace_symbols(
        &self,
        request_id: u64,
    ) -> Result<Vec<VikerWorkspaceSymbol>, VikerError> {
        Ok(self
            .lock_state()?
            .workspace_symbols
            .get(&request_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(workspace_symbol_from_core)
            .collect())
    }

    pub fn format_document(&self, editor: Arc<VikerEditor>) -> Result<VikerLspRequest, VikerError> {
        let snapshot = self.snapshot_editor_document(editor.clone())?;
        if let Some(invocation) =
            language::resolve_formatter(snapshot.language.spec(), &snapshot.config, &snapshot.path)
        {
            let output = formatter::format_text(&invocation, &self.root_path, &snapshot.text)?;
            if output != snapshot.text {
                {
                    let mut core_editor = editor.lock()?;
                    core_editor.replace_document_text(&output);
                }
                self.sync_document(editor)?;
            }
            return Ok(VikerLspRequest {
                id: None,
                completed: true,
            });
        }

        let document = self.sync_document(editor.clone())?;
        let language = syntax_language_to_core(document.language);
        let id = {
            let mut state = self.lock_state()?;
            let tab_width = state
                .documents
                .get(&document.uri)
                .map(|document| document.tab_width)
                .unwrap_or(4);
            let server_key = document_server_key(&state, &document.uri)?;
            let server = ready_workspace_server(&mut state, &server_key)?;
            let params = serde_json::json!({
                "textDocument": { "uri": document.uri.clone() },
                "options": { "tabSize": tab_width, "insertSpaces": true }
            });
            let id = self.runtime.block_on(
                server
                    .client
                    .send_request("textDocument/formatting", params),
            )?;
            state.pending.insert(
                id,
                WorkspacePendingRequest {
                    kind: WorkspacePendingKind::Format,
                    language,
                    server_key,
                    uri: Some(document.uri.clone()),
                    editor: Some(editor),
                },
            );
            id
        };
        Ok(request_from_id(id))
    }
}

impl VikerEditor {
    fn from_core_editor(editor: Editor) -> Arc<Self> {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create VikerKit runtime");
        Arc::new(Self {
            editor: Mutex::new(editor),
            lsp: Mutex::new(VikerLspState::new()),
            runtime,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Editor>, VikerError> {
        self.editor
            .lock()
            .map_err(|_| VikerError::EditorUnavailable {
                message: "editor state lock is poisoned".to_string(),
            })
    }

    fn lock_lsp(&self) -> Result<MutexGuard<'_, VikerLspState>, VikerError> {
        self.lsp.lock().map_err(|_| VikerError::EditorUnavailable {
            message: "LSP state lock is poisoned".to_string(),
        })
    }

    fn notify_lsp_change(&self) -> Result<(), VikerError> {
        let (text, version) = {
            let editor = self.lock()?;
            (editor.document.rope.to_string(), editor.document.version)
        };
        let mut state = self.lock_lsp()?;
        if state.last_notified_version == version {
            return Ok(());
        }
        let Some(uri) = state.file_uri.clone() else {
            return Ok(());
        };
        let Some(client) = &mut state.client else {
            return Ok(());
        };
        if !client.initialized {
            return Ok(());
        }
        self.runtime
            .block_on(client.did_change(&uri, &text, version))?;
        state.last_notified_version = version;
        Ok(())
    }

    fn notify_lsp_did_save(&self) -> Result<(), VikerError> {
        let mut state = self.lock_lsp()?;
        let Some(uri) = state.file_uri.clone() else {
            return Ok(());
        };
        let Some(client) = &mut state.client else {
            return Ok(());
        };
        if client.initialized {
            self.runtime.block_on(client.did_save(&uri))?;
        }
        Ok(())
    }

    fn handle_lsp_message(&self, message: LspMessage) -> Result<Vec<VikerLspEvent>, VikerError> {
        let mut events = Vec::new();
        match message {
            LspMessage::Response { id, result, error } => {
                let mut editor = self.lock()?;
                let mut state = self.lock_lsp()?;

                let is_init = state
                    .client
                    .as_ref()
                    .is_some_and(|client| id == client.initialize_id && !client.initialized);
                if is_init {
                    if let Some(error) = error {
                        let message = lsp_error_message(&error, "LSP initialize failed");
                        events.push(VikerLspEvent {
                            kind: VikerLspEventKind::Error,
                            message: Some(message),
                        });
                        return Ok(events);
                    }

                    let uri = state.file_uri.clone();
                    let text = editor.document.rope.to_string();
                    let version = editor.document.version;
                    if let Some(client) = &mut state.client {
                        self.runtime.block_on(client.send_initialized())?;
                        if let Some(uri) = &uri {
                            self.runtime
                                .block_on(client.did_open(uri, &text, version))?;
                        }
                    }
                    state.last_notified_version = version;
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::Ready,
                        message: Some("LSP ready".to_string()),
                    });
                    return Ok(events);
                }

                if let Some(error) = error {
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::Error,
                        message: Some(lsp_error_message(&error, "LSP request failed")),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_completion_id {
                    editor.pending_completion_id = None;
                    let items = result
                        .as_ref()
                        .map(lsp::parse_completions)
                        .unwrap_or_default();
                    editor.completions = items;
                    editor.completion_index = 0;
                    editor.showing_completion = !editor.completions.is_empty();
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::CompletionUpdated,
                        message: Some(format!("{} completion item(s)", editor.completions.len())),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_hover_id {
                    editor.pending_hover_id = None;
                    editor.hover_text = result.as_ref().and_then(lsp::parse_hover);
                    editor.showing_hover = editor.hover_text.is_some();
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::HoverUpdated,
                        message: editor.hover_text.clone(),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_references_id {
                    editor.pending_references_id = None;
                    editor.references = result
                        .as_ref()
                        .map(lsp::parse_references)
                        .unwrap_or_default();
                    editor.reference_index = 0;
                    editor.showing_references = !editor.references.is_empty();
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::ReferencesUpdated,
                        message: Some(format!("{} reference(s)", editor.references.len())),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_workspace_symbol_id {
                    editor.pending_workspace_symbol_id = None;
                    editor.workspace_symbol_results = result
                        .as_ref()
                        .map(lsp::parse_workspace_symbols)
                        .unwrap_or_default();
                    editor.workspace_symbol_index = 0;
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::WorkspaceSymbolsUpdated,
                        message: Some(format!(
                            "{} workspace symbol(s)",
                            editor.workspace_symbol_results.len()
                        )),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_format_id {
                    editor.pending_format_id = None;
                    let count = result
                        .as_ref()
                        .map(|value| apply_format_result(&mut editor, value))
                        .unwrap_or(0);
                    state.last_notified_version = editor.document.version;
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::FormattingApplied,
                        message: Some(format!("{count} formatting edit(s)")),
                    });
                    return Ok(events);
                }

                if Some(id) == editor.pending_rename_id {
                    editor.pending_rename_id = None;
                    let count = match (result.as_ref(), state.file_uri.as_deref()) {
                        (Some(value), Some(uri)) => apply_rename_result(&mut editor, uri, value),
                        _ => 0,
                    };
                    state.last_notified_version = editor.document.version;
                    events.push(VikerLspEvent {
                        kind: VikerLspEventKind::RenameApplied,
                        message: Some(format!("{count} rename edit(s)")),
                    });
                }
            }
            LspMessage::Notification { method, params } => {
                if method == "textDocument/publishDiagnostics" {
                    let mut editor = self.lock()?;
                    let state = self.lock_lsp()?;
                    if lsp::diagnostics_uri(&params).as_deref() == state.file_uri.as_deref() {
                        editor.diagnostics = lsp::parse_diagnostics(&params);
                        events.push(VikerLspEvent {
                            kind: VikerLspEventKind::DiagnosticsUpdated,
                            message: Some(format!("{} diagnostic(s)", editor.diagnostics.len())),
                        });
                    }
                }
            }
            LspMessage::ServerRequest { id, method, params } => {
                let editor = self.lock()?;
                let mut state = self.lock_lsp()?;
                if let Some(client) = &mut state.client {
                    let response = match method.as_str() {
                        "window/workDoneProgress/create" | "client/registerCapability" => {
                            serde_json::Value::Null
                        }
                        "workspace/configuration" => lsp::workspace_configuration_response(
                            &params,
                            editor.config.tab_width,
                            true,
                        ),
                        "workspace/applyEdit" => serde_json::json!({ "applied": false }),
                        _ => serde_json::Value::Null,
                    };
                    self.runtime.block_on(client.respond(&id, response))?;
                }
            }
        }
        Ok(events)
    }
}

impl VikerLspWorkspace {
    fn lock_state(&self) -> Result<MutexGuard<'_, VikerLspWorkspaceState>, VikerError> {
        self.state
            .lock()
            .map_err(|_| VikerError::EditorUnavailable {
                message: "workspace LSP state lock is poisoned".to_string(),
            })
    }

    fn snapshot_editor_document(
        &self,
        editor: Arc<VikerEditor>,
    ) -> Result<EditorLspSnapshot, VikerError> {
        let (path, text, version, tab_width, config, syntax_language) = {
            let core_editor = editor.lock()?;
            let path =
                core_editor
                    .document
                    .path
                    .clone()
                    .ok_or_else(|| VikerError::InvalidInput {
                        message: "shared LSP requires a file-backed document".to_string(),
                    })?;
            (
                path,
                core_editor.document.rope.to_string(),
                core_editor.document.version,
                core_editor.config.tab_width,
                core_editor.config.clone(),
                core_editor.syntax_language(),
            )
        };
        let path = canonical_document_path(&path);
        ensure_document_in_workspace(&self.root_path, &path)?;
        let language = syntax_language.ok_or_else(|| VikerError::InvalidInput {
            message: format!("no LSP language is configured for {}", path.display()),
        })?;
        let spec = language.spec();
        if spec.lsp.is_none() {
            return Err(VikerError::InvalidInput {
                message: format!("{} has no configured LSP", spec.id),
            });
        }
        Ok(EditorLspSnapshot {
            uri: lsp::path_to_uri(&path),
            path,
            language,
            text,
            version,
            tab_width,
            config,
            editor,
        })
    }

    fn ensure_workspace_server(
        &self,
        state: &mut VikerLspWorkspaceState,
        language: language::LanguageKind,
        config: &viker_core::config::Config,
    ) -> Result<WorkspaceLspServerKey, VikerError> {
        let (server_key, invocation) = workspace_server_key(language, config, &self.root_path)?;
        if state.servers.contains_key(&server_key) {
            return Ok(server_key);
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let client = self.runtime.block_on(LspClient::start(
            &self.root_path,
            language,
            invocation,
            event_tx,
        ))?;
        state.servers.insert(
            server_key.clone(),
            WorkspaceLspServer {
                client,
                event_rx,
                primary_language: language,
            },
        );
        Ok(server_key)
    }

    fn sync_snapshot(
        &self,
        snapshot: EditorLspSnapshot,
        prefer_change: bool,
    ) -> Result<(), VikerError> {
        let mut state = self.lock_state()?;
        let server_key =
            self.ensure_workspace_server(&mut state, snapshot.language, &snapshot.config)?;

        let existing = state.documents.get(&snapshot.uri);
        let previous_server_key = existing.map(|document| document.server_key.clone());
        let previous_was_opened = existing.is_some_and(|document| document.opened);
        let was_opened =
            existing.is_some_and(|document| document.opened && document.server_key == server_key);
        let previous_version = existing.map(|document| document.version);
        let should_notify = previous_version != Some(snapshot.version);

        if let Some(previous_server_key) = previous_server_key.as_ref()
            && previous_server_key != &server_key
            && previous_was_opened
            && let Some(server) = state.servers.get_mut(previous_server_key)
            && server.client.initialized
        {
            let _ = self
                .runtime
                .block_on(server.client.did_close(&snapshot.uri));
        }

        state.documents.insert(
            snapshot.uri.clone(),
            WorkspaceDocument {
                uri: snapshot.uri.clone(),
                path: snapshot.path,
                language: snapshot.language,
                server_key: server_key.clone(),
                text: snapshot.text.clone(),
                version: snapshot.version,
                tab_width: snapshot.tab_width,
                opened: was_opened,
                editor: Some(snapshot.editor),
            },
        );

        let Some(server) = state.servers.get_mut(&server_key) else {
            return Ok(());
        };
        if !server.client.initialized {
            return Ok(());
        }

        if !was_opened {
            self.runtime
                .block_on(server.client.did_open_with_language_id(
                    &snapshot.uri,
                    snapshot.language.spec().lsp_language_id,
                    &snapshot.text,
                    snapshot.version,
                ))?;
            if let Some(document) = state.documents.get_mut(&snapshot.uri) {
                document.opened = true;
            }
        } else if prefer_change && should_notify {
            self.runtime.block_on(server.client.did_change(
                &snapshot.uri,
                &snapshot.text,
                snapshot.version,
            ))?;
        }

        Ok(())
    }

    fn handle_workspace_lsp_message(
        &self,
        server_key: WorkspaceLspServerKey,
        primary_language: language::LanguageKind,
        message: LspMessage,
    ) -> Result<Vec<VikerLspWorkspaceEvent>, VikerError> {
        let mut events = Vec::new();
        match message {
            LspMessage::Response { id, result, error } => {
                let init_result = {
                    let state = self.lock_state()?;
                    let is_init = state.servers.get(&server_key).is_some_and(|server| {
                        id == server.client.initialize_id && !server.client.initialized
                    });
                    if is_init {
                        let documents = if error.is_none() {
                            state
                                .documents
                                .values()
                                .filter(|document| document.server_key == server_key)
                                .map(|document| {
                                    (
                                        document.uri.clone(),
                                        document.language.spec().lsp_language_id.to_string(),
                                        document.text.clone(),
                                        document.version,
                                    )
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        Some((error.clone(), documents))
                    } else {
                        None
                    }
                };

                if let Some((error, documents)) = init_result {
                    if let Some(error) = error {
                        events.push(workspace_event(
                            VikerLspEventKind::Error,
                            Some(primary_language),
                            None,
                            None,
                            Some(lsp_error_message(&error, "LSP initialize failed")),
                        ));
                        return Ok(events);
                    }

                    let mut state = self.lock_state()?;
                    if let Some(server) = state.servers.get_mut(&server_key) {
                        self.runtime.block_on(server.client.send_initialized())?;
                        for (uri, language_id, text, version) in &documents {
                            self.runtime
                                .block_on(server.client.did_open_with_language_id(
                                    uri,
                                    language_id,
                                    text,
                                    *version,
                                ))?;
                        }
                    }
                    for (uri, _, _, _) in &documents {
                        if let Some(document) = state.documents.get_mut(uri) {
                            document.opened = true;
                        }
                    }
                    events.push(workspace_event(
                        VikerLspEventKind::Ready,
                        Some(primary_language),
                        None,
                        None,
                        Some(format!("{} LSP ready", primary_language.spec().id)),
                    ));
                    return Ok(events);
                }

                let pending = {
                    let mut state = self.lock_state()?;
                    state.pending.remove(&id)
                };

                if let Some(error) = error {
                    events.push(workspace_event(
                        VikerLspEventKind::Error,
                        pending
                            .as_ref()
                            .map(|request| request.language)
                            .or(Some(primary_language)),
                        pending.as_ref().and_then(|request| request.uri.clone()),
                        Some(id),
                        Some(lsp_error_message(&error, "LSP request failed")),
                    ));
                    return Ok(events);
                }

                let Some(pending) = pending else {
                    return Ok(events);
                };

                match pending.kind {
                    WorkspacePendingKind::Completion => {
                        let items = result
                            .as_ref()
                            .map(lsp::parse_completions)
                            .unwrap_or_default();
                        if let Some(editor) = pending.editor.as_ref() {
                            let mut core_editor = editor.lock()?;
                            core_editor.completions = items.clone();
                            core_editor.completion_index = 0;
                            core_editor.showing_completion = !core_editor.completions.is_empty();
                        }
                        self.lock_state()?
                            .completions
                            .insert(id.max(0) as u64, items.clone());
                        events.push(workspace_event(
                            VikerLspEventKind::CompletionUpdated,
                            Some(pending.language),
                            pending.uri,
                            Some(id),
                            Some(format!("{} completion item(s)", items.len())),
                        ));
                    }
                    WorkspacePendingKind::Hover => {
                        let hover = result.as_ref().and_then(lsp::parse_hover);
                        if let Some(editor) = pending.editor.as_ref() {
                            let mut core_editor = editor.lock()?;
                            core_editor.hover_text = hover.clone();
                            core_editor.showing_hover = core_editor.hover_text.is_some();
                        }
                        self.lock_state()?
                            .hovers
                            .insert(id.max(0) as u64, hover.clone());
                        events.push(workspace_event(
                            VikerLspEventKind::HoverUpdated,
                            Some(pending.language),
                            pending.uri,
                            Some(id),
                            hover,
                        ));
                    }
                    WorkspacePendingKind::References => {
                        let references = result
                            .as_ref()
                            .map(lsp::parse_references)
                            .unwrap_or_default();
                        if let Some(editor) = pending.editor.as_ref() {
                            let mut core_editor = editor.lock()?;
                            core_editor.references = references.clone();
                            core_editor.reference_index = 0;
                            core_editor.showing_references = !core_editor.references.is_empty();
                        }
                        self.lock_state()?
                            .references
                            .insert(id.max(0) as u64, references.clone());
                        events.push(workspace_event(
                            VikerLspEventKind::ReferencesUpdated,
                            Some(pending.language),
                            pending.uri,
                            Some(id),
                            Some(format!("{} reference(s)", references.len())),
                        ));
                    }
                    WorkspacePendingKind::WorkspaceSymbols => {
                        let symbols = result
                            .as_ref()
                            .map(lsp::parse_workspace_symbols)
                            .unwrap_or_default();
                        self.lock_state()?
                            .workspace_symbols
                            .insert(id.max(0) as u64, symbols.clone());
                        events.push(workspace_event(
                            VikerLspEventKind::WorkspaceSymbolsUpdated,
                            Some(pending.language),
                            None,
                            Some(id),
                            Some(format!("{} workspace symbol(s)", symbols.len())),
                        ));
                    }
                    WorkspacePendingKind::Format => {
                        let Some(editor) = pending.editor else {
                            return Ok(events);
                        };
                        let count = {
                            let mut core_editor = editor.lock()?;
                            result
                                .as_ref()
                                .map(|value| apply_format_result(&mut core_editor, value))
                                .unwrap_or(0)
                        };
                        if let Ok(snapshot) = self.snapshot_editor_document(editor) {
                            self.sync_snapshot(snapshot, true)?;
                        }
                        events.push(workspace_event(
                            VikerLspEventKind::FormattingApplied,
                            Some(pending.language),
                            pending.uri,
                            Some(id),
                            Some(format!("{count} formatting edit(s)")),
                        ));
                    }
                }
            }
            LspMessage::Notification { method, params } => {
                if method == "textDocument/publishDiagnostics" {
                    let Some(uri) = lsp::diagnostics_uri(&params) else {
                        return Ok(events);
                    };
                    let diagnostics = lsp::parse_diagnostics(&params);
                    let (editor, document_language) = {
                        let mut state = self.lock_state()?;
                        state.diagnostics.insert(uri.clone(), diagnostics.clone());
                        let document = state
                            .documents
                            .get(&uri)
                            .map(|document| (document.editor.clone(), document.language));
                        document.unwrap_or((None, primary_language))
                    };
                    if let Some(editor) = editor {
                        editor.lock()?.diagnostics = diagnostics.clone();
                    }
                    events.push(workspace_event(
                        VikerLspEventKind::DiagnosticsUpdated,
                        Some(document_language),
                        Some(uri),
                        None,
                        Some(format!("{} diagnostic(s)", diagnostics.len())),
                    ));
                }
            }
            LspMessage::ServerRequest { id, method, params } => {
                let mut state = self.lock_state()?;
                let tab_width = state
                    .documents
                    .values()
                    .find(|document| document.server_key == server_key)
                    .map(|document| document.tab_width)
                    .unwrap_or(4);
                if let Some(server) = state.servers.get_mut(&server_key) {
                    let response = match method.as_str() {
                        "window/workDoneProgress/create" | "client/registerCapability" => {
                            serde_json::Value::Null
                        }
                        "workspace/configuration" => {
                            lsp::workspace_configuration_response(&params, tab_width, true)
                        }
                        "workspace/applyEdit" => serde_json::json!({ "applied": false }),
                        _ => serde_json::Value::Null,
                    };
                    self.runtime
                        .block_on(server.client.respond(&id, response))?;
                }
            }
        }
        Ok(events)
    }
}

impl VikerLspState {
    fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            client: None,
            event_tx,
            event_rx,
            language: None,
            root_path: None,
            file_uri: None,
            last_notified_version: -1,
        }
    }

    fn clear(&mut self) {
        self.client = None;
        self.language = None;
        self.root_path = None;
        self.file_uri = None;
        self.last_notified_version = -1;
    }
}

impl VikerLspWorkspaceState {
    fn new() -> Self {
        Self {
            servers: HashMap::new(),
            documents: HashMap::new(),
            pending: HashMap::new(),
            diagnostics: HashMap::new(),
            completions: HashMap::new(),
            hovers: HashMap::new(),
            references: HashMap::new(),
            workspace_symbols: HashMap::new(),
        }
    }
}

impl Drop for VikerEditor {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lsp.lock() {
            if let Some(client) = &mut state.client {
                let _ = self.runtime.block_on(client.shutdown());
            }
            state.clear();
        }
    }
}

impl Drop for VikerLspWorkspace {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            for (_, mut server) in state.servers.drain() {
                let _ = self.runtime.block_on(server.client.shutdown());
            }
        }
    }
}

struct EditorLspSnapshot {
    uri: String,
    path: PathBuf,
    language: language::LanguageKind,
    text: String,
    version: i64,
    tab_width: usize,
    config: viker_core::config::Config,
    editor: Arc<VikerEditor>,
}

impl EditorLspSnapshot {
    fn document(&self) -> VikerLspDocument {
        VikerLspDocument {
            uri: self.uri.clone(),
            path: self.path.to_string_lossy().to_string(),
            language: syntax_language_from_core(self.language),
            version: self.version.max(0) as u64,
        }
    }
}

fn canonical_workspace_root(root_path: &str) -> Result<PathBuf, VikerError> {
    let root = PathBuf::from(root_path);
    let root = root.canonicalize().map_err(|error| VikerError::Io {
        message: format!("failed to resolve workspace root {root_path}: {error}"),
    })?;
    if !root.is_dir() {
        return Err(VikerError::InvalidInput {
            message: format!("workspace root is not a directory: {}", root.display()),
        });
    }
    Ok(root)
}

fn canonical_document_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn ensure_document_in_workspace(root: &Path, path: &Path) -> Result<(), VikerError> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(VikerError::InvalidInput {
            message: format!(
                "document {} is outside workspace {}",
                path.display(),
                root.display()
            ),
        })
    }
}

impl WorkspaceLspServerKey {
    fn from_invocation(invocation: &language::ToolInvocation) -> Self {
        Self {
            command: invocation.command.clone(),
            args: invocation.args.clone(),
        }
    }
}

fn workspace_server_key(
    language: language::LanguageKind,
    config: &viker_core::config::Config,
    root_path: &Path,
) -> Result<(WorkspaceLspServerKey, language::ToolInvocation), VikerError> {
    let spec = language.spec();
    let invocation =
        language::resolve_lsp(spec, config, root_path).ok_or_else(|| VikerError::InvalidInput {
            message: format!("LSP is disabled or unavailable for {}", spec.id),
        })?;
    Ok((
        WorkspaceLspServerKey::from_invocation(&invocation),
        invocation,
    ))
}

fn document_server_key(
    state: &VikerLspWorkspaceState,
    uri: &str,
) -> Result<WorkspaceLspServerKey, VikerError> {
    state
        .documents
        .get(uri)
        .map(|document| document.server_key.clone())
        .ok_or_else(|| VikerError::InvalidInput {
            message: format!("document is not registered with the workspace LSP: {uri}"),
        })
}

fn ready_workspace_server<'a>(
    state: &'a mut VikerLspWorkspaceState,
    server_key: &WorkspaceLspServerKey,
) -> Result<&'a mut WorkspaceLspServer, VikerError> {
    let Some(server) = state.servers.get_mut(server_key) else {
        return Err(VikerError::InvalidInput {
            message: format!("LSP is not running: {}", server_key.command),
        });
    };
    if !server.client.initialized {
        return Err(VikerError::InvalidInput {
            message: format!("LSP is still initializing: {}", server_key.command),
        });
    }
    Ok(server)
}

fn workspace_status_from_state(
    root_path: &Path,
    state: &VikerLspWorkspaceState,
) -> VikerLspWorkspaceStatus {
    let mut servers: Vec<_> = state
        .servers
        .iter()
        .map(|(server_key, server)| {
            workspace_server_status_from_state(
                root_path,
                server_key,
                server.primary_language,
                state,
                None,
            )
        })
        .collect();
    servers.sort_by(|a, b| format!("{:?}", a.language).cmp(&format!("{:?}", b.language)));

    let mut documents: Vec<_> = state
        .documents
        .values()
        .map(workspace_document_from_state)
        .collect();
    documents.sort_by(|a, b| a.path.cmp(&b.path));

    VikerLspWorkspaceStatus {
        root_path: root_path.to_string_lossy().to_string(),
        servers,
        documents,
    }
}

fn workspace_server_status_from_state(
    root_path: &Path,
    server_key: &WorkspaceLspServerKey,
    language: language::LanguageKind,
    state: &VikerLspWorkspaceState,
    message: Option<String>,
) -> VikerLspServerStatus {
    let open_document_count = state
        .documents
        .values()
        .filter(|document| &document.server_key == server_key)
        .count() as u64;
    VikerLspServerStatus {
        running: state.servers.contains_key(server_key),
        initialized: state
            .servers
            .get(server_key)
            .is_some_and(|server| server.client.initialized),
        language: syntax_language_from_core(language),
        root_path: root_path.to_string_lossy().to_string(),
        open_document_count,
        message,
    }
}

fn workspace_document_from_state(document: &WorkspaceDocument) -> VikerLspDocument {
    VikerLspDocument {
        uri: document.uri.clone(),
        path: document.path.to_string_lossy().to_string(),
        language: syntax_language_from_core(document.language),
        version: document.version.max(0) as u64,
    }
}

fn workspace_event(
    kind: VikerLspEventKind,
    language: Option<language::LanguageKind>,
    uri: Option<String>,
    request_id: Option<i64>,
    message: Option<String>,
) -> VikerLspWorkspaceEvent {
    VikerLspWorkspaceEvent {
        kind,
        language: language.map(syntax_language_from_core),
        uri,
        request_id: request_id.map(|id| id.max(0) as u64),
        message,
    }
}

fn ready_lsp_client(state: &mut VikerLspState) -> Result<&mut LspClient, VikerError> {
    let Some(client) = state.client.as_mut() else {
        return Err(VikerError::InvalidInput {
            message: "LSP is not running".to_string(),
        });
    };
    if !client.initialized {
        return Err(VikerError::InvalidInput {
            message: "LSP is still initializing".to_string(),
        });
    }
    Ok(client)
}

fn ready_lsp_uri(state: &VikerLspState) -> Result<String, VikerError> {
    let Some(client) = state.client.as_ref() else {
        return Err(VikerError::InvalidInput {
            message: "LSP is not running".to_string(),
        });
    };
    if !client.initialized {
        return Err(VikerError::InvalidInput {
            message: "LSP is still initializing".to_string(),
        });
    }
    state
        .file_uri
        .clone()
        .ok_or_else(|| VikerError::InvalidInput {
            message: "LSP has no open document".to_string(),
        })
}

fn lsp_status_from_state(state: &VikerLspState, message: Option<String>) -> VikerLspStatus {
    VikerLspStatus {
        running: state.client.is_some(),
        initialized: state
            .client
            .as_ref()
            .is_some_and(|client| client.initialized),
        language: state.language.map(syntax_language_from_core),
        root_path: state
            .root_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        file_uri: state.file_uri.clone(),
        message,
    }
}

fn git_diff_mode_to_core(mode: VikerGitDiffMode) -> core_git::GitDiffMode {
    match mode {
        VikerGitDiffMode::Worktree => core_git::GitDiffMode::Worktree,
        VikerGitDiffMode::Staged => core_git::GitDiffMode::Staged,
        VikerGitDiffMode::Head => core_git::GitDiffMode::Head,
    }
}

fn git_diff_mode_from_core(mode: core_git::GitDiffMode) -> VikerGitDiffMode {
    match mode {
        core_git::GitDiffMode::Worktree => VikerGitDiffMode::Worktree,
        core_git::GitDiffMode::Staged => VikerGitDiffMode::Staged,
        core_git::GitDiffMode::Head => VikerGitDiffMode::Head,
    }
}

fn git_change_from_core(change: core_git::GitChangeKind) -> VikerGitChangeKind {
    match change {
        core_git::GitChangeKind::Added => VikerGitChangeKind::Added,
        core_git::GitChangeKind::Modified => VikerGitChangeKind::Modified,
        core_git::GitChangeKind::Deleted => VikerGitChangeKind::Deleted,
        core_git::GitChangeKind::Renamed => VikerGitChangeKind::Renamed,
        core_git::GitChangeKind::Copied => VikerGitChangeKind::Copied,
        core_git::GitChangeKind::Typechange => VikerGitChangeKind::Typechange,
        core_git::GitChangeKind::Untracked => VikerGitChangeKind::Untracked,
        core_git::GitChangeKind::Conflicted => VikerGitChangeKind::Conflicted,
        core_git::GitChangeKind::Unknown => VikerGitChangeKind::Unknown,
    }
}

fn git_line_kind_from_core(kind: core_git::GitLineKind) -> VikerGitLineKind {
    match kind {
        core_git::GitLineKind::Context => VikerGitLineKind::Context,
        core_git::GitLineKind::Addition => VikerGitLineKind::Addition,
        core_git::GitLineKind::Deletion => VikerGitLineKind::Deletion,
        core_git::GitLineKind::Other => VikerGitLineKind::Other,
    }
}

fn git_diff_from_core(diff: core_git::GitDiff) -> VikerGitDiff {
    VikerGitDiff {
        repository_root: diff.repository_root,
        mode: git_diff_mode_from_core(diff.mode),
        branch: diff.branch,
        head: diff.head,
        files: diff
            .files
            .into_iter()
            .map(git_file_diff_from_core)
            .collect(),
    }
}

fn git_file_diff_from_core(file: core_git::GitFileDiff) -> VikerGitFileDiff {
    VikerGitFileDiff {
        old_path: file.old_path,
        new_path: file.new_path,
        change: git_change_from_core(file.change),
        binary: file.binary,
        hunks: file.hunks.into_iter().map(git_hunk_from_core).collect(),
    }
}

fn git_hunk_from_core(hunk: core_git::GitDiffHunk) -> VikerGitDiffHunk {
    VikerGitDiffHunk {
        id: hunk.id,
        header: hunk.header,
        old_start: hunk.old_start as u64,
        old_lines: hunk.old_lines as u64,
        new_start: hunk.new_start as u64,
        new_lines: hunk.new_lines as u64,
        lines: hunk.lines.into_iter().map(git_line_from_core).collect(),
    }
}

fn git_line_from_core(line: core_git::GitDiffLine) -> VikerGitDiffLine {
    VikerGitDiffLine {
        old_line: line.old_line.map(u64::from),
        new_line: line.new_line.map(u64::from),
        kind: git_line_kind_from_core(line.kind),
        prefix: line.prefix,
        content: line.content,
        highlights: line
            .highlights
            .into_iter()
            .map(git_highlight_from_core)
            .collect(),
    }
}

fn git_highlight_from_core(highlight: core_git::GitPatchHighlight) -> VikerGitPatchHighlight {
    VikerGitPatchHighlight {
        start_column: highlight.start_column as u64,
        end_column: highlight.end_column as u64,
        token: syntax_token_from_core(highlight.token),
        style: highlight_style_from_core(highlight.style),
    }
}

fn git_status_from_core(status: core_git::GitRepositoryStatus) -> VikerGitStatus {
    VikerGitStatus {
        repository_root: status.repository_root,
        branch: status.branch,
        head: status.head,
        detached: status.detached,
        files: status
            .files
            .into_iter()
            .map(git_status_file_from_core)
            .collect(),
        branches: status
            .branches
            .into_iter()
            .map(git_branch_from_core)
            .collect(),
        stashes: status
            .stashes
            .into_iter()
            .map(git_stash_from_core)
            .collect(),
    }
}

fn git_status_file_from_core(file: core_git::GitFileStatus) -> VikerGitFileStatus {
    VikerGitFileStatus {
        path: file.path,
        old_path: file.old_path,
        index: file.index.map(git_change_from_core),
        worktree: file.worktree.map(git_change_from_core),
        staged: file.staged,
        unstaged: file.unstaged,
        untracked: file.untracked,
        conflicted: file.conflicted,
    }
}

fn git_branch_from_core(branch: core_git::GitBranch) -> VikerGitBranch {
    VikerGitBranch {
        name: branch.name,
        is_current: branch.is_current,
        is_remote: branch.is_remote,
        upstream: branch.upstream,
        target: branch.target,
    }
}

fn git_stash_from_core(stash: core_git::GitStash) -> VikerGitStash {
    VikerGitStash {
        index: stash.index as u64,
        message: stash.message,
        oid: stash.oid,
    }
}

fn git_report_from_core(report: core_git::GitOperationReport) -> VikerGitOperationReport {
    VikerGitOperationReport {
        message: report.message,
        conflicts: report.conflicts,
    }
}

fn file_search_result_from_core(result: core_search::FileSearchResult) -> VikerFileSearchResult {
    VikerFileSearchResult {
        path: result.path,
        score: result.score,
        matched_indices: result
            .matched_indices
            .into_iter()
            .map(|index| index as u64)
            .collect(),
    }
}

fn file_content_search_result_from_core(
    result: core_search::FileContentSearchResult,
) -> VikerFileContentSearchResult {
    VikerFileContentSearchResult {
        path: result.path,
        row: result.row as u64,
        column: result.column as u64,
        text: result.text,
        score: result.score,
        matched_indices: result
            .matched_indices
            .into_iter()
            .map(|index| index as u64)
            .collect(),
    }
}

fn request_from_id(id: i64) -> VikerLspRequest {
    VikerLspRequest {
        id: Some(id.max(0) as u64),
        completed: false,
    }
}

fn checked_u32(value: u64, name: &str) -> Result<u32, VikerError> {
    u32::try_from(value).map_err(|_| VikerError::InvalidInput {
        message: format!("{name} does not fit in an LSP position"),
    })
}

fn lsp_error_message(value: &serde_json::Value, fallback: &str) -> String {
    value
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or(fallback)
        .to_string()
}

fn apply_format_result(editor: &mut Editor, result: &serde_json::Value) -> usize {
    let Some(edits) = result.as_array() else {
        return 0;
    };
    let mut text_edits: Vec<_> = edits.iter().filter_map(lsp::parse_text_edit).collect();
    apply_text_edits(editor, &mut text_edits)
}

fn apply_rename_result(editor: &mut Editor, file_uri: &str, result: &serde_json::Value) -> usize {
    let mut edits = lsp::parse_rename_edits(result, file_uri);
    apply_text_edits(editor, &mut edits)
}

fn apply_text_edits(editor: &mut Editor, edits: &mut [lsp::LspTextEdit]) -> usize {
    if edits.is_empty() {
        return 0;
    }

    editor.history.save(&editor.document.rope, editor.cursor);
    edits.sort_by(|a, b| {
        b.start_line
            .cmp(&a.start_line)
            .then(b.start_col.cmp(&a.start_col))
    });
    let mut applied = 0usize;
    for edit in edits {
        let line_count = editor.document.rope.len_lines();
        if edit.start_line as usize >= line_count {
            continue;
        }
        let end_line = (edit.end_line as usize).min(line_count.saturating_sub(1));
        let start_idx =
            editor.document.rope.line_to_char(edit.start_line as usize) + edit.start_col as usize;
        let end_idx = editor.document.rope.line_to_char(end_line) + edit.end_col as usize;
        let start_idx = start_idx.min(editor.document.rope.len_chars());
        let end_idx = end_idx.min(editor.document.rope.len_chars());
        if start_idx < end_idx {
            editor.document.rope.remove(start_idx..end_idx);
        }
        if !edit.new_text.is_empty() {
            editor.document.rope.insert(start_idx, &edit.new_text);
        }
        applied += 1;
    }
    if applied > 0 {
        editor.document.modified = true;
        editor.document.bump_version();
        editor.syntax_tree = None;
        editor.line_styles.clear();
        editor.styles_offset = 0;
        editor.clamp_cursor();
    }
    applied
}

fn process_core_key(editor: &mut Editor, key: KeyInput) -> Vec<VikerEffect> {
    let Some(invocation) = keymap::map_key(editor, key) else {
        return Vec::new();
    };
    let Some(action) = input::execute_invocation(editor, invocation) else {
        return Vec::new();
    };

    match action {
        DeferredAction::PlayMacro(register) => {
            let Some(keys) = editor.macros.get(&register).cloned() else {
                return Vec::new();
            };
            let mut effects = Vec::new();
            for macro_key in keys {
                if let Some(invocation) = keymap::map_key(editor, macro_key) {
                    if let Some(action) = input::execute_invocation(editor, invocation) {
                        if !matches!(action, DeferredAction::PlayMacro(_)) {
                            effects.push(effect_from_deferred(action));
                        }
                    }
                }
            }
            effects
        }
        other => vec![effect_from_deferred(other)],
    }
}

fn effect_from_deferred(action: DeferredAction) -> VikerEffect {
    match action {
        DeferredAction::Rename(name) => VikerEffect {
            kind: VikerEffectKind::Rename,
            payload: Some(name),
        },
        DeferredAction::DidSave => VikerEffect {
            kind: VikerEffectKind::DidSave,
            payload: None,
        },
        DeferredAction::OpenFile(path) => VikerEffect {
            kind: VikerEffectKind::OpenFile,
            payload: Some(path),
        },
        DeferredAction::SyncFileUri => VikerEffect {
            kind: VikerEffectKind::SyncFileUri,
            payload: None,
        },
        DeferredAction::ShellCommand(command) => VikerEffect {
            kind: VikerEffectKind::ShellCommand,
            payload: Some(command),
        },
        DeferredAction::FormatDocument => VikerEffect {
            kind: VikerEffectKind::FormatDocument,
            payload: None,
        },
        DeferredAction::PlayMacro(register) => VikerEffect {
            kind: VikerEffectKind::PlayMacro,
            payload: Some(register.to_string()),
        },
        DeferredAction::Git(command) => VikerEffect {
            kind: VikerEffectKind::Git,
            payload: Some(format!("{command:?}")),
        },
    }
}

fn key_input_from_event(event: VikerKeyEvent) -> Result<KeyInput, VikerError> {
    let code = match event.key {
        VikerKey::Character => {
            let text = event.text.ok_or_else(|| VikerError::InvalidInput {
                message: "character keys require text".to_string(),
            })?;
            let mut chars = text.chars();
            let ch = chars.next().ok_or_else(|| VikerError::InvalidInput {
                message: "character key text is empty".to_string(),
            })?;
            if chars.next().is_some() {
                return Err(VikerError::InvalidInput {
                    message: "character key text must contain exactly one character".to_string(),
                });
            }
            KeyCode::Char(ch)
        }
        VikerKey::Escape => KeyCode::Esc,
        VikerKey::Enter => KeyCode::Enter,
        VikerKey::Backspace => KeyCode::Backspace,
        VikerKey::Tab => KeyCode::Tab,
        VikerKey::Backtab => KeyCode::BackTab,
        VikerKey::Up => KeyCode::Up,
        VikerKey::Down => KeyCode::Down,
        VikerKey::Left => KeyCode::Left,
        VikerKey::Right => KeyCode::Right,
    };
    Ok(KeyInput {
        code,
        ctrl: event.ctrl,
        alt: event.alt,
    })
}

fn snapshot_from_editor(editor: &Editor) -> VikerSnapshot {
    VikerSnapshot {
        text: editor.document.rope.to_string(),
        line_count: editor.document.line_count() as u64,
        cursor: position_from_core(editor.cursor),
        visual_anchor: editor.visual_anchor.map(position_from_core),
        mode: mode_from_core(editor.mode),
        file_path: editor
            .document
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        file_name: editor.document.file_name().to_string(),
        modified: editor.document.modified,
        status_message: editor.status_message.clone(),
        command_buffer: editor.command_buffer.clone(),
        search_query: editor.search_query.clone(),
        register_display: editor.register_display(),
    }
}

fn register_summaries(editor: &Editor) -> Vec<VikerRegisterSummary> {
    let mut summaries = Vec::new();
    let mut registers: Vec<_> = editor
        .registers
        .iter()
        .filter(|(_, register)| !register.content.is_empty())
        .collect();
    registers.sort_by_key(|(name, _)| **name);
    for (name, register) in registers {
        summaries.push(VikerRegisterSummary {
            name: name.to_string(),
            prefix: preview_text(&register.content, 48),
            linewise: register.linewise,
            is_macro: false,
        });
    }

    let mut macros: Vec<_> = editor
        .macros
        .iter()
        .filter(|(_, keys)| !keys.is_empty())
        .collect();
    macros.sort_by_key(|(name, _)| **name);
    for (name, keys) in macros {
        summaries.push(VikerRegisterSummary {
            name: name.to_string(),
            prefix: preview_macro(keys),
            linewise: false,
            is_macro: true,
        });
    }

    summaries
}

fn lsp_server_infos() -> Vec<VikerLspServerInfo> {
    language::LanguageKind::all()
        .iter()
        .filter_map(|spec| {
            let tool = spec.lsp?;
            let command = tool.command.to_string();
            let args = tool.args.iter().map(|arg| (*arg).to_string()).collect();
            Some(VikerLspServerInfo {
                language: syntax_language_from_core(spec.kind),
                language_id: spec.lsp_language_id.to_string(),
                name: spec.id.to_string(),
                installed: command_on_path(&command),
                installable: lsp_install_hint(&command).is_some(),
                install_hint: lsp_install_hint(&command).map(str::to_string),
                command,
                args,
            })
        })
        .collect()
}

fn command_on_path(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

fn lsp_install_hint(command: &str) -> Option<&'static str> {
    match command {
        "rust-analyzer" => Some("rustup component add rust-analyzer"),
        "vscode-html-language-server" | "vscode-css-language-server" => {
            Some("npm install -g vscode-langservers-extracted")
        }
        "typescript-language-server" => {
            Some("npm install -g typescript typescript-language-server")
        }
        "basedpyright-langserver" => Some("npm install -g basedpyright"),
        "fish-lsp" => Some("install fish-lsp and ensure fish-lsp is on PATH"),
        "bash-language-server" => Some("npm install -g bash-language-server"),
        _ => None,
    }
}

fn syntax_language_to_core(language: VikerSyntaxLanguage) -> language::LanguageKind {
    match language {
        VikerSyntaxLanguage::Rust => language::LanguageKind::Rust,
        VikerSyntaxLanguage::Markdown => language::LanguageKind::Markdown,
        VikerSyntaxLanguage::Html => language::LanguageKind::Html,
        VikerSyntaxLanguage::Css => language::LanguageKind::Css,
        VikerSyntaxLanguage::JavaScript => language::LanguageKind::JavaScript,
        VikerSyntaxLanguage::Jsx => language::LanguageKind::Jsx,
        VikerSyntaxLanguage::TypeScript => language::LanguageKind::TypeScript,
        VikerSyntaxLanguage::Tsx => language::LanguageKind::Tsx,
        VikerSyntaxLanguage::Python => language::LanguageKind::Python,
        VikerSyntaxLanguage::Fish => language::LanguageKind::Fish,
        VikerSyntaxLanguage::Bash => language::LanguageKind::Bash,
        VikerSyntaxLanguage::Zsh => language::LanguageKind::Zsh,
    }
}

fn syntax_language_from_core(language: SyntaxLanguage) -> VikerSyntaxLanguage {
    match language {
        SyntaxLanguage::Rust => VikerSyntaxLanguage::Rust,
        SyntaxLanguage::Markdown => VikerSyntaxLanguage::Markdown,
        SyntaxLanguage::Html => VikerSyntaxLanguage::Html,
        SyntaxLanguage::Css => VikerSyntaxLanguage::Css,
        SyntaxLanguage::JavaScript => VikerSyntaxLanguage::JavaScript,
        SyntaxLanguage::Jsx => VikerSyntaxLanguage::Jsx,
        SyntaxLanguage::TypeScript => VikerSyntaxLanguage::TypeScript,
        SyntaxLanguage::Tsx => VikerSyntaxLanguage::Tsx,
        SyntaxLanguage::Python => VikerSyntaxLanguage::Python,
        SyntaxLanguage::Fish => VikerSyntaxLanguage::Fish,
        SyntaxLanguage::Bash => VikerSyntaxLanguage::Bash,
        SyntaxLanguage::Zsh => VikerSyntaxLanguage::Zsh,
    }
}

fn color_from_core(color: RgbColor) -> VikerColor {
    VikerColor {
        red: color.0,
        green: color.1,
        blue: color.2,
    }
}

fn syntax_token_from_core(token: SyntaxToken) -> VikerSyntaxToken {
    match token {
        SyntaxToken::Text => VikerSyntaxToken::Text,
        SyntaxToken::Keyword => VikerSyntaxToken::Keyword,
        SyntaxToken::TypeName => VikerSyntaxToken::TypeName,
        SyntaxToken::Tag => VikerSyntaxToken::Tag,
        SyntaxToken::Attribute => VikerSyntaxToken::Attribute,
        SyntaxToken::Constructor => VikerSyntaxToken::Constructor,
        SyntaxToken::Function => VikerSyntaxToken::Function,
        SyntaxToken::Method => VikerSyntaxToken::Method,
        SyntaxToken::Macro => VikerSyntaxToken::Macro,
        SyntaxToken::StringLiteral => VikerSyntaxToken::StringLiteral,
        SyntaxToken::Escape => VikerSyntaxToken::Escape,
        SyntaxToken::Character => VikerSyntaxToken::Character,
        SyntaxToken::NumberLiteral => VikerSyntaxToken::NumberLiteral,
        SyntaxToken::BooleanLiteral => VikerSyntaxToken::BooleanLiteral,
        SyntaxToken::Constant => VikerSyntaxToken::Constant,
        SyntaxToken::Comment => VikerSyntaxToken::Comment,
        SyntaxToken::Variable => VikerSyntaxToken::Variable,
        SyntaxToken::Parameter => VikerSyntaxToken::Parameter,
        SyntaxToken::Property => VikerSyntaxToken::Property,
        SyntaxToken::Module => VikerSyntaxToken::Module,
        SyntaxToken::Label => VikerSyntaxToken::Label,
        SyntaxToken::Punctuation => VikerSyntaxToken::Punctuation,
        SyntaxToken::Operator => VikerSyntaxToken::OperatorToken,
        SyntaxToken::Heading => VikerSyntaxToken::Heading,
        SyntaxToken::RawText => VikerSyntaxToken::RawText,
        SyntaxToken::Link => VikerSyntaxToken::Link,
        SyntaxToken::LinkUrl => VikerSyntaxToken::LinkUrl,
        SyntaxToken::Emphasis => VikerSyntaxToken::Emphasis,
        SyntaxToken::Strong => VikerSyntaxToken::Strong,
        SyntaxToken::Unknown => VikerSyntaxToken::Unknown,
    }
}

fn highlight_style_from_core(style: SyntaxStyle) -> VikerHighlightStyle {
    VikerHighlightStyle {
        foreground: style.fg.map(color_from_core),
        italic: style.italic,
    }
}

fn highlight_span_from_core(span: HighlightSpan) -> VikerHighlightSpan {
    VikerHighlightSpan {
        row: span.row as u64,
        start_column: span.start_col as u64,
        end_column: span.end_col as u64,
        token: syntax_token_from_core(span.token),
        style: highlight_style_from_core(span.style),
    }
}

fn diagnostic_from_core(diagnostic: lsp::LspDiagnostic) -> VikerDiagnostic {
    VikerDiagnostic {
        start_line: diagnostic.start_line as u64,
        start_column: diagnostic.start_col as u64,
        end_line: diagnostic.end_line as u64,
        end_column: diagnostic.end_col as u64,
        severity: diagnostic.severity,
        message: diagnostic.message,
    }
}

fn completion_from_core(item: lsp::LspCompletionItem) -> VikerCompletionItem {
    VikerCompletionItem {
        label: item.label,
        detail: item.detail,
        insert_text: item.insert_text,
        kind: item.kind as u64,
    }
}

fn location_from_core(location: lsp::LspLocation) -> VikerLocation {
    VikerLocation {
        uri: location.uri,
        start_line: location.start_line as u64,
        start_column: location.start_col as u64,
        end_line: location.end_line as u64,
        end_column: location.end_col as u64,
    }
}

fn workspace_symbol_from_core(symbol: lsp::LspSymbolInfo) -> VikerWorkspaceSymbol {
    VikerWorkspaceSymbol {
        name: symbol.name,
        kind: symbol.kind as u64,
        kind_label: lsp::symbol_kind_label(symbol.kind).to_string(),
        uri: symbol.uri,
        start_line: symbol.start_line as u64,
        start_column: symbol.start_col as u64,
    }
}

fn mode_from_core(mode: Mode) -> VikerMode {
    match mode {
        Mode::Normal => VikerMode::Normal,
        Mode::Insert => VikerMode::Insert,
        Mode::Replace => VikerMode::Replace,
        Mode::Visual => VikerMode::Visual,
        Mode::VisualLine => VikerMode::VisualLine,
        Mode::VisualBlock => VikerMode::VisualBlock,
        Mode::Command => VikerMode::Command,
        Mode::Search => VikerMode::Search,
    }
}

fn selection_mode_to_core(mode: VikerSelectionMode) -> SelectionMode {
    match mode {
        VikerSelectionMode::Character => SelectionMode::Character,
        VikerSelectionMode::Line => SelectionMode::Line,
        VikerSelectionMode::Block => SelectionMode::Block,
    }
}

fn position_from_core(position: Position) -> VikerPosition {
    VikerPosition {
        row: position.row as u64,
        column: position.col as u64,
    }
}

fn view_cell_from_core(cell: viker_core::editor::display::ViewCell) -> VikerViewCell {
    VikerViewCell {
        row: cell.row as u64,
        column: cell.col as u64,
    }
}

fn position_to_core(position: VikerPosition) -> Result<Position, VikerError> {
    Ok(Position {
        row: checked_index(position.row, "row")?,
        col: checked_index(position.column, "column")?,
    })
}

fn normalize_text(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

fn checked_index(value: u64, name: &str) -> Result<usize, VikerError> {
    usize::try_from(value).map_err(|_| VikerError::InvalidInput {
        message: format!("{name} does not fit on this platform"),
    })
}

fn ensure_row_in_bounds(editor: &Editor, row: usize) -> Result<(), VikerError> {
    if row >= editor.document.line_count() {
        Err(VikerError::InvalidInput {
            message: format!("row {row} is out of bounds"),
        })
    } else {
        Ok(())
    }
}

fn line_without_trailing_newline(line: &str) -> String {
    line.trim_end_matches('\n').to_string()
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars().take(max_chars) {
        match ch {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn preview_key(key: &KeyInput) -> String {
    let mut out = String::new();
    if key.ctrl {
        out.push_str("C-");
    }
    if key.alt {
        out.push_str("A-");
    }
    match key.code {
        KeyCode::Char(ch) => out.push(ch),
        KeyCode::Esc => out.push_str("<Esc>"),
        KeyCode::Enter => out.push_str("<CR>"),
        KeyCode::Backspace => out.push_str("<BS>"),
        KeyCode::Tab => out.push_str("<Tab>"),
        KeyCode::BackTab => out.push_str("<S-Tab>"),
        KeyCode::Up => out.push_str("<Up>"),
        KeyCode::Down => out.push_str("<Down>"),
        KeyCode::Left => out.push_str("<Left>"),
        KeyCode::Right => out.push_str("<Right>"),
    }
    out
}

fn preview_macro(keys: &[KeyInput]) -> String {
    let mut out = String::new();
    for key in keys.iter().take(12) {
        out.push_str(&preview_key(key));
    }
    if keys.len() > 12 {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_event(ch: char) -> VikerKeyEvent {
        VikerKeyEvent {
            key: VikerKey::Character,
            text: Some(ch.to_string()),
            ctrl: false,
            alt: false,
        }
    }

    fn type_keys(editor: &Arc<VikerEditor>, input: &str) {
        for ch in input.chars() {
            editor.process_key(char_event(ch)).unwrap();
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("vikerkit-{name}-{unique}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn swift_exports_skim_file_and_content_search() {
        let root = temp_workspace("search");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join(".gitignore"), "target\n").unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn answer() -> i32 {\n    42\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/generated.rs"), "ignored\n").unwrap();

        let root_path = root.to_string_lossy().to_string();
        let files = viker_project_files(root_path.clone()).unwrap();
        assert_eq!(files, vec![".gitignore", "src/lib.rs"]);

        let file_results = viker_search_files(root_path.clone(), "srl".to_string(), 10).unwrap();
        assert!(
            file_results
                .iter()
                .any(|result| result.path == "src/lib.rs" && !result.matched_indices.is_empty())
        );

        let content_results =
            viker_search_file_contents(root_path.clone(), "answer".to_string(), 10).unwrap();
        let content = content_results
            .iter()
            .find(|result| result.path == "src/lib.rs")
            .unwrap();
        assert_eq!(content.row, 0);
        assert!(content.text.contains("answer"));
        assert!(!content.matched_indices.is_empty());

        let workspace = VikerLspWorkspace::open(root_path).unwrap();
        assert_eq!(workspace.project_files().unwrap(), files);
        assert!(
            workspace
                .search_files("srl".to_string(), 10)
                .unwrap()
                .iter()
                .any(|result| result.path == "src/lib.rs")
        );
        assert!(
            workspace
                .search_file_contents("answer".to_string(), 10)
                .unwrap()
                .iter()
                .any(|result| result.path == "src/lib.rs" && result.text.contains("answer"))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_editor_accepts_text_and_vim_keys() {
        let editor = VikerEditor::from_text("hello".to_string());

        editor
            .process_key(VikerKeyEvent {
                key: VikerKey::Escape,
                text: None,
                ctrl: false,
                alt: false,
            })
            .unwrap();
        editor.process_key(char_event('I')).unwrap();
        editor.input_text("Say ".to_string()).unwrap();
        editor
            .process_key(VikerKeyEvent {
                key: VikerKey::Escape,
                text: None,
                ctrl: false,
                alt: false,
            })
            .unwrap();

        let snapshot = editor.snapshot().unwrap();
        assert_eq!(snapshot.text, "Say hello\n");
        assert!(matches!(snapshot.mode, VikerMode::Normal));
    }

    #[test]
    fn swift_editor_vim_replaces_characters() {
        let editor = VikerEditor::from_text("abcdef".to_string());

        type_keys(&editor, "lrZ");
        assert_eq!(editor.text().unwrap(), "aZcdef\n");

        type_keys(&editor, "l3r!");
        assert_eq!(editor.text().unwrap(), "aZ!!!f\n");
    }

    #[test]
    fn swift_editor_vim_counts_drive_movement_and_operators() {
        let editor = VikerEditor::from_text(
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n".to_string(),
        );

        type_keys(&editor, "9j");
        assert_eq!(editor.cursor().unwrap().row, 9);

        type_keys(&editor, "gg3dd");
        assert_eq!(
            editor.text().unwrap(),
            "four\nfive\nsix\nseven\neight\nnine\nten\n"
        );
    }

    #[test]
    fn swift_editor_vim_zz_centers_core_viewport() {
        let text = (1..=30)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        let editor = VikerEditor::from_text(text);

        editor.set_viewport_size(80, 5).unwrap();
        type_keys(&editor, "20Gzz");

        let cursor = editor.cursor().unwrap();
        let view_cell = editor.cursor_view_cell().unwrap().unwrap();
        assert_eq!(cursor.row, 19);
        assert_eq!(view_cell.row, 2);
    }

    #[test]
    fn swift_editor_exposes_lines_and_cursor() {
        let editor = VikerEditor::from_text("one\ntwo\nthree".to_string());

        assert_eq!(editor.line_count().unwrap(), 4);
        assert_eq!(editor.line(1).unwrap(), "two");
        assert_eq!(
            editor.lines(1, 2).unwrap(),
            vec!["two".to_string(), "three".to_string()]
        );
        assert_eq!(editor.cursor().unwrap().row, 0);
    }

    #[test]
    fn swift_editor_exposes_cursor_and_selection_placement() {
        let editor = VikerEditor::from_text("one two\nthree\n".to_string());

        let cursor = editor.set_cursor(1, 2).unwrap();
        assert_eq!(cursor.row, 1);
        assert_eq!(cursor.column, 2);

        editor
            .set_selection(
                VikerPosition { row: 0, column: 1 },
                VikerPosition { row: 1, column: 2 },
                VikerSelectionMode::Character,
            )
            .unwrap();
        let snapshot = editor.snapshot().unwrap();
        assert!(matches!(snapshot.mode, VikerMode::Visual));
        assert_eq!(snapshot.visual_anchor.unwrap().column, 1);

        editor.clear_selection().unwrap();
        assert!(matches!(editor.mode().unwrap(), VikerMode::Normal));

        assert!(editor.select_word_at(0, 5).unwrap());
        let snapshot = editor.snapshot().unwrap();
        assert_eq!(snapshot.visual_anchor.unwrap().column, 4);
        assert_eq!(snapshot.cursor.column, 6);

        assert!(editor.select_line_at(1).unwrap());
        assert!(matches!(editor.mode().unwrap(), VikerMode::VisualLine));
    }

    #[test]
    fn swift_editor_exposes_display_cells_and_visual_columns() {
        let editor = VikerEditor::from_text("\tab界e\u{301}x\n".to_string());

        assert_eq!(editor.line_display_width(0).unwrap(), 10);
        assert_eq!(editor.display_column_for_position(0, 4).unwrap(), 8);
        assert_eq!(editor.display_column_for_position(0, 6).unwrap(), 9);

        let wide = editor.position_for_display_column(0, 7).unwrap();
        assert_eq!(wide.column, 3);
        let after_wide = editor.position_for_display_column(0, 8).unwrap();
        assert_eq!(after_wide.column, 4);

        let cells = editor.display_cells(0).unwrap();
        assert_eq!(cells[0].char_start, 0);
        assert_eq!(cells[0].cell_width, 4);
        assert_eq!(cells[3].cell_start, 6);
        assert_eq!(cells[3].cell_width, 2);
        assert_eq!(cells[4].char_start, 4);
        assert_eq!(cells[4].char_end, 6);

        editor.set_cursor(0, 4).unwrap();
        assert_eq!(editor.cursor_display_column().unwrap(), 8);
        let view_cell = editor.cursor_view_cell().unwrap().unwrap();
        assert_eq!(view_cell.row, 0);
        assert_eq!(view_cell.column, 8);
    }

    #[test]
    fn swift_editor_exposes_syntax_highlight_spans() {
        let editor = VikerEditor::from_text(
            "#!/usr/bin/env python3\ndef greet(name):\n    return 'hi'\n".to_string(),
        );

        assert!(matches!(
            editor.syntax_language().unwrap(),
            Some(VikerSyntaxLanguage::Python)
        ));

        let spans = editor.highlight_spans(1, 2).unwrap();
        assert!(!spans.is_empty());
        assert!(spans.iter().all(|span| span.row >= 1 && span.row < 3));
        assert!(spans.iter().any(|span| span.row == 1
            && span.start_column <= 1
            && 1 < span.end_column
            && matches!(span.token, VikerSyntaxToken::Keyword)
            && span.style.foreground.as_ref().is_some_and(|color| {
                color.red == 198 && color.green == 120 && color.blue == 221
            })));

        let style = editor.highlight_style_at(1, 1).unwrap();
        assert!(style.foreground.is_some());
    }

    #[test]
    fn swift_editor_returns_empty_highlights_without_language() {
        let editor = VikerEditor::from_text("plain text\n".to_string());

        assert!(editor.syntax_language().unwrap().is_none());
        assert!(editor.highlight_spans(0, 1).unwrap().is_empty());
    }

    #[test]
    fn swift_editor_can_force_and_clear_language() {
        let editor = VikerEditor::from_text("fn main() {\n    let answer = 42;\n}\n".to_string());

        assert!(editor.syntax_language().unwrap().is_none());

        editor
            .set_language(Some(VikerSyntaxLanguage::Rust))
            .unwrap();
        assert!(matches!(
            editor.syntax_language().unwrap(),
            Some(VikerSyntaxLanguage::Rust)
        ));
        assert!(!editor.highlight_spans(0, 3).unwrap().is_empty());

        editor.set_language(None).unwrap();
        assert!(editor.syntax_language().unwrap().is_none());
        assert!(editor.highlight_spans(0, 3).unwrap().is_empty());
    }

    #[test]
    fn swift_editor_lists_lsp_servers_for_install_menu() {
        let editor = VikerEditor::new();
        let servers = editor.list_lsp_servers().unwrap();

        assert!(servers.iter().any(|server| {
            matches!(server.language, VikerSyntaxLanguage::TypeScript)
                && server.command == "typescript-language-server"
                && server.install_hint.is_some()
        }));
        assert!(servers.iter().any(|server| {
            matches!(server.language, VikerSyntaxLanguage::Python)
                && server.command == "basedpyright-langserver"
        }));
        assert!(servers.iter().all(|server| !server.name.is_empty()));
    }

    #[test]
    fn swift_editor_reports_lsp_status_and_requires_file_backed_start() {
        let editor = VikerEditor::new();

        let status = editor.lsp_status().unwrap();
        assert!(!status.running);
        assert!(status.language.is_none());

        let err = editor.start_lsp().unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));

        let err = editor.request_completion(0, 0).unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));
    }

    #[test]
    fn swift_editor_lsp_uses_forced_language() {
        let root = temp_workspace("forced-lsp-language");
        let path = root.join("notes.txt");
        std::fs::write(&path, "# Notes\n").unwrap();
        let editor = VikerEditor::open(path.to_string_lossy().to_string()).unwrap();

        editor
            .set_language(Some(VikerSyntaxLanguage::Markdown))
            .unwrap();
        let err = editor.start_lsp().unwrap_err();
        assert!(err.to_string().contains("markdown"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_workspace_reports_empty_status_and_lsp_menu() {
        let root = temp_workspace("empty-status");
        let workspace = VikerLspWorkspace::open(root.to_string_lossy().to_string()).unwrap();

        let status = workspace.status().unwrap();
        assert_eq!(
            status.root_path,
            root.canonicalize().unwrap().to_string_lossy().to_string()
        );
        assert!(status.servers.is_empty());
        assert!(status.documents.is_empty());
        assert!(
            workspace
                .list_lsp_servers()
                .unwrap()
                .iter()
                .any(|server| matches!(server.language, VikerSyntaxLanguage::Rust))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_workspace_rejects_untitled_and_outside_documents() {
        let root = temp_workspace("document-scope");
        let outside = temp_workspace("outside-document-scope");
        let outside_file = outside.join("main.rs");
        std::fs::write(&outside_file, "fn main() {}\n").unwrap();
        let workspace = VikerLspWorkspace::open(root.to_string_lossy().to_string()).unwrap();

        let err = workspace.open_document(VikerEditor::new()).unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));

        let outside_editor = VikerEditor::open(outside_file.to_string_lossy().to_string()).unwrap();
        let err = workspace.open_document(outside_editor).unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn swift_workspace_rejects_languages_without_lsp() {
        let root = temp_workspace("no-lsp");
        let markdown = root.join("README.md");
        std::fs::write(&markdown, "# Notes\n").unwrap();
        let workspace = VikerLspWorkspace::open(root.to_string_lossy().to_string()).unwrap();
        let editor = VikerEditor::open(markdown.to_string_lossy().to_string()).unwrap();

        let err = workspace.open_document(editor).unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));

        let err = workspace
            .start_lsp(VikerSyntaxLanguage::Markdown)
            .unwrap_err();
        assert!(matches!(err, VikerError::InvalidInput { .. }));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_workspace_groups_typescript_family_by_lsp_invocation() {
        let root = temp_workspace("server-groups");
        let config = viker_core::config::Config::default();
        let ts_key = workspace_server_key(language::LanguageKind::TypeScript, &config, &root)
            .unwrap()
            .0;

        for language in [
            language::LanguageKind::JavaScript,
            language::LanguageKind::Jsx,
            language::LanguageKind::Tsx,
        ] {
            let key = workspace_server_key(language, &config, &root).unwrap().0;
            assert_eq!(key, ts_key);
        }

        let html_key = workspace_server_key(language::LanguageKind::Html, &config, &root)
            .unwrap()
            .0;
        assert_ne!(html_key, ts_key);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swift_editor_reports_save_without_path() {
        let editor = VikerEditor::new();
        let err = editor.save().unwrap_err();
        assert!(matches!(err, VikerError::Io { .. }));
    }
}
