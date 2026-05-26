# Viker

A Vim-like text editor written in Rust.
Viker is organized as a Cargo workspace with a reusable editor core, a
standalone Vim binding state machine, and two frontends: TUI (terminal) and GUI
(desktop window). It also has a UniFFI Swift target for building a local
`VikerKit` Swift package backed by an xcframework.

Viker includes tree-sitter highlighting and language-aware LSP/formatting hooks for Rust, Markdown, HTML, CSS, JavaScript, TypeScript, Python, fish, Bash/sh, and zsh.

## Documentation

- [Viker Guide](docs/viker-guide.md): shortcuts, app workflows, configuration, and library integration.
- [Swift Example](swift/Example/README.md): standalone macOS editor example that depends only on the local `VikerKit` Swift package.
- [VikerKit](swift/VikerKit/README.md): generated Swift package plus the reusable AppKit `VikerEditorComponent`.

## Requirements

- Rust nightly (edition 2024)
- Optional language servers in `PATH` for LSP features:
  `rust-analyzer`, `vscode-html-language-server`, `vscode-css-language-server`,
  `typescript-language-server`, `basedpyright-langserver`, `fish-lsp`,
  and `bash-language-server`
- Optional formatters in `PATH` for `:format`:
  `prettier`, `ruff`, `fish_indent`, and `shfmt`

## Build and Run

```bash
# TUI
cargo build -p viker-tui
cargo run -p viker-tui --bin viker -- <filepath>
cargo run -p viker-tui --bin viker -- <folder>

# GUI
cargo build -p viker-egui
cargo run -p viker-egui --bin viker-gui -- <filepath>
cargo run -p viker-egui --bin viker-gui

# Embeddable egui example
cargo run -p viker-egui --example embedded_markdown

# Swift package / xcframework
cargo test -p viker-swift
scripts/build-viker-swift-xcframework.sh

# Swift macOS example
swift build --package-path swift/Example
swift run --package-path swift/Example VikerExample
```

Starting the GUI without a file shows an `Open Folder` button. The selected
folder becomes the project root for `Ctrl-P`, workspace/LSP roots, and formatter
working directories for files inside that folder. Passing a folder path to the
GUI or TUI also opens it as the project root.

The GUI project view includes a compact left file navigator. It respects
`.gitignore`, shows files as a tree, has a bottom search field, and can filter
to git-modified files or files opened recently in the current session.

The Swift package build produces `swift/VikerKit`, with a generated
`VikerKitFFI.xcframework` binary target for iOS and macOS plus generated
UniFFI Swift sources. Add that folder as a local package in Xcode and `import
VikerKit`.

The repository root is also a Swift package, so downstream apps can depend on
the VikerKit repository directly and use the `VikerKit` product:

```swift
.package(url: "https://github.com/terhechte/VikerKit.git", from: "0.1.1")
```

Add `.product(name: "VikerKit", package: "VikerKit")` to the consuming target's
dependencies.

Remote SwiftPM consumers download `VikerKitFFI.xcframework` from the matching
GitHub release asset. The checked-in `swift/VikerKit` package still keeps a
local `VikerKitFFI.xcframework` for development and the example app.

For a new release, rebuild the xcframework, zip it, compute its SwiftPM
checksum, and update the workspace version:

```bash
scripts/build-viker-swift-xcframework.sh
ditto -c -k --sequesterRsrc --keepParent swift/VikerKit/VikerKitFFI.xcframework VikerKitFFI.xcframework.zip
swift package compute-checksum VikerKitFFI.xcframework.zip
scripts/set-viker-version.sh 0.2.0 <checksum>
```

SwiftPM resolves package versions from Git tags, so after rebuilding `VikerKit`
and committing the release changes, tag the release as `0.2.0`.

On macOS, `VikerKit` includes `VikerEditorComponent`, a reusable AppKit editor
view around the Viker core. Initialize it with `VikerEditorConfiguration` to
choose the color scheme, status bar visibility, top toolbar items, LSP loading,
initial insert/normal mode, insert-only behavior, and line-number gutter.

The standalone Swift example in `swift/Example` is a macOS executable package
that uses `VikerKit` through the local `../VikerKit` package dependency. It can
open a sample document by default or a file path passed on the command line.
On macOS, `VikerKit` owns the native `libgit2` system-library link through
`pkg-config`, so the example package does not declare that dependency itself.

## Features

### Vim Editing

