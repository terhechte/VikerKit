mod transport;

use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::language::{self, LanguageKind, ToolInvocation};

// ── Message types ──────────────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
pub enum LspMessage {
    Response {
        id: i64,
        result: Option<Value>,
        error: Option<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

impl LspMessage {
    fn from_value(msg: Value) -> Option<Self> {
        let obj = msg.as_object()?;
        if let Some(id_val) = obj.get("id") {
            if let Some(method) = obj.get("method") {
                // Server → Client request
                Some(LspMessage::ServerRequest {
                    id: id_val.clone(),
                    method: method.as_str()?.to_string(),
                    params: obj.get("params").cloned().unwrap_or(Value::Null),
                })
            } else {
                // Response to our request
                Some(LspMessage::Response {
                    id: id_val.as_i64()?,
                    result: obj.get("result").cloned(),
                    error: obj.get("error").cloned(),
                })
            }
        } else if let Some(method) = obj.get("method") {
            Some(LspMessage::Notification {
                method: method.as_str()?.to_string(),
                params: obj.get("params").cloned().unwrap_or(Value::Null),
            })
        } else {
            None
        }
    }
}

// ── Diagnostic / Completion types (parsed from JSON) ───────────────

#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub severity: u8, // 1=Error 2=Warning 3=Info 4=Hint
    pub message: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LspCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub kind: u32, // CompletionItemKind numeric value
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LspLocation {
    pub uri: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LspCodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<Value>,
    pub command: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct LspSymbolInfo {
    pub name: String,
    pub kind: u32,
    pub uri: String,
    pub start_line: u32,
    pub start_col: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LspWorkspaceEdit {
    pub uri: String,
    pub edits: Vec<LspTextEdit>,
}

#[derive(Debug, Clone)]
pub struct LspTextEdit {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub new_text: String,
}

// ── AppEvent ───────────────────────────────────────────────────────

/// Unified event type for the main loop.
#[allow(dead_code)]
pub enum AppEvent {
    Key(crate::key::KeyInput),
    Mouse(crate::key::MouseInput),
    Resize(u16, u16),
    Lsp(LspMessage),
}

// ── LspClient ──────────────────────────────────────────────────────

pub struct LspClient {
    stdin: tokio::io::BufWriter<tokio::process::ChildStdin>,
    next_id: i64,
    pub initialize_id: i64,
    pub initialized: bool,
    pub language: LanguageKind,
    pub language_id: &'static str,
    _child: Child,
}

pub fn supports_lsp(path: &Path) -> bool {
    language::supports_lsp(path)
}

impl LspClient {
    /// Start a language server and begin the initialize handshake.
    /// `event_tx` is used by the background reader to push LSP messages.
    pub async fn start(
        root_path: &Path,
        language: LanguageKind,
        invocation: ToolInvocation,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Self> {
        let mut child = Command::new(&invocation.command)
            .args(&invocation.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        // Spawn background reader task
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            while let Ok(raw) = transport::read_message(&mut reader).await {
                if let Some(msg) = LspMessage::from_value(raw)
                    && event_tx.send(AppEvent::Lsp(msg)).is_err()
                {
                    break;
                }
            }
        });

        let mut client = Self {
            stdin: tokio::io::BufWriter::new(stdin),
            next_id: 1,
            initialize_id: 0,
            initialized: false,
            language,
            language_id: language.spec().lsp_language_id,
            _child: child,
        };

        // Send initialize request
        let root_uri = path_to_uri(&root_path.canonicalize().unwrap_or(root_path.to_path_buf()));
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": {
                "name": "Viker",
                "version": env!("CARGO_PKG_VERSION")
            },
            "rootPath": root_path.to_string_lossy(),
            "rootUri": root_uri,
            "capabilities": {
                "workspace": {
                    "applyEdit": true,
                    "configuration": true,
                    "workspaceFolders": true
                },
                "textDocument": {
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false
                        }
                    },
                    "publishDiagnostics": {
                        "relatedInformation": true
                    },
                    "synchronization": {
                        "didSave": true,
                        "dynamicRegistration": false
                    },
                    "definition": {
                        "dynamicRegistration": false
                    },
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["plaintext"]
                    },
                    "references": {
                        "dynamicRegistration": false
                    },
                    "rename": {
                        "dynamicRegistration": false,
                        "prepareSupport": false
                    },
                    "codeAction": {
                        "dynamicRegistration": false,
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": ["quickfix", "refactor", "source"]
                            }
                        }
                    },
                    "formatting": {
                        "dynamicRegistration": false
                    }
                }
            }
        });
        client.initialize_id = client.send_request("initialize", params).await?;

        Ok(client)
    }

    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<i64> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        transport::write_message(&mut self.stdin, &msg).await?;
        Ok(id)
    }

    pub async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        transport::write_message(&mut self.stdin, &msg).await?;
        Ok(())
    }

    /// Send `initialized` notification (after receiving initialize response).
    pub async fn send_initialized(&mut self) -> Result<()> {
        self.initialized = true;
        self.send_notification("initialized", json!({})).await
    }

    pub async fn did_open(&mut self, uri: &str, text: &str, version: i64) -> Result<()> {
        self.did_open_with_language_id(uri, self.language_id, text, version)
            .await
    }

    pub async fn did_open_with_language_id(
        &mut self,
        uri: &str,
        language_id: &str,
        text: &str,
        version: i64,
    ) -> Result<()> {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": text,
                }
            }),
        )
        .await
    }

    pub async fn did_change(&mut self, uri: &str, text: &str, version: i64) -> Result<()> {
        self.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        )
        .await
    }

    pub async fn completion(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.send_request(
            "textDocument/completion",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    /// Respond to a server request (e.g., window/workDoneProgress/create).
    pub async fn respond(&mut self, id: &Value, result: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        transport::write_message(&mut self.stdin, &msg).await
    }

    pub async fn goto_definition(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.send_request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    pub async fn hover(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.send_request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    pub async fn references(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.send_request(
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "context": { "includeDeclaration": true }
            }),
        )
        .await
    }

    pub async fn rename(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<i64> {
        self.send_request(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "newName": new_name
            }),
        )
        .await
    }

    pub async fn code_action(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        diagnostics: &[LspDiagnostic],
    ) -> Result<i64> {
        // Build diagnostic context: include diagnostics that cover this position
        let diag_array: Vec<Value> = diagnostics
            .iter()
            .filter(|d| d.start_line <= line && d.end_line >= line)
            .map(|d| {
                json!({
                    "range": {
                        "start": { "line": d.start_line, "character": d.start_col },
                        "end": { "line": d.end_line, "character": d.end_col }
                    },
                    "severity": d.severity,
                    "message": d.message
                })
            })
            .collect();

        self.send_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": line, "character": character },
                    "end": { "line": line, "character": character }
                },
                "context": {
                    "diagnostics": diag_array
                }
            }),
        )
        .await
    }

    pub async fn workspace_symbol(&mut self, query: &str) -> Result<i64> {
        self.send_request(
            "workspace/symbol",
            json!({
                "query": query
            }),
        )
        .await
    }

    pub async fn did_save(&mut self, uri: &str) -> Result<()> {
        self.send_notification(
            "textDocument/didSave",
            json!({
                "textDocument": { "uri": uri }
            }),
        )
        .await
    }

    pub async fn did_close(&mut self, uri: &str) -> Result<()> {
        self.send_notification(
            "textDocument/didClose",
            json!({
                "textDocument": { "uri": uri }
            }),
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.send_request("shutdown", Value::Null).await;
        let _ = self.send_notification("exit", Value::Null).await;
        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────

pub fn path_to_uri(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or(path.to_path_buf());
    let path_str = abs.to_string_lossy();
    let mut encoded = String::with_capacity(path_str.len() + 16);
    encoded.push_str("file://");
    for byte in path_str.as_bytes() {
        match *byte {
            // Unreserved characters (RFC 3986) + '/' and ':'
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(*byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

pub fn uri_to_path(uri: &str) -> Option<String> {
    let encoded = uri.strip_prefix("file://")?;
    let mut decoded = Vec::with_capacity(encoded.len());
    let bytes = encoded.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            decoded.push(val);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).ok()
}

pub fn find_project_root(file_path: &Path) -> std::path::PathBuf {
    language::find_project_root(file_path)
}

pub fn diagnostics_uri(params: &Value) -> Option<String> {
    params
        .get("uri")
        .and_then(|uri| uri.as_str())
        .map(String::from)
}

pub fn workspace_configuration_response(
    params: &Value,
    tab_size: usize,
    insert_spaces: bool,
) -> Value {
    let Some(items) = params.get("items").and_then(|items| items.as_array()) else {
        return Value::Null;
    };
    Value::Array(
        items
            .iter()
            .map(
                |item| match item.get("section").and_then(|section| section.as_str()) {
                    Some("formattingOptions") => json!({
                        "tabSize": tab_size,
                        "insertSpaces": insert_spaces,
                    }),
                    _ => json!({}),
                },
            )
            .collect(),
    )
}

/// Parse `textDocument/publishDiagnostics` params into our diagnostic type.
pub fn parse_diagnostics(params: &Value) -> Vec<LspDiagnostic> {
    let mut out = Vec::new();
    if let Some(diags) = params.get("diagnostics").and_then(|d| d.as_array()) {
        for d in diags {
            let range = &d["range"];
            let start = &range["start"];
            let end = &range["end"];
            let severity = d["severity"].as_u64().unwrap_or(1) as u8;
            let message = d["message"].as_str().unwrap_or("").to_string();
            out.push(LspDiagnostic {
                start_line: start["line"].as_u64().unwrap_or(0) as u32,
                start_col: start["character"].as_u64().unwrap_or(0) as u32,
                end_line: end["line"].as_u64().unwrap_or(0) as u32,
                end_col: end["character"].as_u64().unwrap_or(0) as u32,
                severity,
                message,
            });
        }
    }
    out
}

/// Parse a goto definition response into locations.
/// Response can be Location | Location[] | LocationLink[] | null.
pub fn parse_goto_definition(result: &Value) -> Vec<LspLocation> {
    let mut out = Vec::new();

    fn parse_location(val: &Value) -> Option<LspLocation> {
        let uri = val.get("uri")?.as_str()?.to_string();
        let range = val.get("range").or_else(|| val.get("targetRange"))?;
        let start = &range["start"];
        let end = &range["end"];
        Some(LspLocation {
            uri,
            start_line: start["line"].as_u64()? as u32,
            start_col: start["character"].as_u64()? as u32,
            end_line: end["line"].as_u64()? as u32,
            end_col: end["character"].as_u64()? as u32,
        })
    }

    if let Some(arr) = result.as_array() {
        for item in arr {
            // Could be Location or LocationLink
            let uri_field = if item.get("uri").is_some() {
                item
            } else if let Some(target_uri) = item.get("targetUri") {
                // LocationLink: use targetUri + targetRange
                let mut loc = serde_json::Map::new();
                loc.insert("uri".to_string(), target_uri.clone());
                if let Some(range) = item.get("targetRange") {
                    loc.insert("range".to_string(), range.clone());
                }
                let val = Value::Object(loc);
                if let Some(l) = parse_location(&val) {
                    out.push(l);
                }
                continue;
            } else {
                continue;
            };
            if let Some(l) = parse_location(uri_field) {
                out.push(l);
            }
        }
    } else if result.is_object()
        && let Some(l) = parse_location(result)
    {
        out.push(l);
    }

    out
}

/// Parse a hover response into displayable text.
pub fn parse_hover(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;

    if let Some(s) = contents.as_str() {
        return Some(s.to_string());
    }

    // MarkedString object: { language, value }
    if let Some(value) = contents.get("value").and_then(|v| v.as_str()) {
        return Some(value.to_string());
    }

    // MarkupContent: { kind, value }
    if let Some(value) = contents.get("value").and_then(|v| v.as_str()) {
        return Some(value.to_string());
    }

    // Array of MarkedString
    if let Some(arr) = contents.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                if let Some(s) = item.as_str() {
                    Some(s.to_string())
                } else {
                    item.get("value").and_then(|v| v.as_str()).map(String::from)
                }
            })
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("\n\n"));
        }
    }

    None
}

