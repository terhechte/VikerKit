# Recent Changes Audit

This branch is based on `main` at `d574f12` and currently contains the following
major changes.

## Workspace Split

- The old single-crate layout was split into `viker-core`, `viker-vim`,
  `viker-egui`, `viker-tui`, and `viker-swift`.
- `viker-core` remains frontend-neutral.
- The TUI and egui apps both depend on `viker-core` and `viker-vim`.
- The Swift target wraps the same editor and Vim keymap through UniFFI.

## Editor And Vim Core

- Vim bindings were moved into `viker-vim`.
- The app editor and `VimCore` share the same key mapping.
- Added or expanded support for counts, registers, macro previews, text objects,
  visual-block editing, marks, jump list navigation, case operations,
  increment/decrement, find-repeat, and dot-repeat coverage.
- Added display-cell metrics for tabs, wide characters, and combining
  characters.
- Added cursor and selection placement helpers that frontends can use for mouse
  interaction.

## Languages, Highlighting, LSP, And Formatting

- Added tree-sitter highlighting and language detection for Rust, Markdown,
  HTML, CSS, JavaScript, JSX, TypeScript, TSX, Python, fish, Bash/sh, and zsh.
- Added semantic highlight token styles and public highlight-span queries.
- Added language-specific LSP defaults and formatter resolution.
- Added workspace symbol search, code actions, diagnostics, rename, formatting,
  completion, hover, go-to-definition, and references parsing helpers.

## Project, Search, And Git

- Added gitignore-aware project file scanning and fuzzy file filtering.
- Added content search helpers for Swift and future frontend use.
- Added git status, branch, diff, stage/unstage, hunk stage/unstage, delete,
  amend, stash, merge, and rebase command support.
- Added syntax-highlighted git diff popup rendering in both TUI and egui.

## Frontend Availability

- The egui app exposes the full editor surface, project folder picker, project
  sidebar, file search, workspace symbols, LSP actions, formatting, git commands,
  panes, mouse placement and selection, and embeddable `show_inside` usage.
- The TUI exposes the same editor, Vim keymap, LSP actions, formatting, git
  commands, panes, popups, mouse placement and selection, and fuzzy project file
  finder. A folder argument now establishes the TUI project root for file search,
  LSP roots, formatter working directories, and git commands.
- egui now mirrors TUI behavior for cross-file go-to-definition/reference jumps
  and executes LSP code-action commands when an action supplies a command.

## Swift / VikerKit

- Added UniFFI bindings and an xcframework build script.
- Exposed editor snapshots, lines, cursor/mode state, register summaries, save
  APIs, Vim key processing, display-cell metrics, syntax highlight spans,
  language forcing, file/content search, and a shared LSP workspace surface.

## Follow-up Risks

- LSP is still active-buffer oriented and does not maintain a multi-server
  workspace.
- Workspace edits still apply only to the active file.
- The GUI project sidebar is intentionally GUI-only.
- Clipboard integration uses `clipboard-rs` instead of shelling out to macOS pasteboard tools.
