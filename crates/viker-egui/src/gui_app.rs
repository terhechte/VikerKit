use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;

use anyhow::{Context, Result, bail};
use viker_core::config::ConfigLoadResult;
use viker_core::editor::document::Document;
use viker_core::editor::pane::AreaRect;
use viker_core::editor::selection::{Position, SelectionMode};
use viker_core::editor::{DeferredAction, Editor};
use viker_core::git::{self, GitDiffOptions, GitEditorCommand, GitOperationReport};
use viker_core::input;
use viker_core::input::command::Command;
use viker_core::key::{KeyCode, KeyInput};
use viker_core::language::{self, LanguageKind};
use viker_core::lsp::{self, LspClient, LspMessage};
use viker_vim::keymap;

/// Search macOS font directories for a font file matching the given name.
fn find_font_file(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        format!("{home}/Library/Fonts"),
        "/Library/Fonts".to_string(),
        "/System/Library/Fonts".to_string(),
        "/System/Library/Fonts/Supplemental".to_string(),
    ];
    let name_lower = name.to_lowercase();
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_lowercase();
                if fname.contains(&name_lower)
                    && (fname.ends_with(".ttf") || fname.ends_with(".otf"))
                {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

/// Register a custom font as the primary Monospace font.
fn setup_custom_font(ctx: &egui::Context, font_path: &std::path::Path) -> Result<(), String> {
    let font_data = std::fs::read(font_path).map_err(|e| format!("Failed to read font: {e}"))?;

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "custom_mono".to_owned(),
        egui::FontData::from_owned(font_data).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "custom_mono".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("custom_mono".to_owned());

    ctx.set_fonts(fonts);
    Ok(())
}

fn canonical_dir(path: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    canonical.is_dir().then_some(canonical)
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

pub struct VikerEditor {
    editor: Editor,
    project_root: Option<PathBuf>,
    project_sidebar: crate::gui::project_sidebar::ProjectSidebar,
    show_startup_project_picker: bool,
    runtime: tokio::runtime::Runtime,
    lsp_client: Option<LspClient>,
    lsp_language: Option<LanguageKind>,
    lsp_root: Option<PathBuf>,
    lsp_rx: std_mpsc::Receiver<LspMessage>,
    lsp_tx: std_mpsc::Sender<LspMessage>,
    file_uri: Option<String>,
    last_notified_version: i64,
    /// Pane pixel rects from last frame, used for mouse hit-testing.
    last_pane_rects: Vec<(usize, egui::Rect)>,
    mouse_selection_anchor: Option<Position>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VikerEditorResponse {
    pub should_quit: bool,
}

pub type GuiApp = VikerEditor;

impl VikerEditor {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<String>,
        config_result: ConfigLoadResult,
    ) -> Self {
        let startup_path = path.as_deref().map(PathBuf::from);
        let project_root = startup_path
            .as_ref()
            .filter(|path| path.is_dir())
            .and_then(|path| canonical_dir(path));
        let startup_file = startup_path
            .as_ref()
            .filter(|path| !path.is_dir())
            .map(PathBuf::as_path);
        let document = match startup_file {
            Some(p) => {
                Document::open(&p.to_string_lossy()).unwrap_or_else(|_| Document::new_empty())
            }
            None => Document::new_empty(),
        };

        let (lsp_tx, lsp_rx) = std_mpsc::channel();
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let mut editor = Editor::with_config(document, config_result.config);
        if let Some(warning) = config_result.warning {
            editor.status_message = Some(warning);
        }

        let mut app = Self {
            editor,
            project_root,
            project_sidebar: crate::gui::project_sidebar::ProjectSidebar::default(),
            show_startup_project_picker: path.is_none(),
            runtime,
            lsp_client: None,
            lsp_language: None,
            lsp_root: None,
            lsp_rx,
            lsp_tx,
            file_uri: None,
            last_notified_version: 0,
            last_pane_rects: Vec::new(),
            mouse_selection_anchor: None,
        };
        if let Some(root) = app.project_root.clone() {
            app.project_sidebar.refresh(&root);
        }

        // Apply custom font if configured
        if let Some(ref font_name) = app.editor.config.font_family {
            match find_font_file(font_name) {
                Some(path) => {
                    if let Err(e) = setup_custom_font(&cc.egui_ctx, &path) {
                        app.editor.status_message = Some(e);
                    }
                }
                None => {
                    app.editor.status_message = Some(format!("Font not found: {font_name}"));
                }
            }
        }

        // Start LSP
        if let Some(path) = startup_file {
            let path = path.to_path_buf();
            if let Ok(canonical) = std::fs::canonicalize(&path) {
                app.editor.document.path = Some(canonical.clone());
                if lsp::supports_lsp(&canonical) {
                    app.start_lsp(&canonical);
                }
            }
        }

        app
    }

    pub fn from_editor(editor: Editor) -> Self {
        let (lsp_tx, lsp_rx) = std_mpsc::channel();
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let mut component = Self {
            editor,
            project_root: None,
            project_sidebar: crate::gui::project_sidebar::ProjectSidebar::default(),
            show_startup_project_picker: false,
            runtime,
            lsp_client: None,
            lsp_language: None,
            lsp_root: None,
            lsp_rx,
            lsp_tx,
            file_uri: None,
            last_notified_version: 0,
            last_pane_rects: Vec::new(),
            mouse_selection_anchor: None,
        };

        if let Some(path) = component.editor.document.path.clone()
            && lsp::supports_lsp(&path)
        {
            component.start_lsp(&path);
        }

        component
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {
        self.set_project_root_path(root.into());
    }

    pub fn show(&mut self, ctx: &egui::Context) -> VikerEditorResponse {
        if self.should_show_project_picker() {
            return self.show_project_picker(ctx);
        }

        let mut response = VikerEditorResponse::default();
        self.draw_top_toolbar(ctx);
        if let Some(path) = self.draw_project_sidebar_panel(ctx) {
            self.open_file(&path);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::gui::BG_COLOR))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                response = self.prepare_frame(ctx, rect);
                self.last_pane_rects = crate::gui::render_in_rect(&self.editor, ui, rect);
                ui.allocate_rect(rect, egui::Sense::hover());
            });

        // Request repaint for continuous updates (LSP messages, etc.)
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        response
    }

    pub fn show_inside(&mut self, ui: &mut egui::Ui) -> VikerEditorResponse {
        if self.should_show_project_picker() {
            return self.show_project_picker_inside(ui);
        }

        let ctx = ui.ctx().clone();
        if self.project_root.is_none() {
            let rect = ui.available_rect_before_wrap();
            let response = self.prepare_frame(&ctx, rect);
            self.last_pane_rects = crate::gui::render_in_rect(&self.editor, ui, rect);
            ui.allocate_rect(rect, egui::Sense::hover());
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return response;
        }

        let rect = ui.available_rect_before_wrap();
        let toolbar_height = 26.0_f32.min(rect.height());
        let toolbar_rect = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.max.x, rect.min.y + toolbar_height),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(toolbar_rect), |ui| {
            self.draw_toolbar_contents(ui);
        });

        let mut content_rect =
            egui::Rect::from_min_max(egui::pos2(rect.min.x, toolbar_rect.max.y), rect.max);
        if let Some(root) = self.project_root.clone()
            && self.project_sidebar.visible
        {
            self.ensure_sidebar_root(&root);
            self.project_sidebar
                .refresh_git_status_if_stale(&root, std::time::Duration::from_secs(3));
            let sidebar_width = if content_rect.width() >= 220.0 {
                content_rect.width().min(260.0).max(180.0)
            } else {
                content_rect.width() * 0.45
            };
            let sidebar_rect = egui::Rect::from_min_max(
                content_rect.min,
                egui::pos2(content_rect.min.x + sidebar_width, content_rect.max.y),
            );
            let active_rel = self.active_project_rel_path();
            let open_path = ui
                .allocate_new_ui(egui::UiBuilder::new().max_rect(sidebar_rect), |ui| {
                    crate::gui::project_sidebar::draw_sidebar(
                        ui,
                        &mut self.project_sidebar,
                        &root,
                        active_rel.as_deref(),
                    )
                })
                .inner;
            if let Some(path) = open_path {
                self.open_file(&path);
            }
            content_rect.min.x = sidebar_rect.max.x;
        }

        let response = self.prepare_frame(&ctx, content_rect);

        // Render and save pane rects for next frame's mouse handling
        self.last_pane_rects = crate::gui::render_in_rect(&self.editor, ui, content_rect);
        ui.allocate_rect(rect, egui::Sense::hover());

        // Request repaint for continuous updates (LSP messages, etc.)
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        response
    }

    fn should_show_project_picker(&self) -> bool {
        self.show_startup_project_picker && self.project_root.is_none()
    }

    fn show_project_picker(&mut self, ctx: &egui::Context) -> VikerEditorResponse {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::gui::BG_COLOR))
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    self.draw_open_folder_button(ui);
                });
            });

        VikerEditorResponse::default()
    }

    fn show_project_picker_inside(&mut self, ui: &mut egui::Ui) -> VikerEditorResponse {
        let rect = ui.available_rect_before_wrap();
        ui.painter_at(rect)
            .rect_filled(rect, 0.0, crate::gui::BG_COLOR);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.centered_and_justified(|ui| {
                self.draw_open_folder_button(ui);
            });
        });
        ui.allocate_rect(rect, egui::Sense::hover());

        VikerEditorResponse::default()
    }

    fn draw_open_folder_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Open Folder").clicked() {
            self.pick_project_folder();
        }
    }

    fn pick_project_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        if let Ok(cwd) = std::env::current_dir() {
            dialog = dialog.set_directory(cwd);
        }
        if let Some(folder) = dialog.pick_folder() {
            self.set_project_root_path(folder);
        }
    }

    fn set_project_root_path(&mut self, root: PathBuf) {
        let Some(root) = canonical_dir(&root) else {
            self.editor.status_message = Some(format!("Not a folder: {}", root.display()));
            return;
        };
        self.project_root = Some(root.clone());
        self.project_sidebar.refresh(&root);
        if let Some(path) = self.editor.document.path.clone() {
            self.note_recent_file(&path);
        }
        self.show_startup_project_picker = false;
        self.editor.status_message = Some(format!("Project: {}", root.display()));
    }

    fn draw_top_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("viker_project_toolbar")
            .exact_height(26.0)
            .show(ctx, |ui| {
                self.draw_toolbar_contents(ui);
            });
    }

    fn draw_toolbar_contents(&mut self, ui: &mut egui::Ui) {
        crate::gui::project_sidebar::draw_toolbar(
            ui,
            &mut self.project_sidebar,
            self.project_root.as_deref(),
        );
    }

    fn draw_project_sidebar_panel(&mut self, ctx: &egui::Context) -> Option<String> {
        let root = self.project_root.clone()?;
        if !self.project_sidebar.visible {
            return None;
        }
        self.ensure_sidebar_root(&root);
        self.project_sidebar
            .refresh_git_status_if_stale(&root, std::time::Duration::from_secs(3));
        let active_rel = self.active_project_rel_path();
        let mut open_path = None;
        egui::SidePanel::left("viker_project_sidebar")
            .resizable(true)
            .default_width(260.0)
            .width_range(180.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(crate::gui::BG_COLOR)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                open_path = crate::gui::project_sidebar::draw_sidebar(
                    ui,
                    &mut self.project_sidebar,
                    &root,
                    active_rel.as_deref(),
                );
            });
        open_path
    }

    fn ensure_sidebar_root(&mut self, root: &Path) {
        if self.project_sidebar.root() != Some(root) {
            self.project_sidebar.refresh(root);
        }
    }

    fn active_project_rel_path(&self) -> Option<String> {
        let root = self.project_root.as_ref()?;
        let path = self.editor.document.path.as_ref()?;
        crate::gui::project_sidebar::project_relative_path(root, path)
    }

    fn note_recent_file(&mut self, path: &Path) {
        if let Some(root) = self.project_root.as_ref() {
            self.project_sidebar.note_opened(root, path);
        }
    }

    fn refresh_sidebar_git_status(&mut self) {
        if let Some(root) = self.project_root.as_ref() {
            self.project_sidebar.refresh_git_status(root);
        }
    }

    fn prepare_frame(&mut self, ctx: &egui::Context, avail: egui::Rect) -> VikerEditorResponse {
        // Handle runtime font family change
        if self.editor.font_family_changed {
            self.editor.font_family_changed = false;
            match &self.editor.config.font_family {
                Some(font_name) => match find_font_file(font_name) {
                    Some(path) => {
                        if let Err(e) = setup_custom_font(ctx, &path) {
                            self.editor.status_message = Some(e);
                        }
                    }
                    None => {
                        ctx.set_fonts(egui::FontDefinitions::default());
                        self.editor.status_message =
                            Some(format!("Font not found: {font_name} (reset to default)"));
                        self.editor.config.font_family = None;
                    }
                },
                None => {
                    ctx.set_fonts(egui::FontDefinitions::default());
                }
            }
        }

        // Process pending LSP messages
        self.process_lsp_messages();

        // Handle keyboard input unless an egui control, such as the sidebar
        // search field, currently owns keyboard focus.
        if !ctx.wants_keyboard_input() {
            let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
            let mut suppress_next_text_event = false;
            for event in &events {
                match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        let key_input = egui_key_to_key_input(*key, *modifiers);
                        if let Some(ki) = key_input {
                            self.handle_key(ki, ctx);
                            suppress_next_text_event =
                                should_suppress_text_after_modified_key(*key, *modifiers);
                        }
                    }
                    egui::Event::Text(text) => {
                        if suppress_next_text_event {
                            suppress_next_text_event = false;
                            continue;
                        }

                        for ch in text.chars() {
                            if !ch.is_control() {
                                let ki = KeyInput {
                                    code: KeyCode::Char(ch),
                                    ctrl: false,
                                    alt: false,
                                };
                                self.handle_key(ki, ctx);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Update viewport dimensions
        let font_size = self.editor.config.font_size;
        let char_width = font_size * 0.6;
        let line_height = font_size * 1.4;
        let cols = (avail.width() / char_width) as u16;
        let rows = (avail.height() / line_height) as u16;

        let tab_rows: u16 = if self.editor.buffers.len() > 1 { 1 } else { 0 };
        let command_rows: u16 = 1;
        let pane_area = AreaRect::new(
            0,
            tab_rows,
            cols,
            rows.saturating_sub(tab_rows + command_rows),
        );
        self.editor.editor_area = pane_area;

        let pane_rects = self.editor.pane_layout.layout(pane_area);
        for &(pane_id, rect) in &pane_rects {
            let editor_height = rect.height.saturating_sub(1);
            let editor_width = rect.width;
            if pane_id == self.editor.active_pane_id {
                self.editor.view.width = editor_width;
                self.editor.view.height = editor_height;
            } else if let Some(pane) = self.editor.panes.iter_mut().find(|p| p.id == pane_id) {
                pane.view.width = editor_width;
                pane.view.height = editor_height;
            }
        }

        self.editor.scroll();
        self.editor.update_highlights();

        // Handle mouse input (uses pane rects from previous frame) unless
        // another egui widget consumed the pointer this frame.
        if !ctx.wants_pointer_input() {
            self.handle_mouse(ctx);
        }

        VikerEditorResponse {
            should_quit: self.editor.should_quit,
        }
    }

    fn start_lsp(&mut self, file_path: &Path) {
        let Some(spec) = language::spec_for_path(file_path) else {
            return;
        };
        let Some(invocation) = language::resolve_lsp(spec, &self.editor.config, file_path) else {
            if let Some(lsp) = &mut self.lsp_client {
                let _ = self.runtime.block_on(lsp.shutdown());
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
            let _ = self.runtime.block_on(lsp.shutdown());
        }
        self.lsp_client = None;
        self.lsp_language = None;
        self.lsp_root = None;
        self.file_uri = None;

        let tx = self.lsp_tx.clone();

        // Create a channel for the LSP async bridge
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        match self
            .runtime
            .block_on(LspClient::start(&root, spec.kind, invocation, event_tx))
        {
            Ok(client) => {
                self.lsp_client = Some(client);
                self.lsp_language = Some(spec.kind);
                self.lsp_root = Some(root.clone());
                self.editor.status_message = Some(format!(
                    "LSP: starting {} (root: {})",
                    spec.id,
                    root.display()
                ));

                // Spawn a task to bridge async LSP messages to sync channel
                self.runtime.spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        if let viker_core::lsp::AppEvent::Lsp(msg) = event
                            && tx.send(msg).is_err()
                        {
                            break;
                        }
                    }
                });
            }
            Err(e) => {
                self.editor.status_message = Some(format!("LSP: failed to start: {e}"));
            }
        }
    }

    fn process_lsp_messages(&mut self) {
        while let Ok(msg) = self.lsp_rx.try_recv() {
            self.handle_lsp_message(msg);
        }
    }

    fn handle_lsp_message(&mut self, msg: LspMessage) {
        match msg {
            LspMessage::Response { id, result, error } => {
                // Check if this is the initialize response
                let is_init = self
                    .lsp_client
                    .as_ref()
                    .is_some_and(|lsp| id == lsp.initialize_id && !lsp.initialized);

                if is_init {
                    if error.is_some() {
                        self.editor.status_message = Some("LSP: initialize failed".to_string());
                        return;
                    }
                    // Extract data before borrowing lsp_client mutably
                    let lsp_language = self.lsp_client.as_ref().map(|lsp| lsp.language);
                    let path_data = self.editor.document.path.as_ref().and_then(|path| {
                        if language::spec_for_path(path).map(|spec| spec.kind) == lsp_language {
                            let uri = lsp::path_to_uri(path);
                            let text = self.editor.document.rope.to_string();
                            let version = self.editor.document.version;
                            Some((uri, text, version))
                        } else {
                            None
                        }
                    });

                    if let Some(lsp) = &mut self.lsp_client {
                        let _ = self.runtime.block_on(lsp.send_initialized());
                        if let Some((ref uri, ref text, version)) = path_data {
                            let _ = self.runtime.block_on(lsp.did_open(uri, text, version));
                        }
                    }
                    if let Some((uri, _, version)) = path_data {
                        self.file_uri = Some(uri);
                        self.last_notified_version = version;
                    }
                    self.editor.status_message = Some("LSP: ready".to_string());
                    return;
                }

                // Completion
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

                // Goto definition
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
                                self.open_file(&path);
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

                // Hover
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

                // References
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

                // Format
                if Some(id) == self.editor.pending_format_id {
                    self.editor.pending_format_id = None;
                    if let Some(result) = result {
                        self.apply_format_edits(&result);
                    }
                    return;
                }

                // Code actions
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

                // Workspace symbols
                if Some(id) == self.editor.pending_workspace_symbol_id {
                    self.editor.pending_workspace_symbol_id = None;
                    if let Some(result) = result {
                        let symbols = lsp::parse_workspace_symbols(&result);
                        self.editor.workspace_symbol_results = symbols;
                        self.editor.workspace_symbol_index = 0;
                    }
                    return;
                }

                // Rename
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
                    let _ = self.runtime.block_on(lsp.respond(&id, response));
                }
            }
        }
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
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);
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
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);
        let count = edits.len();
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

    fn handle_key(&mut self, key: KeyInput, ctx: &egui::Context) {
        // Record for macros
        if self.editor.recording_macro.is_some() {
            let is_stop = matches!(key.code, KeyCode::Char('q'))
                && !key.ctrl
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

            if !matches!(
                cmd,
                Command::TriggerCompletion
                    | Command::AcceptCompletion
                    | Command::CancelCompletion
                    | Command::CompletionNext
                    | Command::CompletionPrev
            ) && self.editor.showing_completion
            {
                self.editor.cancel_completion();
            }

            let deferred = input::execute_invocation(&mut self.editor, invocation);

            // Handle async LSP commands
            if trigger_completion {
                self.request_completion();
            }
            if trigger_goto {
                self.request_goto_definition();
            }
            if trigger_hover {
                self.request_hover();
            }
            if trigger_refs {
                self.request_references();
            }
            if trigger_ref_jump {
                self.jump_to_reference();
            }
            if trigger_file_finder {
                let entries = self.scan_project_files();
                self.editor.open_file_finder(entries);
            }
            if trigger_code_action {
                self.request_code_action();
            }
            if trigger_code_action_accept {
                self.accept_code_action();
            }
            if trigger_ws_symbol {
                self.editor.open_workspace_symbols();
            }
            if trigger_ws_confirm {
                self.jump_to_workspace_symbol();
            }
            if self.editor.workspace_symbol_needs_request {
                self.editor.workspace_symbol_needs_request = false;
                self.request_workspace_symbols();
            }

            // Handle deferred actions
            if let Some(action) = deferred {
                self.handle_deferred(action);
            }
        }

        // Send LSP didChange
        self.notify_lsp_change();

        ctx.request_repaint();
    }

    fn handle_deferred(&mut self, action: DeferredAction) {
        match action {
            DeferredAction::Rename(new_name) => {
                self.request_rename(&new_name);
            }
            DeferredAction::DidSave => {
                self.format_on_save_if_configured();
                self.refresh_sidebar_git_status();
                self.notify_lsp_did_save();
            }
            DeferredAction::OpenFile(path) => {
                self.open_file(&path);
            }
            DeferredAction::SyncFileUri => {
                self.sync_file_uri();
            }
            DeferredAction::ShellCommand(cmd) => {
                // Run shell command and capture output
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr_out = String::from_utf8_lossy(&output.stderr);
                        let msg = if !stdout.is_empty() {
                            stdout.lines().next().unwrap_or("").to_string()
                        } else if !stderr_out.is_empty() {
                            stderr_out.lines().next().unwrap_or("").to_string()
                        } else {
                            format!("Exit: {}", output.status.code().unwrap_or(-1))
                        };
                        self.editor.status_message = Some(msg);
                    }
                    Err(e) => {
                        self.editor.status_message = Some(format!("Error: {e}"));
                    }
                }
            }
            DeferredAction::FormatDocument => {
                self.request_formatting();
            }
            DeferredAction::PlayMacro(ch) => {
                self.play_macro(ch);
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

        let mut refresh_sidebar = false;
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
                refresh_sidebar = true;
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::stage_files(&anchor, &paths))
            }
            GitEditorCommand::UnstageFiles(paths) => {
                refresh_sidebar = true;
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::unstage_files(&anchor, &paths))
            }
            GitEditorCommand::StageHunk { path, hunk_id } => {
                refresh_sidebar = true;
                git::stage_hunk(&anchor, &path, &hunk_id)
            }
            GitEditorCommand::UnstageHunk { path, hunk_id } => {
                refresh_sidebar = true;
                git::unstage_hunk(&anchor, &path, &hunk_id)
            }
            GitEditorCommand::DeleteFiles(paths) => {
                refresh_sidebar = true;
                let paths = self.git_paths_or_current(paths);
                paths.and_then(|paths| git::delete_files(&anchor, &paths))
            }
            GitEditorCommand::CreateBranch(name) => git::create_branch(&anchor, &name),
            GitEditorCommand::CheckoutBranch(name) => {
                refresh_sidebar = true;
                git::checkout_branch(&anchor, &name)
            }
            GitEditorCommand::Amend { message } => git::amend(&anchor, message.as_deref()),
            GitEditorCommand::StashPush { message } => {
                refresh_sidebar = true;
                git::stash_push(&anchor, message.as_deref())
            }
            GitEditorCommand::StashApply { index } => {
                refresh_sidebar = true;
                git::stash_apply(&anchor, index)
            }
            GitEditorCommand::StashPop { index } => {
                refresh_sidebar = true;
                git::stash_pop(&anchor, index)
            }
            GitEditorCommand::Merge { branch } => {
                refresh_sidebar = true;
                git::merge_branch(&anchor, &branch)
            }
            GitEditorCommand::Rebase { upstream } => {
                refresh_sidebar = true;
                git::rebase_onto(&anchor, &upstream)
            }
        };

        self.editor.status_message = Some(match result {
            Ok(report) => git_report_message(report),
            Err(e) => format!("Git: {e}"),
        });
        if refresh_sidebar {
            self.refresh_sidebar_git_status();
        }
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

    fn handle_mouse(&mut self, ctx: &egui::Context) {
        let font_size = self.editor.config.font_size;
        let char_width = font_size * 0.6;
        let line_height = font_size * 1.4;

        // --- Scroll wheel ---
        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta);
        if scroll_delta.y.abs() > 0.1 {
            let lines = (scroll_delta.y.abs() / line_height).ceil() as usize;
            let lines = lines.max(3);
            if scroll_delta.y > 0.0 {
                self.editor.scroll_viewport_up(lines);
            } else {
                self.editor.scroll_viewport_down(lines);
            }
            ctx.request_repaint();
        }

        let (
            primary_pressed,
            primary_down,
            primary_released,
            primary_double_clicked,
            primary_triple_clicked,
            shift_down,
            pointer_pos,
        ) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.pointer
                    .button_triple_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
                i.pointer.interact_pos(),
            )
        });

        if !(primary_pressed
            || primary_down
            || primary_released
            || primary_double_clicked
            || primary_triple_clicked)
        {
            return;
        }
        let pointer_pos = match pointer_pos {
            Some(pos) => pos,
            None => {
                if primary_released {
                    self.mouse_selection_anchor = None;
                }
                return;
            }
        };

        // Ignore editor placement while popups are showing.
        if self.editor.showing_completion
            || self.editor.showing_hover
            || self.editor.showing_references
            || self.editor.showing_code_actions
            || self.editor.showing_diagnostics
            || self.editor.showing_file_finder
            || self.editor.showing_workspace_symbols
            || self.editor.showing_git_diff
        {
            if primary_released {
                self.mouse_selection_anchor = None;
            }
            return;
        }

        // Find which pane contains the pointer.
        let clicked_pane = self
            .last_pane_rects
            .iter()
            .find(|(_, rect)| rect.contains(pointer_pos));
        let (pane_id, pane_rect) = match clicked_pane {
            Some(&(id, rect)) => (id, rect),
            None => {
                if primary_released {
                    self.mouse_selection_anchor = None;
                }
                return;
            }
        };

        // Switch pane focus if needed
        if pane_id != self.editor.active_pane_id {
            self.editor.save_active_pane();
            self.editor.load_pane(pane_id);
            self.sync_file_uri();
            self.mouse_selection_anchor = None;
        }

        // Determine the editor area (pane minus status line)
        let gutter_width = self.editor.gutter_width();
        let gutter_px = gutter_width as f32 * char_width;
        let editor_rows = (self.editor.view.height) as f32 * line_height;
        let editor_area_bottom = pane_rect.min.y + editor_rows;

        // Pointer on status line -> just focus the pane.
        if pointer_pos.y >= editor_area_bottom {
            if primary_released {
                self.mouse_selection_anchor = None;
            }
            ctx.request_repaint();
            return;
        }

        // Pointer in gutter -> ignore placement.
        if pointer_pos.x < pane_rect.min.x + gutter_px {
            if primary_released {
                self.mouse_selection_anchor = None;
            }
            ctx.request_repaint();
            return;
        }

        // Convert pixel position to screen coordinates
        let screen_col_f = (pointer_pos.x - pane_rect.min.x - gutter_px) / char_width;
        let screen_row_f = (pointer_pos.y - pane_rect.min.y) / line_height;
        let screen_col = screen_col_f.max(0.0) as usize;
        let screen_row = screen_row_f.max(0.0) as usize;

        let position = self.editor.position_for_view_cell(screen_row, screen_col);

        if primary_triple_clicked {
            self.editor.select_line_at(position.row);
            self.mouse_selection_anchor = None;
            ctx.request_repaint();
            return;
        }
        if primary_double_clicked {
            if self
                .editor
                .select_word_at(position.row, position.col)
                .is_none()
            {
                self.editor.set_cursor_position(position.row, position.col);
            }
            self.mouse_selection_anchor = None;
            ctx.request_repaint();
            return;
        }

        if primary_pressed {
            if shift_down {
                self.editor.extend_selection_to(position.row, position.col);
                self.mouse_selection_anchor = self.editor.visual_anchor;
            } else {
                let cursor = self.editor.set_cursor_for_view_cell(screen_row, screen_col);
                self.mouse_selection_anchor = Some(cursor);
            }
            ctx.request_repaint();
            return;
        }

        if primary_down && let Some(anchor) = self.mouse_selection_anchor {
            if position != anchor {
                self.editor
                    .set_selection(anchor, position, SelectionMode::Character);
            }
            ctx.request_repaint();
            return;
        }

        if primary_released {
            self.mouse_selection_anchor = None;
            ctx.request_repaint();
        }
    }

    fn play_macro(&mut self, ch: char) {
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
                        DeferredAction::PlayMacro(_) => {}
                        other => self.handle_deferred(other),
                    }
                }
            }
            self.notify_lsp_change();
            if self.editor.should_quit {
                break;
            }
        }
    }

    // LSP request helpers (blocking via runtime)

    fn notify_lsp_change(&mut self) {
        let version = self.editor.document.version;
        if version == self.last_notified_version {
            return;
        }
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let text = self.editor.document.rope.to_string();
            let _ = self.runtime.block_on(lsp.did_change(uri, &text, version));
            self.last_notified_version = version;
        }
    }

    fn notify_lsp_did_save(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let _ = self.runtime.block_on(lsp.did_save(uri));
        }
    }

    fn request_completion(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = self.runtime.block_on(lsp.completion(uri, line, character)) {
                self.editor.pending_completion_id = Some(id);
            }
        }
    }

    fn request_goto_definition(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = self
                .runtime
                .block_on(lsp.goto_definition(uri, line, character))
            {
                self.editor.pending_goto_id = Some(id);
            }
        }
    }

    fn request_hover(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = self.runtime.block_on(lsp.hover(uri, line, character)) {
                self.editor.pending_hover_id = Some(id);
            }
        }
    }

    fn request_references(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = self.runtime.block_on(lsp.references(uri, line, character)) {
                self.editor.pending_references_id = Some(id);
            }
        }
    }

    fn request_rename(&mut self, new_name: &str) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            if let Ok(id) = self
                .runtime
                .block_on(lsp.rename(uri, line, character, new_name))
            {
                self.editor.pending_rename_id = Some(id);
            }
        }
    }

    fn request_workspace_symbols(&mut self) {
        if let Some(lsp) = &mut self.lsp_client {
            if !lsp.initialized {
                return;
            }
            let query = self.editor.workspace_symbol_query.clone();
            if let Ok(id) = self.runtime.block_on(lsp.workspace_symbol(&query)) {
                self.editor.pending_workspace_symbol_id = Some(id);
            }
        }
    }

    fn jump_to_workspace_symbol(&mut self) {
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
                self.open_file(&path);
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

    fn request_code_action(&mut self) {
        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let line = self.editor.cursor.row as u32;
            let character = self.editor.cursor.col as u32;
            let diagnostics = self.editor.diagnostics.clone();
            if let Ok(id) =
                self.runtime
                    .block_on(lsp.code_action(uri, line, character, &diagnostics))
            {
                self.editor.pending_code_action_id = Some(id);
            }
        }
    }

    fn accept_code_action(&mut self) {
        if self.editor.code_actions.is_empty() {
            self.editor.dismiss_code_actions();
            return;
        }
        let action = self.editor.code_actions[self.editor.code_action_index].clone();
        self.editor.dismiss_code_actions();
        if let Some(ref edit) = action.edit {
            self.apply_workspace_edit(edit);
        }
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
            let _ = self.runtime.block_on(lsp.send_request(
                "workspace/executeCommand",
                serde_json::json!({
                    "command": cmd_str,
                    "arguments": arguments,
                }),
            ));
        }
        self.editor.status_message = Some(format!("Applied: {}", action.title));
        self.notify_lsp_change();
    }

    fn apply_workspace_edit(&mut self, edit: &serde_json::Value) {
        let file_uri = match &self.file_uri {
            Some(uri) => uri.clone(),
            None => return,
        };
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
        self.editor
            .history
            .save(&self.editor.document.rope, self.editor.cursor);
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

    fn request_formatting(&mut self) {
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
                    self.notify_lsp_change();
                }
                Err(e) => {
                    self.editor.status_message = Some(format!("Format failed: {e}"));
                }
            }
            return;
        }

        if let (Some(lsp), Some(uri)) = (&mut self.lsp_client, &self.file_uri) {
            if !lsp.initialized {
                return;
            }
            let params = serde_json::json!({
                "textDocument": { "uri": uri },
                "options": { "tabSize": 4, "insertSpaces": true }
            });
            match self
                .runtime
                .block_on(lsp.send_request("textDocument/formatting", params))
            {
                Ok(id) => {
                    self.editor.pending_format_id = Some(id);
                }
                Err(_) => {
                    self.editor.status_message = Some("Format request failed".to_string());
                }
            }
        }
    }

    fn format_on_save_if_configured(&mut self) {
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
                        self.notify_lsp_change();
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

    fn jump_to_reference(&mut self) {
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
            self.open_file(&path);
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

    fn sync_file_uri(&mut self) {
        if let (Some(lsp), Some(old_uri)) = (&mut self.lsp_client, &self.file_uri)
            && lsp.initialized
        {
            let _ = self.runtime.block_on(lsp.send_notification(
                "textDocument/didClose",
                serde_json::json!({"textDocument": {"uri": old_uri}}),
            ));
        }
        // Reset version tracking for the new file
        self.last_notified_version = self.editor.document.version;
        if let Some(path) = self.editor.document.path.clone() {
            self.note_recent_file(&path);
            if !lsp::supports_lsp(&path) {
                self.file_uri = None;
                return;
            }
            self.start_lsp(&path);
            if let Some(lsp) = &mut self.lsp_client
                && lsp.initialized
                && language::spec_for_path(&path).is_some_and(|spec| spec.kind == lsp.language)
            {
                let uri = lsp::path_to_uri(&path);
                let text = self.editor.document.rope.to_string();
                let version = self.editor.document.version;
                let _ = self.runtime.block_on(lsp.did_open(&uri, &text, version));
                self.file_uri = Some(uri);
                self.last_notified_version = version;
            }
        } else {
            self.file_uri = None;
        }
    }

    fn open_file(&mut self, rel_path: &str) {
        if self.editor.showing_file_finder {
            self.editor.file_finder_cancel();
        }
        let full_path = self.resolve_open_path(rel_path);
        if let Some(idx) = self.editor.find_buffer_by_path(&full_path) {
            if idx != self.editor.current_buffer {
                self.editor.switch_buffer(idx);
                self.sync_file_uri();
            }
            return;
        }
        match Document::open(&full_path.to_string_lossy()) {
            Ok(doc) => {
                self.editor.add_buffer(doc);
                self.editor.status_message =
                    Some(format!("\"{}\"", self.editor.document.file_name()));
                // Reset version tracking for the new file
                self.last_notified_version = self.editor.document.version;
                self.file_uri = None;
                self.sync_file_uri();
            }
            Err(e) => {
                self.editor.status_message = Some(format!("Error opening file: {e}"));
            }
        }
    }

    fn scan_project_files(&self) -> Vec<String> {
        crate::gui::project_sidebar::scan_project_files(&self.file_browser_root())
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
}

impl eframe::App for VikerEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let response = self.show(ctx);
        if response.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;

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
            "viker-gui-project-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        TempProject { root }
    }

    fn empty_app() -> VikerEditor {
        VikerEditor::from_editor(Editor::new(Document::new_empty()))
    }

    #[test]
    fn set_project_root_records_canonical_folder() {
        let project = temp_project("canonical");
        let expected = std::fs::canonicalize(&project.root).unwrap();
        let mut app = empty_app();

        app.set_project_root(project.root.clone());

        assert_eq!(app.project_root(), Some(expected.as_path()));
        assert!(!app.should_show_project_picker());
    }

    #[test]
    fn project_file_scan_uses_selected_folder_and_gitignore() {
        let project = temp_project("scan");
        std::fs::create_dir_all(project.root.join("src")).unwrap();
        std::fs::create_dir_all(project.root.join("node_modules")).unwrap();
        std::fs::create_dir_all(project.root.join(".hidden")).unwrap();
        std::fs::write(project.root.join(".gitignore"), "node_modules/\n.hidden/\n").unwrap();
        std::fs::write(project.root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.root.join("node_modules/pkg.js"), "ignored\n").unwrap();
        std::fs::write(project.root.join(".hidden/file.txt"), "ignored\n").unwrap();
        let mut app = empty_app();
        app.set_project_root(project.root.clone());

        assert_eq!(app.scan_project_files(), vec![".gitignore", "src/main.rs"]);
    }

    #[test]
    fn open_file_resolves_relative_paths_against_project_root() {
        let project = temp_project("open");
        let file = project.root.join("docs/readme.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "# Read\n").unwrap();
        let expected_path = std::fs::canonicalize(&file).unwrap();
        let mut app = empty_app();
        app.set_project_root(project.root.clone());
        app.editor
            .open_file_finder(vec!["docs/readme.md".to_string()]);

        app.open_file("docs/readme.md");

        assert_eq!(app.editor().document.rope.to_string(), "# Read\n");
        assert_eq!(
            app.editor().document.path.as_deref(),
            Some(expected_path.as_path())
        );
        assert!(!app.editor().showing_file_finder);
    }

    #[test]
    fn project_root_drives_lsp_root_for_files_inside_project() {
        let project = temp_project("lsp-inside");
        std::fs::write(project.root.join("package.json"), "{}\n").unwrap();
        let file = project.root.join("src/app.ts");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "const x = 1;\n").unwrap();
        let expected_root = std::fs::canonicalize(&project.root).unwrap();
        let mut app = empty_app();
        app.set_project_root(project.root.clone());

        assert_eq!(app.root_for_file(&file), expected_root);
    }

    #[test]
    fn files_outside_project_keep_file_specific_lsp_roots() {
        let project = temp_project("lsp-project");
        let outside = temp_project("lsp-outside");
        std::fs::write(outside.root.join("package.json"), "{}\n").unwrap();
        let file = outside.root.join("app.ts");
        std::fs::write(&file, "const x = 1;\n").unwrap();
        let expected_root = std::fs::canonicalize(&outside.root).unwrap();
        let mut app = empty_app();
        app.set_project_root(project.root.clone());

        assert_eq!(app.root_for_file(&file), expected_root);
    }
}

