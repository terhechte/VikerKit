#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 <version> [vikerkit-ffi-checksum]" >&2
  echo "Example: $0 0.2.0 fc973038eb46748a08bf28388697f04fde7239e69590a88156a80920ecb07ae7" >&2
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 64
fi

NEW_VERSION="$1"
VIKERKIT_FFI_CHECKSUM="${2:-}"
SEMVER_RE='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?(\+[0-9A-Za-z][0-9A-Za-z.-]*)?$'
SHA256_RE='^[0-9a-fA-F]{64}$'

if [[ ! "$NEW_VERSION" =~ $SEMVER_RE ]]; then
  echo "error: '$NEW_VERSION' is not a valid semver version" >&2
  usage
  exit 64
fi

if [[ -n "$VIKERKIT_FFI_CHECKSUM" && ! "$VIKERKIT_FFI_CHECKSUM" =~ $SHA256_RE ]]; then
  echo "error: '$VIKERKIT_FFI_CHECKSUM' is not a valid SwiftPM checksum" >&2
  usage
  exit 64
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/Cargo.toml"
CARGO_LOCK="$ROOT/Cargo.lock"
PACKAGE_SWIFT="$ROOT/Package.swift"
README="$ROOT/README.md"

CURRENT_VERSION="$(
  awk '
    $0 == "[workspace.package]" { in_section = 1; next }
    in_section && /^\[/ { exit }
    in_section && /^version = / {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$CARGO_TOML"
)"

if [[ -z "$CURRENT_VERSION" ]]; then
  echo "error: could not find [workspace.package] version in $CARGO_TOML" >&2
  exit 1
fi

workspace_package_names=()
while IFS= read -r manifest; do
  package_name="$(
    awk '
      $0 == "[package]" { in_section = 1; next }
      in_section && /^\[/ { exit }
      in_section && /^name = / {
        gsub(/"/, "", $3)
        print $3
        exit
      }
    ' "$manifest"
  )"
  if [[ -n "$package_name" ]]; then
    workspace_package_names+=("$package_name")
  fi
done < <(find "$ROOT/crates" -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

export NEW_VERSION

perl -0pi -e '
  my $version = $ENV{"NEW_VERSION"};
  my $count = s/(\[workspace\.package\][^\[]*?^version = ")[^"]+(")/$1$version$2/ms;
  die "error: expected to update exactly one workspace package version\n" unless $count == 1;
' "$CARGO_TOML"

if [[ -f "$CARGO_LOCK" && ${#workspace_package_names[@]} -gt 0 ]]; then
  WORKSPACE_PACKAGE_NAMES="$(printf '%s\n' "${workspace_package_names[@]}")"
  export WORKSPACE_PACKAGE_NAMES

  perl -0pi -e '
    my $version = $ENV{"NEW_VERSION"};
    my @names = grep { length } split /\n/, $ENV{"WORKSPACE_PACKAGE_NAMES"};
    my $name_pattern = join "|", map { quotemeta } @names;
    my $count = s/(\[\[package\]\]\nname = "(?:$name_pattern)"\nversion = ")[^"]+(")/$1$version$2/g;
    warn "warning: no workspace package versions were updated in Cargo.lock\n" if $count == 0;
  ' "$CARGO_LOCK"
fi

if [[ -f "$README" ]]; then
  perl -0pi -e '
    my $version = $ENV{"NEW_VERSION"};
    my $count = s/(\.package\(url: "[^"]*VikerKit[^"]*", from: ")[^"]+(")/$1$version$2/g;
    warn "warning: no VikerKit SwiftPM dependency version was updated in README.md\n" if $count == 0;
  ' "$README"
fi

if [[ -f "$PACKAGE_SWIFT" ]]; then
  perl -0pi -e '
    my $version = $ENV{"NEW_VERSION"};
    my $count = s#(https://github\.com/terhechte/VikerKit/releases/download/)[^/"]+(/VikerKitFFI\.xcframework\.zip)#$1$version$2#g;
    warn "warning: no VikerKitFFI release URL was updated in Package.swift\n" if $count == 0;
  ' "$PACKAGE_SWIFT"

  if [[ -n "$VIKERKIT_FFI_CHECKSUM" ]]; then
    export VIKERKIT_FFI_CHECKSUM
    perl -0pi -e '
      my $checksum = $ENV{"VIKERKIT_FFI_CHECKSUM"};
      my $count = s/(checksum: ")[0-9a-fA-F]{64}(")/$1$checksum$2/g;
      warn "warning: no VikerKitFFI checksum was updated in Package.swift\n" if $count == 0;
    ' "$PACKAGE_SWIFT"
  fi
fi

if [[ "$CURRENT_VERSION" == "$NEW_VERSION" ]]; then
  echo "Viker version is already $NEW_VERSION."
else
  echo "Updated Viker version: $CURRENT_VERSION -> $NEW_VERSION"
fi

if [[ ${#workspace_package_names[@]} -gt 0 ]]; then
  echo "Workspace crates: ${workspace_package_names[*]}"
fi

if [[ -z "$VIKERKIT_FFI_CHECKSUM" ]]; then
  echo "Next: run scripts/build-viker-swift-xcframework.sh, zip VikerKitFFI.xcframework, compute its SwiftPM checksum, and update Package.swift."
else
  echo "Updated VikerKitFFI checksum in Package.swift."
fi

echo "Then commit the changes and tag the release as $NEW_VERSION for SwiftPM."
