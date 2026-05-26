# Viker Guide

Viker is a Rust editor toolkit for embedding Vim-style editing into other
frontends. The repository includes a terminal app, an egui desktop app, a Swift
FFI surface, and reusable Rust crates for host applications.

## Running The Apps

```bash
# Terminal UI. Pass a file, a folder, or no path.
cargo run -p viker-tui --bin viker -- path/to/file.rs
cargo run -p viker-tui --bin viker -- path/to/project
cargo run -p viker-tui --bin viker

# egui desktop app. Pass a file, a folder, or no path.
cargo run -p viker-egui --bin viker-gui -- path/to/file.rs
cargo run -p viker-egui --bin viker-gui -- path/to/project
cargo run -p viker-egui --bin viker-gui

# Embeddable egui example.
cargo run -p viker-egui --example embedded_markdown
```

Passing a folder sets the project root. The project root is used for fuzzy file
search, LSP roots, formatter working directories, and git commands. Starting the
GUI without an argument shows an `Open Folder` button. Starting the TUI without
an argument creates an empty buffer and uses the current directory when a project
operation needs a root.

## App Workflow

Viker starts in Normal mode. Press `i`, `a`, `o`, or `O` to enter Insert mode,
then press `Esc` to return to Normal mode. Press `:` for command mode and `/` or
`?` for search.

The editor has buffers and panes:

- `Ctrl-P` opens the project file finder.
- `Ctrl-T` opens workspace symbol search when an LSP is active.
- `Ctrl-W v` and `Ctrl-W s` split the current view.
- `gt` and `gT` move across buffers.
- `:w` saves, `:q` quits, and `:wq` writes then quits.

LSP requests are active-buffer oriented. When a file in another language or root
is opened, the active language server is restarted for that file. Viker can
still open cross-file definitions and references, but it does not keep multiple
language servers alive at the same time.

The GUI includes a project sidebar with git-modified and recently-opened file
filters. The TUI has the same editor core, Vim bindings, fuzzy file finder,
workspace symbols, git commands, LSP actions, formatting, split panes, and mouse
selection, but it does not render the GUI file-tree sidebar.

## Shortcut Reference

### Modes And Insert

| Shortcut | Action |
| --- | --- |
| `i` / `a` | Insert before / after cursor |
| `I` / `A` | Insert at first non-blank / end of line |
| `o` / `O` | Open a line below / above |
| `R` | Replace mode |
| `v` / `V` / `Ctrl-V` | Visual, visual-line, visual-block |
| `Esc` | Return to Normal mode or dismiss popup |

### Movement

| Shortcut | Action |
| --- | --- |
| `h` `j` `k` `l` | Move left, down, up, right |
| `w` `b` `e` | Word forward, backward, end |
| `W` `B` `E` | Whitespace-delimited WORD movement |
| `0` `$` `^` | Line start, line end, first non-blank |
| `gg` / `G` | File start / file end |
| `{` `}` | Previous / next paragraph |
| `(` `)` | Previous / next sentence |
| `[[` `]]` | Previous / next section marker |
| `H` `M` `L` | Cursor to high, middle, low viewport row |
| `Ctrl-D` `Ctrl-U` | Half-page down / up |
| `Ctrl-F` `Ctrl-B` | Page down / up |
| `gj` `gk` | Move by document line when wrap is enabled |
| `%` | Jump to matching bracket |
| `f{char}` `F{char}` | Find character forward / backward |
| `t{char}` `T{char}` | Till character forward / backward |
| `;` `,` | Repeat last find forward / backward |

### Editing Operators

| Shortcut | Action |
| --- | --- |
| `d{motion}` / `dd` | Delete by motion / line |
| `c{motion}` / `cc` | Change by motion / line |
| `y{motion}` / `yy` | Yank by motion / line |
| `p` / `P` | Paste after / before |
| `x` / `X` | Delete character under / before cursor |
| `s` / `S` | Substitute character / line |
| `J` | Join lines |
| `r{char}` | Replace one character |
| `.` | Repeat the last change |
| `u` / `Ctrl-R` | Undo / redo |
| `>` `<` `=` | Indent, dedent, or format a motion |
| `!{motion}` | Filter a motion through a shell command |
| `~` | Toggle character case |
| `gu{motion}` / `gU{motion}` / `g~{motion}` | Lower, upper, or toggle a motion |
| `Ctrl-A` / `Ctrl-X` | Increment / decrement number |

### Text Objects And Visual Mode

| Shortcut | Action |
| --- | --- |
| `iw` / `aw` | Inner / around word |
| `i"` `a"` | Inner / around quoted string |
| `i'` `a'` | Inner / around single-quoted string |
| `i(` `a(`, `i[` `a[`, `i{` `a{` | Inner / around bracketed text |
| `ip` / `ap` | Inner / around paragraph |
| `it` / `at` | Inner / around tag |
| Visual `d` `y` `c` | Delete, yank, or change selection |
| Visual `>` `<` | Indent or dedent selection |
| Visual `o` / `O` | Swap selection anchor |
| Visual-block `I` / `A` | Insert or append on each selected row |

### Registers, Marks, And Macros