fn should_suppress_text_after_modified_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    modifiers.alt && matches!(key, egui::Key::F | egui::Key::B)
}

/// Convert egui Key + Modifiers to our KeyInput.
/// Returns None for keys that will be handled via Text events (character input).
fn egui_key_to_key_input(key: egui::Key, modifiers: egui::Modifiers) -> Option<KeyInput> {
    let ctrl = modifiers.ctrl || modifiers.mac_cmd;
    let alt = modifiers.alt;

    // Special keys that are always handled
    let code = match key {
        egui::Key::Escape => Some(KeyCode::Esc),
        egui::Key::Enter => Some(KeyCode::Enter),
        egui::Key::Backspace => Some(KeyCode::Backspace),
        egui::Key::Tab => {
            if modifiers.shift {
                Some(KeyCode::BackTab)
            } else {
                Some(KeyCode::Tab)
            }
        }
        egui::Key::ArrowUp => Some(KeyCode::Up),
        egui::Key::ArrowDown => Some(KeyCode::Down),
        egui::Key::ArrowLeft => Some(KeyCode::Left),
        egui::Key::ArrowRight => Some(KeyCode::Right),
        // Ctrl/Option key combinations
        _ if ctrl || alt => {
            // Map letter keys with modifiers. Plain text input still comes from
            // Event::Text, but modified keys need to reach the editor keymap.
            match key {
                egui::Key::A => Some(KeyCode::Char('a')),
                egui::Key::B => Some(KeyCode::Char('b')),
                egui::Key::C => Some(KeyCode::Char('c')),
                egui::Key::D => Some(KeyCode::Char('d')),
                egui::Key::E => Some(KeyCode::Char('e')),
                egui::Key::F => Some(KeyCode::Char('f')),
                egui::Key::G => Some(KeyCode::Char('g')),
                egui::Key::H => Some(KeyCode::Char('h')),
                egui::Key::I => Some(KeyCode::Char('i')),
                egui::Key::J => Some(KeyCode::Char('j')),
                egui::Key::K => Some(KeyCode::Char('k')),
                egui::Key::L => Some(KeyCode::Char('l')),
                egui::Key::M => Some(KeyCode::Char('m')),
                egui::Key::N => Some(KeyCode::Char('n')),
                egui::Key::O => Some(KeyCode::Char('o')),
                egui::Key::P => Some(KeyCode::Char('p')),
                egui::Key::Q => Some(KeyCode::Char('q')),
                egui::Key::R => Some(KeyCode::Char('r')),
                egui::Key::S => Some(KeyCode::Char('s')),
                egui::Key::T => Some(KeyCode::Char('t')),
                egui::Key::U => Some(KeyCode::Char('u')),
                egui::Key::V => Some(KeyCode::Char('v')),
                egui::Key::W => Some(KeyCode::Char('w')),
                egui::Key::X => Some(KeyCode::Char('x')),
                egui::Key::Y => Some(KeyCode::Char('y')),
                egui::Key::Z => Some(KeyCode::Char('z')),
                egui::Key::Space => Some(KeyCode::Char(' ')),
                _ => None,
            }
        }
        // Regular character keys will be handled by Text events
        _ => None,
    };

    code.map(|c| KeyInput { code: c, ctrl, alt })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_letter_keys_are_forwarded_to_editor_keymap() {
        let ctrl = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };

        assert_eq!(
            egui_key_to_key_input(egui::Key::E, ctrl),
            Some(KeyInput {
                code: KeyCode::Char('e'),
                ctrl: true,
                alt: false,
            })
        );
        assert_eq!(
            egui_key_to_key_input(egui::Key::N, ctrl),
            Some(KeyInput {
                code: KeyCode::Char('n'),
                ctrl: true,
                alt: false,
            })
        );
        assert_eq!(
            egui_key_to_key_input(egui::Key::V, ctrl),
            Some(KeyInput {
                code: KeyCode::Char('v'),
                ctrl: true,
                alt: false,
            })
        );
    }

    #[test]
    fn option_word_bindings_suppress_generated_text_event() {
        let option = egui::Modifiers {
            alt: true,
            ..Default::default()
        };

        assert!(should_suppress_text_after_modified_key(
            egui::Key::F,
            option
        ));
        assert!(should_suppress_text_after_modified_key(
            egui::Key::B,
            option
        ));
        assert!(!should_suppress_text_after_modified_key(
            egui::Key::A,
            option
        ));
    }
}
