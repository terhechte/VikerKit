use std::io::{Stderr, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{
    Event, EventStream, KeyCode as CrosstermKeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;
use tokio::sync::mpsc;

use crate::ui;
use viker_core::config::ConfigLoadResult;
use viker_core::editor::document::Document;
use viker_core::editor::pane::AreaRect;
use viker_core::editor::selection::{Position, SelectionMode};
use viker_core::editor::{DeferredAction, Editor};
use viker_core::git::{self, GitDiffOptions, GitEditorCommand, GitOperationReport};
use viker_core::input;
use viker_core::input::command::Command;
use viker_core::key::{KeyCode, KeyInput, MouseInput, MouseKind};
use viker_core::language::{self, LanguageKind};
use viker_core::lsp::{self, AppEvent, LspClient, LspMessage};
use viker_core::search;
use viker_vim::keymap;

fn key_input_from_crossterm(key: KeyEvent) -> Option<KeyInput> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let code = match key.code {
        CrosstermKeyCode::Char(c) => KeyCode::Char(c),
        CrosstermKeyCode::Esc => KeyCode::Esc,
        CrosstermKeyCode::Enter => KeyCode::Enter,
        CrosstermKeyCode::Backspace => KeyCode::Backspace,
        CrosstermKeyCode::Tab => KeyCode::Tab,
        CrosstermKeyCode::BackTab => KeyCode::BackTab,
        CrosstermKeyCode::Up => KeyCode::Up,
        CrosstermKeyCode::Down => KeyCode::Down,
        CrosstermKeyCode::Left => KeyCode::Left,
        CrosstermKeyCode::Right => KeyCode::Right,
        _ => return None,
    };
    Some(KeyInput { code, ctrl, alt })
}

fn mouse_input_from_crossterm(mouse: MouseEvent) -> Option<MouseInput> {
    let kind = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => MouseKind::Down,
        MouseEventKind::Drag(MouseButton::Left) => MouseKind::Drag,
        MouseEventKind::Up(MouseButton::Left) => MouseKind::Up,
        MouseEventKind::ScrollUp => MouseKind::ScrollUp,
        MouseEventKind::ScrollDown => MouseKind::ScrollDown,
        MouseEventKind::ScrollLeft => MouseKind::ScrollLeft,
        MouseEventKind::ScrollRight => MouseKind::ScrollRight,
        _ => return None,
    };
    Some(MouseInput {
        kind,
        row: mouse.row,
        col: mouse.column,
        shift: mouse.modifiers.contains(KeyModifiers::SHIFT),
        ctrl: mouse.modifiers.contains(KeyModifiers::CONTROL),
        alt: mouse.modifiers.contains(KeyModifiers::ALT),
    })
}

fn area_contains(area: AreaRect, col: u16, row: u16) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn git_diff_summary(diff: &git::GitDiff) -> String {
    let hunk_count: usize = diff.files.iter().map(|file| file.hunks.len()).sum();
    let line_count: usize = diff
        .files
        .iter()
        .flat_map(|file| &file.hunks)
        .map(|hunk| hunk.lines.len())
        .sum();
    format!(
        "Git diff {:?}: {} file(s), {} hunk(s), {} line(s)",
        diff.mode,
        diff.files.len(),
        hunk_count,
        line_count
    )
}

fn git_report_message(report: GitOperationReport) -> String {
    if report.conflicts.is_empty() {
        format!("Git: {}", report.message)
    } else {
        format!(
            "Git: {} ({} conflict(s))",
            report.message,
            report.conflicts.len()
        )
    }
}

pub struct App {
    pub editor: Editor,
    project_root: Option<PathBuf>,
    lsp_client: Option<LspClient>,
    lsp_language: Option<LanguageKind>,
    lsp_root: Option<PathBuf>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    file_uri: Option<String>,
    last_notified_version: i64,
    last_pane_rects: Vec<(usize, AreaRect)>,
    mouse_selection_anchor: Option<Position>,
}

