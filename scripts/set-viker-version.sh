#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 <version>" >&2
  echo "Example: $0 0.2.0" >&2
}

if [[ $# -ne 1 ]]; then
  usage
  exit 64
fi

NEW_VERSION="$1"
SEMVER_RE='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?(\+[0-9A-Za-z][0-9A-Za-z.-]*)?$'

if [[ ! "$NEW_VERSION" =~ $SEMVER_RE ]]; then
  echo "error: '$NEW_VERSION' is not a valid semver version" >&2
  usage
  exit 64
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/Cargo.toml"
CARGO_LOCK="$ROOT/Cargo.lock"
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

if [[ "$CURRENT_VERSION" == "$NEW_VERSION" ]]; then
  echo "Viker version is already $NEW_VERSION."
else
  echo "Updated Viker version: $CURRENT_VERSION -> $NEW_VERSION"
fi

if [[ ${#workspace_package_names[@]} -gt 0 ]]; then
  echo "Workspace crates: ${workspace_package_names[*]}"
fi

echo "Next: run scripts/build-viker-swift-xcframework.sh, commit the changes, then tag the release as v$NEW_VERSION for SwiftPM."