| Shortcut | Action |
| --- | --- |
| `"{register}` | Select a register for the next operation |
| `"_` | Black-hole register |
| `"A`..`"Z` | Append to named register |
| `m{mark}` | Set mark |
| `'{mark}` / `` `{mark}`` | Jump to mark line / exact position |
| `''` / `` `` `` | Jump to previous line / exact position |
| `Ctrl-O` / `Ctrl-I` | Jump backward / forward |
| `q{register}` | Start macro recording |
| `q` | Stop macro recording |
| `@{register}` / `@@` | Play macro / replay last macro |

### Search And Replace

| Shortcut | Action |
| --- | --- |
| `/` / `?` | Forward / backward regex search |
| `n` / `N` | Next / previous search result |
| `*` / `#` | Search word under cursor forward / backward |
| `:nohlsearch` | Clear search highlighting |
| `:s/old/new/[g][i]` | Replace on current line |
| `:%s/old/new/[g][i]` | Replace in the whole file |

### Files, Panes, And LSP

| Shortcut | Action |
| --- | --- |
| `Ctrl-P` | Fuzzy project file finder |
| `Ctrl-T` | Workspace symbol search |
| `Ctrl-W v` / `Ctrl-W s` | Vertical / horizontal split |
| `Ctrl-W h/j/k/l` | Move between panes |
| `Ctrl-W w` | Next pane |
| `Ctrl-W q` / `Ctrl-W o` | Close pane / keep only this pane |
| `gt` / `gT` | Next / previous buffer |
| `gd` | Go to definition |
| `gr` | Find references |
| `K` | Hover |
| `ga` | Code actions |
| `]d` / `[d` | Next / previous diagnostic |
| `]D` | Diagnostics list |
| Insert `Ctrl-Space` | Trigger completion |
| Completion `Tab` / `Shift-Tab` | Next / previous completion |
| Completion `Enter` | Accept completion |

## Commands

| Command | Action |
| --- | --- |
| `:w`, `:write` | Save current file |
| `:q`, `:quit` | Quit |
| `:wq` | Save and quit |
| `:e <path>` | Open file |
| `:bn` / `:bp` | Next / previous buffer |
| `:split [path]` / `:vsplit [path]` | Split pane, optionally with a file |
| `:set wrap` / `:set nowrap` | Toggle soft wrap |
| `:set relativenumber` / `:set norelativenumber` | Toggle relative line numbers |
| `:set scrolloff=N` | Configure scroll margin |
| `:set tabstop=N` | Configure tab display width |
| `:set fontsize=N` | GUI font size, 8 through 48 |
| `:rename <name>` | LSP rename |
| `:format` | Format using configured formatter or LSP |
| `:diagnostics` | Diagnostics list |
| `:git status` / `:git diff` | Git status or diff popup |
| `:git add [path]` / `:git reset [path]` | Stage or unstage files |
| `:git branch [name]` / `:git checkout <name>` | List/create or switch branches |
| `:git stash [message]` / `:git stash pop [index]` | Stash or pop changes |
| `:git merge <branch>` / `:git rebase <upstream>` | Merge or rebase |

## Configuration

Viker reads `~/.config/viker/config.json`, or
`$XDG_CONFIG_HOME/viker/config.json` if `XDG_CONFIG_HOME` is set. Missing
fields use defaults. Runtime `:set` changes affect only the current session.

```json
{
  "scroll_off": 8,
  "wrap": true,
  "relative_number": true,
  "font_size": 16.0,
  "font_family": "JetBrains Mono",
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

## How The Library Works

Viker is split into small crates so host apps can choose the pieces they need:

- `viker-core` owns the editor state, buffers, selections, panes, search,
  syntax highlighting, LSP parsing, formatting, configuration, and command
  execution.
- `viker-vim` maps `KeyInput` values into command invocations. It can run
  against the full `Editor` or the smaller `VimCore` compatibility state.
- `viker-egui` provides a desktop app plus an embeddable egui widget.
- `viker-tui` provides a ratatui/crossterm terminal frontend.
- `viker-swift` exposes a UniFFI bridge for Swift experiments.

The frontend integration loop is:

1. Convert frontend input into `viker_core::key::KeyInput`.
2. Pass the key to `viker_vim::keymap::map_key`.
3. Execute the returned `CommandInvocation` with
   `viker_core::input::execute_invocation`.
4. Handle any `DeferredAction` in the host app. Deferred actions are operations
   that need platform or async work, such as file open, LSP rename, formatting,
   shell filters, macro playback, or git.
5. Render the editor state using the host UI toolkit.

Minimal Rust sketch:

```rust
use viker_core::editor::document::Document;
use viker_core::editor::{DeferredAction, Editor};
use viker_core::input;
use viker_core::key::{KeyCode, KeyInput};
use viker_vim::keymap;

let mut editor = Editor::new(Document::new_empty());
let key = KeyInput {
    code: KeyCode::Char('i'),
    ctrl: false,
    alt: false,
};

if let Some(invocation) = keymap::map_key(&mut editor, key) {
    if let Some(action) = input::execute_invocation(&mut editor, invocation) {
        match action {
            DeferredAction::OpenFile(path) => {
                // Let the host app decide how paths are resolved and opened.
                println!("open {path}");
            }
            other => {
                println!("host action: {other:?}");
            }
        }
    }
}
```

For egui hosts, use `viker_egui::gui_app::VikerEditor::from_editor` and call
`show_inside(ui)` from your own UI. The example at
`crates/viker-egui/examples/embedded_markdown.rs` shows a complete embedded
editor next to independent application controls.

## Current Limits

- Clipboard integration uses `clipboard-rs` for native text clipboard access on supported platforms.
- LSP workspace edits are applied only to the active file.
- The GUI project sidebar is not rendered in the TUI.
- LSP is active-buffer oriented rather than a persistent multi-server workspace.
