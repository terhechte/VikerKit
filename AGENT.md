# Viker Agent Guide

This file is a working map for coding agents making changes in this repository.
It describes the current workspace layout, targets, feature boundaries, and major
capabilities of the app.

## Project Shape

Viker is a Rust 2024 Vim-like editor organized as a Cargo workspace. The code
is split so reusable editor logic can evolve independently from frontend UI code:

- `viker-core`: abstract editor, buffer, panes, highlighting, LSP, formatting,
  configuration, command execution, and frontend-neutral key/event types.
- `viker-vim`: Vim binding state machine and tests, built on `viker-core`.
- `viker-egui`: desktop GUI app and embeddable egui editor widget.
- `viker-tui`: terminal frontend built with `ratatui` and `crossterm`.
- `viker-swift`: UniFFI Swift binding layer and static library target used to
  build the local `VikerKit` Swift package / xcframework.

The intended dependency direction is:

```text
       viker-core
            ^
            |
       viker-vim
        ^       ^       ^
        |       |       |
viker-egui  viker-tui  viker-swift
```

The core crate must not depend on `egui`, `eframe`, `ratatui`, `crossterm`,
native file dialogs, or other frontend-specific APIs.

## Cargo Targets

Workspace crates:

- `crates/viker-core`: library crate `viker_core`.
- `crates/viker-vim`: library crate `viker_vim`.
- `crates/viker-egui`: library crate `viker_egui`, binary `viker-gui`,
  example `embedded_markdown`.
- `crates/viker-tui`: library crate `viker_tui`, binary `viker`.
- `crates/viker-swift`: library crate `viker_swift`, staticlib/cdylib for
  UniFFI, helper binary `uniffi-bindgen-swift`.

Useful commands:

```bash
cargo fmt
cargo test
cargo test -p viker-vim vim_core
cargo test -p viker-swift
cargo check -p viker-core
cargo check -p viker-egui
cargo check -p viker-tui
cargo run -p viker-tui --bin viker -- <file>
cargo run -p viker-egui --bin viker-gui -- <file-or-folder>
cargo run -p viker-egui --example embedded_markdown
scripts/build-viker-swift-xcframework.sh
```

There are no workspace-level `tui` or `gui` feature flags anymore. Build the
specific package you need.

## Core Modules

- `crates/viker-core/src/editor/mod.rs`: main editor state and editing
  operations for the app editor. Owns buffers, panes, modes, cursor/view state,
  commands, registers, marks, search, completion state, diagnostics state, and
  deferred actions.
- `crates/viker-core/src/vim.rs`: shared standalone Vim editing engine used by
  the Vim conformance tests and wrapped by `viker-vim` for key processing.
- `crates/viker-core/src/editor/document.rs`: document rope, path, save/open,
  modified flag, and version tracking.
- `crates/viker-core/src/editor/pane.rs`: frontend-independent split-pane
  layout, pane state, and `AreaRect`.
- `crates/viker-core/src/editor/history.rs`: undo/redo snapshots.
- `crates/viker-core/src/editor/selection.rs`: selection and cursor position
  types.
- `crates/viker-core/src/editor/view.rs`: viewport state and scroll helpers.
- `crates/viker-core/src/editor/wrap.rs`: soft-wrap screen/document
  coordinate helpers.
- `crates/viker-core/src/input/mod.rs`: executes commands against `Editor` and
  returns deferred side effects for frontends.
- `crates/viker-core/src/input/command.rs`: command and invocation
  definitions.
- `crates/viker-core/src/input/mode.rs`: editor mode definitions.
- `crates/viker-core/src/key.rs`: frontend-neutral key representation.
- `crates/viker-core/src/buffer/mod.rs`: shared buffer display utilities.

## Vim Module

- `crates/viker-vim/src/vim.rs`: thin wrapper around `viker_core::vim::VimCore`
  that connects the shared Vim engine to the keymap.
