use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub const ROOT_ENV: &str = "HERDR_REPORADAR_ROOT";

pub fn resolve_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return absolute(root);
    }
    if let Some(root) = env::var_os(ROOT_ENV).filter(|value| !value.is_empty()) {
        return absolute(PathBuf::from(root));
    }
    if let Ok(context) = env::var("HERDR_PLUGIN_CONTEXT_JSON")
        && let Ok(value) = serde_json::from_str::<Value>(&context)
        && let Some(root) = context_path(&value)
    {
        return absolute(root);
    }
    absolute(env::current_dir().context("could not determine the current directory")?)
}

pub fn extract_pane_launch_cwd_from_reader(mut reader: impl Read) -> Result<PathBuf> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid pane JSON")?;
    let pane = value.pointer("/result/pane").unwrap_or(&value);
    pane.get("cwd")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .context("pane JSON contains no launch working directory")
}

pub fn extract_workspace_checkout_from_reader(mut reader: impl Read) -> Result<PathBuf> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid workspace JSON")?;
    const POINTERS: &[&str] = &[
        "/result/workspace/worktree/checkout_path",
        "/workspace/worktree/checkout_path",
        "/worktree/checkout_path",
    ];
    POINTERS
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .find(|path| !path.is_empty())
        .map(PathBuf::from)
        .context("workspace JSON contains no worktree checkout")
}

pub fn extract_workspace_root_from_reader(mut reader: impl Read) -> Result<PathBuf> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid plugin context JSON")?;
    context_path(&value).context("plugin context contains no workspace directory")
}

pub fn extract_context_pane_id_from_reader(mut reader: impl Read) -> Result<String> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid plugin context JSON")?;
    const POINTERS: &[&str] = &[
        "/focused_pane_id",
        "/context/focused_pane_id",
        "/result/context/focused_pane_id",
        "/focused_pane/pane_id",
        "/pane_id",
    ];
    POINTERS
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .find(|pane| !pane.is_empty())
        .map(ToOwned::to_owned)
        .context("plugin context contains no focused pane id")
}

pub fn extract_opened_pane_id_from_reader(mut reader: impl Read) -> Result<String> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid plugin pane response")?;
    find_string_by_key(&value, "pane_id")
        .map(ToOwned::to_owned)
        .context("plugin pane response contains no pane id")
}

pub fn workspace_ids_from_reader(mut reader: impl Read) -> Result<Vec<String>> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid workspace list JSON")?;
    let workspaces = value
        .pointer("/result/workspaces")
        .or_else(|| value.get("workspaces"))
        .and_then(Value::as_array)
        .context("workspace list JSON contains no workspaces")?;

    workspaces
        .iter()
        .map(|workspace| {
            let id = workspace
                .get("workspace_id")
                .and_then(Value::as_str)
                .context("workspace entry contains no workspace id")?;
            validate_host_id(id, "workspace")?;
            Ok(id.to_owned())
        })
        .collect()
}

pub fn active_tab_from_workspace_reader(
    mut reader: impl Read,
    expected_workspace: &str,
) -> Result<String> {
    validate_host_id(expected_workspace, "workspace")?;
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid workspace JSON")?;
    let workspace = value.pointer("/result/workspace").unwrap_or(&value);
    let workspace_id = workspace
        .get("workspace_id")
        .and_then(Value::as_str)
        .context("workspace JSON contains no workspace id")?;
    if workspace_id != expected_workspace {
        bail!("workspace response is for {workspace_id}, not {expected_workspace}");
    }
    let tab_id = workspace
        .get("active_tab_id")
        .and_then(Value::as_str)
        .context("workspace JSON contains no active tab id")?;
    validate_host_id(tab_id, "tab")?;
    Ok(tab_id.to_owned())
}

