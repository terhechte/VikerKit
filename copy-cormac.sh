#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE="$ROOT/swift/VikerKit"
DEST_ROOT="${1:-"$ROOT/../../Swift/Cormac"}"
DEST="$DEST_ROOT/VikerKit"

if [[ ! -f "$SOURCE/Package.swift" ]]; then
  echo "Missing VikerKit package at $SOURCE" >&2
  exit 1
fi

if [[ ! -d "$SOURCE/VikerKitFFI.xcframework" ]]; then
  echo "Missing VikerKitFFI.xcframework. Run scripts/build-viker-swift-xcframework.sh first." >&2
  exit 1
fi

rm -rf "$DEST"
mkdir -p "$DEST/Sources/VikerKit" "$DEST/Sources/CLibgit2"

cp "$SOURCE/Package.swift" "$DEST/"
rsync -a --exclude ".DS_Store" "$SOURCE/VikerKitFFI.xcframework/" "$DEST/VikerKitFFI.xcframework/"
rsync -a --exclude ".DS_Store" "$SOURCE/Sources/CLibgit2/" "$DEST/Sources/CLibgit2/"
rsync -a \
  --include "*/" \
  --include "*.swift" \
  --exclude "*" \
  "$SOURCE/Sources/VikerKit/" "$DEST/Sources/VikerKit/"

echo "Copied deployable VikerKit package to $DEST"
