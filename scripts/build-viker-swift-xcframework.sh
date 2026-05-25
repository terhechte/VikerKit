#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_PACKAGE="viker-swift"
LIB_NAME="viker_swift"
FFI_MODULE_NAME="VikerKitFFI"
PACKAGE_DIR="$ROOT/swift/VikerKit"
BUILD_DIR="$ROOT/target/viker-swift"
RELEASE_DIR="release"

DEVICE_TARGET="aarch64-apple-ios"
SIM_TARGETS=("aarch64-apple-ios-sim")
MACOS_TARGETS=("aarch64-apple-darwin")

if [[ "${VIKER_SWIFT_INCLUDE_X86_64_SIM:-0}" == "1" ]]; then
  SIM_TARGETS+=("x86_64-apple-ios")
fi

if [[ "${VIKER_SWIFT_INCLUDE_X86_64_MACOS:-1}" != "0" ]]; then
  MACOS_TARGETS+=("x86_64-apple-darwin")
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
  echo "xcodebuild is required to create the xcframework" >&2
  exit 1
fi

if ! command -v lipo >/dev/null 2>&1; then
  echo "lipo is required to create universal Apple libraries" >&2
  exit 1
fi

fix_static_library_modulemap() {
  local modulemap="$1/module.modulemap"
  if [[ ! -f "$modulemap" ]]; then
    echo "Missing generated module map: $modulemap" >&2
    exit 1
  fi
  perl -0pi -e "s/\\Aframework module \\Q$FFI_MODULE_NAME\\E /module $FFI_MODULE_NAME /" "$modulemap"
}

echo "Installing Rust Apple targets"
rustup target add "$DEVICE_TARGET" "${SIM_TARGETS[@]}" "${MACOS_TARGETS[@]}"

echo "Building Rust static libraries"
cargo build -p "$CRATE_PACKAGE" --release --target "$DEVICE_TARGET"
for target in "${SIM_TARGETS[@]}"; do
  cargo build -p "$CRATE_PACKAGE" --release --target "$target"
done
for target in "${MACOS_TARGETS[@]}"; do
  cargo build -p "$CRATE_PACKAGE" --release --target "$target"
done

DEVICE_LIB="$ROOT/target/$DEVICE_TARGET/$RELEASE_DIR/lib$LIB_NAME.a"
SIM_LIB="$BUILD_DIR/lib$LIB_NAME-simulator.a"
MACOS_LIB="$BUILD_DIR/lib$LIB_NAME-macos.a"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

if [[ "${#SIM_TARGETS[@]}" -eq 1 ]]; then
  cp "$ROOT/target/${SIM_TARGETS[0]}/$RELEASE_DIR/lib$LIB_NAME.a" "$SIM_LIB"
else
  SIM_INPUTS=()
  for target in "${SIM_TARGETS[@]}"; do
    SIM_INPUTS+=("$ROOT/target/$target/$RELEASE_DIR/lib$LIB_NAME.a")
  done
  lipo -create "${SIM_INPUTS[@]}" -output "$SIM_LIB"
fi

if [[ "${#MACOS_TARGETS[@]}" -eq 1 ]]; then
  cp "$ROOT/target/${MACOS_TARGETS[0]}/$RELEASE_DIR/lib$LIB_NAME.a" "$MACOS_LIB"
else
  MACOS_INPUTS=()
  for target in "${MACOS_TARGETS[@]}"; do
    MACOS_INPUTS+=("$ROOT/target/$target/$RELEASE_DIR/lib$LIB_NAME.a")
  done
  lipo -create "${MACOS_INPUTS[@]}" -output "$MACOS_LIB"
fi

GEN_SWIFT_DIR="$BUILD_DIR/generated-swift"
DEVICE_HEADERS="$BUILD_DIR/device-headers"
SIM_HEADERS="$BUILD_DIR/simulator-headers"
MACOS_HEADERS="$BUILD_DIR/macos-headers"
mkdir -p "$GEN_SWIFT_DIR" "$DEVICE_HEADERS" "$SIM_HEADERS" "$MACOS_HEADERS"

echo "Generating UniFFI Swift bindings"
cargo run -p "$CRATE_PACKAGE" --bin uniffi-bindgen-swift -- \
  "$DEVICE_LIB" "$GEN_SWIFT_DIR" --swift-sources
cargo run -p "$CRATE_PACKAGE" --bin uniffi-bindgen-swift -- \
  "$DEVICE_LIB" "$DEVICE_HEADERS" --headers
cargo run -p "$CRATE_PACKAGE" --bin uniffi-bindgen-swift -- \
  "$DEVICE_LIB" "$DEVICE_HEADERS" --xcframework --modulemap \
  --module-name "$FFI_MODULE_NAME" --modulemap-filename module.modulemap
fix_static_library_modulemap "$DEVICE_HEADERS"
cp -R "$DEVICE_HEADERS/." "$SIM_HEADERS/"
cp -R "$DEVICE_HEADERS/." "$MACOS_HEADERS/"

echo "Creating Swift package layout"
rm -rf "$PACKAGE_DIR/$FFI_MODULE_NAME.xcframework"
mkdir -p "$PACKAGE_DIR/Sources/VikerKit"
find "$PACKAGE_DIR/Sources/VikerKit" -type f -name '*.swift' -delete
find "$GEN_SWIFT_DIR" -maxdepth 1 -type f -name '*.swift' -exec cp {} "$PACKAGE_DIR/Sources/VikerKit/" \;

echo "Creating xcframework"
xcodebuild -create-xcframework \
  -library "$DEVICE_LIB" -headers "$DEVICE_HEADERS" \
  -library "$SIM_LIB" -headers "$SIM_HEADERS" \
  -library "$MACOS_LIB" -headers "$MACOS_HEADERS" \
  -output "$PACKAGE_DIR/$FFI_MODULE_NAME.xcframework"

echo "Built $PACKAGE_DIR/$FFI_MODULE_NAME.xcframework"
echo "Generated Swift sources in $PACKAGE_DIR/Sources/VikerKit"
