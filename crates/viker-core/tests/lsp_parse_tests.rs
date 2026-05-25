use serde_json::json;
use viker_core::lsp;

// ── Diagnostic parsing ──────────────────────────────────────────────

#[test]
fn parse_diagnostics_basic() {
    let params = json!({
        "diagnostics": [{
            "range": {
                "start": {"line": 5, "character": 10},
                "end": {"line": 5, "character": 15}
            },
            "severity": 1,
            "message": "expected `;`"
        }]
    });
    let diags = lsp::parse_diagnostics(&params);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].start_line, 5);
    assert_eq!(diags[0].start_col, 10);
    assert_eq!(diags[0].end_line, 5);
    assert_eq!(diags[0].end_col, 15);
    assert_eq!(diags[0].severity, 1);
    assert_eq!(diags[0].message, "expected `;`");
}

#[test]
fn parse_diagnostics_multiple() {
    let params = json!({
        "diagnostics": [
            {
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                "severity": 1,
                "message": "error"
            },
            {
                "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}},
                "severity": 2,
                "message": "warning"
            }
        ]
    });
    let diags = lsp::parse_diagnostics(&params);
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].severity, 1);
    assert_eq!(diags[1].severity, 2);
}

#[test]
fn parse_diagnostics_empty() {
    let params = json!({"diagnostics": []});
    let diags = lsp::parse_diagnostics(&params);
    assert!(diags.is_empty());
}

#[test]
fn parse_diagnostics_no_field() {
    let params = json!({});
    let diags = lsp::parse_diagnostics(&params);
    assert!(diags.is_empty());
}

// ── Completion parsing ──────────────────────────────────────────────

#[test]
fn parse_completions_items_object() {
    let result = json!({
        "items": [
            {"label": "println!", "detail": "macro", "kind": 15},
            {"label": "print!", "insertText": "print!($0)", "kind": 15}
        ]
    });
    let items = lsp::parse_completions(&result);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].label, "println!");
    assert_eq!(items[1].insert_text.as_deref(), Some("print!($0)"));
}

#[test]
fn parse_completions_direct_array() {
    let result = json!([
        {"label": "foo", "kind": 6}
    ]);
    let items = lsp::parse_completions(&result);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "foo");
}

#[test]
fn parse_completions_empty() {
    let result = json!(null);
    let items = lsp::parse_completions(&result);
    assert!(items.is_empty());
}

// ── Goto definition parsing ─────────────────────────────────────────

#[test]
fn parse_goto_single_location() {
    let result = json!({
        "uri": "file:///src/main.rs",
        "range": {
            "start": {"line": 10, "character": 4},
            "end": {"line": 10, "character": 12}
        }
    });
    let locs = lsp::parse_goto_definition(&result);
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].uri, "file:///src/main.rs");
    assert_eq!(locs[0].start_line, 10);
    assert_eq!(locs[0].start_col, 4);
}

#[test]
fn parse_goto_array() {
    let result = json!([
        {
            "uri": "file:///a.rs",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}
        },
        {
            "uri": "file:///b.rs",
            "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 8}}
        }
    ]);
    let locs = lsp::parse_goto_definition(&result);
    assert_eq!(locs.len(), 2);
}

#[test]
fn parse_goto_null() {
    let result = json!(null);
    let locs = lsp::parse_goto_definition(&result);
    assert!(locs.is_empty());
}

// ── Hover parsing ───────────────────────────────────────────────────

#[test]
fn parse_hover_string_contents() {
    let result = json!({"contents": "fn main()"});
    let hover = lsp::parse_hover(&result);
    assert_eq!(hover.as_deref(), Some("fn main()"));
}

#[test]
fn parse_hover_markup_content() {
    let result = json!({"contents": {"kind": "markdown", "value": "# Title"}});
    let hover = lsp::parse_hover(&result);
    assert_eq!(hover.as_deref(), Some("# Title"));
}

#[test]
fn parse_hover_null() {
    let result = json!(null);
    let hover = lsp::parse_hover(&result);
    assert!(hover.is_none());
}

// ── References parsing ──────────────────────────────────────────────

#[test]
fn parse_references_basic() {
    let result = json!([
        {
            "uri": "file:///src/lib.rs",
            "range": {"start": {"line": 5, "character": 0}, "end": {"line": 5, "character": 10}}
        }
    ]);
    let refs = lsp::parse_references(&result);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].uri, "file:///src/lib.rs");
}

