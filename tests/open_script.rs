#![cfg(unix)]

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::{TempDir, tempdir};

const FAKE_HERDR: &str = r#"#!/usr/bin/env bash
set -euo pipefail

state="$FAKE_HERDR_STATE"
log="$state/calls.log"
line="CALL"
for argument in "$@"; do line+=" $argument"; done
printf '%s\n' "$line" >> "$log"
if [[ "${FAKE_CAPTURE_LOCKS:-0}" == 1 ]]; then
  for lock in "$HERDR_PLUGIN_STATE_DIR"/*.lock; do
    [[ -d "$lock" ]] || continue
    printf 'LOCK %s\n' "$(basename "$lock")" >> "$log"
  done
fi

workspace_row() {
  awk -F '\t' -v wanted="$1" '$1 == wanted { print; exit }' "$state/workspaces.tsv"
}

pane_row() {
  awk -F '\t' -v wanted="$1" '$1 == wanted { print; exit }' "$state/panes.tsv"
}

if [[ "$1 $2" == "workspace list" ]]; then
  printf '%s' '{"result":{"workspaces":['
  separator=""
  while IFS=$'\t' read -r workspace tab checkout; do
    [[ -n "$workspace" ]] || continue
    printf '%s{"workspace_id":"%s","active_tab_id":"%s"}' "$separator" "$workspace" "$tab"
    separator=,
  done < "$state/workspaces.tsv"
  printf '%s\n' ']}}'
elif [[ "$1 $2" == "workspace get" ]]; then
  row="$(workspace_row "$3")"
  [[ -n "$row" ]] || exit 1
  IFS=$'\t' read -r workspace tab checkout <<< "$row"
  if [[ "$checkout" == - ]]; then
    printf '{"result":{"workspace":{"workspace_id":"%s","active_tab_id":"%s"}}}\n' "$workspace" "$tab"
  else
    printf '{"result":{"workspace":{"workspace_id":"%s","active_tab_id":"%s","worktree":{"checkout_path":"%s"}}}}\n' "$workspace" "$tab" "$checkout"
  fi
elif [[ "$1 $2" == "pane list" ]]; then
  workspace=""
  shift 2
  while [[ $# -gt 0 ]]; do
    case "$1" in --workspace) workspace="$2"; shift 2;; *) shift;; esac
  done
  printf '%s' '{"result":{"panes":['
  separator=""
  while IFS=$'\t' read -r pane pane_workspace tab focused title process cwd; do
    [[ "$pane_workspace" == "$workspace" ]] || continue
    printf '%s{"pane_id":"%s","workspace_id":"%s","tab_id":"%s","focused":%s,"cwd":"%s"' \
      "$separator" "$pane" "$pane_workspace" "$tab" "$focused" "$cwd"
    if [[ "$title" != - ]]; then printf ',"label":"%s"' "$title"; fi
    printf '}'
    separator=,
  done < "$state/panes.tsv"
  printf '%s\n' ']}}'
elif [[ "$1 $2" == "pane get" ]]; then
  row="$(pane_row "$3")"
  [[ -n "$row" ]] || exit 1
  IFS=$'\t' read -r pane workspace tab focused title process cwd <<< "$row"
  printf '{"result":{"pane":{"pane_id":"%s","workspace_id":"%s","tab_id":"%s","cwd":"%s","foreground_cwd":"%s"}}}\n' \
    "$pane" "$workspace" "$tab" "$cwd" "${FAKE_FOREGROUND_CWD:-$cwd}"
elif [[ "$1 $2" == "pane process-info" ]]; then
  pane=""
  shift 2
  while [[ $# -gt 0 ]]; do
    case "$1" in --pane) pane="$2"; shift 2;; *) shift;; esac
  done
  row="$(pane_row "$pane")"
  [[ -n "$row" ]] || exit 1
  IFS=$'\t' read -r pane workspace tab focused title process cwd <<< "$row"
  if [[ "$process" == radar ]]; then
    printf '%s\n' '{"result":{"process_info":{"foreground_processes":[{"name":"herdr-reporadar","argv0":"herdr-reporadar","argv":["/bundle/herdr-reporadar"]}]}}}'
  else
    printf '%s\n' '{"result":{"process_info":{"foreground_processes":[{"name":"bash","argv0":"bash","argv":["bash","-c","echo herdr-reporadar"],"cmdline":"bash -c echo herdr-reporadar"}]}}}'
  fi
elif [[ "$1 $2 $3" == "plugin pane open" ]]; then
  shift 3
  workspace=""
  workspace_option=0
  target=""
  root=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --workspace) workspace="$2"; workspace_option=1; shift 2;;
      --target-pane) target="$2"; shift 2;;
      --env) root="${2#HERDR_REPORADAR_ROOT=}"; shift 2;;
      *) shift;;
    esac
  done
  [[ "$workspace_option" -eq 0 ]] || exit 2
  row="$(pane_row "$target")"
  [[ -n "$row" ]] || exit 1
  IFS=$'\t' read -r ignored ignored_workspace tab ignored_focused ignored_title ignored_process ignored_cwd <<< "$row"
  workspace="$ignored_workspace"
  if [[ "${FAKE_FAIL_FIRST_OPEN:-0}" == 1 && ! -e "$state/open-failed" ]]; then
    touch "$state/open-failed"
    awk -F '\t' -v target="$target" '$1 != target' "$state/panes.tsv" > "$state/panes.tmp.$$"
    mv "$state/panes.tmp.$$" "$state/panes.tsv"
    replacement="${workspace}:pZ"
    tab="$(workspace_row "$workspace" | awk -F '\t' '{print $2}')"
    printf '%s\t%s\t%s\ttrue\t-\tother\t%s\n' "$replacement" "$workspace" "$tab" "$root" >> "$state/panes.tsv"
    exit 1
  fi
  pane="${workspace}:pR"
  if [[ -z "$(pane_row "$pane")" ]]; then
    printf '%s\t%s\t%s\tfalse\tRepoRadar\tradar\t%s\n' "$pane" "$workspace" "$tab" "$root" >> "$state/panes.tsv"
  fi
  printf 'OPEN %s %s\n' "$workspace" "$root" >> "$log"
  printf '{"result":{"plugin_pane":{"pane":{"pane_id":"%s"}}}}\n' "$pane"
elif [[ "$1 $2" == "pane move" ]]; then
  pane="$3"
  shift 3
  tab=""
  workspace=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --tab) tab="$2"; shift 2;;
      --workspace) workspace="$2"; shift 2;;
      *) shift;;
    esac
  done
  [[ -z "$workspace" ]] || exit 2
  awk -F '\t' -v OFS='\t' -v pane="$pane" -v tab="$tab" \
    '$1 == pane {$3 = tab} {print}' "$state/panes.tsv" > "$state/panes.tmp.$$"
  mv "$state/panes.tmp.$$" "$state/panes.tsv"
elif [[ "$1 $2 $3" == "plugin pane close" ]]; then
  pane="$4"
  awk -F '\t' -v pane="$pane" '$1 != pane' "$state/panes.tsv" > "$state/panes.tmp.$$"
  mv "$state/panes.tmp.$$" "$state/panes.tsv"
elif [[ "$1 $2 $3" == "plugin pane focus" || "$1 $2" == "pane resize" ]]; then
  :
else
  printf 'unsupported fake Herdr command: %s\n' "$line" >&2
  exit 2
fi
"#;

struct Fixture {
    _temp: TempDir,
    state: PathBuf,
    fake_herdr: PathBuf,
}

impl Fixture {
    fn new(workspaces: &str, panes: &str) -> Self {
        let temp = tempdir().unwrap();
        let state = temp.path().join("state");
        fs::create_dir_all(state.join("locks")).unwrap();
        fs::write(state.join("workspaces.tsv"), workspaces).unwrap();
        fs::write(state.join("panes.tsv"), panes).unwrap();
        fs::write(state.join("calls.log"), "").unwrap();
        let fake_herdr = temp.path().join("herdr");
        fs::write(&fake_herdr, FAKE_HERDR).unwrap();
        let mut permissions = fs::metadata(&fake_herdr).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_herdr, permissions).unwrap();
        Self {
            _temp: temp,
            state,
            fake_herdr,
        }
    }

    fn command(&self, script: &str) -> Command {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let mut command = Command::new("bash");
        command
            .arg(format!("{manifest_dir}/scripts/{script}"))
            .current_dir(manifest_dir)
            .env("FAKE_HERDR_STATE", &self.state)
            .env("HERDR_BIN_PATH", &self.fake_herdr)
            .env("HERDR_PLUGIN_ROOT", manifest_dir)
            .env("HERDR_REPORADAR_BIN", env!("CARGO_BIN_EXE_herdr-reporadar"))
            .env("HERDR_SOCKET_PATH", "/tmp/reporadar-test.sock")
            .env("HERDR_PLUGIN_STATE_DIR", self.state.join("locks"))
            .env_remove("HERDR_TAB_ID")
            .env_remove("HERDR_PANE_ID")
            .env_remove("HERDR_WORKSPACE_ID")
            .env_remove("HERDR_PLUGIN_EVENT");
        command
    }

    fn event(&self, event: &str, workspace: &str, tab: &str, pane: &str) -> Output {
        self.event_command(event, workspace, tab, pane)
            .output()
            .unwrap()
    }

    fn event_command(&self, event: &str, workspace: &str, tab: &str, pane: &str) -> Command {
        let mut command = self.command("auto-open.sh");
        command
            .env("HERDR_PLUGIN_EVENT", event)
            .env("HERDR_WORKSPACE_ID", workspace)
            .env("HERDR_TAB_ID", tab)
            .env("HERDR_PANE_ID", pane);
        command
    }

    fn log(&self) -> String {
        fs::read_to_string(self.state.join("calls.log")).unwrap()
    }

    fn panes(&self) -> String {
        fs::read_to_string(self.state.join("panes.tsv")).unwrap()
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn automatic_event_opens_in_the_explicit_workspace_without_focus() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let output = fixture.event("workspace.focused", "w1", "w1:t1", "w1:p1");
    assert_success(&output);

    let log = fixture.log();
    assert!(log.contains("CALL plugin pane open"));
    assert!(log.contains("--target-pane w1:p1"));
    assert!(!log.contains("CALL plugin pane open --workspace"));
    assert!(log.contains("--env HERDR_REPORADAR_ROOT=/workspace --no-focus"));
    assert!(!log.contains("CALL plugin pane focus"));
}

#[test]
fn manual_action_reconciles_then_focuses_the_sidebar() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let mut command = fixture.command("open.sh");
    let output = command
        .env("HERDR_WORKSPACE_ID", "w1")
        .env("HERDR_TAB_ID", "w1:t1")
        .env("HERDR_PANE_ID", "w1:p1")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(fixture.log().contains("CALL plugin pane focus w1:pR"));
}

#[test]
fn existing_sidebar_is_moved_to_the_active_tab_instead_of_reopened() {
    let fixture = Fixture::new(
        "w1\tw1:t2\t-\n",
        concat!(
            "w1:p1\tw1\tw1:t2\ttrue\t-\tother\t/workspace\n",
            "w1:pR\tw1\tw1:t1\tfalse\tRepoRadar\tradar\t/workspace\n"
        ),
    );
    let output = fixture.event("tab.focused", "w1", "w1:t2", "w1:p1");
    assert_success(&output);

    let log = fixture.log();
    assert!(log.contains("CALL pane move w1:pR"));
    assert!(log.contains("--tab w1:t2 --target-pane w1:p1 --split right --ratio 0.18 --no-focus"));
    assert!(!log.contains("CALL plugin pane open"));
    assert!(fixture.panes().contains("w1:pR\tw1\tw1:t2"));
}

#[test]
fn startup_reconciles_all_restored_workspaces_sequentially() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\nw2\tw2:t1\t-\n",
        concat!(
            "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/one\n",
            "w2:p1\tw2\tw2:t1\ttrue\t-\tother\t/two\n"
        ),
    );
    let output = fixture
        .command("auto-open.sh")
        .env("HERDR_PLUGIN_EVENT", "startup")
        .output()
        .unwrap();
    assert_success(&output);
    let log = fixture.log();
    assert_eq!(log.matches("CALL plugin pane open").count(), 2);
    assert!(log.contains("OPEN w1 /one"));
    assert!(log.contains("OPEN w2 /two"));
}

#[test]
fn concurrent_events_converge_on_one_sidebar() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let mut first = fixture.event_command("workspace.created", "w1", "w1:t1", "w1:p1");
    let mut second = fixture.event_command("workspace.focused", "w1", "w1:t1", "w1:p1");
    let first = first.spawn().unwrap();
    let second = second.spawn().unwrap();
    assert!(first.wait_with_output().unwrap().status.success());
    assert!(second.wait_with_output().unwrap().status.success());
    assert_eq!(fixture.log().matches("CALL plugin pane open").count(), 1);
}

#[test]
fn title_collision_is_not_treated_as_a_plugin_owned_pane() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\tRepoRadar\tother\t/workspace\n",
    );
    let output = fixture.event("workspace.focused", "w1", "w1:t1", "w1:p1");
    assert_success(&output);
    let log = fixture.log();
    assert!(log.contains("CALL plugin pane open"));
    assert!(!log.contains("CALL plugin pane close w1:p1"));
}

#[test]
fn verified_duplicates_are_healed_with_plugin_close() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        concat!(
            "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
            "w1:pA\tw1\tw1:t1\tfalse\tRepoRadar\tradar\t/workspace\n",
            "w1:pB\tw1\tw1:t2\tfalse\tRepoRadar\tradar\t/workspace\n"
        ),
    );
    let output = fixture.event("workspace.focused", "w1", "w1:t1", "w1:p1");
    assert_success(&output);
    assert!(fixture.log().contains("CALL plugin pane close w1:pB"));
    assert!(!fixture.panes().contains("w1:pB"));
}

#[test]
fn worktree_checkout_wins_and_foreground_cwd_never_becomes_the_root() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t/checkout\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/launch\n",
    );
    let output = fixture
        .event_command("workspace.focused", "w1", "w1:t1", "w1:p1")
        .env("FAKE_FOREGROUND_CWD", "/launch/child")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(fixture.log().contains("OPEN w1 /checkout"));
    assert!(!fixture.log().contains("/launch/child"));
}

#[test]
fn target_closure_during_open_is_retried_against_fresh_state() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let output = fixture
        .event_command("workspace.focused", "w1", "w1:t1", "w1:p1")
        .env("FAKE_FAIL_FIRST_OPEN", "1")
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(fixture.log().matches("CALL plugin pane open").count(), 2);
    assert!(fixture.panes().contains("w1:pR"));
}

#[test]
fn vanished_workspace_is_a_quiet_noop() {
    let fixture = Fixture::new("", "");
    let output = fixture.event("workspace.focused", "w9", "w9:t1", "w9:p1");
    assert_success(&output);
    assert!(!fixture.log().contains("CALL plugin pane open"));
}

#[test]
fn stale_lock_is_recovered_and_lock_name_is_session_scoped() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let checksum = checksum("/tmp/reporadar-test.sock");
    let stale = fixture
        .state
        .join("locks")
        .join(format!("reconcile-{checksum}-w1.lock"));
    fs::create_dir(&stale).unwrap();
    fs::write(stale.join("owner"), "999999\n").unwrap();
    let output = fixture
        .event_command("workspace.focused", "w1", "w1:t1", "w1:p1")
        .env("FAKE_CAPTURE_LOCKS", "1")
        .output()
        .unwrap();
    assert_success(&output);

    let second = fixture
        .event_command("workspace.focused", "w1", "w1:t1", "w1:p1")
        .env("HERDR_SOCKET_PATH", "/tmp/reporadar-other.sock")
        .env("FAKE_CAPTURE_LOCKS", "1")
        .output()
        .unwrap();
    assert_success(&second);
    let log = fixture.log();
    let locks = log
        .lines()
        .filter_map(|line| line.strip_prefix("LOCK "))
        .collect::<HashSet<_>>();
    assert!(
        locks.len() >= 2,
        "expected session-specific locks: {locks:?}"
    );
}

#[test]
fn live_lock_wait_is_bounded_to_about_two_seconds() {
    let fixture = Fixture::new(
        "w1\tw1:t1\t-\n",
        "w1:p1\tw1\tw1:t1\ttrue\t-\tother\t/workspace\n",
    );
    let checksum = checksum("/tmp/reporadar-test.sock");
    let lock = fixture
        .state
        .join("locks")
        .join(format!("reconcile-{checksum}-w1.lock"));
    fs::create_dir(&lock).unwrap();
    fs::write(lock.join("owner"), format!("{}\n", std::process::id())).unwrap();

    let start = Instant::now();
    let output = fixture
        .command("reconcile.sh")
        .args(["--workspace", "w1", "--no-focus"])
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    assert!(!output.status.success());
    assert!(
        elapsed >= Duration::from_millis(1800),
        "elapsed: {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(4), "elapsed: {elapsed:?}");
}

#[test]
fn manifest_registers_only_the_intended_lifecycle_hooks() {
    let manifest =
        fs::read_to_string(format!("{}/herdr-plugin.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(manifest.matches("[[startup]]").count(), 1);
    for event in ["workspace.created", "workspace.focused", "tab.focused"] {
        assert!(manifest.contains(&format!("on = \"{event}\"")));
    }
    assert!(!manifest.contains("pane.closed"));
    assert!(!manifest.contains("pane.exited"));
}

fn checksum(value: &str) -> String {
    let mut child = Command::new("cksum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(value.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}
