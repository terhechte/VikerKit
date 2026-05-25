use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    pub command: &'static str,
    pub args: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocation {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatterSpec {
    pub tool: ToolSpec,
}

#[derive(Debug, Clone, Copy)]
pub struct LanguageSpec {
    pub kind: LanguageKind,
    pub id: &'static str,
    pub lsp_language_id: &'static str,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub shebangs: &'static [&'static str],
    pub root_markers: &'static [&'static str],
    pub lsp: Option<ToolSpec>,
    pub formatter: Option<FormatterSpec>,
}

const PRETTIER: FormatterSpec = FormatterSpec {
    tool: ToolSpec {
        command: "prettier",
        args: &["--stdin-filepath", "{path}"],
    },
};

const LANGUAGE_SPECS: &[LanguageSpec] = &[
    LanguageSpec {
        kind: LanguageKind::Rust,
        id: "rust",
        lsp_language_id: "rust",
        extensions: &["rs"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["Cargo.toml", "rust-project.json", ".git"],
        lsp: Some(ToolSpec {
            command: "rust-analyzer",
            args: &[],
        }),
        formatter: None,
    },
    LanguageSpec {
        kind: LanguageKind::Markdown,
        id: "markdown",
        lsp_language_id: "markdown",
        extensions: &["md", "markdown", "mdown", "mkd"],
        filenames: &[],
        shebangs: &[],
        root_markers: &[".git"],
        lsp: None,
        formatter: None,
    },
    LanguageSpec {
        kind: LanguageKind::Html,
        id: "html",
        lsp_language_id: "html",
        extensions: &["html", "htm"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["package.json", ".git"],
        lsp: Some(ToolSpec {
            command: "vscode-html-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::Css,
        id: "css",
        lsp_language_id: "css",
        extensions: &["css"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["package.json", ".git"],
        lsp: Some(ToolSpec {
            command: "vscode-css-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::JavaScript,
        id: "javascript",
        lsp_language_id: "javascript",
        extensions: &["js", "mjs", "cjs"],
        filenames: &[],
        shebangs: &["node"],
        root_markers: &["package.json", "jsconfig.json", "tsconfig.json", ".git"],
        lsp: Some(ToolSpec {
            command: "typescript-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::Jsx,
        id: "javascriptreact",
        lsp_language_id: "javascriptreact",
        extensions: &["jsx"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["package.json", "jsconfig.json", "tsconfig.json", ".git"],
        lsp: Some(ToolSpec {
            command: "typescript-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::TypeScript,
        id: "typescript",
        lsp_language_id: "typescript",
        extensions: &["ts", "mts", "cts"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["tsconfig.json", "package.json", ".git"],
        lsp: Some(ToolSpec {
            command: "typescript-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::Tsx,
        id: "typescriptreact",
        lsp_language_id: "typescriptreact",
        extensions: &["tsx"],
        filenames: &[],
        shebangs: &[],
        root_markers: &["tsconfig.json", "package.json", ".git"],
        lsp: Some(ToolSpec {
            command: "typescript-language-server",
            args: &["--stdio"],
        }),
        formatter: Some(PRETTIER),
    },
    LanguageSpec {
        kind: LanguageKind::Python,
        id: "python",
        lsp_language_id: "python",
        extensions: &["py", "pyw"],
        filenames: &[],
        shebangs: &["python", "python3"],
        root_markers: &[
            "pyproject.toml",
            "pyrightconfig.json",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "uv.lock",
            "poetry.lock",
            ".git",
        ],
        lsp: Some(ToolSpec {
            command: "basedpyright-langserver",
            args: &["--stdio"],
        }),
        formatter: Some(FormatterSpec {
            tool: ToolSpec {
                command: "ruff",
                args: &["format", "--stdin-filename", "{path}", "--quiet", "-"],
            },
        }),
    },
    LanguageSpec {
        kind: LanguageKind::Fish,
        id: "fish",
        lsp_language_id: "fish",
        extensions: &["fish"],
        filenames: &["config.fish"],
        shebangs: &["fish"],
        root_markers: &["config.fish", ".git"],
        lsp: Some(ToolSpec {
            command: "fish-lsp",
            args: &["start"],
        }),
        formatter: Some(FormatterSpec {
            tool: ToolSpec {
                command: "fish_indent",
                args: &[],
            },
        }),
    },
    LanguageSpec {
        kind: LanguageKind::Bash,
        id: "bash",
        lsp_language_id: "shellscript",
        extensions: &["sh", "bash", "bats"],
        filenames: &[".bashrc", ".bash_profile", ".bash_login", ".profile"],
        shebangs: &["sh", "bash", "dash"],
        root_markers: &[".git"],
        lsp: Some(ToolSpec {
            command: "bash-language-server",
            args: &["start"],
        }),
        formatter: Some(FormatterSpec {
            tool: ToolSpec {
                command: "shfmt",
                args: &["-ln", "bash", "-filename", "{path}"],
            },
        }),
    },
    LanguageSpec {
        kind: LanguageKind::Zsh,
        id: "zsh",
        lsp_language_id: "shellscript",
        extensions: &["zsh"],
        filenames: &[".zshrc", ".zprofile", ".zlogin", ".zlogout", ".zshenv"],
        shebangs: &["zsh"],
        root_markers: &[".git"],
        lsp: Some(ToolSpec {
            command: "bash-language-server",
            args: &["start"],
        }),
        formatter: None,
    },
];

impl LanguageKind {
    pub fn spec(self) -> &'static LanguageSpec {
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.kind == self)
            .expect("language kind missing from registry")
    }

    pub fn from_path(path: Option<&Path>) -> Option<Self> {
        let path = path?;
        let file_name = path.file_name()?.to_str()?;
        let file_name_lower = file_name.to_ascii_lowercase();

        for spec in LANGUAGE_SPECS {
            if spec
                .filenames
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&file_name_lower))
            {
                return Some(spec.kind);
            }
        }

        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.extensions.iter().any(|candidate| *candidate == ext))
            .map(|spec| spec.kind)
    }

    pub fn from_path_and_text(path: Option<&Path>, text: &str) -> Option<Self> {
        Self::from_path(path).or_else(|| Self::from_shebang(text))
    }

    pub fn from_shebang(text: &str) -> Option<Self> {
        let first = text.lines().next()?.trim();
        let command_line = first.strip_prefix("#!")?.trim();
        let mut parts = command_line.split_whitespace();
        let first_command = parts
            .next()
            .and_then(|part| Path::new(part).file_name())
            .and_then(|part| part.to_str())?
            .to_ascii_lowercase();
        let command = if first_command == "env" {
            parts
                .find(|part| !part.starts_with('-'))
                .and_then(|part| Path::new(part).file_name())
                .and_then(|part| part.to_str())?
                .to_ascii_lowercase()
        } else {
            first_command
        };

        for spec in LANGUAGE_SPECS {
            if spec.shebangs.iter().any(|candidate| *candidate == command) {
                return Some(spec.kind);
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [LanguageSpec] {
        LANGUAGE_SPECS
    }
}

pub fn spec_for_path(path: &Path) -> Option<&'static LanguageSpec> {
    LanguageKind::from_path(Some(path)).map(LanguageKind::spec)
}

#[allow(dead_code)]
pub fn spec_for_path_and_text(path: Option<&Path>, text: &str) -> Option<&'static LanguageSpec> {
    LanguageKind::from_path_and_text(path, text).map(LanguageKind::spec)
}

pub fn supports_lsp(path: &Path) -> bool {
    spec_for_path(path).is_some_and(|spec| spec.lsp.is_some())
}

#[allow(dead_code)]
pub fn supports_formatting(path: &Path) -> bool {
    spec_for_path(path).is_some_and(|spec| spec.formatter.is_some())
}

pub fn format_on_save(spec: &LanguageSpec, config: &Config) -> bool {
    config
        .languages
        .get(spec.id)
        .and_then(|cfg| cfg.format_on_save)
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn resolve_tool(tool: ToolSpec, path: &Path) -> ToolInvocation {
    let path = path.to_string_lossy();
    ToolInvocation {
        command: tool.command.to_string(),
        args: tool
            .args
            .iter()
            .map(|arg| arg.replace("{path}", &path))
            .collect(),
    }
}

pub fn resolve_lsp(spec: &LanguageSpec, config: &Config, path: &Path) -> Option<ToolInvocation> {
    resolve_configured_tool(spec, spec.lsp, config, path, ToolSlot::Lsp)
}

pub fn resolve_formatter(
    spec: &LanguageSpec,
    config: &Config,
    path: &Path,
) -> Option<ToolInvocation> {
    resolve_configured_tool(
        spec,
        spec.formatter.map(|fmt| fmt.tool),
        config,
        path,
        ToolSlot::Formatter,
    )
}

#[derive(Debug, Clone, Copy)]
enum ToolSlot {
    Lsp,
    Formatter,
}

fn resolve_configured_tool(
    spec: &LanguageSpec,
    default: Option<ToolSpec>,
    config: &Config,
    path: &Path,
    slot: ToolSlot,
) -> Option<ToolInvocation> {
    let lang_config = config.languages.get(spec.id);
    let configured = lang_config.and_then(|cfg| match slot {
        ToolSlot::Lsp => cfg.lsp.as_ref(),
        ToolSlot::Formatter => cfg.formatter.as_ref(),
    });
    if configured.and_then(|cfg| cfg.enabled) == Some(false) {
        return None;
    }

    let command = configured
        .and_then(|cfg| cfg.command.as_deref())
        .or_else(|| default.map(|tool| tool.command))?;
    let args: Vec<String> = if let Some(args) = configured.and_then(|cfg| cfg.args.as_ref()) {
        args.clone()
    } else {
        default
            .map(|tool| tool.args.iter().map(|arg| (*arg).to_string()).collect())
            .unwrap_or_default()
    };
    let path = path.to_string_lossy();
    Some(ToolInvocation {
        command: command.replace("{path}", &path),
        args: args
            .into_iter()
            .map(|arg| arg.replace("{path}", &path))
            .collect(),
    })
}

pub fn find_project_root(file_path: &Path) -> PathBuf {
    let spec = spec_for_path(file_path);
    let markers = spec
        .map(|spec| spec.root_markers)
        .unwrap_or(&[".git"] as &[&str]);
    find_project_root_with_markers(file_path, markers)
}

pub fn find_project_root_for_language(file_path: &Path, language: LanguageKind) -> PathBuf {
    find_project_root_with_markers(file_path, language.spec().root_markers)
}

fn find_project_root_with_markers(file_path: &Path, markers: &[&str]) -> PathBuf {
    let start = if file_path.is_file() {
        file_path.parent().unwrap_or(file_path)
    } else {
        file_path
    };
    let mut dir = start.to_path_buf();
    loop {
        if markers.iter().any(|marker| dir.join(marker).exists()) {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap_or_default()
}