#[test]
fn parse_references_null() {
    let result = json!(null);
    let refs = lsp::parse_references(&result);
    assert!(refs.is_empty());
}

// ── Code actions parsing ────────────────────────────────────────────

#[test]
fn parse_code_actions_basic() {
    let result = json!([
        {
            "title": "Add missing import",
            "kind": "quickfix",
            "edit": {"changes": {}}
        }
    ]);
    let actions = lsp::parse_code_actions(&result);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].title, "Add missing import");
}

#[test]
fn parse_code_actions_empty() {
    let result = json!([]);
    let actions = lsp::parse_code_actions(&result);
    assert!(actions.is_empty());
}

// ── Workspace symbol parsing ────────────────────────────────────────

#[test]
fn parse_workspace_symbols_basic() {
    let result = json!([
        {
            "name": "MyStruct",
            "kind": 23,
            "location": {
                "uri": "file:///src/lib.rs",
                "range": {"start": {"line": 10, "character": 0}, "end": {"line": 10, "character": 8}}
            }
        }
    ]);
    let symbols = lsp::parse_workspace_symbols(&result);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "MyStruct");
    assert_eq!(symbols[0].kind, 23);
}

// ── URI conversion ──────────────────────────────────────────────────

#[test]
fn path_to_uri_basic() {
    let path = std::path::Path::new("/Users/test/src/main.rs");
    let uri = lsp::path_to_uri(path);
    assert!(uri.starts_with("file:///"));
    assert!(uri.contains("main.rs"));
}

#[test]
fn uri_to_path_basic() {
    let path = lsp::uri_to_path("file:///Users/test/main.rs");
    assert_eq!(path.as_deref(), Some("/Users/test/main.rs"));
}

#[test]
fn uri_to_path_invalid() {
    let path = lsp::uri_to_path("not-a-uri");
    assert!(path.is_none());
}

// ── Symbol kind label ───────────────────────────────────────────────

#[test]
fn symbol_kind_labels() {
    assert_eq!(lsp::symbol_kind_label(12), "fn");
    assert_eq!(lsp::symbol_kind_label(23), "struct");
    assert_eq!(lsp::symbol_kind_label(5), "class");
    assert_eq!(lsp::symbol_kind_label(999), "sym");
}

// ── Text edit parsing ───────────────────────────────────────────────

#[test]
fn parse_text_edit_basic() {
    let edit = json!({
        "range": {
            "start": {"line": 0, "character": 0},
            "end": {"line": 0, "character": 5}
        },
        "newText": "hello"
    });
    let te = lsp::parse_text_edit(&edit);
    assert!(te.is_some());
    let te = te.unwrap();
    assert_eq!(te.start_line, 0);
    assert_eq!(te.start_col, 0);
    assert_eq!(te.end_line, 0);
    assert_eq!(te.end_col, 5);
    assert_eq!(te.new_text, "hello");
}

#[test]
fn parse_rename_edits_basic() {
    let result = json!({
        "changes": {
            "file:///src/main.rs": [
                {
                    "range": {"start": {"line": 1, "character": 4}, "end": {"line": 1, "character": 7}},
                    "newText": "bar"
                }
            ]
        }
    });
    let edits = lsp::parse_rename_edits(&result, "file:///src/main.rs");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "bar");
}

#[test]
fn parse_rename_edits_wrong_uri() {
    let result = json!({
        "changes": {
            "file:///other.rs": [
                {
                    "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}},
                    "newText": "baz"
                }
            ]
        }
    });
    let edits = lsp::parse_rename_edits(&result, "file:///src/main.rs");
    assert!(edits.is_empty());
}

#[test]
fn extracts_diagnostic_uri() {
    let params = json!({
        "uri": "file:///tmp/app.ts",
        "diagnostics": []
    });
    assert_eq!(
        lsp::diagnostics_uri(&params),
        Some("file:///tmp/app.ts".to_string())
    );
}

#[test]
fn builds_workspace_configuration_response() {
    let params = json!({
        "items": [
            {"scopeUri": "file:///tmp/app.ts", "section": "formattingOptions"},
            {"scopeUri": "file:///tmp/app.ts", "section": "unknown"}
        ]
    });
    let response = lsp::workspace_configuration_response(&params, 2, true);
    assert_eq!(
        response,
        json!([
            {"tabSize": 2, "insertSpaces": true},
            {}
        ])
    );
}
