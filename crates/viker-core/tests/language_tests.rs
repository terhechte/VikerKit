use std::path::Path;

use viker_core::config::{Config, LanguageConfig, ToolConfig};
use viker_core::language::{LanguageKind, resolve_tool, spec_for_path};

#[test]
fn detects_requested_languages_from_extensions() {
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("index.html"))),
        Some(LanguageKind::Html)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("style.css"))),
        Some(LanguageKind::Css)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("app.jsx"))),
        Some(LanguageKind::Jsx)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("component.tsx"))),
        Some(LanguageKind::Tsx)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("script.py"))),
        Some(LanguageKind::Python)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("config.fish"))),
        Some(LanguageKind::Fish)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new("setup.zsh"))),
        Some(LanguageKind::Zsh)
    );
}

#[test]
fn detects_shell_filenames_and_shebangs() {
    assert_eq!(
        LanguageKind::from_path(Some(Path::new(".bashrc"))),
        Some(LanguageKind::Bash)
    );
    assert_eq!(
        LanguageKind::from_path(Some(Path::new(".zprofile"))),
        Some(LanguageKind::Zsh)
    );
    assert_eq!(
        LanguageKind::from_path_and_text(None, "#!/usr/bin/env python3\nprint('hi')\n"),
        Some(LanguageKind::Python)
    );
    assert_eq!(
        LanguageKind::from_path_and_text(None, "#!/bin/zsh\necho hi\n"),
        Some(LanguageKind::Zsh)
    );
}

#[test]
fn exposes_lsp_and_formatter_defaults() {
    let ts = spec_for_path(Path::new("src/main.ts")).unwrap();
    assert_eq!(ts.lsp.unwrap().command, "typescript-language-server");
    assert_eq!(ts.formatter.unwrap().tool.command, "prettier");

    let python = spec_for_path(Path::new("tool.py")).unwrap();
    assert_eq!(python.lsp.unwrap().command, "basedpyright-langserver");
    assert_eq!(python.formatter.unwrap().tool.command, "ruff");

    let zsh = spec_for_path(Path::new(".zshrc")).unwrap();
    assert!(zsh.formatter.is_none());
}

#[test]
fn resolves_path_placeholders_in_tool_args() {
    let html = spec_for_path(Path::new("index.html")).unwrap();
    let formatter = html.formatter.unwrap().tool;
    let invocation = resolve_tool(formatter, Path::new("/tmp/index.html"));
    assert_eq!(invocation.command, "prettier");
    assert_eq!(invocation.args, vec!["--stdin-filepath", "/tmp/index.html"]);
}

#[test]
fn config_can_override_or_disable_tools() {
    let mut config = Config::default();
    config.languages.insert(
        "typescript".to_string(),
        LanguageConfig {
            lsp: Some(ToolConfig {
                command: Some("custom-ts-lsp".to_string()),
                args: Some(vec!["--socket".to_string(), "{path}".to_string()]),
                enabled: None,
            }),
            formatter: Some(ToolConfig {
                command: None,
                args: None,
                enabled: Some(false),
            }),
            format_on_save: Some(true),
        },
    );

    let path = Path::new("/tmp/app.ts");
    let spec = spec_for_path(path).unwrap();
    let lsp = viker_core::language::resolve_lsp(spec, &config, path).unwrap();
    assert_eq!(lsp.command, "custom-ts-lsp");
    assert_eq!(lsp.args, vec!["--socket", "/tmp/app.ts"]);
    assert!(viker_core::language::format_on_save(spec, &config));
    assert!(viker_core::language::resolve_formatter(spec, &config, path).is_none());
}
