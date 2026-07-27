#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_root="${HERDR_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
radar_bin="${HERDR_REPORADAR_BIN:-$plugin_root/target/release/herdr-reporadar}"

workspace=""
preferred_tab=""
preferred_pane=""
focus_policy="no-focus"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --workspace)
      [[ $# -ge 2 ]] || { printf 'RepoRadar: --workspace requires an id\n' >&2; exit 2; }
      workspace="$2"
      shift 2
      ;;
    --tab)
      [[ $# -ge 2 ]] || { printf 'RepoRadar: --tab requires an id\n' >&2; exit 2; }
      preferred_tab="$2"
      shift 2
      ;;
    --target-pane)
      [[ $# -ge 2 ]] || { printf 'RepoRadar: --target-pane requires an id\n' >&2; exit 2; }
      preferred_pane="$2"
      shift 2
      ;;
    --focus)
      focus_policy="focus"
      shift
      ;;
    --no-focus)
      focus_policy="no-focus"
      shift
      ;;
    *)
      printf 'RepoRadar: unknown reconciler argument %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

validate_id() {
  local kind="$1"
  local value="$2"
  if [[ -z "$value" || ${#value} -gt 120 || ! "$value" =~ ^[A-Za-z0-9:_-]+$ ]]; then
    printf 'RepoRadar: invalid %s id %q\n' "$kind" "$value" >&2
    exit 2
  fi
}

validate_id workspace "$workspace"
[[ -z "$preferred_tab" ]] || validate_id tab "$preferred_tab"
[[ -z "$preferred_pane" ]] || validate_id pane "$preferred_pane"

if [[ -z "${HERDR_SOCKET_PATH:-}" || -z "${HERDR_PLUGIN_STATE_DIR:-}" ]]; then
  printf 'RepoRadar: lifecycle context is missing socket or state directory\n' >&2
  exit 1
fi

mkdir -p "$HERDR_PLUGIN_STATE_DIR"
checksum_line="$(printf '%s' "$HERDR_SOCKET_PATH" | cksum)"
session_checksum="${checksum_line%% *}"
lock_dir="$HERDR_PLUGIN_STATE_DIR/reconcile-${session_checksum}-${workspace}.lock"
lock_owned=0
last_layout_error=""

release_lock() {
  local owner=""
  if [[ "$lock_owned" -eq 1 && -f "$lock_dir/owner" ]]; then
    IFS= read -r owner < "$lock_dir/owner" || true
    if [[ "$owner" == "$$" ]]; then
      rm -f -- "$lock_dir/owner"
      rmdir -- "$lock_dir" 2>/dev/null || true
    fi
  fi
}
trap release_lock EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

acquire_lock() {
  local attempt owner current
  for ((attempt = 0; attempt < 40; attempt++)); do
    if mkdir "$lock_dir" 2>/dev/null; then
      printf '%s\n' "$$" > "$lock_dir/owner"
      lock_owned=1
      return 0
    fi
    owner=""
    if [[ -f "$lock_dir/owner" ]]; then
      IFS= read -r owner < "$lock_dir/owner" || true
    fi
    if [[ "$owner" =~ ^[0-9]+$ ]] && ! kill -0 "$owner" 2>/dev/null; then
      current=""
      IFS= read -r current < "$lock_dir/owner" 2>/dev/null || true
      if [[ "$current" == "$owner" ]]; then
        rm -f -- "$lock_dir/owner"
        rmdir -- "$lock_dir" 2>/dev/null || true
      fi
      continue
    fi
    sleep 0.05
  done

  # A process can die between mkdir and writing owner. Recover only after the
  # full wait, when a live owner would long since have populated the file.
  if [[ ! -e "$lock_dir/owner" ]]; then
    rmdir -- "$lock_dir" 2>/dev/null || true
    if mkdir "$lock_dir" 2>/dev/null; then
      printf '%s\n' "$$" > "$lock_dir/owner"
      lock_owned=1
      return 0
    fi
  fi
  return 1
}

if ! acquire_lock; then
  printf 'RepoRadar: timed out waiting for workspace lock %s\n' "$workspace" >&2
  exit 1
fi

close_duplicates() {
  local keeper="$1"
  shift
  local pane
  for pane in "$@"; do
    [[ "$pane" == "$keeper" ]] && continue
    "$herdr_bin" plugin pane close "$pane" >/dev/null 2>&1 || true
  done
}

for ((state_attempt = 1; state_attempt <= 3; state_attempt++)); do
  if ! workspace_json="$("$herdr_bin" workspace get "$workspace" 2>/dev/null)"; then
    exit 0
  fi
  if ! active_tab="$(printf '%s' "$workspace_json" | "$radar_bin" --active-tab "$workspace")"; then
    printf 'RepoRadar: invalid state for workspace %s\n' "$workspace" >&2
    exit 1
  fi

  # A hook may wait behind a newer focus event. Always trust the post-lock
  # active tab rather than moving a pane back to a stale event tab.
  if [[ -n "$preferred_tab" && "$preferred_tab" != "$active_tab" ]]; then
    preferred_pane=""
  fi

  if ! panes_json="$("$herdr_bin" pane list --workspace "$workspace" 2>/dev/null)"; then
    if ((state_attempt < 3)); then sleep 0.1; continue; fi
    printf 'RepoRadar: could not list panes for workspace %s\n' "$workspace" >&2
    exit 1
  fi
  if ! candidate_lines="$(printf '%s' "$panes_json" | "$radar_bin" --candidate-panes "$workspace")"; then
    printf 'RepoRadar: invalid pane state for workspace %s\n' "$workspace" >&2
    exit 1
  fi

  candidate_ids=()
  candidate_tabs=()
  while IFS=$'\t' read -r pane tab; do
    [[ -n "$pane" ]] || continue
    if process_json="$("$herdr_bin" pane process-info --pane "$pane" 2>/dev/null)" \
      && printf '%s' "$process_json" | "$radar_bin" --verify-process >/dev/null 2>&1; then
      candidate_ids+=("$pane")
      candidate_tabs+=("$tab")
    fi
  done <<< "$candidate_lines"
  candidate_count="${#candidate_ids[@]}"

  if [[ -n "$candidate_lines" && $candidate_count -eq 0 && $state_attempt -lt 3 ]]; then
    sleep 0.1
    continue
  fi

  keeper=""
  for ((index = 0; index < candidate_count; index++)); do
    if [[ "${candidate_tabs[$index]}" == "$active_tab" \
      && ( -z "$keeper" || "${candidate_ids[$index]}" < "$keeper" ) ]]; then
      keeper="${candidate_ids[$index]}"
    fi
  done
  if [[ -z "$keeper" && $candidate_count -gt 0 ]]; then
    for pane in "${candidate_ids[@]}"; do
      if [[ -z "$keeper" || "$pane" < "$keeper" ]]; then
        keeper="$pane"
      fi
    done
  fi

  keeper_tab=""
  for ((index = 0; index < candidate_count; index++)); do
    if [[ "${candidate_ids[$index]}" == "$keeper" ]]; then
      keeper_tab="${candidate_tabs[$index]}"
      break
    fi
  done

  if [[ -n "$keeper" && "$keeper_tab" == "$active_tab" ]]; then
    close_duplicates "$keeper" "${candidate_ids[@]}"
    if [[ "$focus_policy" == "focus" ]]; then
      "$herdr_bin" plugin pane focus "$keeper" >/dev/null
    fi
    printf '%s\n' "$keeper"
    exit 0
  fi

  select_args=(--select-target-pane "$workspace" "$active_tab" "$preferred_pane")
  if ((candidate_count > 0)); then
    select_args+=("${candidate_ids[@]}")
  fi
  if ! target="$(printf '%s' "$panes_json" | "$radar_bin" "${select_args[@]}")"; then
    printf 'RepoRadar: could not select a target pane in %s\n' "$workspace" >&2
    exit 1
  fi
  if [[ -z "$target" ]]; then
    if ((state_attempt < 3)); then sleep 0.1; continue; fi
    exit 0
  fi

  if [[ -n "$keeper" ]]; then
    if move_response="$("$herdr_bin" pane move "$keeper" \
      --tab "$active_tab" \
      --target-pane "$target" \
      --split right \
      --ratio 0.18 \
      --no-focus 2>&1)"; then
      close_duplicates "$keeper" "${candidate_ids[@]}"
      if [[ "$focus_policy" == "focus" ]]; then
        "$herdr_bin" plugin pane focus "$keeper" >/dev/null
      fi
      printf '%s\n' "$keeper"
      exit 0
    fi
    last_layout_error="$move_response"
  else
    root=""
    root="$(printf '%s' "$workspace_json" | "$radar_bin" --extract-workspace-checkout 2>/dev/null || true)"
    if [[ -z "$root" ]] && pane_json="$("$herdr_bin" pane get "$target" 2>/dev/null)"; then
      root="$(printf '%s' "$pane_json" | "$radar_bin" --extract-pane-launch-cwd 2>/dev/null || true)"
    fi
    if [[ -z "$root" ]]; then
      if ((state_attempt < 3)); then sleep 0.1; continue; fi
      printf 'RepoRadar: could not determine a stable root for workspace %s\n' "$workspace" >&2
      exit 1
    fi

    open_args=(
      plugin pane open
      --plugin herdr-reporadar
      --entrypoint radar
      --placement split
      --target-pane "$target"
      --direction right
      --env "HERDR_REPORADAR_ROOT=$root"
      --no-focus
    )
    if response="$("$herdr_bin" "${open_args[@]}" 2>&1)" \
      && opened="$(printf '%s' "$response" | "$radar_bin" --extract-opened-pane 2>/dev/null)" \
      && [[ -n "$opened" ]]; then
      "$herdr_bin" pane resize \
        --pane "$opened" \
        --direction right \
        --amount 0.32 >/dev/null 2>&1 || true
      if ((candidate_count > 0)); then
        close_duplicates "$opened" "${candidate_ids[@]}"
      fi
      if [[ "$focus_policy" == "focus" ]]; then
        "$herdr_bin" plugin pane focus "$opened" >/dev/null
      fi
      printf '%s\n' "$opened"
      exit 0
    fi
    last_layout_error="$response"
  fi

  if ((state_attempt < 3)); then
    sleep 0.1
  fi
done

if [[ -n "$last_layout_error" ]]; then
  printf 'RepoRadar: last Herdr layout error: %s\n' "$last_layout_error" >&2
fi
printf 'RepoRadar: reconciliation did not converge for workspace %s\n' "$workspace" >&2
exit 1