pub fn candidate_panes_from_reader(
    mut reader: impl Read,
    expected_workspace: &str,
) -> Result<Vec<(String, String)>> {
    validate_host_id(expected_workspace, "workspace")?;
    let panes = pane_records(&mut reader)?;
    let mut candidates = Vec::new();
    for pane in panes {
        if pane.workspace_id != expected_workspace || !pane.radar_title {
            continue;
        }
        candidates.push((pane.pane_id, pane.tab_id));
    }
    candidates.sort();
    Ok(candidates)
}

pub fn select_target_pane_from_reader(
    mut reader: impl Read,
    expected_workspace: &str,
    expected_tab: &str,
    preferred: Option<&str>,
    excluded: &[String],
) -> Result<Option<String>> {
    validate_host_id(expected_workspace, "workspace")?;
    validate_host_id(expected_tab, "tab")?;
    if let Some(preferred) = preferred {
        validate_host_id(preferred, "pane")?;
    }
    for pane in excluded {
        validate_host_id(pane, "pane")?;
    }

    let mut panes = pane_records(&mut reader)?
        .into_iter()
        .filter(|pane| {
            pane.workspace_id == expected_workspace
                && pane.tab_id == expected_tab
                && !excluded.iter().any(|excluded| excluded == &pane.pane_id)
        })
        .collect::<Vec<_>>();
    panes.sort_by(|left, right| left.pane_id.cmp(&right.pane_id));

    if let Some(preferred) = preferred
        && panes.iter().any(|pane| pane.pane_id == preferred)
    {
        return Ok(Some(preferred.to_owned()));
    }
    if let Some(focused) = panes.iter().find(|pane| pane.focused) {
        return Ok(Some(focused.pane_id.clone()));
    }
    Ok(panes.first().map(|pane| pane.pane_id.clone()))
}

pub fn is_reporadar_process_from_reader(mut reader: impl Read) -> Result<bool> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid pane process JSON")?;
    let processes = value
        .pointer("/result/process_info/foreground_processes")
        .or_else(|| value.pointer("/process_info/foreground_processes"))
        .and_then(Value::as_array)
        .context("pane process JSON contains no foreground processes")?;

    Ok(processes.iter().any(|process| {
        process
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(is_reporadar_executable)
            || process
                .get("argv0")
                .and_then(Value::as_str)
                .is_some_and(is_reporadar_executable)
            || process
                .get("argv")
                .and_then(Value::as_array)
                .and_then(|argv| argv.first())
                .and_then(Value::as_str)
                .is_some_and(is_reporadar_executable)
    }))
}

fn context_path(value: &Value) -> Option<PathBuf> {
    const POINTERS: &[&str] = &[
        "/worktree/checkout_path",
        "/context/worktree/checkout_path",
        "/result/context/worktree/checkout_path",
        "/worktree/path",
        "/workspace_cwd",
        "/context/workspace_cwd",
        "/result/context/workspace_cwd",
        "/workspace/root",
        "/workspace/cwd",
        "/worktree/root",
        "/focused_pane/cwd",
        "/pane/cwd",
        "/cwd",
    ];
    POINTERS
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .find(|path| !path.is_empty())
        .map(PathBuf::from)
}

#[derive(Debug)]
struct PaneRecord {
    pane_id: String,
    tab_id: String,
    workspace_id: String,
    focused: bool,
    radar_title: bool,
}

fn pane_records(mut reader: impl Read) -> Result<Vec<PaneRecord>> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid pane list JSON")?;
    let panes = value
        .pointer("/result/panes")
        .or_else(|| value.get("panes"))
        .and_then(Value::as_array)
        .context("pane list JSON contains no panes")?;

    panes
        .iter()
        .map(|pane| {
            let pane_id = pane
                .get("pane_id")
                .and_then(Value::as_str)
                .context("pane entry contains no pane id")?;
            let tab_id = pane
                .get("tab_id")
                .and_then(Value::as_str)
                .context("pane entry contains no tab id")?;
            let workspace_id = pane
                .get("workspace_id")
                .and_then(Value::as_str)
                .context("pane entry contains no workspace id")?;
            validate_host_id(pane_id, "pane")?;
            validate_host_id(tab_id, "tab")?;
            validate_host_id(workspace_id, "workspace")?;
            let radar_title = [
                "label",
                "terminal_title_stripped",
                "terminal_title",
                "title",
            ]
            .into_iter()
            .filter_map(|key| pane.get(key).and_then(Value::as_str))
            .any(|title| title == "RepoRadar" || title == "herdr-reporadar");
            Ok(PaneRecord {
                pane_id: pane_id.to_owned(),
                tab_id: tab_id.to_owned(),
                workspace_id: workspace_id.to_owned(),
                focused: pane
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                radar_title,
            })
        })
        .collect()
}