- `crates/viker-vim/src/keymap.rs`: maps `KeyInput` plus keymap state to
  command invocations, including counts, pending operators, text objects,
  register selection, file finder, and popup modes.
- `crates/viker-vim/src/editor_adapter.rs`: implements the Vim keymap state
  trait for `viker_core::editor::Editor`, allowing app frontends to use the
  same Vim mapping.
- `crates/viker-vim/tests/vim_core_tests.rs`: primary conformance tests for
  Vim bindings. Prefer adding binding tests here before GUI/TUI tests.

`VimCore::process_key` remains the main public test surface. GUI and TUI
behavior should generally fall out of `viker-vim` key mapping plus
`viker-core` command execution.

## Language, Highlighting, LSP, Formatting

- `crates/viker-core/src/language.rs`: language registry, file/shebang
  detection, root markers, default LSP/formatter commands, config override
  resolution, and format-on-save settings.
- `crates/viker-core/src/highlight/mod.rs`: tree-sitter highlighter setup for
  supported languages.
- `crates/viker-core/src/highlight/theme.rs` and
  `crates/viker-core/src/highlight/style.rs`: syntax styles and theme colors.
- `crates/viker-core/src/lsp/mod.rs`: JSON-RPC LSP client, request/response
  parsing helpers, URI conversion, diagnostic parsing,
  completion/hover/reference/code-action/rename parsing, and project-root
  fallback helpers.
- `crates/viker-core/src/lsp/transport.rs`: LSP process transport.
- `crates/viker-core/src/formatter.rs`: external stdin/stdout formatter
  runner.
- `crates/viker-core/src/config.rs`: loads `~/.config/viker/config.json`,
  honoring `$XDG_CONFIG_HOME`.

Supported language/filetype coverage currently includes Rust, Markdown, HTML,
CSS, JavaScript, JSX, TypeScript, TSX, Python, fish, Bash/sh, and zsh.

Default external tools:

- Rust: `rust-analyzer`, LSP formatting.
- HTML/CSS: `vscode-html-language-server`, `vscode-css-language-server`,
  `prettier`.
- JavaScript/TypeScript/JSX/TSX: `typescript-language-server`, `prettier`.
- Python: `basedpyright-langserver`, `ruff format`.
- fish: `fish-lsp`, `fish_indent`.
- Bash/sh: `bash-language-server`, `shfmt`.
- zsh: `bash-language-server` best-effort, no default formatter.

LSP is active-buffer oriented. Switching to a different language/root restarts
the active server instead of maintaining multiple concurrent LSP clients.

## Frontends

### TUI

- `crates/viker-tui/src/main.rs`: terminal setup/teardown and TUI app startup.
- `crates/viker-tui/src/app.rs`: TUI app loop, crossterm event conversion,
  Vim key mapping, LSP lifecycle, file opening, formatting, and deferred action
  handling.
- `crates/viker-tui/src/ui/*`: ratatui rendering for editor view, status line,
  tab bar, command line, completions, hover, references, diagnostics, file
  finder, code actions, and workspace symbols.

The TUI starts with a file if one is passed, otherwise with an empty buffer. It
does not implement the GUI project sidebar.

### GUI

- `crates/viker-egui/src/main.rs`: eframe startup.
- `crates/viker-egui/src/gui_app.rs`: GUI app wrapper around the shared
  `Editor`; owns native folder selection, explicit project root, Vim key
  mapping, GUI LSP lifecycle, mouse handling, top toolbar, project sidebar
  integration, and embedded/full-window rendering.
- `crates/viker-egui/src/gui/mod.rs`: egui render orchestration for the editor
  surface.
- `crates/viker-egui/src/gui/*`: egui rendering for editor view, tab bar,
  command line, popups, diagnostics, and project sidebar.
