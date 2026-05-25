# VikerExample

`VikerExample` is a small macOS editor application that embeds the reusable
`VikerEditorComponent` from `VikerKit`. It is intentionally packaged separately
from `VikerKit` so the example only depends on the public Swift package API.

## Requirements

- macOS 13 or newer
- Swift 5.9 or newer
- A generated `swift/VikerKit/VikerKitFFI.xcframework`
- The native dependencies declared by `VikerKit` available to SwiftPM. On
  macOS, the current package expects `pkg-config` to find `libgit2`.

If the xcframework is missing or stale, regenerate `VikerKit` from the repo
root:

```bash
scripts/build-viker-swift-xcframework.sh
```

## Build

From this directory:

```bash
swift build
```

From the repo root:

```bash
swift build --package-path swift/Example
```

## Run

Open the default sample file:

```bash
swift run --package-path swift/Example VikerExample
```

Open a specific file:

```bash
swift run --package-path swift/Example VikerExample /path/to/file.swift
```

The example links `VikerKit` through a local package dependency at
`../VikerKit`. It does not depend directly on the generated FFI binary target
or declare additional Swift package dependencies.

The editor view is configured with `VikerEditorConfiguration`, which controls
the color scheme, status bar, top toolbar items, LSP startup, initial editor
mode, insert-only mode, and line-number gutter.