impl App {
    pub fn new(path: Option<String>, config_result: ConfigLoadResult) -> Result<Self> {
        let mut project_root = None;
        let document = match path {
            Some(p) => {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    project_root = Some(std::fs::canonicalize(&path).unwrap_or(path));
                    Document::new_empty()
                } else {
                    Document::open(&path.to_string_lossy())?
                }
            }
            None => Document::new_empty(),
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let mut editor = Editor::with_config(document, config_result.config);
        if let Some(warning) = config_result.warning {
            editor.status_message = Some(warning);
        } else if let Some(root) = &project_root {
            editor.status_message = Some(format!("Project: {}", root.display()));
        }

        Ok(Self {
            editor,
            project_root,
            lsp_client: None,
            lsp_language: None,
            lsp_root: None,
            event_rx,
            event_tx,
            file_uri: None,
            last_notified_version: 0,
            last_pane_rects: Vec::new(),
            mouse_selection_anchor: None,
        })
    }

    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stderr>>) -> Result<()> {
        // Start LSP if we have a file with a path
        if let Some(path) = &self.editor.document.path {
            let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            self.editor.document.path = Some(path.clone());
            if lsp::supports_lsp(&path) {
                self.start_lsp(&path).await;
            }
        }

        // Spawn crossterm event reader into the event channel
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind == KeyEventKind::Press
                            && let Some(ki) = key_input_from_crossterm(key)
                            && tx.send(AppEvent::Key(ki)).is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(Event::Resize(w, h))) => {
                        if tx.send(AppEvent::Resize(w, h)).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        if let Some(mouse) = mouse_input_from_crossterm(mouse)
                            && tx.send(AppEvent::Mouse(mouse)).is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                    None => break,
                }
            }
        });

        // Render tick interval
        let mut render_interval = tokio::time::interval(Duration::from_millis(16));

        loop {
            tokio::select! {
                _ = render_interval.tick() => {
                    // Calculate pane area dimensions
                    let size = terminal.size()?;
                    let tab_rows: u16 = if self.editor.buffers.len() > 1 { 1 } else { 0 };
                    let command_rows: u16 = 1;
                    let pane_area = AreaRect::new(
                        0,
                        tab_rows,
                        size.width,
                        size.height.saturating_sub(tab_rows + command_rows),
                    );
                    self.editor.editor_area = pane_area;

                    // Calculate layout rects for all panes
                    let pane_rects = self.editor.pane_layout.layout(pane_area);
                    self.last_pane_rects = pane_rects.clone();

                    // Update view dimensions for each pane
                    for &(pane_id, rect) in &pane_rects {
                        // Each pane rect includes editor area + status line (1 row)
                        let editor_height = rect.height.saturating_sub(1);
                        let editor_width = rect.width;

                        if pane_id == self.editor.active_pane_id {
                            // Active pane uses top-level editor fields
                            self.editor.view.width = editor_width;
                            self.editor.view.height = editor_height;
                        } else {
                            // Inactive panes update their saved view
                            if let Some(pane) = self.editor.panes.iter_mut().find(|p| p.id == pane_id) {
                                pane.view.width = editor_width;
                                pane.view.height = editor_height;
                            }
                        }
                    }

                    self.editor.scroll();

                    // Update syntax highlights for visible viewport
                    self.editor.update_highlights();

                    // Render
                    terminal.draw(|frame| {
                        ui::render(&self.editor, frame);
                    })?;
                }

                Some(event) = self.event_rx.recv() => {
                    match event {
                        AppEvent::Key(key) => {
                            // Record key events for macro recording
                            if self.editor.recording_macro.is_some() {
                                // Don't record the 'q' that stops recording
                                let is_stop = matches!(
                                    key.code,
                                    viker_core::key::KeyCode::Char('q')
                                ) && !key.ctrl
                                    && self.editor.mode == viker_core::input::mode::Mode::Normal;
                                if !is_stop {
                                    self.editor.macro_buffer.push(key);
                                }
                            }

                            if let Some(invocation) = keymap::map_key(&mut self.editor, key) {
                                let cmd = &invocation.command;
                                let trigger_completion = matches!(cmd, Command::TriggerCompletion);
                                let trigger_goto = matches!(cmd, Command::GotoDefinition);
                                let trigger_hover = matches!(cmd, Command::Hover);
                                let trigger_refs = matches!(cmd, Command::FindReferences);
                                let trigger_ref_jump = matches!(cmd, Command::ReferenceJump);
                                let trigger_file_finder = matches!(cmd, Command::OpenFileFinder);
                                let trigger_code_action = matches!(cmd, Command::CodeAction);
                                let trigger_code_action_accept = matches!(cmd, Command::CodeActionAccept);
                                let trigger_ws_symbol = matches!(cmd, Command::WorkspaceSymbol);
                                let trigger_ws_confirm = matches!(cmd, Command::WorkspaceSymbolConfirm);

                                // Dismiss completion on non-completion input
                                if !matches!(
                                    cmd,
                                    Command::TriggerCompletion
                                    | Command::AcceptCompletion
                                    | Command::CancelCompletion
                                    | Command::CompletionNext
                                    | Command::CompletionPrev
                                ) && self.editor.showing_completion {
                                    self.editor.cancel_completion();
                                }

                                let deferred = input::execute_invocation(&mut self.editor, invocation);

                                // Handle async LSP commands
                                if trigger_completion {
                                    self.request_completion().await;
                                }
                                if trigger_goto {
                                    self.request_goto_definition().await;
                                }
                                if trigger_hover {
                                    self.request_hover().await;
                                }
                                if trigger_refs {
                                    self.request_references().await;
                                }
                                if trigger_ref_jump {
                                    self.jump_to_reference().await;
                                }
                                if trigger_file_finder {
                                    let entries = self.scan_project_files();
                                    self.editor.open_file_finder(entries);
                                }
                                if trigger_code_action {
                                    self.request_code_action().await;
                                }
                                if trigger_code_action_accept {
                                    self.accept_code_action().await;
                                }
                                if trigger_ws_symbol {
                                    self.editor.open_workspace_symbols();
                                }
                                if trigger_ws_confirm {
                                    self.jump_to_workspace_symbol().await;
                                }
                                // Send workspace symbol request if query changed
                                if self.editor.workspace_symbol_needs_request {
                                    self.editor.workspace_symbol_needs_request = false;
                                    self.request_workspace_symbols().await;
                                }

                                // Handle deferred actions
                                if let Some(action) = deferred {
                                    self.handle_deferred(action, terminal).await;
                                }
                            }

                            // Send LSP didChange after edits
                            self.notify_lsp_change().await;
                        }

                        AppEvent::Resize(_, _) => {
                            // Size will be updated on next render tick
                        }

                        AppEvent::Mouse(mouse) => {
                            self.handle_mouse(mouse).await;
                        }

                        AppEvent::Lsp(msg) => {
                            self.handle_lsp_message(msg).await;
                        }
                    }

                    if self.editor.should_quit {
                        break;
                    }
                }
            }
        }

        // Shutdown LSP
        if let Some(lsp) = &mut self.lsp_client {
            let _ = lsp.shutdown().await;
        }

        Ok(())
    }

    async fn handle_mouse(&mut self, mouse: MouseInput) {
        match mouse.kind {
            MouseKind::ScrollUp => {
                self.editor.scroll_viewport_up(3);
                return;
            }
            MouseKind::ScrollDown => {
                self.editor.scroll_viewport_down(3);
                return;
            }
            MouseKind::ScrollLeft | MouseKind::ScrollRight => return,
            MouseKind::Up => {
                self.mouse_selection_anchor = None;
                return;
            }
            MouseKind::Down | MouseKind::Drag => {}
        }

        if self.editor.showing_completion
            || self.editor.showing_hover
            || self.editor.showing_references
            || self.editor.showing_code_actions
            || self.editor.showing_diagnostics
            || self.editor.showing_file_finder
            || self.editor.showing_workspace_symbols
            || self.editor.showing_git_diff
        {
            return;
        }

        let pane = self
            .last_pane_rects
            .iter()
            .find(|(_, rect)| area_contains(*rect, mouse.col, mouse.row))
            .copied();
        let Some((pane_id, pane_rect)) = pane else {
            return;
        };

        if mouse.kind == MouseKind::Down && pane_id != self.editor.active_pane_id {
            self.editor.save_active_pane();
            self.editor.load_pane(pane_id);
            self.sync_file_uri().await;
            self.mouse_selection_anchor = None;
        }

        let editor_height = self.editor.view.height;
        if mouse.row >= pane_rect.y.saturating_add(editor_height) {
            return;
        }

        let gutter_width = self.editor.gutter_width();
        let text_start = pane_rect.x.saturating_add(gutter_width);
        if mouse.col < text_start {
            return;
        }

        let screen_row = mouse.row.saturating_sub(pane_rect.y) as usize;
        let screen_col = mouse.col.saturating_sub(text_start) as usize;
        let position = self.editor.position_for_view_cell(screen_row, screen_col);

        match mouse.kind {
            MouseKind::Down => {
                if mouse.shift {
                    self.editor.extend_selection_to(position.row, position.col);
                    self.mouse_selection_anchor = self.editor.visual_anchor;
                } else {
                    let cursor = self.editor.set_cursor_for_view_cell(screen_row, screen_col);
                    self.mouse_selection_anchor = Some(cursor);
                }
            }
            MouseKind::Drag => {
                if let Some(anchor) = self.mouse_selection_anchor {
                    if position != anchor {
                        self.editor
                            .set_selection(anchor, position, SelectionMode::Character);
                    }
                }
            }
            _ => {}
        }
    }

    async fn start_lsp(&mut self, file_path: &Path) {
        let Some(spec) = language::spec_for_path(file_path) else {
            return;
        };
        let Some(invocation) = language::resolve_lsp(spec, &self.editor.config, file_path) else {
            if let Some(lsp) = &mut self.lsp_client {
                let _ = lsp.shutdown().await;
            }
            self.lsp_client = None;
            self.lsp_language = None;
            self.lsp_root = None;
            self.file_uri = None;
            return;
        };
        let root = self.root_for_file(file_path);
        if self.lsp_client.is_some()
            && self.lsp_language == Some(spec.kind)
            && self.lsp_root.as_deref() == Some(root.as_path())
        {
            return;
        }

        if let Some(lsp) = &mut self.lsp_client {
            let _ = lsp.shutdown().await;
        }
        self.lsp_client = None;
        self.lsp_language = None;
        self.lsp_root = None;
        self.file_uri = None;

        let tx = self.event_tx.clone();

        match LspClient::start(&root, spec.kind, invocation, tx).await {
            Ok(client) => {
                self.lsp_client = Some(client);
                self.lsp_language = Some(spec.kind);
                self.lsp_root = Some(root.clone());
                self.editor.status_message = Some(format!(
                    "LSP: starting {} (root: {})",
                    spec.id,
                    root.display()
                ));
            }
            Err(e) => {
                self.editor.status_message = Some(format!("LSP: failed to start: {e}"));
            }
        }
    }

    async fn handle_lsp_message(&mut self, msg: LspMessage) {
        match msg {
            LspMessage::Response { id, result, error } => {
                // Handle initialize response
                if let Some(lsp) = &mut self.lsp_client
                    && id == lsp.initialize_id
                    && !lsp.initialized
                {
                    if error.is_some() {
                        self.editor.status_message = Some("LSP: initialize failed".to_string());
                        return;
                    }
                    let _ = lsp.send_initialized().await;

                    // Send didOpen for the current file
                    if let Some(path) = &self.editor.document.path
                        && language::spec_for_path(path)
                            .is_some_and(|spec| spec.kind == lsp.language)
                    {
                        let uri = lsp::path_to_uri(path);
                        let text = self.editor.document.rope.to_string();
                        let version = self.editor.document.version;
                        let _ = lsp.did_open(&uri, &text, version).await;
                        self.file_uri = Some(uri);
                        self.last_notified_version = version;
                    }

                    self.editor.status_message = Some("LSP: ready".to_string());
                    return;
                }

                // Handle completion response
                if Some(id) == self.editor.pending_completion_id {
                    self.editor.pending_completion_id = None;
                    if let Some(result) = result {
                        let items = lsp::parse_completions(&result);
                        if !items.is_empty() {
                            self.editor.completions = items;
                            self.editor.completion_index = 0;
                            self.editor.showing_completion = true;
                        }
                    }
                    return;
                }

                // Handle goto definition response
                if Some(id) == self.editor.pending_goto_id {
                    self.editor.pending_goto_id = None;
                    if let Some(result) = result {
                        let locations = lsp::parse_goto_definition(&result);
                        if let Some(loc) = locations.first() {
                            self.editor.push_jump();
                            let current_uri = self.file_uri.as_deref().unwrap_or("");
                            if loc.uri == current_uri {
                                self.editor.cursor.row = loc.start_line as usize;
                                self.editor.cursor.col = loc.start_col as usize;
                                self.editor.clamp_cursor();
                                self.editor.scroll();
                            } else if let Some(path) = lsp::uri_to_path(&loc.uri) {
                                let target_line = loc.start_line;
                                let target_col = loc.start_col;
                                self.open_file(&path).await;
                                self.editor.cursor.row = target_line as usize;
                                self.editor.cursor.col = target_col as usize;
                                self.editor.clamp_cursor();
                                self.editor.scroll();
                            } else {
                                let name = loc.uri.rsplit('/').next().unwrap_or(&loc.uri);
                                self.editor.status_message = Some(format!(
                                    "Definition in {}:{}:{}",
                                    name,
                                    loc.start_line + 1,
                                    loc.start_col + 1
                                ));
                            }
                        } else {
                            self.editor.status_message = Some("No definition found".to_string());
                        }
                    }
                    return;
                }

                // Handle hover response
                if Some(id) == self.editor.pending_hover_id {
                    self.editor.pending_hover_id = None;
                    if let Some(result) = result {
                        if let Some(text) = lsp::parse_hover(&result) {
                            self.editor.hover_text = Some(text);
                            self.editor.showing_hover = true;
                        } else {
                            self.editor.status_message = Some("No hover info".to_string());
                        }
                    }
                    return;
                }

                // Handle references response
                if Some(id) == self.editor.pending_references_id {
                    self.editor.pending_references_id = None;
                    if let Some(result) = result {
                        let locations = lsp::parse_references(&result);
                        if !locations.is_empty() {
                            self.editor.references = locations;
                            self.editor.reference_index = 0;
                            self.editor.showing_references = true;
                        } else {
                            self.editor.status_message = Some("No references found".to_string());
                        }
                    }
                    return;
                }

                // Handle formatting response
                if Some(id) == self.editor.pending_format_id {
                    self.editor.pending_format_id = None;
                    if let Some(result) = result {
                        self.apply_format_edits(&result);
                    }
                    return;
                }

                // Handle code action response
                if Some(id) == self.editor.pending_code_action_id {
                    self.editor.pending_code_action_id = None;
                    if let Some(result) = result {
                        let actions = lsp::parse_code_actions(&result);
                        if !actions.is_empty() {
                            self.editor.code_actions = actions;
                            self.editor.code_action_index = 0;
                            self.editor.showing_code_actions = true;
                        } else {
                            self.editor.status_message =
                                Some("No code actions available".to_string());
                        }
                    }
                    return;
                }

                // Handle workspace symbol response
                if Some(id) == self.editor.pending_workspace_symbol_id {
                    self.editor.pending_workspace_symbol_id = None;
                    if let Some(result) = result {
                        let symbols = lsp::parse_workspace_symbols(&result);
                        self.editor.workspace_symbol_results = symbols;
                        self.editor.workspace_symbol_index = 0;
                    }
                    return;
                }

                // Handle rename response
                if Some(id) == self.editor.pending_rename_id {
                    self.editor.pending_rename_id = None;
                    if let Some(ref err) = error {
                        let msg = err
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Rename failed");
                        self.editor.status_message = Some(format!("Rename error: {msg}"));
                        return;
                    }
                    if let Some(result) = result {
                        self.apply_rename_edits(&result);
                    }
                }
            }

            LspMessage::Notification { method, params } => {
                if method == "textDocument/publishDiagnostics" {
                    if lsp::diagnostics_uri(&params).as_deref() == self.file_uri.as_deref() {
                        self.editor.diagnostics = lsp::parse_diagnostics(&params);
                    }
                }
            }

            LspMessage::ServerRequest { id, method, params } => {
                // Respond to server requests (e.g., window/workDoneProgress/create)
                if let Some(lsp) = &mut self.lsp_client {
                    let response = match method.as_str() {
                        "window/workDoneProgress/create" | "client/registerCapability" => {
                            serde_json::Value::Null
                        }
                        "workspace/configuration" => lsp::workspace_configuration_response(
                            &params,
                            self.editor.config.tab_width,
                            true,
                        ),
                        "workspace/applyEdit" => serde_json::json!({ "applied": false }),
                        _ => serde_json::Value::Null,
                    };
                    let _ = lsp.respond(&id, response).await;
                }
            }
        }
    }

    async fn notify_lsp_change(&mut self) {
        let version = self.editor.document.version;
        if version == self.last_notified_version {
            return;
        }
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let text = self.editor.document.rope.to_string();
            let _ = lsp.did_change(uri, &text, version).await;
            self.last_notified_version = version;
        }
    }

    async fn request_completion(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = lsp.completion(uri, line, character).await {
                self.editor.pending_completion_id = Some(id);
            }
        }
    }

    async fn request_goto_definition(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = lsp.goto_definition(uri, line, character).await {
                self.editor.pending_goto_id = Some(id);
            }
        }
    }

    async fn request_hover(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = lsp.hover(uri, line, character).await {
                self.editor.pending_hover_id = Some(id);
            }
        }
    }

    async fn request_references(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = lsp.references(uri, line, character).await {
                self.editor.pending_references_id = Some(id);
            }
        }
    }

    async fn request_rename(&mut self, new_name: &str) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = lsp.rename(uri, line, character, new_name).await {
                self.editor.pending_rename_id = Some(id);
            }
        }
    }

    async fn request_workspace_symbols(&mut self) {
        if let Some(lsp) = &mut self.lsp_client {
            if !lsp.initialized {
                return;
            }
            let query = self.editor.workspace_symbol_query.clone();
            if let Ok(id) = lsp.workspace_symbol(&query).await {
                self.editor.pending_workspace_symbol_id = Some(id);
            }
        }
    }

    async fn jump_to_workspace_symbol(&mut self) {
        if let Some(sym) = self.editor.workspace_symbol_selected() {
            self.editor.workspace_symbol_cancel();
            self.editor.push_jump();
            let current_uri = self.file_uri.as_deref().unwrap_or("");
            if sym.uri == current_uri {
                self.editor.cursor.row = sym.start_line as usize;
                self.editor.cursor.col = sym.start_col as usize;
                self.editor.clamp_cursor();
                self.editor.scroll();
            } else if let Some(path) = lsp::uri_to_path(&sym.uri) {
                let target_line = sym.start_line;
                let target_col = sym.start_col;
                self.open_file(&path).await;
                self.editor.cursor.row = target_line as usize;
                self.editor.cursor.col = target_col as usize;
                self.editor.clamp_cursor();
                self.editor.scroll();
            } else {
                let name = sym.uri.rsplit('/').next().unwrap_or(&sym.uri);
                self.editor.status_message = Some(format!(
                    "Symbol in {}:{}:{}",
                    name,
                    sym.start_line + 1,
                    sym.start_col + 1
                ));
            }
        }
    }

    async fn request_code_action(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            let diagnostics = self.editor.diagnostics.clone();
            if let Ok(id) = lsp.code_action(uri, line, character, &diagnostics).await {
                self.editor.pending_code_action_id = Some(id);
            }
        }
    }

    async fn accept_code_action(&mut self) {
        if self.editor.code_actions.is_empty() {
            self.editor.dismiss_code_actions();
            return;
        }
        let action = self.editor.code_actions[self.editor.code_action_index].clone();
        self.editor.dismiss_code_actions();

        // Apply workspace edit if present
        if let Some(ref edit) = action.edit {
            self.apply_workspace_edit(edit);
        }

        // Execute command if present
        if let Some(ref command) = action.command
            && let Some(lsp) = &mut self.lsp_client
            && lsp.initialized
        {
            let cmd_str = command
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let arguments = command
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Array(Vec::new()));
            let _ = lsp
                .send_request(
                    "workspace/executeCommand",
                    serde_json::json!({
                        "command": cmd_str,
                        "arguments": arguments,
                    }),
                )
                .await;
        }

        self.editor.status_message = Some(format!("Applied: {}", action.title));
        self.notify_lsp_change().await;
    }

    fn apply_workspace_edit(&mut self, edit: &serde_json::Value) {
        let file_uri = match &self.file_uri {
            Some(uri) => uri.clone(),
            None => return,
        };

        // Collect edits from "changes" or "documentChanges"
        let mut text_edits: Vec<lsp::LspTextEdit> = Vec::new();

        if let Some(changes) = edit.get("changes").and_then(|c| c.as_object())
            && let Some(file_edits) = changes.get(&file_uri).and_then(|e| e.as_array())
        {
            for e in file_edits {
                if let Some(te) = lsp::parse_text_edit(e) {
                    text_edits.push(te);
                }
            }
        }

        if text_edits.is_empty()
            && let Some(doc_changes) = edit.get("documentChanges").and_then(|c| c.as_array())
        {
            for dc in doc_changes {
                let uri = dc
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str());
                if uri == Some(&file_uri)
                    && let Some(edit_arr) = dc.get("edits").and_then(|e| e.as_array())
                {
                    for e in edit_arr {
                        if let Some(te) = lsp::parse_text_edit(e) {
                            text_edits.push(te);
                        }
                    }
                }
            }
        }

        if text_edits.is_empty() {
            return;
        }

        // Save undo before applying
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);

        // Sort in reverse order for safe application
        text_edits.sort_by(|a, b| {
            b.start_line
                .cmp(&a.start_line)
                .then(b.start_col.cmp(&a.start_col))
        });

        for te in &text_edits {
            let line_count = self.editor.document.rope.len_lines();
            if (te.start_line as usize) >= line_count {
                continue;
            }
            let end_line = (te.end_line as usize).min(line_count.saturating_sub(1));
            let start_idx = self
                .editor
                .document
                .rope
                .line_to_char(te.start_line as usize)
                + te.start_col as usize;
            let end_idx = self.editor.document.rope.line_to_char(end_line) + te.end_col as usize;
            let end_idx = end_idx.min(self.editor.document.rope.len_chars());
            let start_idx = start_idx.min(self.editor.document.rope.len_chars());
            if start_idx < end_idx {
                self.editor.document.rope.remove(start_idx..end_idx);
            }
            if !te.new_text.is_empty() {
                self.editor.document.rope.insert(start_idx, &te.new_text);
            }
        }

        self.editor.document.modified = true;
        self.editor.document.bump_version();
        self.editor.clamp_cursor();
    }

    async fn jump_to_reference(&mut self) {
        if self.editor.references.is_empty() {
            return;
        }
        let loc = self.editor.references[self.editor.reference_index].clone();
        let current_uri = self.file_uri.as_deref().unwrap_or("");
        if loc.uri == current_uri {
            self.editor.cursor.row = loc.start_line as usize;
            self.editor.cursor.col = loc.start_col as usize;
            self.editor.clamp_cursor();
            self.editor.scroll();
            self.editor.dismiss_popup();
        } else if let Some(path) = lsp::uri_to_path(&loc.uri) {
            let target_line = loc.start_line;
            let target_col = loc.start_col;
            self.open_file(&path).await;
            self.editor.cursor.row = target_line as usize;
            self.editor.cursor.col = target_col as usize;
            self.editor.clamp_cursor();
            self.editor.scroll();
            self.editor.dismiss_popup();
        } else {
            let name = loc.uri.rsplit('/').next().unwrap_or(&loc.uri);
            self.editor.status_message = Some(format!(
                "Reference in {}:{}:{}",
                name,
                loc.start_line + 1,
                loc.start_col + 1
            ));
            self.editor.dismiss_popup();
        }
    }

    fn apply_rename_edits(&mut self, result: &serde_json::Value) {
        let file_uri = match &self.file_uri {
            Some(uri) => uri.clone(),
            None => return,
        };
        let edits = lsp::parse_rename_edits(result, &file_uri);
        if edits.is_empty() {
            self.editor.status_message = Some("No edits to apply".to_string());
            return;
        }

        // Save undo before applying all edits
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);

        let count = edits.len();
        // Edits are already sorted in reverse order by parse_rename_edits
        for edit in &edits {
            let start_line_char = self
                .editor
                .document
                .rope
                .line_to_char(edit.start_line as usize);
            let end_line_char = self
                .editor
                .document
                .rope
                .line_to_char(edit.end_line as usize);
            let start_idx = start_line_char + edit.start_col as usize;
            let end_idx = end_line_char + edit.end_col as usize;
            let end_idx = end_idx.min(self.editor.document.rope.len_chars());
            if start_idx < end_idx {
                self.editor.document.rope.remove(start_idx..end_idx);
            }
            if !edit.new_text.is_empty() {
                self.editor.document.rope.insert(start_idx, &edit.new_text);
            }
        }

        self.editor.document.modified = true;
        self.editor.document.bump_version();
        self.editor.clamp_cursor();
        self.editor.status_message = Some(format!("Renamed: {count} occurrence(s)"));
    }

    fn apply_format_edits(&mut self, result: &serde_json::Value) {
        let edits = match result.as_array() {
            Some(arr) => arr,
            None => {
                self.editor.status_message = Some("Formatted (no changes)".to_string());
                return;
            }
        };
        if edits.is_empty() {
            self.editor.status_message = Some("Formatted (no changes)".to_string());
            return;
        }

        // Save undo before applying
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);

        // Parse and sort edits in reverse order (apply from bottom to top)
        let mut text_edits: Vec<(usize, usize, usize, usize, String)> = edits
            .iter()
            .filter_map(|edit| {
                let range = edit.get("range")?;
                let start = range.get("start")?;
                let end = range.get("end")?;
                let new_text = edit.get("newText")?.as_str()?.to_string();
                Some((
                    start.get("line")?.as_u64()? as usize,
                    start.get("character")?.as_u64()? as usize,
                    end.get("line")?.as_u64()? as usize,
                    end.get("character")?.as_u64()? as usize,
                    new_text,
                ))
            })
            .collect();

        // Sort in reverse order by position
        text_edits.sort_by(|a, b| (b.2, b.3).cmp(&(a.2, a.3)));

        for (start_line, start_col, end_line, end_col, new_text) in &text_edits {
            let line_count = self.editor.document.rope.len_lines();
            if *start_line >= line_count {
                continue;
            }
            let end_line = (*end_line).min(line_count.saturating_sub(1));
            let start_idx = self.editor.document.rope.line_to_char(*start_line) + start_col;
            let end_idx = self.editor.document.rope.line_to_char(end_line) + end_col;
            let end_idx = end_idx.min(self.editor.document.rope.len_chars());
            let start_idx = start_idx.min(self.editor.document.rope.len_chars());
            if start_idx < end_idx {
                self.editor.document.rope.remove(start_idx..end_idx);
            }
            if !new_text.is_empty() {
                self.editor.document.rope.insert(start_idx, new_text);
            }
        }

        self.editor.document.modified = true;
        self.editor.document.bump_version();
        self.editor.clamp_cursor();
        self.editor.status_message = Some("Formatted".to_string());
    }

    async fn notify_lsp_did_save(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let _ = lsp.did_save(uri).await;
        }
    }

    fn scan_project_files(&self) -> Vec<String> {
        search::scan_project_files(self.file_browser_root())
    }

    async fn sync_file_uri(&mut self) {
        // Close old file in LSP
        if let (Some(lsp), Some(old_uri)) = (&mut self.lsp_client, &self.file_uri)
            && lsp.initialized
        {
            let _ = lsp
                .send_notification(
                    "textDocument/didClose",
                    serde_json::json!({
                        "textDocument": { "uri": old_uri }
                    }),
                )
                .await;
        }

        // Reset version tracking for the new file
        self.last_notified_version = self.editor.document.version;

        // Open new file in LSP
        if let Some(path) = self.editor.document.path.clone() {
            if !lsp::supports_lsp(&path) {
                self.file_uri = None;
                return;
            }
            self.start_lsp(&path).await;
            if let Some(lsp) = &mut self.lsp_client
                && lsp.initialized
                && language::spec_for_path(&path).is_some_and(|spec| spec.kind == lsp.language)
            {
                let uri = lsp::path_to_uri(&path);
                let text = self.editor.document.rope.to_string();
                let version = self.editor.document.version;
                let _ = lsp.did_open(&uri, &text, version).await;
                self.file_uri = Some(uri);
                self.last_notified_version = version;
            }
        } else {
            self.file_uri = None;
        }
    }

    async fn open_file(&mut self, rel_path: &str) {
        if self.editor.showing_file_finder {
            self.editor.file_finder_cancel();
        }
        let full_path = self.resolve_open_path(rel_path);

        // Check if already open in a buffer
        if let Some(idx) = self.editor.find_buffer_by_path(&full_path) {
            if idx != self.editor.current_buffer {
                self.editor.switch_buffer(idx);
                self.sync_file_uri().await;
            }
            return;
        }

        // Close old file in LSP
        if let (Some(lsp), Some(old_uri)) = (&mut self.lsp_client, &self.file_uri)
            && lsp.initialized
        {
            let _ = lsp
                .send_notification(
                    "textDocument/didClose",
                    serde_json::json!({
                        "textDocument": { "uri": old_uri }
                    }),
                )
                .await;
        }

        // Open new document as a new buffer
        match Document::open(&full_path.to_string_lossy()) {
            Ok(doc) => {
                self.editor.add_buffer(doc);
                self.editor.status_message =
                    Some(format!("\"{}\"", self.editor.document.file_name()));

                // Reset version tracking for the new file
                self.last_notified_version = self.editor.document.version;
                self.file_uri = None;
                self.sync_file_uri().await;
            }
            Err(e) => {
                self.editor.status_message = Some(format!("Error opening file: {e}"));
            }
        }
    }

    async fn handle_deferred(
        &mut self,
        action: DeferredAction,
        terminal: &mut Terminal<CrosstermBackend<Stderr>>,
    ) {
        match action {
            DeferredAction::Rename(new_name) => {
                self.request_rename(&new_name).await;
            }
            DeferredAction::DidSave => {
                self.format_on_save_if_configured().await;
                self.notify_lsp_did_save().await;
            }
            DeferredAction::OpenFile(path) => {
                self.open_file(&path).await;
            }
            DeferredAction::SyncFileUri => {
                self.sync_file_uri().await;
            }
            DeferredAction::ShellCommand(cmd) => {
                self.run_shell_command(&cmd, terminal);
            }
            DeferredAction::FormatDocument => {
                self.request_formatting().await;
            }
            DeferredAction::PlayMacro(ch) => {
                self.play_macro(ch, terminal).await;
            }
            DeferredAction::Git(command) => {
                self.handle_git_command(command);
            }
        }
    }

    fn handle_git_command(&mut self, command: GitEditorCommand) {
        let Some(anchor) = self.git_anchor_path() else {
            self.editor.status_message = Some("Git: no repository path available".to_string());
            return;
        };

        let result = match command {
            GitEditorCommand::Status => match git::repository_status(&anchor) {
                Ok(status) => {
                    let branch = status.branch.as_deref().unwrap_or("detached");
                    self.editor.status_message = Some(format!(
                        "Git {branch}: {} changed, {} branch(es), {} stash(es)",
                        status.files.len(),
                        status.branches.len(),
                        status.stashes.len()
                    ));
                    return;
                }
                Err(e) => Err(e),
            },
            GitEditorCommand::Branches => match git::repository_branches(&anchor) {
                Ok(branches) => {
                    let summary = branches
                        .iter()
                        .take(8)
                        .map(|branch| {
                            if branch.is_current {
                                format!("*{}", branch.name)
                            } else {
                                branch.name.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.editor.status_message = Some(if summary.is_empty() {
                        "Git: no branches".to_string()
                    } else {
                        format!("Branches: {summary}")
                    });
                    return;
                }
                Err(e) => Err(e),
            },
            GitEditorCommand::Diff { mode, paths } => match git::repository_diff(
                &anchor,
                GitDiffOptions {
                    mode,
                    pathspecs: paths,
                    ..GitDiffOptions::default()
                },
            ) {
                Ok(diff) => {
                    let summary = git_diff_summary(&diff);
                    self.editor.open_git_diff(diff);
                    self.editor.status_message = Some(summary);
                    return;
                }
                Err(e) => Err(e),
            },
            GitEditorCommand::StageFiles(paths) => {
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::stage_files(&anchor, &paths))
            }
            GitEditorCommand::UnstageFiles(paths) => {
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::unstage_files(&anchor, &paths))
            }
            GitEditorCommand::StageHunk { path, hunk_id } => {
                git::stage_hunk(&anchor, &path, &hunk_id)
            }
            GitEditorCommand::UnstageHunk { path, hunk_id } => {
                git::unstage_hunk(&anchor, &path, &hunk_id)
            }
            GitEditorCommand::DeleteFiles(paths) => {
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::delete_files(&anchor, &paths))
            }
            GitEditorCommand::CreateBranch(name) => git::create_branch(&anchor, &name),
            GitEditorCommand::CheckoutBranch(name) => git::checkout_branch(&anchor, &name),
            GitEditorCommand::Amend { message } => git::amend(&anchor, message.as_deref()),
            GitEditorCommand::StashPush { message } => git::stash_push(&anchor, message.as_deref()),
            GitEditorCommand::StashApply { index } => git::stash_apply(&anchor, index),
            GitEditorCommand::StashPop { index } => git::stash_pop(&anchor, index),
            GitEditorCommand::Merge { branch } => git::merge_branch(&anchor, &branch),
            GitEditorCommand::Rebase { upstream } => git::rebase_onto(&anchor, &upstream),
        };

        self.editor.status_message = Some(match result {
            Ok(report) => git_report_message(report),
            Err(e) => format!("Git: {e}"),
        });
    }

    fn git_anchor_path(&self) -> Option<PathBuf> {
        self.project_root
            .clone()
            .or_else(|| self.editor.document.path.clone())
            .or_else(|| std::env::current_dir().ok())
    }

    fn git_paths_or_current(&self, paths: Vec<String>) -> Result<Vec<String>> {
        if !paths.is_empty() {
            return Ok(paths);
        }
        let Some(path) = self.editor.document.path.as_ref() else {
            bail!("Git: no current file path");
        };
        let status = git::repository_status(path)?;
        let root = PathBuf::from(status.repository_root);
        let rel = path
            .strip_prefix(&root)
            .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
        Ok(vec![rel.to_string_lossy().replace('\\', "/")])
    }

    fn root_for_file(&self, file_path: &Path) -> PathBuf {
        if let Some(project_root) = &self.project_root {
            let file_path =
                std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
            if file_path.starts_with(project_root) {
                return project_root.clone();
            }
        }
        let root = lsp::find_project_root(file_path);
        std::fs::canonicalize(&root).unwrap_or(root)
    }

    fn file_browser_root(&self) -> PathBuf {
        if let Some(project_root) = &self.project_root {
            return project_root.clone();
        }
        if let Some(path) = &self.editor.document.path {
            return lsp::find_project_root(path);
        }
        std::env::current_dir().unwrap_or_default()
    }

    fn resolve_open_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        let resolved = if path.is_absolute() {
            path
        } else {
            self.file_browser_root().join(path)
        };
        std::fs::canonicalize(&resolved).unwrap_or(resolved)
    }

    fn run_shell_command(&mut self, cmd: &str, terminal: &mut Terminal<CrosstermBackend<Stderr>>) {
        // Leave alternate screen and disable raw mode
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();

        // Run the command
        let status = std::process::Command::new("sh").arg("-c").arg(cmd).status();

        // Show result and wait for Enter
        match status {
            Ok(s) => {
                eprintln!("\n[Process exited with {}]", s.code().unwrap_or(-1));
            }
            Err(e) => {
                eprintln!("\n[Error: {e}]");
            }
        }
        eprint!("Press ENTER to continue...");
        let _ = std::io::stderr().flush();

        // Wait for Enter key (blocking, raw mode is off)
        let mut buf = [0u8; 1];
        let _ = std::io::Read::read(&mut std::io::stdin(), &mut buf);

        // Re-enter alternate screen and raw mode
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::EnterAlternateScreen);

        // Force a full redraw
        let _ = terminal.clear();
    }

    async fn request_formatting(&mut self) {
        if let Some(path) = self.editor.document.path.clone()
            && let Some(spec) = language::spec_for_path(&path)
            && let Some(invocation) = language::resolve_formatter(spec, &self.editor.config, &path)
        {
            let cwd = self.root_for_file(&path);
            let input = self.editor.document.rope.to_string();
            match viker_core::formatter::format_text(&invocation, &cwd, &input) {
                Ok(output) if output == input => {
                    self.editor.status_message = Some("Formatted (no changes)".to_string());
                }
                Ok(output) => {
                    self.editor.replace_document_text(&output);
                    self.editor.status_message = Some("Formatted".to_string());
                    self.notify_lsp_change().await;
                }
                Err(e) => {
                    self.editor.status_message = Some(format!("Format failed: {e}"));
                }
            }
            return;
        }

        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                self.editor.status_message = Some("LSP not ready".to_string());
                return;
            }
            let params = serde_json::json!({
                "textDocument": { "uri": uri },
                "options": {
                    "tabSize": 4,
                    "insertSpaces": true,
                }
            });
            match lsp.send_request("textDocument/formatting", params).await {
                Ok(id) => {
                    self.editor.pending_format_id = Some(id);
                }
                Err(_) => {
                    self.editor.status_message = Some("Format request failed".to_string());
                }
            }
        } else {
            self.editor.status_message = Some("LSP not available".to_string());
        }
    }

    async fn format_on_save_if_configured(&mut self) {
        let Some(path) = self.editor.document.path.clone() else {
            return;
        };
        let Some(spec) = language::spec_for_path(&path) else {
            return;
        };
        if !language::format_on_save(spec, &self.editor.config) {
            return;
        }
        let Some(invocation) = language::resolve_formatter(spec, &self.editor.config, &path) else {
            return;
        };

        let cwd = self.root_for_file(&path);
        let input = self.editor.document.rope.to_string();
        match viker_core::formatter::format_text(&invocation, &cwd, &input) {
            Ok(output) if output == input => {}
            Ok(output) => {
                self.editor.replace_document_text(&output);
                match self.editor.document.save() {
                    Ok(()) => {
                        self.editor.status_message = Some(format!(
                            "\"{}\" formatted and written",
                            self.editor.document.file_name()
                        ));
                        self.notify_lsp_change().await;
                    }
                    Err(e) => {
                        self.editor.status_message = Some(format!("Format save failed: {e}"));
                    }
                }
            }
            Err(e) => {
                self.editor.status_message = Some(format!("Format failed: {e}"));
            }
        }
    }

    async fn play_macro(&mut self, ch: char, terminal: &mut Terminal<CrosstermBackend<Stderr>>) {
        let keys = match self.editor.macros.get(&ch) {
            Some(keys) => keys.clone(),
            None => {
                self.editor.status_message = Some(format!("Macro @{ch} is empty"));
                return;
            }
        };
        self.editor.last_macro = Some(ch);

        for key in &keys {
            if let Some(invocation) = keymap::map_key(&mut self.editor, *key) {
                let deferred = input::execute_invocation(&mut self.editor, invocation);
                if let Some(action) = deferred {
                    match action {
                        DeferredAction::PlayMacro(_) => {} // avoid infinite recursion
                        DeferredAction::Rename(new_name) => {
                            self.request_rename(&new_name).await;
                        }
                        DeferredAction::DidSave => {
                            self.format_on_save_if_configured().await;
                            self.notify_lsp_did_save().await;
                        }
                        DeferredAction::OpenFile(path) => {
                            self.open_file(&path).await;
                        }
                        DeferredAction::SyncFileUri => {
                            self.sync_file_uri().await;
                        }
                        DeferredAction::ShellCommand(cmd) => {
                            self.run_shell_command(&cmd, terminal);
                        }
                        DeferredAction::FormatDocument => {
                            self.request_formatting().await;
                        }
                        DeferredAction::Git(command) => {
                            self.handle_git_command(command);
                        }
                    }
                }
            }
            self.notify_lsp_change().await;

            if self.editor.should_quit {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viker_core::config::Config;
    use viker_core::input::mode::Mode;

    struct TempProject {
        root: PathBuf,
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn temp_project(name: &str) -> TempProject {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "viker-tui-project-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        TempProject { root }
    }

    fn test_app(text: &str) -> App {
        let mut app = App::new(
            None,
            ConfigLoadResult {
                config: Config::default(),
                warning: None,
            },
        )
        .unwrap();
        app.editor.replace_document_text(text);
        app.editor.document.modified = false;
        app.editor.view.width = 20;
        app.editor.view.height = 5;
        app.last_pane_rects = vec![(0, AreaRect::new(0, 0, 20, 6))];
        app
    }

    #[test]
    fn tui_area_contains_respects_edges() {
        let area = AreaRect::new(2, 3, 4, 5);

        assert!(area_contains(area, 2, 3));
        assert!(area_contains(area, 5, 7));
        assert!(!area_contains(area, 6, 7));
        assert!(!area_contains(area, 5, 8));
    }

    #[tokio::test]
    async fn tui_mouse_click_places_cursor_in_text_area() {
        let mut app = test_app("abc\ndef\n");
        let gutter = app.editor.gutter_width();

        app.handle_mouse(MouseInput {
            kind: MouseKind::Down,
            row: 1,
            col: gutter + 2,
            shift: false,
            ctrl: false,
            alt: false,
        })
        .await;

        assert_eq!(app.editor.cursor, Position { row: 1, col: 2 });
        assert_eq!(app.editor.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn tui_mouse_drag_creates_visual_selection() {
        let mut app = test_app("abc\ndef\n");
        let gutter = app.editor.gutter_width();

        app.handle_mouse(MouseInput {
            kind: MouseKind::Down,
            row: 0,
            col: gutter,
            shift: false,
            ctrl: false,
            alt: false,
        })
        .await;
        app.handle_mouse(MouseInput {
            kind: MouseKind::Drag,
            row: 1,
            col: gutter + 1,
            shift: false,
            ctrl: false,
            alt: false,
        })
        .await;

        assert_eq!(app.editor.mode, Mode::Visual);
        assert_eq!(app.editor.visual_anchor, Some(Position { row: 0, col: 0 }));
        assert_eq!(app.editor.cursor, Position { row: 1, col: 1 });
    }

    #[test]
    fn tui_directory_argument_sets_project_root_for_file_search() {
        let project = temp_project("scan");
        std::fs::create_dir_all(project.root.join("src")).unwrap();
        std::fs::create_dir_all(project.root.join("ignored")).unwrap();
        std::fs::write(project.root.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(project.root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.root.join("ignored/file.rs"), "ignored\n").unwrap();

        let app = App::new(
            Some(project.root.to_string_lossy().to_string()),
            ConfigLoadResult {
                config: Config::default(),
                warning: None,
            },
        )
        .unwrap();

        assert_eq!(
            app.project_root.as_deref(),
            Some(std::fs::canonicalize(&project.root).unwrap().as_path())
        );
        assert_eq!(app.scan_project_files(), vec![".gitignore", "src/main.rs"]);
    }

    #[tokio::test]
    async fn tui_open_file_resolves_relative_paths_against_project_root() {
        let project = temp_project("open");
        let file = project.root.join("docs/readme.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "# Read\n").unwrap();
        let expected_path = std::fs::canonicalize(&file).unwrap();
        let mut app = App::new(
            Some(project.root.to_string_lossy().to_string()),
            ConfigLoadResult {
                config: Config::default(),
                warning: None,
            },
        )
        .unwrap();
        app.editor
            .open_file_finder(vec!["docs/readme.md".to_string()]);

        app.open_file("docs/readme.md").await;

        assert_eq!(app.editor.document.rope.to_string(), "# Read\n");
        assert_eq!(
            app.editor.document.path.as_deref(),
            Some(expected_path.as_path())
        );
        assert!(!app.editor.showing_file_finder);
    }

    #[test]
    fn tui_project_root_drives_lsp_root_for_files_inside_project() {
        let project = temp_project("lsp");
        std::fs::write(project.root.join("package.json"), "{}\n").unwrap();
        let file = project.root.join("src/app.ts");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "const x = 1;\n").unwrap();
        let expected_root = std::fs::canonicalize(&project.root).unwrap();
        let app = App::new(
            Some(project.root.to_string_lossy().to_string()),
            ConfigLoadResult {
                config: Config::default(),
                warning: None,
            },
        )
        .unwrap();

        assert_eq!(app.root_for_file(&file), expected_root);
    }
}