- `crates/viker-egui/src/gui/project_sidebar.rs`: GUI-only project navigator.
  It scans the project with `.gitignore` support via `ignore`, renders a tree,
  has a bottom search field, supports git-modified and recent-file filters, and
  opens files relative to the selected project root.
- `crates/viker-egui/examples/embedded_markdown.rs`: demonstrates embedding
  the editor next to independent egui UI.

GUI project behavior:

- Starting `viker-gui` without an argument shows an `Open Folder` button.
- Passing a folder path opens it as the project root.
- Passing a file opens that file directly.
- The toolbar has a compact `Files` toggle for the left project tree.
- The project root is used for sidebar scanning, `Ctrl-P`, LSP roots, and
  formatter working directories for files inside the project.

### Swift / UniFFI

- `crates/viker-swift/src/lib.rs`: UniFFI object/record/enum surface for
  Swift. It wraps `viker_core::Editor`, uses `viker_vim::keymap` for Vim
  input, and exposes snapshots, text/line access, cursor/mode, cursor and
  selection placement, register summaries, save/save-as, command execution, and
  deferred effects.
- `crates/viker-swift/uniffi.toml`: Swift binding names. High-level module is
  `VikerKit`; low-level FFI module is `VikerKitFFI`.
- `scripts/build-viker-swift-xcframework.sh`: builds iOS device/simulator
  and macOS static libraries, runs UniFFI Swift binding generation, creates
  `VikerKitFFI.xcframework`, and populates `swift/VikerKit`.
- `swift/VikerKit`: local Swift package scaffold. Generated Swift sources and
  the xcframework are ignored until the build script creates them.

The Swift layer is meant for native Swift UI experiments. Keep it as a narrow
translation layer and avoid adding Swift/iOS concerns to `viker-core`.

## Major Editor Capabilities

- Vim modes: Normal, Insert, Replace, Visual, Visual Line, Visual Block,
  Command, and Search.
- Counts and operator/motion grammar, including multiplication like `2d3w`.
- Motions, text objects, marks, jumps, find-repeat, search, and diagnostics
  navigation.
- Operators: delete, change, yank, indent/dedent, format, shell filter, case
  changes, and visual operations.
- Registers: unnamed, numbered, yank `0`, small-delete `-`, black-hole `_`,
  named registers, uppercase append, macro registers, and register summaries.
- Macro recording/playback and repeat behavior.
- Split panes and buffer switching.
- Fuzzy file finder and workspace symbol picker.
- LSP completion, hover, go-to-definition, references, code actions, rename,
  diagnostics, workspace symbols, and formatting fallback.

## Testing Strategy

- Add Vim binding behavior to `crates/viker-vim/tests/vim_core_tests.rs` first
  whenever possible.
- Add editor-level tests in `crates/viker-core/tests/editor_tests.rs` when
  behavior depends on the full `Editor` state rather than the smaller Vim state
  machine.
- Add language/highlight/LSP/formatter tests in:
  - `crates/viker-core/tests/language_tests.rs`
  - `crates/viker-core/tests/highlight_tests.rs`
  - `crates/viker-core/tests/lsp_parse_tests.rs`
  - `crates/viker-core/tests/formatter_tests.rs`
- GUI-only project/sidebar behavior currently has unit tests under
  `crates/viker-egui/src/gui_app.rs` and
  `crates/viker-egui/src/gui/project_sidebar.rs`.
- Swift FFI smoke tests live in `crates/viker-swift/src/lib.rs`; keep them
  focused on UniFFI-safe API behavior and generated binding compatibility.

Before handing off substantial changes, run:

```bash
cargo fmt
cargo test
cargo check --workspace
cargo test -p viker-swift
```

## Current Limitations

- LSP is active-buffer oriented, not a persistent multi-server workspace manager.
- zsh uses `bash-language-server` as a best-effort fallback.
- Clipboard support is currently macOS-oriented.
- LSP WorkspaceEdit application is limited to active-file edits.
- The Xcode-style project sidebar is GUI-only.