fn is_reporadar_executable(value: &str) -> bool {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "herdr-reporadar")
}

pub fn validate_host_id(value: &str, kind: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
    {
        bail!("invalid {kind} id: {value:?}");
    }
    Ok(())
}

fn find_string_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Object(map) => map.get(key).and_then(Value::as_str).or_else(|| {
            map.values()
                .find_map(|child| find_string_by_key(child, key))
        }),
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_string_by_key(child, key)),
        _ => None,
    }
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

pub fn stdin() -> io::Stdin {
    io::stdin()
}

pub fn display_root(root: &Path) -> String {
    let home = env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home
        && let Ok(relative) = root.strip_prefix(home)
    {
        return sanitize_display(&format!("~/{}", relative.display()));
    }
    sanitize_display(&root.display().to_string())
}

fn sanitize_display(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn launch_directory_ignores_mutable_foreground_directory() {
        let json = r#"{"result":{"pane":{"cwd":"/workspace","foreground_cwd":"/workspace/repo"}}}"#;
        assert_eq!(
            extract_pane_launch_cwd_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/workspace")
        );
    }

    #[test]
    fn extracts_workspace_worktree_checkout() {
        let json = r#"{"result":{"workspace":{"worktree":{"checkout_path":"/checkout"}}}}"#;
        assert_eq!(
            extract_workspace_checkout_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/checkout")
        );
    }

    #[test]
    fn workspace_directory_wins_over_focused_pane_directory() {
        let json = r#"{
            "workspace_cwd":"/workspace",
            "focused_pane":{"foreground_cwd":"/workspace/repo-a"}
        }"#;
        assert_eq!(
            extract_workspace_root_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/workspace")
        );
    }

    #[test]
    fn worktree_checkout_wins_over_workspace_directory() {
        let json = r#"{
            "workspace_cwd":"/workspace",
            "worktree":{"checkout_path":"/checkout"}
        }"#;
        assert_eq!(
            extract_workspace_root_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/checkout")
        );
    }

    #[test]
    fn finds_workspace_directory_in_action_response_shape() {
        let json = r#"{"result":{"context":{"workspace_cwd":"/workspace"}}}"#;
        assert_eq!(
            extract_workspace_root_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/workspace")
        );
    }

    #[test]
    fn extracts_focused_pane_from_action_context() {
        let json = r#"{"workspace_id":"w5","focused_pane_id":"w5:p1"}"#;
        assert_eq!(
            extract_context_pane_id_from_reader(Cursor::new(json)).unwrap(),
            "w5:p1"
        );
    }

    #[test]
    fn display_root_sanitizes_terminal_control_characters() {
        let rendered = display_root(Path::new("/workspace/evil\u{1b}[2Jrepo"));
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.contains("evil?[2Jrepo"));
    }

    #[test]
    fn lists_and_validates_workspace_ids() {
        let json = r#"{"result":{"workspaces":[
            {"workspace_id":"w2"},{"workspace_id":"wA"}
        ]}}"#;
        assert_eq!(
            workspace_ids_from_reader(Cursor::new(json)).unwrap(),
            vec!["w2".to_owned(), "wA".to_owned()]
        );
    }

    #[test]
    fn rejects_workspace_ids_that_are_not_safe_path_components() {
        let json = r#"{"result":{"workspaces":[{"workspace_id":"../w2"}]}}"#;
        assert!(workspace_ids_from_reader(Cursor::new(json)).is_err());
    }

    #[test]
    fn extracts_active_tab_only_for_the_expected_workspace() {
        let json = r#"{"result":{"workspace":{"workspace_id":"w2","active_tab_id":"w2:t3"}}}"#;
        assert_eq!(
            active_tab_from_workspace_reader(Cursor::new(json), "w2").unwrap(),
            "w2:t3"
        );
        assert!(active_tab_from_workspace_reader(Cursor::new(json), "w1").is_err());
    }

    #[test]
    fn lists_only_exact_title_candidates_in_the_expected_workspace() {
        let json = r#"{"result":{"panes":[
            {"pane_id":"w1:p3","tab_id":"w1:t2","workspace_id":"w1","label":"RepoRadar","focused":false},
            {"pane_id":"w1:p2","tab_id":"w1:t1","workspace_id":"w1","terminal_title_stripped":"RepoRadar","focused":false},
            {"pane_id":"w1:p4","tab_id":"w1:t1","workspace_id":"w1","label":"RepoRadar logs","focused":false},
            {"pane_id":"w2:p1","tab_id":"w2:t1","workspace_id":"w2","label":"RepoRadar","focused":false}
        ]}}"#;
        assert_eq!(
            candidate_panes_from_reader(Cursor::new(json), "w1").unwrap(),
            vec![
                ("w1:p2".to_owned(), "w1:t1".to_owned()),
                ("w1:p3".to_owned(), "w1:t2".to_owned())
            ]
        );
    }

    #[test]
    fn target_selection_prefers_explicit_then_focused_then_lexical() {
        let json = r#"{"result":{"panes":[
            {"pane_id":"w1:p3","tab_id":"w1:t1","workspace_id":"w1","focused":false},
            {"pane_id":"w1:p2","tab_id":"w1:t1","workspace_id":"w1","focused":true},
            {"pane_id":"w1:p1","tab_id":"w1:t1","workspace_id":"w1","focused":false},
            {"pane_id":"w1:p4","tab_id":"w1:t2","workspace_id":"w1","focused":false}
        ]}}"#;
        assert_eq!(
            select_target_pane_from_reader(Cursor::new(json), "w1", "w1:t1", Some("w1:p3"), &[])
                .unwrap(),
            Some("w1:p3".to_owned())
        );
        assert_eq!(
            select_target_pane_from_reader(Cursor::new(json), "w1", "w1:t1", None, &[]).unwrap(),
            Some("w1:p2".to_owned())
        );
        assert_eq!(
            select_target_pane_from_reader(
                Cursor::new(json),
                "w1",
                "w1:t1",
                None,
                &["w1:p2".to_owned()]
            )
            .unwrap(),
            Some("w1:p1".to_owned())
        );
    }

    #[test]
    fn verifies_process_identity_without_trusting_cmdline_substrings() {
        let valid = r#"{"result":{"process_info":{"foreground_processes":[{
            "name":"herdr-reporadar","argv0":"herdr-reporadar",
            "argv":["/bundle/herdr-reporadar"]
        }]}}}"#;
        assert!(is_reporadar_process_from_reader(Cursor::new(valid)).unwrap());

        let deceptive = r#"{"result":{"process_info":{"foreground_processes":[{
            "name":"bash","argv0":"bash",
            "argv":["bash","-c","echo herdr-reporadar"],
            "cmdline":"bash -c echo herdr-reporadar"
        }]}}}"#;
        assert!(!is_reporadar_process_from_reader(Cursor::new(deceptive)).unwrap());
    }

    #[test]
    fn finds_pane_id_in_open_response() {
        let json = r#"{"result":{"pane":{"pane_id":"w1:p3"}}}"#;
        assert_eq!(
            extract_opened_pane_id_from_reader(Cursor::new(json)).unwrap(),
            "w1:p3"
        );
    }
}
