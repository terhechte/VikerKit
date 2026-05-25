# viker-swift

`viker-swift` exposes the frontend-neutral Viker editor through UniFFI so a
Swift application can own rendering, layout, input translation, and native OS
integration while Rust keeps the editor state machine and Vim behavior.

The generated Swift module is `VikerKit`; the low-level C FFI module packaged
in the xcframework is `VikerKitFFI`.

## Local Checks

```bash
cargo test -p viker-swift
cargo build -p viker-swift
cargo run -p viker-swift --bin uniffi-bindgen-swift -- \
  target/debug/libviker_swift.a target/viker-swift-bindgen-smoke \
  --swift-sources --headers
```

## Build The Swift Package

```bash
scripts/build-viker-swift-xcframework.sh
```

This produces an xcframework with iOS device, iOS simulator, and macOS slices:

- `swift/VikerKit/VikerKitFFI.xcframework`
- `swift/VikerKit/Sources/VikerKit/VikerKit.swift`

The script rewrites UniFFI's generated static-library module maps from
`framework module VikerKitFFI` to `module VikerKitFFI`, which lets Swift see
the generated C symbols such as `RustBuffer` from the static-library
xcframework slices.

Add `swift/VikerKit` as a local Swift package in Xcode, then `import
VikerKit`.

Set `VIKER_SWIFT_INCLUDE_X86_64_SIM=1` when you need an Intel simulator slice
in addition to the Apple Silicon simulator slice. The macOS slice includes both
Apple Silicon and Intel by default; set `VIKER_SWIFT_INCLUDE_X86_64_MACOS=0`
to build only the Apple Silicon macOS slice.
