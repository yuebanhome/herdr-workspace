#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn routes_lookup_and_open_to_the_context_workspace() {
    let temp = tempdir().unwrap();
    let fake_herdr = temp.path().join("herdr");
    let log = temp.path().join("calls.log");
    fs::write(
        &fake_herdr,
        r#"#!/usr/bin/env bash
set -euo pipefail
{
  printf 'CALL\n'
  printf 'ARG:%s\n' "$@"
} >> "$FAKE_HERDR_LOG"
if [[ "$1 $2" == "pane list" ]]; then
  printf '%s\n' '{"result":{"panes":[]}}'
elif [[ "$1 $2 $3" == "plugin pane open" ]]; then
  printf '%s\n' '{"result":{"pane":{"pane_id":"w-test:p2"}}}'
fi
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_herdr).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_herdr, permissions).unwrap();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("bash")
        .arg(format!("{manifest_dir}/scripts/open.sh"))
        .current_dir(format!("{manifest_dir}/src"))
        .env("FAKE_HERDR_LOG", &log)
        .env("HERDR_BIN_PATH", &fake_herdr)
        .env("HERDR_PLUGIN_ROOT", manifest_dir)
        .env("HERDR_REPORADAR_BIN", env!("CARGO_BIN_EXE_herdr-reporadar"))
        .env("HERDR_WORKSPACE_ID", "w-test")
        .env_remove("HERDR_PANE_ID")
        .env(
            "HERDR_PLUGIN_CONTEXT_JSON",
            r#"{"workspace_cwd":"/workspace","focused_pane_id":"w-test:p1","focused_pane":{"foreground_cwd":"/workspace/child"}}"#,
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "launcher failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let calls = fs::read_to_string(log).unwrap();
    assert!(
        calls.contains("ARG:list\nARG:--workspace\nARG:w-test"),
        "unexpected calls:\n{calls}"
    );
    assert!(
        calls.contains("ARG:open\nARG:--target-pane\nARG:w-test:p1\nARG:--plugin"),
        "unexpected calls:\n{calls}"
    );
    assert!(
        calls.contains("ARG:HERDR_REPORADAR_ROOT=/workspace"),
        "unexpected calls:\n{calls}"
    );
}