- **Modes**: Normal / Insert / Visual / Visual Line / Command / Search
- **Motions**: `h/j/k/l`, `w/b/e/W/B/E`, `0/$`, `^`, `{/}`, `gg/G`, `H/M/L`, `%`, `f/F/t/T`
- **Editing**: `d/c/y` + motion, `dd/cc/yy`, `p/P`, `x`, `J`, `r`, `~`, `gu/gU/g~`, `Ctrl-A/X`
- **Undo/Redo**: `u` / `Ctrl-R`
- **Repeat**: `.` (dot repeat)
- **Macros**: `q{char}` to record, `@{char}` to play, `@@` to replay last macro
- **Registers**: `"{char}` to select a register
- **Visual Selection**: `v` / `V`, then apply operations like `d/c/y`

### Search and Replace

- `/` for regex search (incremental, smart case)
- `n/N` for next/prev match, `*/#` for word-under-cursor search
- `:s/old/new/[g][i]` for line replacement
- `:%s/old/new/[g][i]` for file-wide replacement (with capture groups)

### LSP

| Key / Command | Action |
|---------------|--------|
| auto / `Ctrl-Space` | Completion |
| `gd` | Go to definition |
| `K` | Hover |
| `gr` | Find references |
| `ga` | Code actions |
| `]D` / `:diagnostics` | Diagnostics list |
| `]d` / `[d` | Next/previous diagnostic |
| `Ctrl-T` | Workspace symbol search |
| `:rename <name>` | Rename symbol |
| `:format` | Format document |

Default language tooling:

| Language | LSP | Formatter |
|----------|-----|-----------|
| Rust | `rust-analyzer` | LSP formatting |
| HTML / CSS | `vscode-html-language-server`, `vscode-css-language-server` | `prettier` |
| JavaScript / TypeScript | `typescript-language-server` | `prettier` |
| Python | `basedpyright-langserver` | `ruff format` |
| fish | `fish-lsp` | `fish_indent` |
| Bash / sh | `bash-language-server` | `shfmt` |
| zsh | `bash-language-server` best-effort | disabled by default |

### Windows and Buffers

- `Ctrl-W v/s` to split panes (vertical/horizontal), `:split` / `:vsplit` with optional file
- `Ctrl-W h/j/k/l` to move between panes, `Ctrl-W q` to close a pane
- `gt/gT` or `:bn/:bp` to switch buffers
- `Ctrl-P` for fuzzy project file finder, `Ctrl-T` for workspace symbol search
- GUI toolbar `Files` button toggles the left project tree sidebar
- `:set wrap` / `:set nowrap` to toggle line wrapping
- `:set fontsize=N` to change GUI font size (8–48, default 14)
- `:set scrolloff=N` / `:set tabstop=N` to adjust scroll offset and tab width

### Configuration

Settings are read from `~/.config/viker/config.json` (`$XDG_CONFIG_HOME` preferred) at startup. All fields are optional; missing fields use defaults. The file is read-only — `:set` changes are session-local.

```json
{
  "scroll_off": 8,
  "wrap": true,
  "font_size": 16.0,
  "tab_width": 4,
  "languages": {
    "typescript": {
      "format_on_save": true,
      "lsp": { "command": "typescript-language-server", "args": ["--stdio"] },
      "formatter": { "command": "prettier", "args": ["--stdin-filepath", "{path}"] }
    },
    "zsh": {
      "formatter": { "enabled": false }
    }
  }
}
```

### Mouse Support

- Click to position cursor in the editor area
- Shift-click or drag to extend a characterwise selection
- Double-click to select a word, triple-click to select a line
- Scroll wheel to scroll the viewport
- Click on a split pane to switch focus
- TUI supports click, drag selection, and scroll wheel where the terminal sends
  mouse events

## Architecture

```text
┌──────────────────────────────────────────────────┐
│ crates/viker-core                              │
│ editor/ input commands/ lsp/ highlight/ config/  │
│ KeyInput, SyntaxStyle, AreaRect                  │
└───────────────┬──────────────────────────────────┘
                │
        ┌───────▼────────┐
        │ viker-vim    │
        │ keymap/VimCore │
        └────────────────┘

┌──────────────────────────────────────────────────┐
│ Frontends / bindings depend on core + vim        │
├────────────────┬────────────────┬────────────────┤
│ viker-tui    │ viker-egui   │ viker-swift  │
│ ratatui        │ egui/eframe    │ UniFFI Swift   │
│ bin: viker   │ bin: GUI       │ VikerKit     │
└────────────────┴────────────────┴────────────────┘
```

The core crate stays frontend-agnostic. UI frameworks convert their native input
and layout types into shared types such as `KeyInput`, `AreaRect`, and
`SyntaxStyle`.

## Limitations

- LSP is active-buffer oriented; switching to a different language/root restarts the active server.
- zsh LSP uses bash-language-server as a best-effort fallback.
- Native clipboard support uses `clipboard-rs` for text registers on supported platforms.
- LSP WorkspaceEdit currently applies only to the active file

## License

MIT
