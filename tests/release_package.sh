#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/herdr-reporadar-package-test.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT

fake_binary="$test_root/herdr-reporadar"
printf '#!/usr/bin/env bash\nexit 0\n' > "$fake_binary"
chmod 0755 "$fake_binary"

archive="$(
  "$repo_root/scripts/package-release.sh" \
    9.8.7 \
    test-target \
    "$fake_binary" \
    "$test_root/dist"
)"

[[ "$archive" == "$test_root/dist/herdr-reporadar-v9.8.7-test-target.tar.gz" ]]
[[ -f "$archive" ]]
mkdir -p "$test_root/extracted"
tar -C "$test_root/extracted" -xzf "$archive"

bundle="$test_root/extracted/herdr-reporadar"
[[ -x "$bundle/target/release/herdr-reporadar" ]]
cmp "$fake_binary" "$bundle/target/release/herdr-reporadar"
[[ -f "$bundle/herdr-plugin.toml" ]]
[[ -f "$bundle/Cargo.lock" ]]
[[ -f "$bundle/src/main.rs" ]]
[[ -x "$bundle/scripts/open.sh" ]]
[[ -x "$bundle/scripts/auto-open.sh" ]]
[[ -x "$bundle/scripts/reconcile.sh" ]]
