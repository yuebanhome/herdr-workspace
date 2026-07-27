#!/usr/bin/env bash
set -euo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_root="${HERDR_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
radar_bin="${HERDR_REPORADAR_BIN:-$plugin_root/target/release/herdr-reporadar}"
reconciler="$plugin_root/scripts/reconcile.sh"

reconcile_workspace() {
  local workspace="$1"
  shift
  if ! bash "$reconciler" --workspace "$workspace" --no-focus "$@" >/dev/null; then
    printf 'RepoRadar: failed to reconcile workspace %s for %s\n' \
      "$workspace" "${HERDR_PLUGIN_EVENT:-unknown event}" >&2
  fi
}

if [[ "${HERDR_PLUGIN_EVENT:-}" == "startup" ]]; then
  if ! workspace_json="$("$herdr_bin" workspace list)"; then
    printf 'RepoRadar: could not enumerate workspaces at startup\n' >&2
    exit 1
  fi
  if ! workspace_ids="$(printf '%s' "$workspace_json" | "$radar_bin" --list-workspaces)"; then
    printf 'RepoRadar: invalid workspace list response at startup\n' >&2
    exit 1
  fi
  while IFS= read -r workspace; do
    [[ -n "$workspace" ]] || continue
    reconcile_workspace "$workspace"
  done <<< "$workspace_ids"
  exit 0
fi

case "${HERDR_PLUGIN_EVENT:-}" in
  workspace.created|workspace.focused|tab.focused) ;;
  *)
    printf 'RepoRadar: ignored unsupported plugin event %s\n' \
      "${HERDR_PLUGIN_EVENT:-<missing>}" >&2
    exit 0
    ;;
esac

workspace="${HERDR_WORKSPACE_ID:-}"
if [[ -z "$workspace" ]]; then
  printf 'RepoRadar: %s event has no workspace context\n' "$HERDR_PLUGIN_EVENT" >&2
  exit 1
fi

event_args=()
if [[ -n "${HERDR_TAB_ID:-}" ]]; then
  event_args+=(--tab "$HERDR_TAB_ID")
fi
if [[ -n "${HERDR_PANE_ID:-}" ]]; then
  event_args+=(--target-pane "$HERDR_PANE_ID")
fi
reconcile_workspace "$workspace" "${event_args[@]}"
