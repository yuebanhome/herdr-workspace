#!/usr/bin/env bash
set -euo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_root="${HERDR_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
radar_bin="$plugin_root/target/release/herdr-reporadar"

workspace_args=()
if [[ -n "${HERDR_WORKSPACE_ID:-}" ]]; then
  workspace_args=(--workspace "$HERDR_WORKSPACE_ID")
fi

existing="$("$herdr_bin" pane list "${workspace_args[@]}" 2>/dev/null | "$radar_bin" --find-pane || true)"
if [[ -n "$existing" ]]; then
  exec "$herdr_bin" plugin pane focus "$existing"
fi

root=""
if [[ -n "${HERDR_PANE_ID:-}" ]]; then
  root="$("$herdr_bin" pane get "$HERDR_PANE_ID" 2>/dev/null | "$radar_bin" --extract-pane-cwd || true)"
fi
if [[ -z "$root" ]]; then
  root="$PWD"
fi

response="$("$herdr_bin" plugin pane open \
  --plugin herdr-reporadar \
  --entrypoint radar \
  --placement split \
  --direction right \
  --env "HERDR_REPORADAR_ROOT=$root" \
  --no-focus)"

opened="$(printf '%s' "$response" | "$radar_bin" --extract-opened-pane || true)"
if [[ -n "$opened" ]]; then
  "$herdr_bin" pane resize \
    --pane "$opened" \
    --direction right \
    --amount 0.32 >/dev/null 2>&1 || true
fi

printf '%s\n' "$response"
