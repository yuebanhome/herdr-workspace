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

pub fn extract_pane_cwd_from_reader(mut reader: impl Read) -> Result<PathBuf> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid pane JSON")?;
    let pane = value.pointer("/result/pane").unwrap_or(&value);
    for key in ["foreground_cwd", "cwd"] {
        if let Some(path) = pane
            .get(key)
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        {
            return Ok(PathBuf::from(path));
        }
    }
    bail!("pane JSON contains no working directory")
}

pub fn extract_opened_pane_id_from_reader(mut reader: impl Read) -> Result<String> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid plugin pane response")?;
    find_string_by_key(&value, "pane_id")
        .map(ToOwned::to_owned)
        .context("plugin pane response contains no pane id")
}

pub fn find_plugin_pane_from_reader(mut reader: impl Read) -> Result<Option<String>> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input).context("invalid pane list JSON")?;
    let panes = value
        .pointer("/result/panes")
        .or_else(|| value.get("panes"))
        .and_then(Value::as_array)
        .context("pane list JSON contains no panes")?;

    for pane in panes {
        let owned_by_plugin = pane
            .get("plugin_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == "herdr-reporadar");
        let titled_as_plugin = ["terminal_title_stripped", "terminal_title", "title"]
            .into_iter()
            .filter_map(|key| pane.get(key).and_then(Value::as_str))
            .any(|title| title == "RepoRadar" || title == "herdr-reporadar");
        if (owned_by_plugin || titled_as_plugin)
            && let Some(id) = pane.get("pane_id").and_then(Value::as_str)
        {
            return Ok(Some(id.to_owned()));
        }
    }
    Ok(None)
}

fn context_path(value: &Value) -> Option<PathBuf> {
    const POINTERS: &[&str] = &[
        "/worktree/path",
        "/worktree/root",
        "/focused_pane/foreground_cwd",
        "/focused_pane/cwd",
        "/pane/foreground_cwd",
        "/pane/cwd",
        "/workspace/root",
        "/workspace/cwd",
        "/foreground_cwd",
        "/cwd",
    ];
    POINTERS
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .find(|path| !path.is_empty())
        .map(PathBuf::from)
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
        return format!("~/{}", relative.display());
    }
    root.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn extracts_foreground_directory_from_pane_response() {
        let json = r#"{"result":{"pane":{"cwd":"/old","foreground_cwd":"/workspace"}}}"#;
        assert_eq!(
            extract_pane_cwd_from_reader(Cursor::new(json)).unwrap(),
            PathBuf::from("/workspace")
        );
    }

    #[test]
    fn finds_owned_plugin_pane() {
        let json = r#"{"result":{"panes":[{"pane_id":"w1:p2","plugin_id":"herdr-reporadar"}]}}"#;
        assert_eq!(
            find_plugin_pane_from_reader(Cursor::new(json)).unwrap(),
            Some("w1:p2".to_owned())
        );
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
