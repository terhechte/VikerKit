# Wasm / TypeScript Feasibility

This is a separate feasibility pass for compiling Viker into a TypeScript
package.

## Commands Checked

```bash
rustup target list --installed
command -v wasm-pack || true
command -v wasm-bindgen || true
cargo check -p viker-core --target wasm32-unknown-unknown
cargo check -p viker-vim --target wasm32-unknown-unknown
```

Environment result:

- `wasm32-unknown-unknown` is installed.
- `wasm-pack` is not installed.
- The standalone `wasm-bindgen` CLI is not installed.

## Current Result

`viker-core` does not currently compile for `wasm32-unknown-unknown`.
`viker-vim` also does not compile, because it depends on `viker-core`.

The first hard failure is `tokio` through `mio`:

```text
error: This wasm target is unsupported by mio. If using Tokio, disable the net feature.
```

That comes from the workspace `tokio = { features = ["full"] }` dependency,
which enables native networking support. The browser/TypeScript wasm target
cannot use that dependency shape.

## What Can Be Ported

### Likely Portable Now After Refactoring

These parts are conceptually wasm-friendly:

- Vim key mapping.
- `VimCore` editing state.
- Motions, operators, text objects, counts, registers, marks, macros, undo/redo,
  search, and repeat behavior.
- Frontend-neutral key types.
- Basic snapshots for text, cursor, mode, selection, and pending effects.

These pieces mostly need `regex`, `ropey`, small data types, and deterministic
editor logic.

### Needs Feature Gating Or Separation

These core features are useful but need a wasm-specific dependency boundary:

- Syntax highlighting: tree-sitter can be made to work in wasm, but the current
  native tree-sitter dependency set is not packaged as a browser TypeScript
  surface.
- Project file scanning: `ignore` and filesystem walking are native concepts.
  A browser package should accept a host-provided file list instead.
- Fuzzy matching: `skim` is a terminal-oriented dependency. Browser use should
  expose a small matching API or accept host-side filtering.

### Not Browser-Wasm Features

These should stay native or be replaced by host callbacks:

- LSP process transport.
- External formatters through `std::process::Command`.
- Git operations through `git2` / libgit2.
- Native clipboard access through `clipboard-rs`.
- Native file opening and filesystem writes.

## Recommended TypeScript Package Scope

The practical first package should be a Vim/editing package, not the full app
core:

```text
@viker/vim
  VikerEditor
  processKey(key: KeyInput): Effect[]
  text(): string
  setText(text: string): void
  cursor(): Cursor
  mode(): Mode
  selection(): Selection | null
  registerSummaries(): RegisterSummary[]
```

Host apps would provide file I/O, clipboard, LSP, formatting, git, and rendering.
The wasm package would emit effects for those external operations instead of
performing them.

## Refactor Needed

To build that package cleanly, split the current native core into a wasm-clean
editing layer and native service layers:

1. Create a small crate for shared frontend-neutral types:
   `KeyInput`, `KeyCode`, `Mode`, `Command`, `Motion`, `Position`, `View`,
   document/history helpers, and display/wrap helpers.
2. Move `VimCore` and the keymap onto that wasm-clean crate.
3. Keep `viker-core` as the native/editor-app crate that adds LSP,
   tree-sitter, git, filesystem search, formatters, and native clipboard.
4. Add a new `viker-wasm` crate with `wasm-bindgen` and TypeScript-facing
   records.
5. Build with `wasm-pack build crates/viker-wasm --target bundler` once
   `wasm-pack` is installed.

## Bottom Line

The full `viker-core` cannot currently become a TypeScript package without
feature-gating or moving native services out of the compile path. The best
near-term wasm package is the Vim binding and editing state machine. That is the
part most aligned with Viker's goal of embedding Vim bindings anywhere.