/// Parse a references response into locations.
pub fn parse_references(result: &Value) -> Vec<LspLocation> {
    let mut out = Vec::new();
    if let Some(arr) = result.as_array() {
        for item in arr {
            if let (Some(uri), Some(range)) =
                (item.get("uri").and_then(|u| u.as_str()), item.get("range"))
            {
                let start = &range["start"];
                let end = &range["end"];
                out.push(LspLocation {
                    uri: uri.to_string(),
                    start_line: start["line"].as_u64().unwrap_or(0) as u32,
                    start_col: start["character"].as_u64().unwrap_or(0) as u32,
                    end_line: end["line"].as_u64().unwrap_or(0) as u32,
                    end_col: end["character"].as_u64().unwrap_or(0) as u32,
                });
            }
        }
    }
    out
}

/// Parse a rename response (WorkspaceEdit) for the given file URI.
pub fn parse_rename_edits(result: &Value, file_uri: &str) -> Vec<LspTextEdit> {
    let mut edits = Vec::new();

    // Try "changes" field first
    if let Some(changes) = result.get("changes").and_then(|c| c.as_object())
        && let Some(file_edits) = changes.get(file_uri).and_then(|e| e.as_array())
    {
        for edit in file_edits {
            if let Some(te) = parse_text_edit(edit) {
                edits.push(te);
            }
        }
    }

    // Try "documentChanges" field
    if edits.is_empty()
        && let Some(doc_changes) = result.get("documentChanges").and_then(|c| c.as_array())
    {
        for dc in doc_changes {
            let uri = dc
                .get("textDocument")
                .and_then(|td| td.get("uri"))
                .and_then(|u| u.as_str());
            if uri == Some(file_uri)
                && let Some(edit_arr) = dc.get("edits").and_then(|e| e.as_array())
            {
                for edit in edit_arr {
                    if let Some(te) = parse_text_edit(edit) {
                        edits.push(te);
                    }
                }
            }
        }
    }

    // Sort edits in reverse order (bottom-to-top, right-to-left) for safe application
    edits.sort_by(|a, b| {
        b.start_line
            .cmp(&a.start_line)
            .then(b.start_col.cmp(&a.start_col))
    });

    edits
}

