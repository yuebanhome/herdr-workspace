#!/usr/bin/env bash
set -euo pipefail

plugin_root="${HERDR_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
reconciler="$plugin_root/scripts/reconcile.sh"

workspace="${HERDR_WORKSPACE_ID:-}"
if [[ -z "$workspace" ]]; then
  printf 'RepoRadar: manual action has no workspace context\n' >&2
  exit 1
fi

args=(--workspace "$workspace" --focus)
if [[ -n "${HERDR_TAB_ID:-}" ]]; then
  args+=(--tab "$HERDR_TAB_ID")
fi
if [[ -n "${HERDR_PANE_ID:-}" ]]; then
  args+=(--target-pane "$HERDR_PANE_ID")
fi

exec bash "$reconciler" "${args[@]}"
