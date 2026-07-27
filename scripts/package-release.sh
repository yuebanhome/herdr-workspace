#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  printf 'usage: %s <version> <target> <binary> <output-dir>\n' "$0" >&2
  exit 2
fi

version="$1"
target="$2"
binary="$3"
output_dir="$4"

if [[ ! "$version" =~ ^[0-9A-Za-z.+-]+$ ]]; then
  printf 'invalid release version: %s\n' "$version" >&2
  exit 2
fi
if [[ ! "$target" =~ ^[0-9A-Za-z_-]+$ ]]; then
  printf 'invalid release target: %s\n' "$target" >&2
  exit 2
fi
if [[ ! -f "$binary" || ! -x "$binary" ]]; then
  printf 'release binary is missing or not executable: %s\n' "$binary" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
staging="$(mktemp -d "${TMPDIR:-/tmp}/herdr-reporadar-package.XXXXXX")"
trap 'rm -rf -- "$staging"' EXIT

bundle="$staging/herdr-reporadar"
mkdir -p "$bundle/scripts" "$bundle/target/release"
cp "$repo_root/Cargo.lock" "$bundle/Cargo.lock"
cp "$repo_root/Cargo.toml" "$bundle/Cargo.toml"
cp "$repo_root/LICENSE" "$bundle/LICENSE"
cp "$repo_root/README.md" "$bundle/README.md"
cp "$repo_root/herdr-plugin.toml" "$bundle/herdr-plugin.toml"
cp -R "$repo_root/src" "$bundle/src"
install -m 0755 "$repo_root/scripts/open.sh" "$bundle/scripts/open.sh"
install -m 0755 "$binary" "$bundle/target/release/herdr-reporadar"

archive="$output_dir/herdr-reporadar-v${version}-${target}.tar.gz"
tar -C "$staging" -czf "$archive" herdr-reporadar

tar -tzf "$archive" | grep -qx 'herdr-reporadar/herdr-plugin.toml'
tar -tzf "$archive" | grep -qx 'herdr-reporadar/target/release/herdr-reporadar'
printf '%s\n' "$archive"