pub fn parse_text_edit(edit: &Value) -> Option<LspTextEdit> {
    let range = edit.get("range")?;
    let start = &range["start"];
    let end = &range["end"];
    let new_text = edit.get("newText")?.as_str()?.to_string();
    Some(LspTextEdit {
        start_line: start["line"].as_u64()? as u32,
        start_col: start["character"].as_u64()? as u32,
        end_line: end["line"].as_u64()? as u32,
        end_col: end["character"].as_u64()? as u32,
        new_text,
    })
}

/// Parse a code action response into our code action type.
pub fn parse_code_actions(result: &Value) -> Vec<LspCodeAction> {
    let items = match result.as_array() {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    items
        .iter()
        .filter_map(|item| {
            // Each item can be a Command or a CodeAction
            let title = item.get("title")?.as_str()?.to_string();
            let kind = item.get("kind").and_then(|k| k.as_str()).map(String::from);
            let edit = item.get("edit").cloned();
            let command = item.get("command").cloned();
            Some(LspCodeAction {
                title,
                kind,
                edit,
                command,
            })
        })
        .collect()
}

/// Parse a workspace/symbol response into our symbol info type.
pub fn parse_workspace_symbols(result: &Value) -> Vec<LspSymbolInfo> {
    let mut out = Vec::new();
    if let Some(arr) = result.as_array() {
        for item in arr {
            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let kind = item.get("kind").and_then(|k| k.as_u64()).unwrap_or(1) as u32;
            if let Some(loc) = item.get("location") {
                let uri = loc
                    .get("uri")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(range) = loc.get("range") {
                    let start = &range["start"];
                    out.push(LspSymbolInfo {
                        name: name.to_string(),
                        kind,
                        uri,
                        start_line: start["line"].as_u64().unwrap_or(0) as u32,
                        start_col: start["character"].as_u64().unwrap_or(0) as u32,
                    });
                }
            }
        }
    }
    out
}

/// Map LSP SymbolKind to a display label.
pub fn symbol_kind_label(kind: u32) -> &'static str {
    match kind {
        1 => "file",
        2 => "mod",
        3 => "ns",
        4 => "pkg",
        5 => "class",
        6 => "fn", // Method
        7 => "prop",
        8 => "field",
        9 => "ctor",
        10 => "enum",
        11 => "iface",
        12 => "fn",
        13 => "var",
        14 => "const",
        15 => "str",
        16 => "num",
        17 => "bool",
        18 => "arr",
        19 => "obj",
        22 => "enum", // EnumMember
        23 => "struct",
        24 => "event",
        25 => "op",
        26 => "type",
        _ => "sym",
    }
}

/// Parse a completion response into our completion item type.
pub fn parse_completions(result: &Value) -> Vec<LspCompletionItem> {
    let items = if let Some(arr) = result.as_array() {
        arr
    } else if let Some(arr) = result.get("items").and_then(|i| i.as_array()) {
        arr
    } else {
        return Vec::new();
    };

    items
        .iter()
        .map(|item| LspCompletionItem {
            label: item["label"].as_str().unwrap_or("").to_string(),
            detail: item
                .get("detail")
                .and_then(|d| d.as_str())
                .map(String::from),
            insert_text: item
                .get("insertText")
                .and_then(|t| t.as_str())
                .map(String::from),
            kind: item["kind"].as_u64().unwrap_or(0) as u32,
        })
        .collect()
}
